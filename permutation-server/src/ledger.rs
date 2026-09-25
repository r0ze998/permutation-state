//! Decision ledger: observations, commitments and reveals (§4.3, §7.5).
//!
//! At every tick opening the server records each civ's observation (the
//! Merkle leaves of what it decides from: the full state, `fog`). Each office's batch then carries
//! `decision_digest(tick, obs_root, policy_id, rationale_hash)`; after the
//! tick resolves the rationale is revealed with a `RevealRationale` order in
//! the same office's batch, so anyone can check it against the commitment in
//! the event chain. Records are kept per (tick, civ, office): every officer,
//! human or AI, seals its own decision (V5 D17).

use std::collections::BTreeMap;

use permutation_rules::decision::{
    decision_digest, merkle_proof, obs_leaves, obs_root, policy_id, rationale_hash, verify_reveal,
    Hash, ObsLeaf, Step, MAX_POLICY, MAX_RATIONALE,
};
use permutation_rules::orders::{Order, OrderBatch};
use permutation_rules::state::{CivId, WorldState};
use sha2::{Digest, Sha256};

use crate::fog::Fog;

/// Encoded orders that fit in one `RevealOrders` transaction next to its
/// signatures, accounts, blockhash and header (the packet limit is 1232 bytes).
pub const BATCH_BYTES: usize = 800;

fn encoded_len(o: &Order) -> usize {
    borsh::to_vec(o).map_or(usize::MAX / 4, |v| v.len())
}

/// Full observations kept for proofs; older ticks keep only their root.
const KEEP_TICKS: u16 = 60;

pub struct Observation {
    pub root: Hash,
    pub leaves: Vec<ObsLeaf>,
}

#[derive(Clone)]
pub struct Record {
    pub tick: u16,
    pub civ: CivId,
    /// Office (`Role::index`).
    pub role: u8,
    pub policy: String,
    pub obs_root: Hash,
    pub digest: Hash,
    pub salt: [u8; 16],
    pub text: String,
    /// Tick whose batch carried the reveal, once it has been processed.
    pub revealed_at: Option<u16>,
    /// Committed by an outside agent: the server only saw the digest on
    /// chain, and learns policy, salt and text from the agent's reveal.
    pub external: bool,
}

impl Record {
    /// Checked with the engine's own function; the browser checks again itself.
    pub fn verified(&self) -> bool {
        verify_reveal(
            &self.digest,
            self.tick,
            &self.obs_root,
            self.policy.as_bytes(),
            &self.salt,
            self.text.as_bytes(),
        )
    }
}

type Key = (u16, CivId, u8);

pub struct Ledger {
    obs: BTreeMap<(u16, CivId), Observation>,
    roots: BTreeMap<(u16, CivId), Hash>,
    records: BTreeMap<Key, Record>,
    secret: [u8; 32],
    counter: u64,
}

