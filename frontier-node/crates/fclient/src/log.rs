//! The PS2 log contract (M1 contract §6): parser, encoder, head function
//! and a chain checker.
//!
//! One `sol_log_data(&[b"PS2", body])` per record, which the runtime prints
//! as `Program data: <base64 "PS2"> <base64 body>`. `body = head(6) ‖ key ‖
//! payload ‖ tail`, `head = ver u8 ‖ kind u8 ‖ bell u32`, `tail = n u8 ‖ n ×
//! {entity_kind u8, seq u64, head [32]}`. For each chained entity
//! `head' = sha256(head ‖ le64(seq') ‖ body_without_tail)`.
//!
//! The key and payload widths are kind-specific and fixed; they belong to
//! `frontier-abi::log` (W1-E). This parser does not need them: it finds the
//! tail from the end (its first byte is `n`, and every link's entity kind is
//! 1–7) and keeps `key ‖ payload` as bytes. When two tail lengths are both
//! consistent it asks the caller's length table ([`BodyLens`]), and without
//! one it refuses (`Ambiguous`) rather than guess.

use base64::Engine;
use sha2::{Digest, Sha256};

use crate::abi::entity;

pub const PS2: &[u8; 3] = b"PS2";
pub const VERSION: u8 = 1;
/// Longest tail a record can carry (the SETTLE record touches the most
/// entities: Citizen, Holding, ≤ 3 Provinces, JoinShard, displaced Citizen
/// and JoinShard).
pub const MAX_LINKS: usize = 12;

/// `{entity_kind, seq, head}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link {
    pub entity: u8,
    pub seq: u64,
    pub head: [u8; 32],
}

/// A decoded PS2 record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub ver: u8,
    pub kind: u8,
    pub bell: u32,
    /// `key ‖ payload` (kind-specific, fixed widths).
    pub key_payload: Vec<u8>,
    pub links: Vec<Link>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogError {
    Short,
    Version(u8),
    /// No tail length is consistent with the body.
    NoTail,
    /// More than one tail length is consistent and no length table decides.
    Ambiguous(Vec<usize>),
    Base64,
}

/// Optional per-kind `key ‖ payload` lengths (from `frontier-abi::log`).
pub trait BodyLens {
    fn key_payload_len(&self, kind: u8) -> Option<usize>;
}

/// No table: decode only when the tail is unambiguous.
pub struct NoLens;
impl BodyLens for NoLens {
    fn key_payload_len(&self, _: u8) -> Option<usize> {
        None
    }
}

impl<F: Fn(u8) -> Option<usize>> BodyLens for F {
    fn key_payload_len(&self, kind: u8) -> Option<usize> {
        self(kind)
    }
}

impl Record {
    /// `ver ‖ kind ‖ bell ‖ key ‖ payload`.
    pub fn body_without_tail(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(6 + self.key_payload.len());
        v.push(self.ver);
        v.push(self.kind);
        v.extend_from_slice(&self.bell.to_le_bytes());
        v.extend_from_slice(&self.key_payload);
        v
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut v = self.body_without_tail();
        v.push(self.links.len() as u8);
        for l in &self.links {
            v.push(l.entity);
            v.extend_from_slice(&l.seq.to_le_bytes());
            v.extend_from_slice(&l.head);
        }
        v
    }

    pub fn decode(body: &[u8]) -> Result<Record, LogError> {
        Self::decode_with(body, &NoLens)
    }

