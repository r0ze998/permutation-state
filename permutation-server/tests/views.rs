//! Views per viewer (seat / watcher / spectator), agent preflight, and the
//! ledger learning an outside agent's commitment from its batch.

use permutation_rules::decision::{decision_digest, policy_id, rationale_hash};
use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::{Role, NOBODY};
use permutation_rules::hex::Hex;
use permutation_rules::orders::{Order, OrderBatch};
use permutation_rules::state::{CivId, Owner, WorldState};
use permutation_rules::tech::Tech;
use permutation_rules::{Preset, Ruleset};
use permutation_server::api::{preflight, world_view};
use permutation_server::fog::Fog;
use permutation_server::ledger::Ledger;

fn season() -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let state = new_season(
        &rules,
        b"permutation-state/world/test-001",
        b"permutation-state/season/test-01",
        &nation_entries(6),
    )
    .unwrap();
    (rules, state)
}

/// Perfect information: a nation's view shows the whole world, as the
/// spectator's does, and adds only its own economy, relations and sight.
#[test]
fn every_view_shows_the_whole_world() {
    let (rules, state) = season();
    let fog = Fog::new(&state);
    let spec = world_view(&state, &rules, None, &fog);
    assert_eq!(spec["spectator"], true);
    assert!(spec["me"].is_null() && spec["economy"].is_null() && spec["sight"].is_null());
    assert!(spec["fog"].as_str().unwrap().chars().all(|c| c == '2'));
    let alive = state.units.iter().filter(|u| u.alive).count();
    assert_eq!(spec["units"].as_array().unwrap().len(), alive);

    let me: CivId = 2;
    let view = world_view(&fog.belief(&state, me), &rules, Some(me), &fog);
    assert_eq!(view["me"], 2);
    assert!(!view["economy"].is_null());
    assert!(view["fog"].as_str().unwrap().chars().all(|c| c == '2'));
    assert_eq!(view["units"].as_array().unwrap().len(), alive);
    for c in view["civs"].as_array().unwrap() {
        assert!(!c["pop"].is_null() && !c["techs"].is_null(), "{c}");
    }
    for c in view["cities"].as_array().unwrap() {
        assert!(c["queue"].is_array(), "every city's queue is public: {c}");
    }
    assert_eq!(
        view["cityStates"].as_array().unwrap().len(),
        state.city_states.len()
    );
    // Sight is display only: at genesis a nation overlooks part of the map.
    let sight = view["sight"].as_str().unwrap();
    assert_eq!(sight.len(), state.map.tiles.len());
    assert!(sight.contains('1') && sight.contains('2'));
}

