//! The program's PS2 records (§6) and the event chains of chained accounts
//! (§4.3).
//!
//! One `sol_log_data(["PS2", body])` per record: a `Program data:` line with
//! two base64 fields. Bodies decode with `frontier_abi::log::decode`, which
//! knows every kind's key and payload widths, so a payload byte can never
//! pass for a tail.
//!
//! [`ChainWatch`] snapshots a chained account's header `(event_seq,
//! event_head)` before a transaction and checks after it that the account's
//! header advanced exactly as the transaction's records say: every link for
//! the entity continues the chain (`seq + 1`, `head = sha256(prev ‖ le64(seq)
//! ‖ body_without_tail)`), and the account ends on the last link.

use base64::Engine;
use frontier_abi::layout::header;
use frontier_abi::log::{self, EntityKind, Kind, Record};
use solana_address::Address;

use crate::chain::Chain;

/// Every `Program data:` line's fields, decoded.
pub fn data_lines(logs: &[String]) -> Vec<Vec<Vec<u8>>> {
    let b64 = base64::engine::general_purpose::STANDARD;
    logs.iter()
        .filter_map(|l| l.strip_prefix("Program data: "))
        .map(|l| {
            l.split(' ')
                .map(|f| b64.decode(f).unwrap_or_default())
                .collect()
        })
        .collect()
}

/// The PS2 bodies in `logs`, in order.
pub fn bodies(logs: &[String]) -> Vec<Vec<u8>> {
    data_lines(logs)
        .into_iter()
        .filter(|f| f.len() == 2 && f[0] == log::PREFIX)
        .map(|mut f| f.pop().unwrap_or_default())
        .collect()
}

/// An owned decoded record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rec {
    pub kind: Kind,
    pub bell: u32,
    pub key: Vec<u8>,
    pub payload: Vec<u8>,
    pub body_without_tail: Vec<u8>,
    pub links: Vec<log::Link>,
}

impl Rec {
    fn of(r: &Record<'_>) -> Rec {
        Rec {
            kind: r.kind,
            bell: r.bell,
            key: r.key.to_vec(),
            payload: r.payload.to_vec(),
            body_without_tail: r.body_without_tail.to_vec(),
            links: r.links.iter().take(r.n_links).flatten().copied().collect(),
        }
    }

    /// A named key or payload field (`in_payload` selects the part).
    pub fn field(&self, name: &str, in_payload: bool) -> &[u8] {
        let (off, w) = log::field(self.kind, name, in_payload)
            .unwrap_or_else(|| panic!("{} has no field {name}", self.kind.name()));
        let src = if in_payload { &self.payload } else { &self.key };
        &src[off..off + w]
    }
    pub fn u64(&self, name: &str) -> u64 {
        le(self.field(name, true))
    }
    pub fn key_u64(&self, name: &str) -> u64 {
        le(self.field(name, false))
    }
    pub fn link(&self, e: EntityKind) -> Option<log::Link> {
        self.links.iter().copied().find(|l| l.entity == e)
    }
}

/// Little-endian integer of up to 8 bytes.
pub fn le(b: &[u8]) -> u64 {
    b.iter().rev().fold(0u64, |acc, x| (acc << 8) | *x as u64)
}

/// Every PS2 record in `logs`; panics on a body that does not decode (the
/// program must never log one) and on a tail outside
/// `frontier_abi::log::chains_of(..).bounds()` (v1.5, W3-A F4: what a
/// verifier or indexer applying the bounds would reject).
pub fn records(logs: &[String]) -> Vec<Rec> {
    bodies(logs)
        .iter()
        .map(|b| {
            let r = log::decode(b)
                .unwrap_or_else(|e| panic!("PS2 body does not decode ({e:?}): {}", hex::encode(b)));
            let rec = Rec::of(&r);
            check_tail(&rec);
            rec
        })
        .collect()
}

