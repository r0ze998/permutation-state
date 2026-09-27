//! V1 — entity chains (contract §6, §8.5; K2, K4).
//!
//! Every record's tail is matched to `frontier_abi::log::chains_of` (kind
//! order, optional links), each link resolved to an account address —
//! from the key and payload where they determine it, through the JOIN and
//! SETTLE facts for a citizen named by its tag or by its holding, and to
//! the transaction's writable account of that kind (account order) where
//! the record leaves it to the transaction (`InTx`) — and each chain is
//! walked: `seq` contiguous from 1, `head = sha256(prev ‖ le64(seq) ‖
//! body_without_tail)`. A CLOSE record's payload must be the chain as it
//! stood, and a creation after CLOSE starts a new chain (v1.5). The final
//! head of every open chain must equal the account's header at the pinned
//! slot; a closed one must be gone; a program account with a head the
//! archive never explained is a gap. **K4 for state:** the final bytes of
//! every program account must equal the post-state of the last transaction
//! that wrote it (a write outside any transaction — `set_account` — is a
//! `HeadMismatch`).
//!
//! Codes: `ChainGap`, `HeadMismatch`, `DuplicateEvent`, `UnknownEntity`,
//! `Undecodable` (K2).

use std::collections::{HashMap, HashSet};

use frontier_abi::layout::AccountKind;
use frontier_abi::log::{self as plog, CitizenRef, EntityKind, EntityRef, Kind};

use super::{ent, Ctx};
use crate::codes::*;
use crate::facts::Facts;
use crate::world::{Key, Rec, World};

const V: &str = "V1";

/// The entity kind of an account by its magic (post-state or final).
pub fn kind_by_magic(d: &[u8]) -> Option<AccountKind> {
    let m = d.get(..8)?;
    AccountKind::ALL.iter().copied().find(|k| k.magic() == m)
}

/// `(seq, head)` of a chained account's header.
pub fn header(d: &[u8]) -> Option<(u64, [u8; 32])> {
    let h = fclient::decode::chained_header(d)?;
    Some((h.event_seq, h.event_head))
}

/// Resolves one expected link of record `n` to an address (`None`:
/// unresolvable). `used` are the addresses this record already took.
#[allow(clippy::too_many_arguments)]
pub fn resolve(
    w: &World,
    f: &Facts,
    kinds: &HashMap<Key, EntityKind>,
    n: usize,
    r: &Rec,
    who: &EntityRef,
    entity: EntityKind,
    used: &[Key],
) -> Option<Key> {
    let citizen = |c: &CitizenRef| -> Option<Key> {
        match c {
            CitizenRef::Tag15(t) => Some(f.ctx.citizen_by_tag15(t)),
            CitizenRef::Tag8(t) => f.by_tag8.get(t).copied(),
            CitizenRef::OfHolding { p, q, site } => f.owner_at((*p, *q, *site), n)?.owner,
        }
    };
    match who {
        EntityRef::JoinShardOf(c) => {
            let a = citizen(c)?;
            let cf = f.citizens.get(&a)?;
            Some(f.ctx.join_shard(cf.faction, cf.shard))
        }
        EntityRef::Citizen(c) => citizen(c),
        EntityRef::InTx => {
            let tx = &w.txs[r.tx];
            tx.keys
                .iter()
                .filter(|(k, wr)| *wr && !used.contains(k))
                .map(|(k, _)| *k)
                .find(|k| kind_of(w, kinds, k, r.tx) == Some(entity))
        }
        other => other.address(&f.ctx),
    }
}

/// The chained entity kind of `k` as of transaction `tx`: its post-state's
/// magic, else a final state's, else what a determinate link named.
fn kind_of(w: &World, kinds: &HashMap<Key, EntityKind>, k: &Key, tx: usize) -> Option<EntityKind> {
    let by = |d: &[u8]| kind_by_magic(d).and_then(EntityKind::of_account);
    w.txs[tx]
        .post_data(k)
        .and_then(by)
        .or_else(|| {
            w.finals
                .get(k)
                .and_then(|a| a.as_ref())
                .and_then(|a| by(&a.data))
        })
        .or_else(|| kinds.get(k).copied())
}

