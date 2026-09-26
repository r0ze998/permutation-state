//! Golden behaviour: whole seasons played by the hosted AI members must
//! produce exactly the recorded state roots, payouts and views.
//!
//! This is the safety net for refactoring: the engine is deterministic, so
//! any change that alters a single byte of any tick's world state, any
//! payout, or the JSON a client sees, fails here. When a change is meant to
//! alter behaviour, regenerate the file and review the diff:
//!
//!     UPDATE_GOLDEN=1 cargo test --release --test golden
//!
//! The file also records `RULES_VERSION` and the four ruleset hashes
//! (`[preset·2 + market]`, as the chain pins them). Roots (or ruleset
//! hashes) that change under the recorded `RULES_VERSION` fail: rules
//! behaviour must not change without a version bump (WP15), and
//! `UPDATE_GOLDEN=1` refuses to re-pin them. `UPDATE_GOLDEN=force` re-pins
//! anyway: allowed only while that version is undeployed (not in
//! DEPLOYS.md), so that one release shares one bump.

use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::hash::sha256;
use permutation_rules::params::RULES_VERSION;
use permutation_rules::payout::settle;
use permutation_rules::rng::Seed;
use permutation_rules::state::CivId;
use permutation_rules::{Preset, Ruleset};
use permutation_server::api;
use permutation_server::driver::{seat_ai_members, AiSeason, Planner};
use permutation_server::ledger::Ledger;
use serde_json::{json, Value};

const SEEDS: u32 = 4;
const MEMBERS: [usize; 6] = [3, 3, 2, 2, 1, 0];
const ENTRY_FEE: u64 = 10_000_000;

fn seed_bytes(tag: &str, i: u32) -> Seed {
    let mut s = [0u8; 32];
    let t = format!("permutation-state/{tag}/golden-{i:04}");
    s[..t.len().min(32)].copy_from_slice(&t.as_bytes()[..t.len().min(32)]);
    s
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// sha256 of a JSON value's canonical text (serde_json keeps key order).
fn digest(v: &Value) -> String {
    hex(&sha256(&[v.to_string().as_bytes()]))[..16].to_string()
}

fn season(i: u32) -> Value {
    let rules = Ruleset::new(Preset::Blitz);
    let mut s = new_season(
        &rules,
        &seed_bytes("world", i),
        &seed_bytes("season", i),
        &nation_entries(MEMBERS.len()),
    )
    .expect("genesis");
    seat_ai_members(&mut s, &rules, &MEMBERS).expect("seating");
    let mut season = AiSeason::new(
        rules,
        s,
        Planner::new(MEMBERS.len()),
        Ledger::seeded(&seed_bytes("ledger", i)),
    );
    let mut chain = [0u8; 32]; // running hash over every tick's root
    let mut views = serde_json::Map::new();
    while !season.over() {
        let mut vrf = [0u8; 32];
        vrf[..2].copy_from_slice(&season.state.tick.to_le_bytes());
        vrf[2..6].copy_from_slice(&i.to_le_bytes());
        let input = season.plan(vrf);
        let root = season.resolve(&input).expect("tick");
        chain = sha256(&[&chain, &root]);
        let (s, rules, fog) = (&season.state, &season.rules, &season.fog);
        if s.tick == 60 || s.tick == rules.ticks_per_season {
            // What each nation's client sees (fogged), and the spectator view.
            let mut per = Vec::new();
            for c in 0..MEMBERS.len() as CivId {
                let belief = fog.belief(s, c);
                per.push(digest(&api::world_view(&belief, rules, Some(c), fog)));
            }
            per.push(digest(&api::world_view(s, rules, None, fog)));
            views.insert(format!("tick{}", s.tick), json!(per));
        }
    }
    let (s, rules) = (&season.state, &season.rules);
    let pool = ENTRY_FEE * s.members.len() as u64 * 8 / 10;
    let p = settle(s, rules, pool, ENTRY_FEE);
    json!({
        "final_root": hex(&s.state_root().unwrap()),
        "roots_chain": hex(&chain),
        "payouts": p.per_member,
        "dust": p.dust,
        "settlement_view": digest(&api::settlement_view(&p)),
        "views": views,
    })
}

/// The ruleset hashes the chain pins, `[preset·2 + market]`.
fn ruleset_hashes() -> Vec<String> {
    [Preset::Blitz, Preset::Season]
        .into_iter()
        .flat_map(|p| {
            [false, true].map(|market| {
                let mut r = Ruleset::new(p);
                r.market_enabled = market;
                hex(&r.hash())
            })
        })
        .collect()
}

/// What a rules version pins: the ruleset hashes and every season's roots.
fn roots(doc: &Value) -> Vec<Value> {
    let mut out = vec![doc["ruleset_hashes"].clone()];
    if let Some(seasons) = doc["seasons"].as_object() {
        for s in seasons.values() {
            out.push(s["final_root"].clone());
            out.push(s["roots_chain"].clone());
        }
    }
    out
}

#[test]
fn seasons_replay_exactly_as_recorded() {
    let doc = json!({
        "rules_version": RULES_VERSION,
        "ruleset_hashes": ruleset_hashes(),
        "seasons": (0..SEEDS)
            .map(|i| (i.to_string(), season(i)))
            .collect::<serde_json::Map<_, _>>(),
    });
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden.json");
    let text = serde_json::to_string_pretty(&doc).unwrap() + "\n";
    let recorded = std::fs::read_to_string(&path).ok();
    let old: Option<Value> = recorded
        .as_deref()
        .and_then(|t| serde_json::from_str(t).ok());
    // Same version, other roots: the rules changed without a bump.
    let unbumped = old.as_ref().is_some_and(|old| {
        old["rules_version"].as_u64() == Some(RULES_VERSION as u64) && roots(old) != roots(&doc)
    });
    match std::env::var("UPDATE_GOLDEN").as_deref() {
        Ok("force") => {
            if unbumped {
                eprintln!(
                    "re-pinned under an unchanged RULES_VERSION ({RULES_VERSION}): allowed only \
                     while that version is undeployed (not in DEPLOYS.md)"
                );
            }
            std::fs::write(&path, &text).unwrap();
            return;
        }
        Ok(_) => {
            assert!(
                !unbumped,
                "rules behaviour changed without a RULES_VERSION bump: not re-pinned \
                 (UPDATE_GOLDEN=force only while RULES_VERSION {RULES_VERSION} is undeployed)"
            );
            std::fs::write(&path, &text).unwrap();
            return;
        }
        Err(_) => {}
    }
    let recorded = recorded.expect("tests/golden.json exists (run with UPDATE_GOLDEN=1)");
    assert!(
        !unbumped,
        "rules behaviour changed without a RULES_VERSION bump"
    );
    assert_eq!(
        recorded, text,
        "behaviour changed: if intended, regenerate with UPDATE_GOLDEN=1 and review the diff"
    );
}
