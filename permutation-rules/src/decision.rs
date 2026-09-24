//! Verifiable decision logs (§4.3, §7.5; V4 §5.4).
//!
//! Each order batch commits, before its tick resolves, to
//!
//! ```text
//! decision_digest = sha256("PS/decision/v1" ‖ tick ‖ obs_root ‖ policy_id ‖ rationale_hash)
//! ```
//!
//! * `obs_root` — Merkle root of the observation: the belief state that
//!   civilization was allowed to see (`vision::belief`), split into leaves
//!   (one per tile, civ, visible city, visible unit, …) so a single fact can
//!   later be proven with a short path.
//! * `policy_id` — hash of a self-declared policy name ("human", "bot/warlord@1", a model id).
//! * `rationale_hash` — hash of a salted reasoning text, revealed after the
//!   tick with `RevealRationale`.
//!
//! What this proves: the decision was fixed before the result, it was taken
//! with only the information that civ could see, and the reason was not
//! rewritten afterwards. It does not prove which model wrote the reason.

use alloc::vec::Vec;
use borsh::BorshSerialize;

use crate::state::{CivId, WorldState};
use crate::vision::Memory;

pub type Hash = [u8; 32];

/// Longest accepted policy name and rationale text, in bytes.
pub const MAX_POLICY: usize = 64;
pub const MAX_RATIONALE: usize = 512;

const LEAF: u8 = 0x00;
const NODE: u8 = 0x01;

fn sha(parts: &[&[u8]]) -> Hash {
    crate::hash::sha256(parts)
}

pub fn policy_id(policy: &[u8]) -> Hash {
    sha(&[b"PS/policy/v1", policy])
}

pub fn rationale_hash(salt: &[u8; 16], text: &[u8]) -> Hash {
    sha(&[b"PS/rationale/v1", salt, text])
}

pub fn decision_digest(tick: u16, obs_root: &Hash, policy_id: &Hash, rationale_hash: &Hash) -> Hash {
    sha(&[b"PS/decision/v1", &tick.to_le_bytes(), obs_root, policy_id, rationale_hash])
}

/// Check a revealed rationale against a committed digest.
pub fn verify_reveal(digest: &Hash, tick: u16, obs_root: &Hash, policy: &[u8], salt: &[u8; 16], text: &[u8]) -> bool {
    decision_digest(tick, obs_root, &policy_id(policy), &rationale_hash(salt, text)) == *digest
}

// ------------------------------------------------------------------ Merkle tree

/// `sha256(0x00 ‖ len(kind) ‖ kind ‖ body)`. The prefix keeps leaves and
/// inner nodes from ever colliding (second-preimage safety).
pub fn leaf_hash(kind: &[u8], body: &[u8]) -> Hash {
    sha(&[&[LEAF], &[kind.len() as u8], kind, body])
}

pub fn node_hash(left: &Hash, right: &Hash) -> Hash {
    sha(&[&[NODE], left, right])
}

/// Binary Merkle root; an odd node at the end of a level is carried up
/// unchanged. The empty tree's root is `sha256("")`.
pub fn merkle_root(leaves: &[Hash]) -> Hash {
    if leaves.is_empty() {
        return sha(&[]);
    }
    let mut level: Vec<Hash> = leaves.to_vec();
    while level.len() > 1 {
        level = level
            .chunks(2)
            .map(|p| if p.len() == 2 { node_hash(&p[0], &p[1]) } else { p[0] })
            .collect();
    }
    level[0]
}

/// One step of an inclusion proof: the sibling, and whether it sits on the left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub sibling: Hash,
    pub left: bool,
}

/// Inclusion proof for `leaves[index]`.
pub fn merkle_proof(leaves: &[Hash], index: usize) -> Vec<Step> {
    let mut proof = Vec::new();
    let mut level: Vec<Hash> = leaves.to_vec();
    let mut i = index;
    while level.len() > 1 {
        let sib = i ^ 1;
        if sib < level.len() {
            proof.push(Step { sibling: level[sib], left: sib < i });
        }
        level = level
            .chunks(2)
            .map(|p| if p.len() == 2 { node_hash(&p[0], &p[1]) } else { p[0] })
            .collect();
        i /= 2;
    }
    proof
}