/// The record's tail fits `chains_of(..)`: its link count within the
/// bounds and every link's entity kind one the record may chain.
#[track_caller]
pub fn check_tail(r: &Rec) {
    let Some(c) = log::chains_of(r.kind, &r.key, &r.payload) else {
        assert!(
            r.links.is_empty(),
            "{}: links without chains",
            r.kind.name()
        );
        return;
    };
    let (lo, hi) = c.bounds();
    assert!(
        (lo..=hi).contains(&r.links.len()),
        "{}: {} links outside chains_of bounds {lo}..={hi}",
        r.kind.name(),
        r.links.len()
    );
    for l in &r.links {
        assert!(
            c.iter().any(|x| x.entity == l.entity),
            "{}: link of {:?} not in chains_of",
            r.kind.name(),
            l.entity
        );
    }
}

/// The records of one kind.
pub fn of_kind(logs: &[String], k: Kind) -> Vec<Rec> {
    records(logs).into_iter().filter(|r| r.kind == k).collect()
}

/// The one record of `k` (panics unless there is exactly one).
#[track_caller]
pub fn one(logs: &[String], k: Kind) -> Rec {
    let mut v = of_kind(logs, k);
    assert_eq!(v.len(), 1, "{} records of {}", v.len(), k.name());
    v.pop().expect("one")
}

/// A chained account's header `(event_seq, event_head)`.
pub fn head_of(c: &Chain, k: &Address) -> (u64, [u8; 32]) {
    let d = c.data(k);
    if d.len() < header::H_SIZE {
        return (0, [0; 32]);
    }
    let seq = le(&d[header::EVENT_SEQ..header::EVENT_SEQ + 8]);
    let head: [u8; 32] = d[header::EVENT_HEAD..header::EVENT_HEAD + 32]
        .try_into()
        .expect("32");
    (seq, head)
}

/// A chained account watched across one transaction.
pub struct ChainWatch {
    pub address: Address,
    pub entity: EntityKind,
    pub before: (u64, [u8; 32]),
}

impl ChainWatch {
    pub fn new(c: &Chain, address: Address, entity: EntityKind) -> ChainWatch {
        ChainWatch {
            address,
            entity,
            before: head_of(c, &address),
        }
    }

    /// Checks that the account's header moved exactly along this entity's
    /// links in `logs` (at least `min_links` of them). Returns the links.
    #[track_caller]
    pub fn check(&self, c: &Chain, logs: &[String], min_links: usize) -> Vec<log::Link> {
        let recs = records(logs);
        let (mut seq, mut head) = self.before;
        let mut seen = vec![];
        for r in &recs {
            if let Some(l) = r.link(self.entity) {
                assert_eq!(
                    l.seq,
                    seq + 1,
                    "{:?} seq continues in {}",
                    self.entity,
                    r.kind.name()
                );
                let want = log::next_head(&head, l.seq, &r.body_without_tail);
                assert_eq!(l.head, want, "{:?} head of {}", self.entity, r.kind.name());
                seq = l.seq;
                head = l.head;
                seen.push(l);
            }
        }
        assert!(
            seen.len() >= min_links,
            "{:?}: {} links, want ≥ {min_links}",
            self.entity,
            seen.len()
        );
        assert_eq!(
            head_of(c, &self.address),
            (seq, head),
            "{:?} account header ends on its last link",
            self.entity
        );
        seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_body_through_a_log_line() {
        let key = 7u64.to_le_bytes();
        let mut payload = vec![0u8; Kind::GENESIS_SEED.spec().payload_len()];
        payload[..8].copy_from_slice(&99u64.to_le_bytes());
        let mut out = [0u8; 256];
        let n =
            log::write_body(Kind::GENESIS_SEED, log::NO_BELL, &key, &payload, &mut out).unwrap();
        let link = log::advance(EntityKind::Season, 3, &[1; 32], &out[..n]).unwrap();
        let end = log::write_tail(&[link], &mut out, n).unwrap();
        let b64 = base64::engine::general_purpose::STANDARD;
        let line = format!(
            "Program data: {} {}",
            b64.encode(b"PS2"),
            b64.encode(&out[..end])
        );
        let recs = records(&[line]);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].kind, Kind::GENESIS_SEED);
        assert_eq!(recs[0].key_u64("season_id"), 7);
        assert_eq!(recs[0].u64("round"), 99);
        assert_eq!(recs[0].link(EntityKind::Season).unwrap().seq, 4);
    }
}
