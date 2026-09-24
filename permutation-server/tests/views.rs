//! Views per viewer (seat / watcher / spectator), agent preflight, and the
//! ledger learning an outside agent's commitment from its batch.

use permutation_rules::decision::{decision_digest, policy_id, rationale_hash};
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::hex::Hex;
use permutation_rules::orders::{Order, OrderBatch};
use permutation_rules::state::{CivId, DeclaredKind, Owner, WorldState};
use permutation_rules::tech::Tech;
use permutation_rules::{Preset, Ruleset};
use permutation_server::api::{preflight, world_view};
use permutation_server::fog::Fog;
use permutation_server::ledger::Ledger;

fn season() -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let entries: Vec<Entry> = (0..6)
        .map(|i| Entry {
            name: ["Aster", "Borealis", "Cinder", "Dunmar", "Ember", "Fjordhal"][i].into(),
            declared_kind: if i == 0 { DeclaredKind::Human } else { DeclaredKind::Agent },
            payout_wallet: [i as u8 + 1; 32],
            exchange_deposit: 0,
        })
        .collect();
    let state = new_season(&rules, b"permutation-state/world/test-001", b"permutation-state/season/test-01", &entries).unwrap();
    (rules, state)
}

#[test]
fn spectator_sees_everything_a_civ_only_its_own() {
    let (rules, state) = season();
    let fog = Fog::new(&state);
    let spec = world_view(&state, &rules, None, &fog);
    assert_eq!(spec["spectator"], true);
    assert!(spec["me"].is_null() && spec["economy"].is_null());
    assert!(spec["fog"].as_str().unwrap().chars().all(|c| c == '2'));
    assert!(spec["civs"].as_array().unwrap().iter().all(|c| !c["pop"].is_null()));
    assert_eq!(spec["units"].as_array().unwrap().len(), state.units.iter().filter(|u| u.alive).count());

    let me: CivId = 2;
    let view = world_view(&fog.belief(&state, me), &rules, Some(me), &fog);
    assert_eq!(view["me"], 2);
    assert!(!view["economy"].is_null());
    for c in view["civs"].as_array().unwrap() {
        assert_eq!(c["pop"].is_null(), c["id"] != 2, "only its own population is known");
    }
    assert!(view["fog"].as_str().unwrap().contains('0'), "a civ does not see the whole map at genesis");
}

#[test]
fn preflight_names_orders_that_would_be_skipped() {
    let (rules, state) = season();
    let fog = Fog::new(&state);
    let me: CivId = 1;
    let belief = fog.belief(&state, me);
    let mine = state.units.iter().find(|u| u.owner == Owner::Civ(me)).unwrap();
    let theirs = state.units.iter().find(|u| u.owner == Owner::Civ(0)).unwrap();
    let far = Hex::new(mine.hex.q + 3, mine.hex.r);
    let orders = vec![
        Order::MoveUnit { unit: theirs.id, path: vec![theirs.hex] },
        Order::MoveUnit { unit: mine.id, path: vec![far] },
        Order::SetResearch { techs: vec![Tech::Writing] },
        Order::SetResearch { techs: vec![Tech::Agriculture, Tech::Writing] },
        Order::DeclareWar { civ: me },
    ];
    let w = preflight(&belief, &rules, me, &orders);
    let codes: Vec<(u64, String)> = w.iter().map(|x| (x["index"].as_u64().unwrap(), x["blocked"]["code"].as_str().unwrap().to_string())).collect();
    // Civ 0's unit is out of sight at genesis, so to civ 1 it does not exist.
    assert!(codes.contains(&(0, "UnknownUnit".into())) || codes.contains(&(0, "NotYours".into())), "{codes:?}");
    assert!(codes.contains(&(1, "PathNotContiguous".into())), "{codes:?}");
    assert!(codes.contains(&(2, "NeedsTech".into())), "{codes:?}");
    assert!(!codes.iter().any(|(i, _)| *i == 3), "prerequisite earlier in the same queue is fine: {codes:?}");
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
    let (policy, salt, text) = (b"agent/v1".to_vec(), [7u8; 16], "北へ探索する".as_bytes().to_vec());
    let digest = decision_digest(0, &obs, &policy_id(&policy), &rationale_hash(&salt, &text));
    ledger.ingest_external(&OrderBatch { civ, tick: 0, decision_digest: digest, orders: vec![] });
    assert!(ledger.record(0, civ).unwrap().revealed_at.is_none());
    ledger.ingest_external(&OrderBatch { civ, tick: 1, decision_digest: [0; 32], orders: vec![Order::RevealRationale { tick: 0, policy, salt, text }] });
    let r = ledger.record(0, civ).unwrap();
    assert_eq!(r.revealed_at, Some(1));
    assert!(r.external && r.verified());
    assert!(ledger.record(1, civ).is_none(), "a zero digest is not a commitment");

    // A reveal that does not match fails verification instead of being trusted.
    let digest2 = decision_digest(1, &ledger.root(0, 4).unwrap(), &policy_id(b"x"), &rationale_hash(&[1; 16], b"y"));
    ledger.ingest_external(&OrderBatch { civ: 4, tick: 0, decision_digest: digest2, orders: vec![] });
    ledger.ingest_external(&OrderBatch { civ: 4, tick: 1, decision_digest: [0; 32], orders: vec![Order::RevealRationale { tick: 0, policy: b"x".to_vec(), salt: [2; 16], text: b"y".to_vec() }] });
    assert!(!ledger.record(0, 4).unwrap().verified());
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
        ledger.commit(t, 1, "bot/test", &long);
    }
    let r = ledger.reveals(1, 3, &[]);
    let size: usize = r.iter().map(|o| borsh::to_vec(o).unwrap().len()).sum();
    assert_eq!(r.len(), 1, "two long reveals do not fit next to each other");
    assert!(size <= BATCH_BYTES);
    assert!(matches!(r[0], Order::RevealRationale { tick: 0, .. }), "oldest first");
    // Short rationales still go three at a time.
    let mut ledger = Ledger::new(b"test");
    for t in 0..3u16 {
        state.tick = t;
        ledger.observe(&state, &fog);
        ledger.commit(t, 1, "bot/test", "short");
    }
    assert_eq!(ledger.reveals(1, 3, &[]).len(), 3);
}