pub fn verify_proof(root: &Hash, leaf: &Hash, proof: &[Step]) -> bool {
    let got = proof.iter().fold(*leaf, |acc, s| if s.left { node_hash(&s.sibling, &acc) } else { node_hash(&acc, &s.sibling) });
    got == *root
}

// ------------------------------------------------------------------ observation leaves

/// One fact of an observation, before hashing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObsLeaf {
    /// `header`, `tile`, `civ`, `city`, `unit`, `city_state`, `diplomacy`, `market`.
    pub kind: &'static str,
    /// Tile index, civ/city/unit/city-state id; 0 for singletons.
    pub id: u32,
    /// Borsh encoding of the fact (§7.5 lists the fields).
    pub body: Vec<u8>,
}

impl ObsLeaf {
    pub fn hash(&self) -> Hash {
        leaf_hash(self.kind.as_bytes(), &self.body)
    }
}

fn enc<T: BorshSerialize>(v: &T) -> Vec<u8> {
    borsh::to_vec(v).expect("borsh encoding of in-memory values cannot fail")
}

/// The observation of `civ` as ordered leaves. `belief` must be
/// `vision::belief(state, civ, seen, memory)` for the same `seen` and `memory`.
pub fn obs_leaves(belief: &WorldState, civ: CivId, seen: &[bool], memory: &Memory) -> Vec<ObsLeaf> {
    let mut out = Vec::new();
    let mut push = |kind: &'static str, id: u32, body: Vec<u8>| out.push(ObsLeaf { kind, id, body });
    push("header", 0, enc(&(b"PS/obs/v1".as_slice(), belief.tick, civ, belief.ruleset_hash)));
    for (i, t) in belief.map.tiles.iter().enumerate() {
        let fog: u8 = if seen[i] { 2 } else if memory.explored[i] { 1 } else { 0 };
        push("tile", i as u32, enc(&(i as u32, t.hex.q, t.hex.r, fog, t.owner_city, t.ruin_peak_pop)));
    }
    for c in &belief.civs {
        push("civ", c.id as u32, enc(c));
    }
    for c in belief.cities.iter().filter(|c| c.alive) {
        let live = c.owner == Some(civ) || belief.map.index_of(c.hex).is_some_and(|i| seen[i]);
        let seen_tick = if live { None } else { memory.city_seen(c.id) };
        push("city", c.id, enc(&(c, seen_tick)));
    }
    for u in belief.units.iter().filter(|u| u.alive) {
        push("unit", u.id, enc(u));
    }
    for cs in &belief.city_states {
        let Some(i) = belief.map.index_of(cs.hex) else { continue };
        if !memory.explored[i] {
            continue;
        }
        let mine = cs.influence.get(civ as usize).copied().unwrap_or(0);
        let top = cs.influence.iter().copied().max().unwrap_or(0);
        push(
            "city_state",
            cs.id as u32,
            enc(&(cs.id, cs.hex.q, cs.hex.r, cs.pop, cs.defense, cs.specialty, cs.suzerain, cs.captured_by, mine, top)),
        );
    }
    let n = belief.civs.len() as CivId;
    let mine: Vec<(CivId, u16, u16)> = (0..n)
        .filter(|o| *o != civ)
        .map(|o| (o, belief.grievance(o, civ), belief.grievance(civ, o)))
        .collect();
    let proposals: Vec<_> = belief.proposals.iter().filter(|p| p.from == civ || p.to == civ).collect();
    push("diplomacy", 0, enc(&(&belief.relations, &belief.truce_until, proposals, mine)));
    push("market", 0, enc(&belief.pools));
    out
}

pub fn obs_root(leaves: &[ObsLeaf]) -> Hash {
    let hashes: Vec<Hash> = leaves.iter().map(ObsLeaf::hash).collect();
    merkle_root(&hashes)
}