/// Why a record's tail cannot be matched to its entities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TailError {
    /// The kind's key does not name its entities.
    Key,
    /// A required link is missing.
    Missing(EntityKind),
    /// A link names no known account.
    Unresolved(EntityKind, String),
    /// More links than the kind names.
    Extra(usize, usize),
}

/// Matches record `n`'s tail to its entities: `(link index, entity kind,
/// address)` per link (V1; the tamper suite's rechain uses the same).
pub fn match_links(
    w: &World,
    f: &Facts,
    kinds: &mut HashMap<Key, EntityKind>,
    n: usize,
    r: &Rec,
) -> Result<Vec<(usize, EntityKind, Key)>, TailError> {
    let exp = plog::chains_of(r.kind, &r.key, &r.payload).ok_or(TailError::Key)?;
    let mut li = 0usize;
    let mut out: Vec<(usize, EntityKind, Key)> = vec![];
    for e in exp.iter() {
        if r.links.get(li).is_none_or(|l| l.entity != e.entity) {
            if e.optional {
                continue;
            }
            return Err(TailError::Missing(e.entity));
        }
        let used: Vec<Key> = out.iter().map(|x| x.2).collect();
        let k = resolve(w, f, kinds, n, r, &e.who, e.entity, &used)
            .ok_or_else(|| TailError::Unresolved(e.entity, format!("{:?}", e.who)))?;
        kinds.insert(k, e.entity);
        out.push((li, e.entity, k));
        li += 1;
    }
    if li < r.links.len() {
        return Err(TailError::Extra(r.links.len(), li));
    }
    Ok(out)
}

