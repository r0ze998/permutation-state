//! PS2 log records and the per-entity event chains (M1 contract §4.3, §6,
//! I-38): the verifier's and the herald's input.
//!
//! One `sol_log_data(&[b"PS2", body])` per record; `body = ver ‖ kind ‖
//! bell ‖ key ‖ payload ‖ tail`, the widths of every kind pinned in
//! `frontier_abi::log::SPECS`. For each chained entity E the record touches
//! (Season, Frontier, JoinShard, Citizen, Holding, Province, ClashInputs):
//! `E.seq += 1; E.head = sha256(E.head ‖ le64(E.seq) ‖ body_without_tail)`,
//! written back into E's header, and the tail carries the new `(seq,
//! head)`. Tail order: ascending entity kind, then the order the caller
//! lists the accounts in (the instruction's account order), as
//! `frontier_abi::log` pins it.
//!
//! [`record`] is pure (host-tested); `emit` (feature `program`) logs it.
//!
//! **No pointer tables on chain.** `frontier_abi::log::Kind::spec()` (and
//! with it `log::write_body`, `log::decode`, `log::field`) compiles to a
//! switch table of pointers *into* `SPECS`; on the SBPF v2 build of
//! platform-tools v1.52 those data relocations carry a non-zero addend in
//! the low word, which the loader (solana-sbpf 0.21) drops, so every entry
//! resolves to `SPECS[0]` [measured in LiteSVM 0.16, W2-A notes]. The
//! program therefore takes the widths from [`WIDTHS`], an integer table the
//! compiler evaluates from `SPECS` (no relocation), and writes the body
//! itself; `build-frontier.sh` refuses any artefact that still carries such
//! a relocation.

use frontier_abi::log::{self, EntityKind, Kind, Link, MAX_LINKS};

use crate::error::{Error, BAD_ACCOUNT, OVERFLOW};
use crate::layout::{chain_of, set_chain};

/// Highest record kind code (`CLOSE` = 70).
pub const MAX_KIND: usize = 70;

/// `(defined, key width, payload width)` per record kind code, evaluated at
/// compile time from `frontier_abi::log::SPECS`.
pub const WIDTHS: [(bool, u16, u16); MAX_KIND + 1] = {
    let mut t = [(false, 0u16, 0u16); MAX_KIND + 1];
    let mut i = 0;
    while i < log::SPECS.len() {
        let s = &log::SPECS[i];
        let mut k = 0usize;
        let mut j = 0;
        while j < s.key.len() {
            k += s.key[j].1;
            j += 1;
        }
        let mut p = 0usize;
        let mut j = 0;
        while j < s.payload.len() {
            p += s.payload[j].1;
            j += 1;
        }
        t[s.kind as usize] = (true, k as u16, p as u16);
        i += 1;
    }
    t
};

/// Writes `body_without_tail` (`ver ‖ kind ‖ bell ‖ key ‖ payload`) into
/// `out`, the widths checked against [`WIDTHS`]; the bytes are
/// `log::write_body`'s (host test `bodies_are_the_abis`).
pub fn write_body(
    kind: Kind,
    bell: u32,
    key: &[u8],
    payload: &[u8],
    out: &mut [u8],
) -> Option<usize> {
    let (defined, kw, pw) = *WIDTHS.get(kind as usize)?;
    if !defined || key.len() != kw as usize || payload.len() != pw as usize {
        return None;
    }
    let n = log::HEAD_LEN + key.len() + payload.len();
    let o = out.get_mut(..n)?;
    o[0] = log::VERSION;
    o[1] = kind as u8;
    o[2..6].copy_from_slice(&bell.to_le_bytes());
    o[6..6 + key.len()].copy_from_slice(key);
    o[6 + key.len()..].copy_from_slice(payload);
    Some(n)
}

/// Largest record: DEPART's body (278 B) plus a full tail (v1.7: 12
/// links, a GATHER with its stamped Holdings).
pub const MAX_RECORD: usize = 800;
const _: () = assert!(MAX_RECORD >= 6 + 8 + 272 + 1 + 41 * MAX_LINKS);

/// A chained account the record advances: its entity kind and its data
/// (header H at offset 0).
pub struct Chained<'a> {
    pub entity: EntityKind,
    pub data: &'a mut [u8],
}