#[test]
fn preflight_names_orders_that_would_be_skipped() {
    let (rules, state) = season();
    let fog = Fog::new(&state);
    let me: CivId = 1;
    let belief = fog.belief(&state, me);
    let mine = state
        .units
        .iter()
        .find(|u| u.owner == Owner::Civ(me))
        .unwrap();
    let theirs = state
        .units
        .iter()
        .find(|u| u.owner == Owner::Civ(0))
        .unwrap();
    let far = Hex::new(mine.hex.q + 3, mine.hex.r);
    let orders = vec![
        Order::MoveUnit {
            unit: theirs.id,
            path: vec![theirs.hex],
        },
        Order::MoveUnit {
            unit: mine.id,
            path: vec![far],
        },
        Order::SetResearch {
            techs: vec![Tech::Writing],
        },
        Order::SetResearch {
            techs: vec![Tech::Agriculture, Tech::Writing],
        },
        Order::DeclareWar { civ: me },
    ];
    let w = preflight(&belief, &rules, me, &orders);
    let codes: Vec<(u64, String)> = w
        .iter()
        .map(|x| {
            (
                x["index"].as_u64().unwrap(),
                x["blocked"]["code"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    // Civ 0's unit is out of sight at genesis, so to civ 1 it does not exist.
    assert!(
        codes.contains(&(0, "UnknownUnit".into())) || codes.contains(&(0, "NotYours".into())),
        "{codes:?}"
    );
    assert!(
        codes.contains(&(1, "PathNotContiguous".into())),
        "{codes:?}"
    );
    assert!(codes.contains(&(2, "NeedsTech".into())), "{codes:?}");
    assert!(
        !codes.iter().any(|(i, _)| *i == 3),
        "prerequisite earlier in the same queue is fine: {codes:?}"
    );
    assert!(codes.contains(&(4, "SameCiv".into())), "{codes:?}");
}

#[test]
fn ledger_verifies_an_outside_agents_reveal() {
    let (_, state) = season();
    let fog = Fog::new(&state);
    let mut ledger = Ledger::new(b"test");
    ledger.observe(&state, &fog);
    let civ: CivId = 5;
    let obs = ledger.root(0, civ).unwrap();
    let (policy, salt, text) = (
        b"agent/v1".to_vec(),
        [7u8; 16],
        "北へ探索する".as_bytes().to_vec(),
    );
    let digest = decision_digest(0, &obs, &policy_id(&policy), &rationale_hash(&salt, &text));
    ledger.ingest_external(&OrderBatch {
        civ,
        tick: 0,
        role: Role::General,
        member: NOBODY,
        adopt: vec![],
        decision_digest: digest,
        orders: vec![],
    });
    assert!(ledger.record(0, civ, 0).unwrap().revealed_at.is_none());
    ledger.ingest_external(&OrderBatch {
        civ,
        tick: 1,
        role: Role::General,
        member: NOBODY,
        adopt: vec![],
        decision_digest: [0; 32],
        orders: vec![Order::RevealRationale {
            tick: 0,
            policy,
            salt,
            text,
        }],
    });
    let r = ledger.record(0, civ, 0).unwrap();
    assert_eq!(r.revealed_at, Some(1));
    assert!(r.external && r.verified());
    assert!(
        ledger.record(1, civ, 0).is_none(),
        "a zero digest is not a commitment"
    );

    // A reveal that does not match fails verification instead of being trusted.
    let digest2 = decision_digest(
        1,
        &ledger.root(0, 4).unwrap(),
        &policy_id(b"x"),
        &rationale_hash(&[1; 16], b"y"),
    );
    ledger.ingest_external(&OrderBatch {
        civ: 4,
        tick: 0,
        role: Role::General,
        member: NOBODY,
        adopt: vec![],
        decision_digest: digest2,
        orders: vec![],
    });
    ledger.ingest_external(&OrderBatch {
        civ: 4,
        tick: 1,
        role: Role::General,
        member: NOBODY,
        adopt: vec![],
        decision_digest: [0; 32],
        orders: vec![Order::RevealRationale {
            tick: 0,
            policy: b"x".to_vec(),
            salt: [2; 16],
            text: b"y".to_vec(),
        }],
    });
    assert!(!ledger.record(0, 4, 0).unwrap().verified());
}

#[test]
fn reveals_fit_in_one_transaction_and_keep_their_order() {
    use permutation_server::ledger::BATCH_BYTES;
    let (_, mut state) = season();
    let fog = Fog::new(&state);
    let mut ledger = Ledger::new(b"test");
    let long = "判断".repeat(80); // 480 bytes
    for t in 0..3u16 {
        state.tick = t;
        ledger.observe(&state, &fog);
        ledger.commit(t, 1, 0, "bot/test", &long);
    }
    let r = ledger.reveals(1, 0, 3, &[]);
    let size: usize = r.iter().map(|o| borsh::to_vec(o).unwrap().len()).sum();
    assert_eq!(r.len(), 1, "two long reveals do not fit next to each other");
    assert!(size <= BATCH_BYTES);
    assert!(
        matches!(r[0], Order::RevealRationale { tick: 0, .. }),
        "oldest first"
    );
    // Short rationales still go three at a time.
    let mut ledger = Ledger::new(b"test");
    for t in 0..3u16 {
        state.tick = t;
        ledger.observe(&state, &fog);
        ledger.commit(t, 1, 0, "bot/test", "short");
    }
    assert_eq!(ledger.reveals(1, 0, 3, &[]).len(), 3);
}