/// Cut `s` to at most `max` bytes on a character boundary.
pub fn clip(s: &str, max: usize) -> String {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

impl Ledger {
    /// A ledger whose rationale salts nobody can predict: the secret mixes
    /// `seed` with the clock and the process id (what a server wants).
    pub fn new(seed: &[u8]) -> Ledger {
        let mut h = Sha256::new();
        h.update(b"PS/ledger-secret");
        h.update(seed);
        h.update(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
                .to_le_bytes(),
        );
        h.update(std::process::id().to_le_bytes());
        Self::with_secret(h.finalize().into())
    }

    /// A reproducible ledger (tests, simulations): the secret is derived from
    /// `seed` alone, so the same inputs give the same digests and roots.
    pub fn seeded(seed: &[u8]) -> Ledger {
        let mut h = Sha256::new();
        h.update(b"PS/ledger-secret/seeded");
        h.update(seed);
        Self::with_secret(h.finalize().into())
    }

    fn with_secret(secret: [u8; 32]) -> Ledger {
        Ledger {
            obs: BTreeMap::new(),
            roots: BTreeMap::new(),
            records: BTreeMap::new(),
            secret,
            counter: 0,
        }
    }

    /// Record every civ's observation of the tick that just opened.
    pub fn observe(&mut self, state: &WorldState, fog: &Fog) {
        for civ in 0..state.civs.len() as CivId {
            let belief = fog.belief(state, civ);
            let leaves = obs_leaves(&belief, civ, fog.seen(civ), fog.memory(civ));
            let root = obs_root(&leaves);
            self.roots.insert((state.tick, civ), root);
            self.obs
                .insert((state.tick, civ), Observation { root, leaves });
        }
        let cutoff = state.tick.saturating_sub(KEEP_TICKS);
        self.obs.retain(|(t, _), _| *t >= cutoff);
    }

    pub fn root(&self, tick: u16, civ: CivId) -> Option<Hash> {
        self.roots.get(&(tick, civ)).copied()
    }

    fn salt(&mut self, tick: u16, civ: CivId) -> [u8; 16] {
        self.counter += 1;
        let mut h = Sha256::new();
        h.update(self.secret);
        h.update(tick.to_le_bytes());
        h.update(civ.to_le_bytes());
        h.update(self.counter.to_le_bytes());
        let d: [u8; 32] = h.finalize().into();
        d[..16].try_into().unwrap()
    }

    /// Commit the decision of `civ`'s office `role` for `tick` (replacing an
    /// earlier commit for the same tick). Returns the digest for the batch.
    pub fn commit(&mut self, tick: u16, civ: CivId, role: u8, policy: &str, text: &str) -> Hash {
        let policy = clip(policy, MAX_POLICY);
        let text = clip(text, MAX_RATIONALE);
        let obs_root = self
            .root(tick, civ)
            .expect("observe() runs when a tick opens");
        let salt = self.salt(tick, civ);
        let digest = decision_digest(
            tick,
            &obs_root,
            &policy_id(policy.as_bytes()),
            &rationale_hash(&salt, text.as_bytes()),
        );
        self.records.insert(
            (tick, civ, role),
            Record {
                tick,
                civ,
                role,
                policy,
                obs_root,
                digest,
                salt,
                text,
                revealed_at: None,
                external: false,
            },
        );
        digest
    }

    /// Learn an outside agent's decision from its resolved batch: the digest
    /// it committed to (against the obs_root this server published for it)
    /// and the reveals it carries for earlier ticks.
    pub fn ingest_external(&mut self, batch: &OrderBatch) {
        let role = batch.role as u8;
        if batch.decision_digest != [0; 32] {
            let obs_root = self.root(batch.tick, batch.civ).unwrap_or([0; 32]);
            self.records
                .entry((batch.tick, batch.civ, role))
                .or_insert(Record {
                    tick: batch.tick,
                    civ: batch.civ,
                    role,
                    policy: String::new(),
                    obs_root,
                    digest: batch.decision_digest,
                    salt: [0; 16],
                    text: String::new(),
                    revealed_at: None,
                    external: true,
                });
        }
        for o in &batch.orders {
            if let Order::RevealRationale {
                tick,
                policy,
                salt,
                text,
            } = o
            {
                if let Some(r) = self
                    .records
                    .get_mut(&(*tick, batch.civ, role))
                    .filter(|r| r.external && r.revealed_at.is_none())
                {
                    r.policy = String::from_utf8_lossy(policy).into_owned();
                    r.salt = *salt;
                    r.text = String::from_utf8_lossy(text).into_owned();
                    r.revealed_at = Some(batch.tick);
                }
            }
        }
    }

    pub fn record(&self, tick: u16, civ: CivId, role: u8) -> Option<&Record> {
        self.records.get(&(tick, civ, role))
    }

    /// Reveal orders for the office's resolved, not yet revealed decisions
    /// (oldest first, at most 3) that fit next to `orders` in one Solana
    /// transaction (`BATCH_BYTES` of encoded orders). Whatever does not fit
    /// waits for the next batch; the oldest always goes first, so none is
    /// skipped.
    pub fn reveals(&self, civ: CivId, role: u8, open_tick: u16, orders: &[Order]) -> Vec<Order> {
        let mut used: usize = orders.iter().map(encoded_len).sum();
        let mut out = Vec::new();
        for r in self
            .records
            .values()
            .filter(|r| {
                r.civ == civ && r.role == role && r.tick < open_tick && r.revealed_at.is_none()
            })
            .take(3)
        {
            let o = Order::RevealRationale {
                tick: r.tick,
                policy: r.policy.as_bytes().to_vec(),
                salt: r.salt,
                text: r.text.as_bytes().to_vec(),
            };
            let n = encoded_len(&o);
            if used + n > BATCH_BYTES {
                break;
            }
            used += n;
            out.push(o);
        }
        out
    }

    /// Mark the reveals that were in an accepted batch of tick `at`.
    pub fn mark_revealed(&mut self, civ: CivId, role: u8, orders: &[Order], at: u16) {
        for o in orders {
            if let Order::RevealRationale { tick, .. } = o {
                if let Some(r) = self.records.get_mut(&(*tick, civ, role)) {
                    r.revealed_at = Some(at);
                }
            }
        }
    }

    /// Resolved decisions, newest first.
    pub fn history(&self, limit: usize) -> Vec<&Record> {
        self.records.values().rev().take(limit).collect()
    }

    /// Inclusion proof for one leaf of an observation still kept in full.
    pub fn proof(
        &self,
        tick: u16,
        civ: CivId,
        kind: &str,
        id: u32,
    ) -> Option<(Hash, &ObsLeaf, Vec<Step>)> {
        let o = self.obs.get(&(tick, civ))?;
        let i = o.leaves.iter().position(|l| l.kind == kind && l.id == id)?;
        let hashes: Vec<Hash> = o.leaves.iter().map(ObsLeaf::hash).collect();
        Some((o.root, &o.leaves[i], merkle_proof(&hashes, i)))
    }
}