    pub fn decode_with(body: &[u8], lens: &dyn BodyLens) -> Result<Record, LogError> {
        if body.len() < 7 {
            return Err(LogError::Short);
        }
        if body[0] != VERSION {
            return Err(LogError::Version(body[0]));
        }
        let kind = body[1];
        let bell = u32::from_le_bytes(body[2..6].try_into().expect("4"));
        let fits = |n: usize| -> bool {
            let tail = 1 + 41 * n;
            if body.len() < 6 + tail {
                return false;
            }
            let t0 = body.len() - tail;
            body[t0] as usize == n
                && (0..n).all(|i| (1..=entity::MAX).contains(&body[t0 + 1 + 41 * i]))
        };
        let cands: Vec<usize> = (0..=MAX_LINKS).filter(|&n| fits(n)).collect();
        let n = match (cands.as_slice(), lens.key_payload_len(kind)) {
            ([], _) => return Err(LogError::NoTail),
            (_, Some(kp)) => {
                let want = body.len().checked_sub(6 + kp).ok_or(LogError::Short)?;
                *cands
                    .iter()
                    .find(|&&n| 1 + 41 * n == want)
                    .ok_or(LogError::NoTail)?
            }
            ([one], None) => *one,
            (many, None) => return Err(LogError::Ambiguous(many.to_vec())),
        };
        let t0 = body.len() - (1 + 41 * n);
        let links = (0..n)
            .map(|i| {
                let o = t0 + 1 + 41 * i;
                Link {
                    entity: body[o],
                    seq: u64::from_le_bytes(body[o + 1..o + 9].try_into().expect("8")),
                    head: body[o + 9..o + 41].try_into().expect("32"),
                }
            })
            .collect();
        Ok(Record {
            ver: body[0],
            kind,
            bell,
            key_payload: body[6..t0].to_vec(),
            links,
        })
    }
}

/// `sha256(prev_head ‖ le64(seq) ‖ body_without_tail)`.
pub fn head(prev: &[u8; 32], seq: u64, body_without_tail: &[u8]) -> [u8; 32] {
    Sha256::new()
        .chain_update(prev)
        .chain_update(seq.to_le_bytes())
        .chain_update(body_without_tail)
        .finalize()
        .into()
}

/// Builds a record's tail for entities whose previous `(seq, head)` are
/// given (for fixtures, the verifier's tamper tests and the herald's tests).
pub fn chain(
    ver: u8,
    kind: u8,
    bell: u32,
    key_payload: &[u8],
    prev: &[(u8, u64, [u8; 32])],
) -> Record {
    let mut r = Record {
        ver,
        kind,
        bell,
        key_payload: key_payload.to_vec(),
        links: vec![],
    };
    let bwt = r.body_without_tail();
    r.links = prev
        .iter()
        .map(|&(e, seq, h)| {
            let s = seq + 1;
            Link {
                entity: e,
                seq: s,
                head: head(&h, s, &bwt),
            }
        })
        .collect();
    r
}

/// PS2 bodies from a transaction's log lines, in order.
pub fn bodies_from_logs(logs: &[String]) -> Result<Vec<Vec<u8>>, LogError> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let mut out = vec![];
    for l in logs {
        let Some(rest) = l.strip_prefix("Program data: ") else {
            continue;
        };
        let mut parts = rest.split(' ');
        let (Some(a), Some(b)) = (parts.next(), parts.next()) else {
            continue;
        };
        let tag = b64.decode(a).map_err(|_| LogError::Base64)?;
        if tag != PS2 {
            continue;
        }
        out.push(b64.decode(b).map_err(|_| LogError::Base64)?);
    }
    Ok(out)
}

/// The `Program data:` line the runtime prints for a record.
pub fn log_line(body: &[u8]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD;
    format!("Program data: {} {}", b64.encode(PS2), b64.encode(body))
}

/// Why a chain does not verify.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChainError {
    /// `seq` is not the previous `seq + 1`.
    Gap {
        entity: [u8; 32],
        expected: u64,
        got: u64,
    },
    /// The recomputed head differs from the record's.
    HeadMismatch { entity: [u8; 32], seq: u64 },
    /// The final head differs from the account's.
    FinalMismatch { entity: [u8; 32] },
}

/// Walks the chains of many entities (the verifier's V1). Entities are
/// named by the caller (normally their address), since a link carries only
/// the entity kind; `frontier-abi::log::chains_of` maps a record to them.
#[derive(Default, Debug)]
pub struct Chains {
    state: std::collections::HashMap<[u8; 32], (u64, [u8; 32])>,
}

impl Chains {
    pub fn new() -> Chains {
        Chains::default()
    }

