//! Verifiable decision logs: Merkle proofs, observation roots, commit-reveal (§4.3, §7.5).

use permutation_rules::decision::{
    decision_digest, leaf_hash, merkle_proof, merkle_root, obs_leaves, obs_root, policy_id,
    rationale_hash, verify_proof, verify_reveal, Hash,
};
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::gov::Role;
use permutation_rules::map::Terrain;
use permutation_rules::orders::{validate_batch, Order, OrderBatch};
use permutation_rules::rng::Seed;
use permutation_rules::state::{Owner, WorldState};
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::vision::{belief, visible, Memory};
use permutation_rules::{Preset, RulesError, Ruleset};

mod common;

fn setup() -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let entries: Vec<Entry> = (0..6)
        .map(|i| Entry {
            name: format!("civ-{i}"),
            treasury: 0,
        })
        .collect();
    let mut s = new_season(&rules, &[5; 32], &[6; 32], &entries).expect("genesis");
    for t in &mut s.map.tiles {
        t.terrain = Terrain::Grassland; // no mountains: tests control sight lines
    }
    (rules, s)
}

fn root_for(s: &WorldState, civ: u16) -> Hash {
    let seen = visible(s, civ);
    let mut mem = Memory::new(s);
    mem.update(s, civ, &seen);
    let b = belief(s, civ, &seen, &mem);
    obs_root(&obs_leaves(&b, civ, &seen, &mem))
}

fn vrf(tick: u16) -> Seed {
    let mut v = [0u8; 32];
    v[..2].copy_from_slice(&tick.to_le_bytes());
    v
}

#[test]
fn every_leaf_has_a_valid_proof_and_tampering_breaks_it() {
    for n in 1..=9usize {
        let leaves: Vec<Hash> = (0..n).map(|i| leaf_hash(b"t", &[i as u8])).collect();
        let root = merkle_root(&leaves);
        for i in 0..n {
            let proof = merkle_proof(&leaves, i);
            assert!(verify_proof(&root, &leaves[i], &proof), "n={n} i={i}");
            assert!(
                !verify_proof(&root, &leaf_hash(b"t", &[99]), &proof),
                "n={n} i={i} forged leaf"
            );
        }
    }
    // Leaves and inner nodes are domain-separated.
    assert_ne!(
        leaf_hash(b"t", &[1]),
        merkle_root(&[leaf_hash(b"t", &[1]), leaf_hash(b"t", &[2])])
    );
}

#[test]
fn the_observation_ignores_what_the_civ_cannot_see() {
    let (_, mut s) = setup();
    let before = root_for(&s, 0);
    assert_eq!(before, root_for(&s, 0), "deterministic");

    // Moving a far-away enemy unit, or changing an enemy's treasury, does not
    // change what civ 0 observed.
    let far = s
        .units
        .iter()
        .position(|u| u.owner == Owner::Civ(3))
        .unwrap();
    s.units[far].hex = s.units[far].hex.neighbors()[0];
    s.civs[3].gold += 50_000;
    assert_eq!(
        root_for(&s, 0),
        before,
        "hidden changes leave obs_root unchanged"
    );

    // Changing something civ 0 does see changes the root.
    let mine = s
        .units
        .iter()
        .position(|u| u.owner == Owner::Civ(0))
        .unwrap();
    s.units[mine].troops += 1000;
    assert_ne!(root_for(&s, 0), before, "visible changes do not");
}

#[test]
fn a_single_fact_can_be_proven_against_the_root() {
    let (_, s) = setup();
    let seen = visible(&s, 0);
    let mut mem = Memory::new(&s);
    mem.update(&s, 0, &seen);
    let b = belief(&s, 0, &seen, &mem);
    let leaves = obs_leaves(&b, 0, &seen, &mem);
    let hashes: Vec<Hash> = leaves.iter().map(|l| l.hash()).collect();
    let root = merkle_root(&hashes);
    // "Tile 5 was fog when civ 0 decided": its leaf carries the fog code.
    let i = leaves
        .iter()
        .position(|l| l.kind == "tile" && l.id == 5)
        .unwrap();
    assert!(verify_proof(&root, &hashes[i], &merkle_proof(&hashes, i)));
}

#[test]
fn reveals_open_the_commitment_and_nothing_else_does() {
    let (_, s) = setup();
    let root = root_for(&s, 0);
    let (salt, text) = ([7u8; 16], "defend the capital".as_bytes());
    let digest = decision_digest(0, &root, &policy_id(b"human"), &rationale_hash(&salt, text));
    assert!(verify_reveal(&digest, 0, &root, b"human", &salt, text));
    assert!(
        !verify_reveal(&digest, 0, &root, b"human", &salt, b"attack Ember"),
        "rewritten reason"
    );
    assert!(
        !verify_reveal(&digest, 0, &root, b"human", &[8; 16], text),
        "wrong salt"
    );
    assert!(
        !verify_reveal(&digest, 0, &root, b"bot/warlord@1", &salt, text),
        "wrong policy"
    );
    assert!(
        !verify_reveal(&digest, 1, &root, b"human", &salt, text),
        "wrong tick"
    );
    assert!(
        !verify_reveal(&digest, 0, &[0; 32], b"human", &salt, text),
        "different observation"
    );
}

#[test]
fn commitments_and_reveals_enter_the_event_chain() {
    let (rules, mut s0) = setup();
    common::staff(&mut s0);
    let general = s0.nations[0].holder(Role::General);
    let run = |digest: Hash| {
        let mut s = s0.clone();
        let batch = OrderBatch {
            civ: 0,
            tick: 0,
            role: Role::General,
            member: general,
            adopt: vec![],
            decision_digest: digest,
            orders: vec![],
        };
        resolve_tick(
            &mut s,
            &rules,
            &TickInput {
                vrf: vrf(0),
                batches: vec![batch],
                ..Default::default()
            },
        )
        .unwrap();
        s
    };
    let (a, b) = (run([1; 32]), run([2; 32]));
    assert_ne!(
        a.event_head, b.event_head,
        "the digest is part of the committed history"
    );

    // A reveal for an unresolved tick is rejected; after it resolves it is recorded.
    let reveal = |tick| Order::RevealRationale {
        tick,
        policy: b"human".to_vec(),
        salt: [7; 16],
        text: b"why".to_vec(),
    };
    let early = OrderBatch {
        civ: 0,
        tick: 1,
        role: Role::General,
        member: general,
        adopt: vec![],
        decision_digest: [3; 32],
        orders: vec![reveal(1)],
    };
    assert_eq!(
        validate_batch(&a, &rules, &early),
        Err(RulesError::RevealTooEarly { tick: 1 })
    );
    let ok = OrderBatch {
        civ: 0,
        tick: 1,
        role: Role::General,
        member: general,
        adopt: vec![],
        decision_digest: [3; 32],
        orders: vec![reveal(0)],
    };
    assert_eq!(validate_batch(&a, &rules, &ok), Ok(0), "reveals are free");
    let (mut with, mut without) = (a.clone(), a.clone());
    resolve_tick(
        &mut with,
        &rules,
        &TickInput {
            vrf: vrf(1),
            batches: vec![ok],
            ..Default::default()
        },
    )
    .unwrap();
    resolve_tick(
        &mut without,
        &rules,
        &TickInput {
            vrf: vrf(1),
            batches: vec![OrderBatch {
                civ: 0,
                tick: 1,
                role: Role::General,
                member: general,
                adopt: vec![],
                decision_digest: [3; 32],
                orders: vec![],
            }],
            ..Default::default()
        },
    )
    .unwrap();
    assert_ne!(with.event_head, without.event_head);
}
