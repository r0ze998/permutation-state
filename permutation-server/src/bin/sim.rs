//! Balance measurement for Game Design V5: many Blitz seasons of AI-run
//! nations, one line per season and a summary against the calibration
//! targets of V5 §6.5.
//!
//!     cargo run --release --bin sim -- [seeds=20] [members=3,3,2,2,1,0]
//!
//! Every nation is run by hosted AI members (`driver`): they stand for
//! office, vote, propose, and the elected officers order within their
//! office. A nation with 0 members is run by the acting official alone and
//! takes no share of the pool. Personas rotate over the start positions
//! with the seed. Every decision is made from the nation's fogged belief.

use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::MemberId;
use permutation_rules::invariants;
use permutation_rules::payout::settle;
use permutation_rules::rng::Seed;
use permutation_rules::scoring::PATH_NAMES;
use permutation_rules::state::{CivId, WorldState};
use permutation_rules::tick::TickInput;
use permutation_rules::{Preset, Ruleset};
use permutation_server::bots::{city_count, Persona, PERSONAS};
use permutation_server::driver::{seat_ai_members, AiSeason, Planner};
use permutation_server::ledger::Ledger;
use std::collections::BTreeMap;

const ENTRY_FEE: u64 = 10_000_000;

struct Season {
    persona: Vec<Persona>,
    members: Vec<usize>,
    tiers: Vec<[u8; 4]>,
    era: Vec<u8>,
    points: Vec<u64>,
    share: Vec<f64>,
    cities: Vec<usize>,
    captured: Vec<u32>,
    budget_use: f64,
    /// Payout per member: (nation, officer ever, merit total, usdc).
    member_pay: Vec<(CivId, bool, u64, u64)>,
    adopted: u32,
    recalls: u32,
    merit_by_path: [u64; 5],
    violations: usize,
}

fn seed_bytes(tag: &str, i: u32) -> Seed {
    let mut s = [0u8; 32];
    let t = format!("permutation-state/{tag}/sim-{i:04}");
    s[..t.len().min(32)].copy_from_slice(&t.as_bytes()[..t.len().min(32)]);
    s
}

/// Ruleset with calibration overrides from `SIM_SET`, e.g.
/// `SIM_SET="science_techs=4,9,13;prosperity_pop=12,22,32,42,55"`.
fn rules() -> Ruleset {
    let mut r = Ruleset::new(Preset::Blitz);
    let spec = std::env::var("SIM_SET").unwrap_or_default();
    for part in spec.split(';').filter(|p| !p.trim().is_empty()) {
        let (k, v) = part.split_once('=').expect("key=values");
        let v: Vec<u32> = v
            .split(',')
            .map(|x| x.trim().parse().expect("number"))
            .collect();
        let set = |dst: &mut [u32]| dst.copy_from_slice(&v[..dst.len()]);
        match k.trim() {
            "hegemony_tiles" => set(&mut r.hegemony_tiles),
            "hegemony_cities" => set(&mut r.hegemony_cities),
            "prosperity_pop" => set(&mut r.prosperity_pop),
            "prosperity_wealth" => set(&mut r.prosperity_wealth),
            "science_techs" => set(&mut r.science_techs),
            "concord_trade" => set(&mut r.concord_trade),
            "concord_partners" => set(&mut r.concord_partners),
            "concord_suzerains" => set(&mut r.concord_suzerains),
            "tier_points" => set(&mut r.tier_points),
            other => panic!("unknown SIM_SET key {other}"),
        }
    }
    r
}