/// Writes the record into `out`, advancing every chained account's header.
/// Returns the record length. `bell` is `log::NO_BELL` before genesis.
pub fn record(
    kind: Kind,
    bell: u32,
    key: &[u8],
    payload: &[u8],
    chained: &mut [Chained<'_>],
    out: &mut [u8; MAX_RECORD],
) -> Result<usize, Error> {
    if chained.len() > MAX_LINKS {
        return Err(BAD_ACCOUNT);
    }
    let n = write_body(kind, bell, key, payload, out).ok_or(BAD_ACCOUNT)?;
    // Stable order by entity kind (insertion sort over indices, ≤ 12).
    let mut order = [0usize; MAX_LINKS];
    for (i, o) in order.iter_mut().enumerate().take(chained.len()) {
        *o = i;
    }
    let m = chained.len();
    for i in 1..m {
        let mut j = i;
        while j > 0 && chained[order[j - 1]].entity > chained[order[j]].entity {
            order.swap(j - 1, j);
            j -= 1;
        }
    }
    let mut links = [Link {
        entity: EntityKind::Season,
        seq: 0,
        head: [0; 32],
    }; MAX_LINKS];
    for (slot, &i) in order.iter().take(m).enumerate() {
        let c = &mut chained[i];
        let (seq, head) = chain_of(c.data)?;
        let link = log::advance(c.entity, seq, &head, &out[..n]).ok_or(OVERFLOW)?;
        set_chain(c.data, link.seq, &link.head)?;
        links[slot] = link;
    }
    log::write_tail(&links[..m], out, n).ok_or(BAD_ACCOUNT)
}

/// Builds the record and logs it with `sol_log_data(["PS2", body])`.
#[cfg(feature = "program")]
pub fn emit(
    kind: Kind,
    bell: u32,
    key: &[u8],
    payload: &[u8],
    chained: &mut [Chained<'_>],
) -> Result<(), Error> {
    let mut out = [0u8; MAX_RECORD];
    let n = record(kind, bell, key, payload, chained, &mut out)?;
    solana_program::log::sol_log_data(&[log::PREFIX, &out[..n]]);
    Ok(())
}

/// A small fixed buffer for building a key or a payload in field order.
pub struct Buf<const N: usize> {
    b: [u8; N],
    n: usize,
    ok: bool,
}

impl<const N: usize> Default for Buf<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Buf<N> {
    pub const fn new() -> Self {
        Buf {
            b: [0; N],
            n: 0,
            ok: true,
        }
    }
    pub fn bytes(mut self, v: &[u8]) -> Self {
        match self.b.get_mut(self.n..self.n + v.len()) {
            Some(d) => {
                d.copy_from_slice(v);
                self.n += v.len();
            }
            None => self.ok = false,
        }
        self
    }
    pub fn u8(self, v: u8) -> Self {
        self.bytes(&[v])
    }
    pub fn u16(self, v: u16) -> Self {
        self.bytes(&v.to_le_bytes())
    }
    pub fn u32(self, v: u32) -> Self {
        self.bytes(&v.to_le_bytes())
    }
    pub fn u64(self, v: u64) -> Self {
        self.bytes(&v.to_le_bytes())
    }
    pub fn i32(self, v: i32) -> Self {
        self.bytes(&v.to_le_bytes())
    }
    pub fn i64(self, v: i64) -> Self {
        self.bytes(&v.to_le_bytes())
    }
    /// The bytes written; `BadAccount` if anything overflowed the buffer.
    pub fn get(&self) -> Result<&[u8], Error> {
        if self.ok {
            Ok(&self.b[..self.n])
        } else {
            Err(BAD_ACCOUNT)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{init_header, AccountKind};

    #[test]
    fn records_advance_chains_in_tail_order() {
        let mut season = alloc::vec![0u8; AccountKind::Season.size()];
        let mut citizen = alloc::vec![0u8; AccountKind::Citizen.size()];
        let mut shard = alloc::vec![0u8; AccountKind::JoinShard.size()];
        init_header(&mut season, AccountKind::Season, 9).unwrap();
        init_header(&mut citizen, AccountKind::Citizen, 9).unwrap();
        init_header(&mut shard, AccountKind::JoinShard, 9).unwrap();
        let key = Buf::<15>::new().bytes(&[1; 15]);
        let payload = Buf::<74>::new()
            .bytes(&[2; 32])
            .u8(3)
            .u8(4)
            .bytes(&[5; 32])
            .i64(77);
        let mut out = [0u8; MAX_RECORD];
        // Listed Citizen first, JoinShard second: the tail sorts by kind
        // (JoinShard 3 before Citizen 4).
        let n = record(
            Kind::JOIN,
            12,
            key.get().unwrap(),
            payload.get().unwrap(),
            &mut [
                Chained {
                    entity: EntityKind::Citizen,
                    data: &mut citizen,
                },
                Chained {
                    entity: EntityKind::JoinShard,
                    data: &mut shard,
                },
            ],
            &mut out,
        )
        .unwrap();
        let r = log::decode(&out[..n]).unwrap();
        assert_eq!(r.kind, Kind::JOIN);
        assert_eq!(r.bell, 12);
        assert_eq!(r.n_links, 2);
        let l0 = r.links[0].unwrap();
        let l1 = r.links[1].unwrap();
        assert_eq!((l0.entity, l0.seq), (EntityKind::JoinShard, 1));
        assert_eq!((l1.entity, l1.seq), (EntityKind::Citizen, 1));
        let body = r.body_without_tail;
        assert_eq!(l0.head, log::next_head(&[0; 32], 1, body));
        assert_eq!(chain_of(&shard).unwrap(), (1, l0.head));
        assert_eq!(chain_of(&citizen).unwrap(), (1, l1.head));
        // a second record chains from the first head
        let mut out2 = [0u8; MAX_RECORD];
        let pay = Buf::<40>::new().bytes(&[0; 32]).i64(1);
        let n2 = record(
            Kind::SESSION,
            13,
            key.get().unwrap(),
            pay.get().unwrap(),
            &mut [Chained {
                entity: EntityKind::Citizen,
                data: &mut citizen,
            }],
            &mut out2,
        )
        .unwrap();
        let r2 = log::decode(&out2[..n2]).unwrap();
        let l = r2.links[0].unwrap();
        assert_eq!(l.seq, 2);
        assert_eq!(l.head, log::next_head(&l1.head, 2, r2.body_without_tail));
        // untouched season
        assert_eq!(chain_of(&season).unwrap(), (0, [0; 32]));
    }

    /// The program's body writer and widths are the ABI's, for every kind.
    #[test]
    fn bodies_are_the_abis() {
        for spec in log::SPECS {
            let key = alloc::vec![0xA5u8; spec.key_len()];
            let payload = alloc::vec![0x5Au8; spec.payload_len()];
            let mut a = [0u8; MAX_RECORD];
            let mut b = [0u8; MAX_RECORD];
            let na = write_body(spec.kind, 77, &key, &payload, &mut a).unwrap();
            let nb = log::write_body(spec.kind, 77, &key, &payload, &mut b).unwrap();
            assert_eq!(&a[..na], &b[..nb], "{}", spec.name);
            assert_eq!(na, spec.body_len());
            assert!(
                write_body(spec.kind, 77, &key[1..], &payload, &mut a).is_none()
                    || spec.key_len() == 0
            );
        }
        let defined = WIDTHS.iter().filter(|w| w.0).count();
        assert_eq!(defined, log::SPECS.len());
        assert!(log::SPECS.iter().all(|s| (s.kind as usize) <= MAX_KIND));
    }

    #[test]
    fn wrong_widths_are_refused() {
        let mut out = [0u8; MAX_RECORD];
        assert!(record(Kind::BEACON, 1, &[1, 2], &[0; 8], &mut [], &mut out).is_err());
        assert!(record(Kind::BEACON, 1, &[1], &[0; 7], &mut [], &mut out).is_err());
        let n = record(Kind::BEACON, 1, &[1], &[0; 8], &mut [], &mut out).unwrap();
        assert_eq!(n, 6 + 1 + 8 + 1);
        assert!(Buf::<2>::new().u32(1).get().is_err());
    }
}