pub fn run(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    for p in &w.problems {
        let d = p.what.clone();
        cx.fail(V, UNDECODABLE, format!("tx#{}", p.tx), 0, Some(p.tx), d);
    }
    let dup_txs: HashSet<usize> = w.duplicate_sigs.iter().map(|x| x.0).collect();
    for (i, s) in &w.duplicate_sigs {
        cx.fail(
            V,
            DUPLICATE_EVENT,
            format!("tx:{s}"),
            0,
            Some(*i),
            "the same transaction appears twice in the archive",
        );
    }
    // Determinate addresses name their kinds (for InTx lookups).
    let mut kinds: HashMap<Key, EntityKind> = HashMap::new();
    let mut st: HashMap<Key, (u64, [u8; 32])> = HashMap::new();
    let mut ent_kind: HashMap<Key, EntityKind> = HashMap::new();
    let mut closed: HashSet<Key> = HashSet::new();
    let mut seen: HashSet<(Key, u64, [u8; 32])> = HashSet::new();
    for (n, r) in w.recs.iter().enumerate() {
        if dup_txs.contains(&r.tx) {
            continue;
        }
        let links = match match_links(w, f, &mut kinds, n, r) {
            Ok(x) => x,
            Err(e) => {
                let (what, detail) = match &e {
                    TailError::Key => (
                        "the record's key does not name its entities".to_string(),
                        String::new(),
                    ),
                    TailError::Missing(k) => (
                        format!("{} {}", r.kind.name(), World::entity_name(*k)),
                        "the tail lacks a link its record kind requires".into(),
                    ),
                    TailError::Unresolved(k, who) => (
                        format!("{} {}", r.kind.name(), World::entity_name(*k)),
                        format!("the {who} link names no known account"),
                    ),
                    TailError::Extra(got, max) => (
                        format!("record {}", r.kind.name()),
                        format!("{got} tail links where the kind names at most {max}"),
                    ),
                };
                cx.fail(V, UNKNOWN_ENTITY, what, r.bell, Some(r.tx), detail);
                continue;
            }
        };
        for (li, entity, k) in links {
            let link = r.links[li];
            ent_kind.insert(k, entity);
            if r.kind == Kind::CLOSE {
                let cur = st.get(&k).copied().unwrap_or((0, [0; 32]));
                if (r.pu64("final_seq"), r.p32("final_head")) != cur {
                    cx.fail(
                        V,
                        HEAD_MISMATCH,
                        ent(World::entity_name(entity), &k),
                        r.bell,
                        Some(r.tx),
                        format!(
                            "CLOSE states seq {} but the chain stood at {}",
                            r.pu64("final_seq"),
                            cur.0
                        ),
                    );
                }
            }
            apply(cx, &mut st, &mut seen, k, entity, r, link);
            if r.kind == Kind::CLOSE {
                st.remove(&k);
                closed.insert(k);
            } else {
                closed.remove(&k);
            }
        }
    }
    // Final heads (K4).
    let mut keys: Vec<Key> = st.keys().copied().collect();
    keys.sort();
    for k in keys {
        let (seq, head) = st[&k];
        let name = World::entity_name(ent_kind[&k]);
        match w.finals.get(&k) {
            None => cx.missing(
                V,
                ent(name, &k),
                0,
                None,
                "no final state of this entity was read",
            ),
            Some(None) => cx.fail(
                V,
                HEAD_MISMATCH,
                ent(name, &k),
                0,
                None,
                format!("the chain ends at seq {seq} but the account is gone without a CLOSE"),
            ),
            Some(Some(a)) => {
                let h = header(&a.data);
                if a.owner.to_bytes() != w.program || h != Some((seq, head)) {
                    cx.fail(
                        V,
                        HEAD_MISMATCH,
                        ent(name, &k),
                        0,
                        None,
                        format!("replayed seq {seq}, on chain {:?}", h.map(|x| x.0)),
                    );
                }
            }
        }
    }
    for k in &closed {
        if let Some(Some(a)) = w.finals.get(k) {
            if a.owner.to_bytes() == w.program && header(&a.data).is_some_and(|h| h.0 > 0) {
                cx.fail(
                    V,
                    HEAD_MISMATCH,
                    ent("closed", k),
                    0,
                    None,
                    "a CLOSE ended this chain but the account still carries a head",
                );
            }
        }
    }
    // Program accounts whose head the archive never explained.
    for (k, a) in w.finals.iter() {
        let Some(a) = a else { continue };
        if a.owner.to_bytes() != w.program {
            continue;
        }
        let chained = kind_by_magic(&a.data).is_some_and(|x| x.chained());
        if chained
            && !st.contains_key(k)
            && !closed.contains(k)
            && header(&a.data).is_some_and(|h| h.0 > 0)
        {
            cx.fail(
                V,
                CHAIN_GAP,
                ent("account", k),
                0,
                None,
                format!(
                    "the account's head is at seq {} but no record of it is in the archive",
                    header(&a.data).map(|h| h.0).unwrap_or(0)
                ),
            );
        }
        // K4 for state: final bytes = the last write's post-state.
        if w.has_post {
            if let Some(Some(last)) = w.last_post(k) {
                if last.owner.to_bytes() == w.program && last.data != a.data {
                    cx.fail(
                        V,
                        HEAD_MISMATCH,
                        ent("account", k),
                        0,
                        None,
                        "the account's bytes differ from the last transaction that wrote it (a write outside any transaction)",
                    );
                }
            }
        }
    }
}

fn apply(
    cx: &mut Ctx,
    st: &mut HashMap<Key, (u64, [u8; 32])>,
    seen: &mut HashSet<(Key, u64, [u8; 32])>,
    k: Key,
    e: EntityKind,
    r: &Rec,
    l: plog::Link,
) {
    let (seq, head) = st.get(&k).copied().unwrap_or((0, [0; 32]));
    let name = World::entity_name(e);
    if l.seq != seq.wrapping_add(1) {
        if seen.contains(&(k, l.seq, l.head)) {
            cx.fail(
                V,
                DUPLICATE_EVENT,
                ent(name, &k),
                r.bell,
                Some(r.tx),
                format!("{} seq {} applied twice", r.kind.name(), l.seq),
            );
            return;
        }
        cx.fail(
            V,
            CHAIN_GAP,
            ent(name, &k),
            r.bell,
            Some(r.tx),
            format!("{} has seq {}, expected {}", r.kind.name(), l.seq, seq + 1),
        );
    } else if plog::next_head(&head, l.seq, &r.bwt) != l.head {
        cx.fail(
            V,
            HEAD_MISMATCH,
            ent(name, &k),
            r.bell,
            Some(r.tx),
            format!("{} seq {}: the head does not chain", r.kind.name(), l.seq),
        );
    }
    seen.insert((k, l.seq, l.head));
    st.insert(k, (l.seq, l.head));
}
