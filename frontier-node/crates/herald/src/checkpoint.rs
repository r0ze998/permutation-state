//! Fold checkpoints (offchain design §8.3 crash safety): the fold's whole
//! [`State`] with the archive sequence it covers, written atomically
//! (temp + fsync + rename) every N slots. A restart loads it and re-folds
//! only the archived records after it; the files those records write again
//! are byte-identical (no-op rewrites). A missing or damaged checkpoint
//! falls back to folding the archive from the start.
//!
//! Format: `"PSFHCK1\0"` · program [32] · season u64 · body · sha256 of
//! everything before it. Body: the fields of [`State`] in declaration
//! order, maps as `n u64` then entries, byte strings as `len u32 ‖ bytes`,
//! all little-endian.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::files::atomic_write;
use crate::fold::{Alarms, BrState, Capture, SealRec, SeedRec, State};

pub const MAGIC: &[u8; 8] = b"PSFHCK1\0";

#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn i16(&mut self, v: i16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn u64(&mut self, v: u64) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn i64(&mut self, v: i64) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn raw(&mut self, v: &[u8]) -> &mut Self {
        self.0.extend_from_slice(v);
        self
    }
    fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.u32(v.len() as u32).raw(v)
    }
    fn cap(&mut self, c: &Capture) -> &mut Self {
        self.u64(c.slot).u64(c.tx).bytes(&c.data)
    }
}

struct R<'a>(&'a [u8], usize);

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.0.get(self.1..self.1.checked_add(n)?)?;
        self.1 += n;
        Some(s)
    }
    fn arr<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.arr()?))
    }
    fn i16(&mut self) -> Option<i16> {
        Some(i16::from_le_bytes(self.arr()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.arr()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.arr()?))
    }
    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.arr()?))
    }
    fn bytes(&mut self) -> Option<Vec<u8>> {
        let n = self.u32()? as usize;
        Some(self.take(n)?.to_vec())
    }
    fn cap(&mut self) -> Option<Capture> {
        Some(Capture {
            slot: self.u64()?,
            tx: self.u64()?,
            data: self.bytes()?,
        })
    }
    fn n(&mut self) -> Option<usize> {
        let n = self.u64()? as usize;
        // Every entry is at least one byte: a length past the end is damage.
        (n <= self.0.len()).then_some(n)
    }
}

/// The checkpoint bytes of `st` for `(program, season)`.
pub fn encode(program: &[u8; 32], season: u64, st: &State) -> Vec<u8> {
    let mut w = W::default();
    w.raw(MAGIC).raw(program).u64(season);
    w.u64(st.folded_through)
        .u64(st.events)
        .u64(st.last_slot)
        .i64(st.last_time);
    w.u64(st.accounts.len() as u64);
    for (k, c) in &st.accounts {
        w.raw(k).cap(c);
    }
    w.u64(st.pending.len() as u64);
    for ((p, q, b), c) in &st.pending {
        w.i16(*p).i16(*q).u32(*b).cap(c);
    }
    w.u64(st.ov_next.len() as u64);
    for (d, b) in &st.ov_next {
        w.u16(*d).u32(*b);
    }
    w.u64(st.clashes.len() as u64);
    for (p, q, b) in &st.clashes {
        w.i16(*p).i16(*q).u32(*b);
    }
    w.u64(st.seeds.len() as u64);
    for ((b, r, n), s) in &st.seeds {
        w.u32(*b).u8(*r).u8(*n).u64(s.round).raw(&s.seed).i64(s.a);
    }
    w.u64(st.anchors.len() as u64);
    for ((b, r), c) in &st.anchors {
        w.u32(*b).u8(*r).cap(c);
    }
    w.u64(st.br.len() as u64);
    for ((b, r), s) in &st.br {
        w.u32(*b).u8(*r).raw(&s.hash).u8(s.fin as u8);
    }
    w.u64(st.seals.len() as u64);
    for (h, s) in &st.seals {
        w.u64(*h).u8(s.outcome).u8(s.code).u32(s.bell);
    }
    w.u64(st.ring_seeds.len() as u64);
    for (d, s) in &st.ring_seeds {
        w.u16(*d).raw(s);
    }
    let a = &st.alarms;
    w.u64(a.rewrites)
        .u64(a.bad_records)
        .u64(a.clash_mismatch)
        .u64(a.clash_unchecked)
        .u64(a.write_errors);
    let h = Sha256::digest(&w.0);
    w.raw(&h);
    w.0
}

