//! Typed accessors over the `frontier-abi` layouts (M1 contract §4.3,
//! §5.3). **Frozen after wave 2** (§11): later waves read and write fields
//! through [`Ro`] / [`Rw`] with the offset constants re-exported here, and
//! add typed records in their own `proc` files, so no change here is
//! needed for a new instruction.
//!
//! The offsets exist once, in `frontier_abi::layout` (every range tiles its
//! account exactly, const-asserted there); nothing here restates a number.
//! [`Ro`] and [`Rw`] are bounds-checked (a short slice is `BadAccount`,
//! never a panic), so a handler that checked an account's size once can use
//! every fixed offset of its layout with `?`.

use frontier_abi::bytes as b;

use crate::error::{Error, BAD_ACCOUNT};
use crate::FrontierError;

pub mod beacon;
pub mod clash;
pub mod land;
pub mod player;
pub mod world;

#[cfg(test)]
mod text_check;

pub use beacon::{anchor_archive, archive_entry, bell_anchor, defence_claim, seed_cache};
pub use clash::{arrival, arrival_day, arrival_slot, clash_inputs};
pub use frontier_abi::layout::{header, rent, AccountKind, Field, RENT_PER_BYTE};
pub use land::{camp, cohort, entry, province, site, summary};
pub use player::{citizen, explore, holding, transit};
pub use world::{beacon_log, defence_pool, frontier, join_shard, province_fund, ring_seed, season};

/// Read-only view of an account's data.
#[derive(Clone, Copy, Debug)]
pub struct Ro<'a>(pub &'a [u8]);

/// Mutable view of an account's data.
#[derive(Debug)]
pub struct Rw<'a>(pub &'a mut [u8]);

macro_rules! getters {
    ($($name:ident: $t:ty = $f:path;)*) => {$(
        #[inline]
        pub fn $name(&self, off: usize) -> Result<$t, Error> {
            $f(self.bytes(), off).ok_or(BAD_ACCOUNT)
        }
    )*};
}

macro_rules! setters {
    ($($name:ident: $t:ty = $f:path;)*) => {$(
        #[inline]
        pub fn $name(&mut self, off: usize, v: $t) -> Result<(), Error> {
            if $f(self.0, off, v) { Ok(()) } else { Err(BAD_ACCOUNT) }
        }
    )*};
}

/// Shared read accessors of [`Ro`] and [`Rw`].
pub trait Read {
    fn bytes(&self) -> &[u8];
}

impl Read for Ro<'_> {
    fn bytes(&self) -> &[u8] {
        self.0
    }
}

impl Read for Rw<'_> {
    fn bytes(&self) -> &[u8] {
        self.0
    }
}

macro_rules! read_impl {
    ($t:ty) => {
        impl $t {
            getters! {
                u8: u8 = b::rd_u8;
                u16: u16 = b::rd_u16;
                u32: u32 = b::rd_u32;
                u64: u64 = b::rd_u64;
                i16: i16 = b::rd_i16;
                i32: i32 = b::rd_i32;
                i64: i64 = b::rd_i64;
            }
            /// `N` bytes at `off`.
            #[inline]
            pub fn arr<const N: usize>(&self, off: usize) -> Result<[u8; N], Error> {
                b::rd_arr(self.bytes(), off).ok_or(BAD_ACCOUNT)
            }
            /// The slice `off..off + n`.
            #[inline]
            pub fn slice(&self, off: usize, n: usize) -> Result<&[u8], Error> {
                off.checked_add(n)
                    .and_then(|end| self.bytes().get(off..end))
                    .ok_or(BAD_ACCOUNT)
            }
            /// Bit `i` of the bitmap starting at `base` (bit 0 = low bit of
            /// the first byte).
            #[inline]
            pub fn bit(&self, base: usize, i: usize) -> Result<bool, Error> {
                Ok(self.u8(base + i / 8)? & (1 << (i % 8)) != 0)
            }
        }
    };
}