fn play(i: u32, members: &[usize]) -> Season {
    let rules = rules();
    let n = members.len();
    let persona: Vec<Persona> = (0..n)
        .map(|c| PERSONAS[(c + i as usize) % PERSONAS.len()])
        .collect();
    let mut s: WorldState = new_season(
        &rules,
        &seed_bytes("world", i),
        &seed_bytes("season", i),
        &nation_entries(n),
    )
    .expect("genesis");
    seat_ai_members(&mut s, &rules, members).expect("registration and first election");
    let mut officers_ever = vec![false; s.members.len()];
    let mut season = AiSeason::new(
        rules,
        s,
        Planner::with_personas(&persona),
        Ledger::seeded(&seed_bytes("ledger", i)),
    );
    let (mut spent, mut available) = (0u64, 0u64);
    let (mut adopted, mut recalls, mut violations) = (0u32, 0u32, 0usize);
    // SIM_DEBUG="seed:tick": check the invariants after every phase of that tick.
    let debug = std::env::var("SIM_DEBUG").ok().and_then(|d| {
        let (a, b) = d.split_once(':')?;
        Some((a.parse::<u32>().ok()?, b.parse::<u16>().ok()?))
    });
    while !season.over() {
        for nat in &season.state.nations {
            for m in nat.offices {
                if let Some(o) = officers_ever.get_mut(m as usize) {
                    *o = true;
                }
            }
        }
        let mut vrf = [0u8; 32];
        vrf[..2].copy_from_slice(&season.state.tick.to_le_bytes());
        vrf[2..6].copy_from_slice(&i.to_le_bytes());
        let input = season.plan(vrf);
        for b in &input.batches {
            spent += b.orders.iter().map(|o| o.cost() as u64).sum::<u64>();
        }
        for c in 0..n {
            available += season.state.civs[c].tick_budget as u64;
        }
        let holders_before: Vec<[MemberId; 4]> =
            season.state.nations.iter().map(|x| x.offices).collect();
        if debug == Some((i, season.state.tick)) {
            debug_tick(&mut season.state, &season.rules, &input);
            season.fog.update(&season.state);
        } else {
            season.resolve(&input).expect("tick");
        }
        let (s, rules) = (&season.state, &season.rules);
        for (c, before) in holders_before.iter().enumerate() {
            let now = s.nations[c].offices;
            if s.tick % rules.term_ticks != 0 {
                recalls += before.iter().zip(now).filter(|(a, b)| **a != *b).count() as u32;
            }
        }
        let v = invariants::check(s, rules);
        if !v.is_empty() {
            violations += v.len();
            eprintln!("seed {i}: invariant violated at tick {}: {v:?}", s.tick);
        }
    }
    let (s, rules) = (&season.state, &season.rules);
    let pool = ENTRY_FEE * s.members.len() as u64 * 8 / 10;
    let p = settle(s, rules, pool, ENTRY_FEE);
    let total: u64 = p.nation_share.iter().sum();
    let mut merit_by_path = [0u64; 5];
    for m in &s.members {
        for (k, x) in m.merit.iter().enumerate() {
            merit_by_path[k] += *x as u64;
        }
    }
    adopted += s.nations.iter().map(|x| x.adopted).sum::<u32>();
    Season {
        persona,
        members: members.to_vec(),
        tiers: p.scores.iter().map(|x| x.tiers).collect(),
        era: p.scores.iter().map(|x| x.era).collect(),
        points: p.scores.iter().map(|x| x.total()).collect(),
        share: p
            .nation_share
            .iter()
            .map(|x| {
                if total == 0 {
                    0.0
                } else {
                    *x as f64 / total as f64
                }
            })
            .collect(),
        cities: (0..n).map(|c| city_count(s, c as CivId)).collect(),
        captured: (0..n)
            .map(|c| permutation_rules::scoring::captured_held(s, c as CivId))
            .collect(),
        budget_use: if available == 0 {
            0.0
        } else {
            spent as f64 / available as f64
        },
        member_pay: s
            .members
            .iter()
            .enumerate()
            .map(|(id, m)| (m.civ, officers_ever[id], m.merit_total(), p.per_member[id]))
            .collect(),
        adopted,
        recalls,
        merit_by_path,
        violations,
    }
}