/// The state in `b`, if it is an intact checkpoint of `(program, season)`.
pub fn decode(program: &[u8; 32], season: u64, b: &[u8]) -> Option<State> {
    if b.len() < 8 + 32 + 8 + 32 {
        return None;
    }
    let (body, h) = b.split_at(b.len() - 32);
    if Sha256::digest(body).as_slice() != h {
        return None;
    }
    let mut r = R(body, 0);
    if r.take(8)? != MAGIC || &r.arr::<32>()? != program || r.u64()? != season {
        return None;
    }
    let mut st = State {
        folded_through: r.u64()?,
        events: r.u64()?,
        last_slot: r.u64()?,
        last_time: r.i64()?,
        ..State::default()
    };
    for _ in 0..r.n()? {
        let k = r.arr::<32>()?;
        st.accounts.insert(k, r.cap()?);
    }
    for _ in 0..r.n()? {
        let k = (r.i16()?, r.i16()?, r.u32()?);
        st.pending.insert(k, r.cap()?);
    }
    for _ in 0..r.n()? {
        let d = r.u16()?;
        st.ov_next.insert(d, r.u32()?);
    }
    let mut clashes = BTreeSet::new();
    for _ in 0..r.n()? {
        clashes.insert((r.i16()?, r.i16()?, r.u32()?));
    }
    st.clashes = clashes;
    for _ in 0..r.n()? {
        let k = (r.u32()?, r.u8()?, r.u8()?);
        st.seeds.insert(
            k,
            SeedRec {
                round: r.u64()?,
                seed: r.arr()?,
                a: r.i64()?,
            },
        );
    }
    for _ in 0..r.n()? {
        let k = (r.u32()?, r.u8()?);
        st.anchors.insert(k, r.cap()?);
    }
    for _ in 0..r.n()? {
        let k = (r.u32()?, r.u8()?);
        st.br.insert(
            k,
            BrState {
                hash: r.arr()?,
                fin: r.u8()? != 0,
            },
        );
    }
    let mut seals = BTreeMap::new();
    for _ in 0..r.n()? {
        let h = r.u64()?;
        seals.insert(
            h,
            SealRec {
                outcome: r.u8()?,
                code: r.u8()?,
                bell: r.u32()?,
            },
        );
    }
    st.seals = seals;
    for _ in 0..r.n()? {
        let d = r.u16()?;
        st.ring_seeds.insert(d, r.arr()?);
    }
    st.alarms = Alarms {
        rewrites: r.u64()?,
        bad_records: r.u64()?,
        clash_mismatch: r.u64()?,
        clash_unchecked: r.u64()?,
        write_errors: r.u64()?,
    };
    (r.1 == body.len()).then_some(st)
}

/// Saves a checkpoint at `path` atomically.
pub fn save(path: &Path, program: &[u8; 32], season: u64, st: &State) -> std::io::Result<()> {
    atomic_write(path, &encode(program, season, st))
}

/// Loads a checkpoint (`None` if missing, damaged or of another season).
pub fn load(path: &Path, program: &[u8; 32], season: u64) -> Option<State> {
    decode(program, season, &std::fs::read(path).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_refuses_damage() {
        let mut st = State {
            folded_through: 9,
            events: 12,
            last_slot: 77,
            last_time: -3,
            ..State::default()
        };
        st.accounts.insert(
            [1; 32],
            Capture {
                slot: 5,
                tx: 6,
                data: vec![1, 2, 3],
            },
        );
        st.pending.insert(
            (-2, 3, 40),
            Capture {
                slot: 1,
                tx: 2,
                data: vec![9],
            },
        );
        st.ov_next.insert(2, 41);
        st.clashes.insert((-2, 3, 40));
        st.seeds.insert(
            (40, 13, 0),
            SeedRec {
                round: 99,
                seed: [5; 32],
                a: 1_800_000_000,
            },
        );
        st.br.insert(
            (40, 13),
            BrState {
                hash: [4; 32],
                fin: true,
            },
        );
        st.seals.insert(
            7,
            SealRec {
                outcome: 8,
                code: 5,
                bell: 41,
            },
        );
        st.ring_seeds.insert(2, [3; 32]);
        st.alarms.rewrites = 1;
        let b = encode(&[8; 32], 3, &st);
        assert_eq!(decode(&[8; 32], 3, &b), Some(st));
        assert_eq!(decode(&[8; 32], 4, &b), None, "another season");
        assert_eq!(decode(&[7; 32], 3, &b), None, "another program");
        let mut bad = b.clone();
        bad[60] ^= 1;
        assert_eq!(decode(&[8; 32], 3, &bad), None, "damaged");
        assert_eq!(decode(&[8; 32], 3, &b[..b.len() - 1]), None, "truncated");
    }
}
