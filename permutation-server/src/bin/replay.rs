//! Bot match + replay recorder.
//!
//! Plays one Blitz match (180 ticks) with the scripted bots on the real
//! engine, checks every invariant each tick, and writes a JSON replay for
//! the viewer:
//!
//!     cargo run --release --bin replay -- ../permutation-rules/viewer/replay.json

use permutation_rules::buildings::Building;
use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::invariants;
use permutation_rules::rng::Seed;
use permutation_rules::state::{CivId, Owner, Relation, WorldState};
use permutation_rules::{Preset, Ruleset};
use permutation_server::bots::{city_count, troops_of, PERSONAS};
use permutation_server::driver::{seat_ai_members, AiSeason, Planner};
use permutation_server::events::diff_events;
use permutation_server::ledger::Ledger;
use std::fmt::Write as _;

// ------------------------------------------------------------------ recording

fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(ch),
        }
    }
    out.push('"');
}

fn rel_code(r: Relation) -> char {
    match r {
        Relation::Peace => 'P',
        Relation::War { .. } => 'W',
        Relation::Nap { .. } => 'N',
        Relation::Alliance { .. } => 'A',
    }
}

fn frame(s: &WorldState, events: &[String]) -> String {
    let mut f = String::new();
    let n = s.civs.len() as CivId;
    write!(f, "{{\"t\":{},\"own\":\"", s.tick).unwrap();
    for t in &s.map.tiles {
        let owner = t
            .owner_city
            .and_then(|c| s.cities.get(c as usize))
            .filter(|c| c.alive)
            .and_then(|c| c.owner);
        f.push(match owner {
            Some(o) => char::from_digit(o as u32, 36).unwrap(),
            None => '.',
        });
    }
    f.push_str("\",\"cities\":[");
    let cities: Vec<String> = s
        .cities
        .iter()
        .filter(|c| c.alive)
        .map(|c| {
            format!(
                "[{},{},{},{},{},{},{},{},{}]",
                c.id,
                c.hex.q,
                c.hex.r,
                c.owner.map_or(-1, |o| o as i32),
                c.pop,
                c.defense / 1000,
                c.buildings.has(Building::Walls) as u8,
                c.buildings.star_gate_stages(),
                c.razing.is_some() as u8
            )
        })
        .collect();
    f.push_str(&cities.join(","));
    f.push_str("],\"units\":[");
    let units: Vec<String> = s
        .units
        .iter()
        .filter(|u| u.alive)
        .map(|u| {
            let owner = match u.owner {
                Owner::Civ(c) => c as i32,
                Owner::Barbarian => -2,
            };
            format!(
                "[{},{},{},{},{},{}]",
                u.id,
                u.hex.q,
                u.hex.r,
                owner,
                u.unit_type as u8,
                u.troops / 100
            )
        })
        .collect();
    f.push_str(&units.join(","));
    f.push_str("],\"civs\":[");
    let civs: Vec<String> = s
        .civs
        .iter()
        .map(|c| {
            let pop: u32 = s.living_cities_of(c.id).map(|x| x.pop).sum();
            format!(
                "[{},{},{},{},{},{},{},{},{},{},{},{}]",
                c.gold / 1000,
                c.scores.science_total,
                c.achievements.era,
                c.achievements.tiers.iter().map(|t| *t as u32).sum::<u32>(),
                c.scores.star_gate_stages,
                c.techs.count(),
                c.war_weariness,
                c.is_aggressor(s.tick.saturating_sub(1), 12) as u8,
                pop,
                city_count(s, c.id),
                c.influence / 1000,
                troops_of(s, c.id)
            )
        })
        .collect();
    f.push_str(&civs.join(","));
    f.push_str("],\"rel\":\"");
    for a in 0..n {
        for b in a + 1..n {
            f.push(rel_code(s.relation(a, b)));
        }
    }
    f.push_str("\",\"cs\":[");
    let cs: Vec<String> = s
        .city_states
        .iter()
        .map(|c| {
            format!(
                "[{},{},{}]",
                c.suzerain.map_or(-1, |x| x as i32),
                c.captured_by.map_or(-1, |x| x as i32),
                c.pop
            )
        })
        .collect();
    f.push_str(&cs.join(","));
    f.push_str("],\"ev\":[");
    let mut first = true;
    for e in events {
        if !first {
            f.push(',');
        }
        first = false;
        json_str(&mut f, e);
    }
    f.push_str("]}");
    f
}