/// Resolve the open tick phase by phase, stopping the simulation at the
/// first phase that breaks an invariant (with the units and city involved).
fn debug_tick(s: &mut WorldState, rules: &Ruleset, input: &TickInput) {
    let tick = s.tick;
    while s.tick == tick {
        let (p, before) = (s.phase_cursor, s.clone());
        permutation_rules::tick::run_phase(s, rules, input, p).expect("phase");
        let v = invariants::check(s, rules);
        if v.is_empty() {
            continue;
        }
        eprintln!("phase {p} broke {v:?}");
        for viol in &v {
            let Some(u) = s.units.get(viol.id as usize) else {
                continue;
            };
            for x in s.units.iter().filter(|x| x.alive && x.hex == u.hex) {
                let b = &before.units[x.id as usize];
                eprintln!("  unit {} {:?} owner {:?} troops {} at {:?} (before: at {:?} alive {} path {:?}) standing {:?}", x.id, x.unit_type, x.owner, x.troops, x.hex, b.hex, b.alive, b.path, x.standing);
            }
            let city = s.cities.iter().find(|c| c.hex == u.hex);
            eprintln!(
                "  city here: {:?}",
                city.map(|c| (c.id, c.owner, c.alive, before.cities[c.id as usize].owner))
            );
        }
        std::process::exit(1);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u32 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(20);
    let members: Vec<usize> = args
        .get(2)
        .map(|a| a.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![3, 3, 2, 2, 1, 0]);
    let mut eras: Vec<u8> = Vec::new();
    let mut era5 = 0u32;
    let mut era_by_persona: BTreeMap<&str, Vec<u8>> = BTreeMap::new();
    let mut tier_counts = [[0u32; 6]; 4];
    let mut top_share = Vec::new();
    let mut over40 = 0;
    let mut pairs: BTreeMap<(usize, usize), u32> = BTreeMap::new();
    let mut dead_points = Vec::new();
    let (mut off_pay, mut off_n, mut non_pay, mut non_n) = (0u64, 0u64, 0u64, 0u64);
    let mut per_member_nation: BTreeMap<usize, (u64, u64)> = BTreeMap::new();
    let (mut adopted, mut recalls, mut budget, mut violations) = (0u32, 0u32, 0.0, 0usize);
    let mut merit_paths = [0u64; 5];
    println!("members per nation: {members:?}");
    println!("seed | eras | points | pool share % | tiers (H/P/S/C) | cities | captured held");
    let from: u32 = std::env::var("SIM_FROM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    for i in from..seeds {
        let r = play(i, &members);
        let counted: Vec<usize> = (0..r.members.len()).filter(|c| r.members[*c] > 0).collect();
        for &c in &counted {
            eras.push(r.era[c]);
            era5 += (r.era[c] == 5) as u32;
            era_by_persona
                .entry(r.persona[c].name())
                .or_default()
                .push(r.era[c]);
            for (p, counts) in tier_counts.iter_mut().enumerate() {
                counts[r.tiers[c][p] as usize] += 1;
            }
            // Which two paths carried the nation to its era.
            let mut t: Vec<(u8, usize)> = r.tiers[c].iter().copied().zip(0..4).collect();
            t.sort_by(|a, b| b.cmp(a));
            if r.era[c] >= 3 {
                let (a, b) = (t[0].1.min(t[1].1), t[0].1.max(t[1].1));
                *pairs.entry((a, b)).or_default() += 1;
            }
        }
        for c in 0..r.members.len() {
            if r.cities[c] == 0 {
                dead_points.push(r.points[c]);
            }
        }
        let top = r.share.iter().cloned().fold(0.0, f64::max);
        top_share.push(top);
        if top > 0.4 {
            over40 += 1;
        }
        for (civ, officer, _merit, pay) in &r.member_pay {
            if *officer {
                off_pay += pay;
                off_n += 1;
            } else {
                non_pay += pay;
                non_n += 1;
            }
            let e = per_member_nation
                .entry(r.members[*civ as usize])
                .or_default();
            e.0 += pay;
            e.1 += 1;
        }
        adopted += r.adopted;
        recalls += r.recalls;
        budget += r.budget_use;
        violations += r.violations;
        for (total, m) in merit_paths.iter_mut().zip(r.merit_by_path) {
            *total += m;
        }
        println!(
            "{i:4} | {:?} | {:?} | {:?} | {} | {:?} | {:?}",
            r.era,
            r.points,
            r.share
                .iter()
                .map(|x| (x * 100.0).round() as u32)
                .collect::<Vec<_>>(),
            r.tiers
                .iter()
                .map(|t| format!("{}{}{}{}", t[0], t[1], t[2], t[3]))
                .collect::<Vec<_>>()
                .join(" "),
            r.cities,
            r.captured,
        );
    }
    eras.sort();
    let median = eras.get(eras.len() / 2).copied().unwrap_or(0);
    println!("\n== {seeds} seasons, V5 §6.5 targets ==");
    println!(
        "era of nations with members: median {median} (target 2–3), distribution {:?}",
        (0..=5)
            .map(|e| eras.iter().filter(|x| **x == e).count())
            .collect::<Vec<_>>()
    );
    println!(
        "era 5 per season: {:.2} (target 0–1)",
        era5 as f64 / seeds as f64
    );
    println!(
        "top nation's pool share > 40%: {over40}/{seeds} (target: rare); mean top share {:.0}%",
        100.0 * top_share.iter().sum::<f64>() / top_share.len().max(1) as f64
    );
    let pair_names: Vec<String> = pairs
        .iter()
        .map(|((a, b), n)| format!("{}+{} {}", PATH_NAMES[*a], PATH_NAMES[*b], n))
        .collect();
    println!(
        "path pairs that reached era 3+: {} (target: every pair possible)",
        if pair_names.is_empty() {
            "none".into()
        } else {
            pair_names.join(", ")
        }
    );
    println!("dead nations' points: {:?} (target ≈ 0)", dead_points);
    for (p, name) in PATH_NAMES.iter().enumerate() {
        println!("  {name:10} tiers 0..5: {:?}", tier_counts[p]);
    }
    for (p, v) in &era_by_persona {
        let avg = v.iter().map(|x| *x as f64).sum::<f64>() / v.len().max(1) as f64;
        println!("  {p:9} mean era {avg:.2} over {} nations", v.len());
    }
    println!(
        "payout per member (USDC): officers {:.2}, others {:.2}; by nation size {}",
        off_pay as f64 / 1e6 / off_n.max(1) as f64,
        non_pay as f64 / 1e6 / non_n.max(1) as f64,
        per_member_nation
            .iter()
            .map(|(k, (sum, n))| format!("{k}:{:.2}", *sum as f64 / 1e6 / *n as f64))
            .collect::<Vec<_>>()
            .join(" "),
    );
    let tm: u64 = merit_paths.iter().sum();
    println!(
        "merit by path: {} ",
        ["hegemony", "prosperity", "science", "concord", "common"]
            .iter()
            .zip(merit_paths)
            .map(|(n, m)| format!("{n} {:.0}%", 100.0 * m as f64 / tm.max(1) as f64))
            .collect::<Vec<_>>()
            .join("  ")
    );
    println!(
        "proposals adopted: {:.1} per season; office changes mid-term (recalls): {:.1} per season",
        adopted as f64 / seeds as f64,
        recalls as f64 / seeds as f64
    );
    println!(
        "order budget used: {:.0}%   invariant violations: {violations}",
        100.0 * budget / seeds.max(1) as f64
    );
}