read_impl!(Ro<'_>);
read_impl!(Rw<'_>);

impl Rw<'_> {
    setters! {
        set_u8: u8 = b::wr_u8;
        set_u16: u16 = b::wr_u16;
        set_u32: u32 = b::wr_u32;
        set_u64: u64 = b::wr_u64;
        set_i16: i16 = b::wr_i16;
        set_i32: i32 = b::wr_i32;
        set_i64: i64 = b::wr_i64;
    }

    /// Writes `v` at `off`.
    #[inline]
    pub fn set_arr(&mut self, off: usize, v: &[u8]) -> Result<(), Error> {
        let end = off.checked_add(v.len()).ok_or(BAD_ACCOUNT)?;
        self.0
            .get_mut(off..end)
            .ok_or(BAD_ACCOUNT)?
            .copy_from_slice(v);
        Ok(())
    }

    /// Sets bit `i` of the bitmap at `base`.
    #[inline]
    pub fn set_bit(&mut self, base: usize, i: usize) -> Result<(), Error> {
        let at = base + i / 8;
        let v = self.u8(at)?;
        self.set_u8(at, v | (1 << (i % 8)))
    }

    /// `a += v` (u64), `Overflow` on wrap.
    pub fn add_u64(&mut self, off: usize, v: u64) -> Result<(), Error> {
        let x = self
            .u64(off)?
            .checked_add(v)
            .ok_or(crate::error::OVERFLOW)?;
        self.set_u64(off, x)
    }

    /// `a += v` (u32), `Overflow` on wrap.
    pub fn add_u32(&mut self, off: usize, v: u32) -> Result<(), Error> {
        let x = self
            .u32(off)?
            .checked_add(v)
            .ok_or(crate::error::OVERFLOW)?;
        self.set_u32(off, x)
    }
}

/// Writes a fresh header (§4.3): the chained header H (seq 0, zero head)
/// for chained kinds, the short header SH otherwise. The account is
/// assumed zeroed (freshly allocated), so every other field starts at 0.
pub fn init_header(d: &mut [u8], kind: AccountKind, season_id: u64) -> Result<(), Error> {
    if d.len() < kind.size() || !frontier_abi::layout::write_header(d, kind, season_id) {
        return Err(BAD_ACCOUNT);
    }
    Ok(())
}

/// Structural presence check of a program account's data: full size, magic
/// of `kind`, `season_id` (`BadAccount`). Owner and address are the
/// caller's (the prologue checks the owner with the data borrowed).
pub fn check_kind(d: &[u8], kind: AccountKind, season_id: u64) -> Result<(), Error> {
    if d.len() < kind.size() {
        return Err(BAD_ACCOUNT);
    }
    match frontier_abi::layout::read_short_header(d) {
        Some((m, id)) if m == kind.magic() && id == season_id => Ok(()),
        _ => Err(FrontierError::BadAccount.into()),
    }
}

/// The event chain of a chained account: `(event_seq, event_head)`.
pub fn chain_of(d: &[u8]) -> Result<(u64, [u8; 32]), Error> {
    let r = Ro(d);
    Ok((r.u64(header::EVENT_SEQ)?, r.arr(header::EVENT_HEAD)?))
}

/// Sets the event chain of a chained account.
pub fn set_chain(d: &mut [u8], seq: u64, head: &[u8; 32]) -> Result<(), Error> {
    let mut w = Rw(d);
    w.set_u64(header::EVENT_SEQ, seq)?;
    w.set_arr(header::EVENT_HEAD, head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessors_are_bounds_checked() {
        let mut d = [0u8; 16];
        let mut w = Rw(&mut d);
        w.set_u64(8, u64::MAX).unwrap();
        assert!(w.set_u64(9, 1).is_err());
        assert!(w.set_arr(15, &[1, 2]).is_err());
        w.set_bit(0, 9).unwrap();
        assert!(w.bit(0, 9).unwrap());
        assert!(!w.bit(0, 8).unwrap());
        assert_eq!(w.add_u64(8, 1), Err(crate::error::OVERFLOW));
        let r = Ro(&d);
        assert_eq!(r.u64(8).unwrap(), u64::MAX);
        assert_eq!(r.u16(0).unwrap(), 0x0200);
        assert!(r.i64(9).is_err());
        assert!(r.slice(10, 7).is_err());
        assert_eq!(r.slice(10, 6).unwrap().len(), 6);
    }

    #[test]
    fn headers_follow_the_kind() {
        for k in AccountKind::ALL {
            let mut d = alloc::vec![0u8; k.size()];
            init_header(&mut d, k, 42).unwrap();
            check_kind(&d, k, 42).unwrap();
            assert!(check_kind(&d, k, 43).is_err());
            assert!(check_kind(&d[..k.size() - 1], k, 42).is_err());
            if k.chained() {
                assert_eq!(Ro(&d).u16(header::LAYOUT_VERSION).unwrap(), 1);
                assert_eq!(chain_of(&d).unwrap(), (0, [0u8; 32]));
                set_chain(&mut d, 7, &[3; 32]).unwrap();
                assert_eq!(chain_of(&d).unwrap(), (7, [3u8; 32]));
            }
            // another kind's magic is refused
            for o in AccountKind::ALL {
                if o != k && o.size() <= k.size() {
                    assert!(check_kind(&d, o, 42).is_err(), "{k:?} as {o:?}");
                }
            }
        }
        assert!(init_header(&mut [0u8; 10], AccountKind::Season, 1).is_err());
    }
}