fn main() {
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "replay.json".to_string());
    let rules = Ruleset::new(Preset::Blitz);
    let world: Seed = *b"permutation-state/world/blitz-01";
    let season: Seed = *b"permutation-state/season/demo-01";
    let mut s = new_season(&rules, &world, &season, &nation_entries(6)).expect("genesis");
    seat_ai_members(&mut s, &rules, &[3, 3, 2, 2, 1, 0]).expect("first election");

    let mut out = String::new();
    out.push_str("{\"meta\":{");
    write!(
        out,
        "\"preset\":\"Blitz\",\"ticks\":{},\"tickSeconds\":{},\"rulesetHash\":\"{}\",",
        rules.ticks_per_season,
        rules.tick_seconds,
        s.ruleset_hash
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
    .unwrap();
    out.push_str("\"civs\":[");
    let civs: Vec<String> = s
        .civs
        .iter()
        .map(|c| {
            let mut e = String::from("{\"name\":");
            json_str(&mut e, &c.name);
            write!(
                e,
                ",\"persona\":\"{}\",\"members\":{}}}",
                PERSONAS[c.id as usize].name(),
                s.nations[c.id as usize].members
            )
            .unwrap();
            e
        })
        .collect();
    out.push_str(&civs.join(","));
    out.push_str("]},\"map\":{");
    write!(out, "\"radius\":{},\"tiles\":[", s.map.radius).unwrap();
    let tiles: Vec<String> = s
        .map
        .tiles
        .iter()
        .map(|t| {
            let res = match t.resource {
                None => -1,
                Some(r) => r as i32,
            };
            format!(
                "[{},{},{},{},{}]",
                t.hex.q, t.hex.r, t.terrain as u8, t.river as u8, res
            )
        })
        .collect();
    out.push_str(&tiles.join(","));
    out.push_str("]},\"cityStates\":[");
    let css: Vec<String> = s
        .city_states
        .iter()
        .map(|c| format!("[{},{},{},\"{:?}\"]", c.id, c.hex.q, c.hex.r, c.specialty))
        .collect();
    out.push_str(&css.join(","));
    out.push_str("],\"frames\":[");
    out.push_str(&frame(&s, &[]));

    let names: Vec<String> = s.civs.iter().map(|c| c.name.clone()).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut roots = Vec::new();
    // Bots decide from their own fogged belief state, like any player (§7.4).
    let mut season = AiSeason::new(rules, s, Planner::new(6), Ledger::seeded(b"replay"));
    while !season.over() {
        let mut vrf = [0u8; 32];
        vrf[..2].copy_from_slice(&season.state.tick.to_le_bytes());
        let input = season.plan(vrf);
        let prev = season.state.clone();
        roots.push(season.resolve(&input).expect("tick"));
        let s = &season.state;
        let v = invariants::check(s, &season.rules);
        assert!(v.is_empty(), "invariant violated at tick {}: {v:?}", s.tick);
        let ev = diff_events(&prev, s, &names);
        out.push(',');
        out.push_str(&frame(s, &ev));
    }
    let s = &season.state;
    out.push_str("],\"finalRoot\":\"");
    out.push_str(
        &roots
            .last()
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
    );
    out.push_str("\"}");
    std::fs::write(&out_path, &out).expect("write replay");
    eprintln!("wrote {} ({} bytes, {} ticks)", out_path, out.len(), s.tick);
    for c in &s.civs {
        eprintln!(
            "{:9} {:8} cities {} pop {:3} techs {:2} stages {} tiers {:?} era {} sci {:5} troops {}",
            c.name,
            PERSONAS[c.id as usize].name(),
            city_count(s, c.id),
            s.living_cities_of(c.id).map(|x| x.pop).sum::<u32>(),
            c.techs.count(),
            c.scores.star_gate_stages,
            c.achievements.tiers,
            c.achievements.era,
            c.scores.science_total,
            troops_of(s, c.id)
        );
    }
}