    /// Applies one link of `record` to entity `id`.
    pub fn apply(&mut self, id: [u8; 32], record: &Record, link: &Link) -> Result<(), ChainError> {
        let (seq, prev) = self.state.get(&id).copied().unwrap_or((0, [0u8; 32]));
        if link.seq != seq + 1 {
            return Err(ChainError::Gap {
                entity: id,
                expected: seq + 1,
                got: link.seq,
            });
        }
        if head(&prev, link.seq, &record.body_without_tail()) != link.head {
            return Err(ChainError::HeadMismatch {
                entity: id,
                seq: link.seq,
            });
        }
        self.state.insert(id, (link.seq, link.head));
        Ok(())
    }

    /// Current `(seq, head)` of an entity.
    pub fn current(&self, id: &[u8; 32]) -> (u64, [u8; 32]) {
        self.state.get(id).copied().unwrap_or((0, [0u8; 32]))
    }

    /// Compares with the on-chain header (or a CLOSE record's final values).
    pub fn check_final(&self, id: [u8; 32], seq: u64, head: &[u8; 32]) -> Result<(), ChainError> {
        if self.current(&id) == (seq, *head) {
            Ok(())
        } else {
            Err(ChainError::FinalMismatch { entity: id })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::kind;

    #[test]
    fn round_trip_and_chain() {
        let e1 = [1u8; 32];
        let r1 = chain(
            1,
            kind::HARVEST,
            7,
            &[5u8; 41],
            &[(entity::HOLDING, 0, [0; 32]), (entity::CITIZEN, 3, [9; 32])],
        );
        let body = r1.encode();
        let d = Record::decode(&body).unwrap();
        assert_eq!(d, r1);
        let mut c = Chains::new();
        c.apply(e1, &d, &d.links[0]).unwrap();
        let r2 = chain(
            1,
            kind::BUILD,
            8,
            &[6u8; 50],
            &[(entity::HOLDING, 1, d.links[0].head)],
        );
        c.apply(e1, &r2, &r2.links[0]).unwrap();
        c.check_final(e1, 2, &r2.links[0].head).unwrap();
        // A dropped record is a gap; a flipped byte a head mismatch.
        let r3 = chain(
            1,
            kind::BUILD,
            9,
            &[6u8; 50],
            &[(entity::HOLDING, 3, [0; 32])],
        );
        assert!(matches!(
            c.apply(e1, &r3, &r3.links[0]),
            Err(ChainError::Gap { .. })
        ));
        let mut bad = r2.clone();
        bad.key_payload[0] ^= 1;
        let mut c2 = Chains::new();
        c2.apply(e1, &d, &d.links[0]).unwrap();
        assert!(matches!(
            c2.apply(e1, &bad, &bad.links[0]),
            Err(ChainError::HeadMismatch { .. })
        ));
    }

    #[test]
    fn unchained_records_and_log_lines() {
        let r = Record {
            ver: 1,
            kind: kind::ANCHOR,
            bell: 3,
            key_payload: vec![0xAB; 60],
            links: vec![],
        };
        let line = log_line(&r.encode());
        let bodies = bodies_from_logs(&[line, "Program log: x".into()]).unwrap();
        assert_eq!(Record::decode(&bodies[0]).unwrap(), r);
    }

    #[test]
    fn ambiguity_is_refused_without_a_table() {
        // A payload whose byte 41 from the end-of-body happens to read as a
        // one-link tail (n = 1, entity 5) while the real tail is empty.
        let mut kp = vec![0u8; 50];
        let len = 6 + kp.len() + 1;
        kp[len - 42 - 6] = 1; // would-be n (body index len − 42)
        kp[len - 42 - 6 + 1] = 5; // would-be entity kind
        let r = Record {
            ver: 1,
            kind: kind::DIVERT,
            bell: 0,
            key_payload: kp.clone(),
            links: vec![],
        };
        let body = r.encode();
        assert!(matches!(Record::decode(&body), Err(LogError::Ambiguous(_))));
        let table = |k: u8| (k == kind::DIVERT).then_some(50usize);
        assert_eq!(Record::decode_with(&body, &table).unwrap(), r);
    }
}
