//! Fee evidence (M1 contract §5.12, I-21): what a keeper write paid, read
//! from the instructions sysvar (read-only key `Sysvar1nstructions…`) and
//! recorded in the account it creates (Reveal's ArrivalSlot, BellAnchor,
//! ClashInputs) for the liveness report and ClaimDefence.
//!
//! Fields: the landing slot (Clock), the `SetComputeUnitPrice` µlamports
//! per CU (0 if absent), the `SetComputeUnitLimit` (0 if absent: the
//! runtime default applies) and the `SetLoadedAccountsDataSizeLimit` (0 if
//! absent: the 64-MiB default applies). The transaction's signature count
//! is assumed 1 (keeper transactions have one signer).
//!
//! [`parse`] walks the sysvar's serialized form without allocating (the
//! layout of `solana-instructions-sysvar`: `u16 n`, `u16 offsets[n]`, then
//! per instruction `u16 n_accounts`, `n_accounts × (u8 flags, [u8; 32])`,
//! `program_id [32]`, `u16 data_len`, data; the current index is the last
//! two bytes). A duplicate ComputeBudget instruction fails the transaction
//! before execution, so the first of each kind is the only one.

use frontier_abi::prologue::ids::COMPUTE_BUDGET_PROGRAM;

use crate::error::{Error, BAD_ACCOUNT};

/// ComputeBudget instruction discriminants.
pub mod cb {
    pub const REQUEST_HEAP_FRAME: u8 = 1;
    pub const SET_COMPUTE_UNIT_LIMIT: u8 = 2;
    pub const SET_COMPUTE_UNIT_PRICE: u8 = 3;
    pub const SET_LOADED_ACCOUNTS_DATA_SIZE_LIMIT: u8 = 4;
}

/// What the transaction bid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bid {
    /// µlamports per CU.
    pub price: u64,
    pub limit: u32,
    pub loaded: u32,
    /// `RequestHeapFrame` bytes (0 if absent), for the retry-ladder report.
    pub heap: u32,
}

/// Evidence as stored: the bid and the landing slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Evidence {
    pub slot: u64,
    pub price: u64,
    pub limit: u32,
    pub loaded: u32,
}

impl Evidence {
    pub fn new(slot: u64, bid: Bid) -> Evidence {
        Evidence {
            slot,
            price: bid.price,
            limit: bid.limit,
            loaded: bid.loaded,
        }
    }
}

fn u16_at(d: &[u8], o: usize) -> Result<u16, Error> {
    frontier_abi::bytes::rd_u16(d, o).ok_or(BAD_ACCOUNT)
}

/// The ComputeBudget bid of the transaction whose instructions sysvar data
/// is `d` (`BadAccount` if the data is malformed).
pub fn parse(d: &[u8]) -> Result<Bid, Error> {
    let n = u16_at(d, 0)? as usize;
    let mut bid = Bid::default();
    let (mut limit, mut price, mut loaded, mut heap) = (false, false, false, false);
    for i in 0..n {
        let mut o = u16_at(d, 2 + 2 * i)? as usize;
        let n_acc = u16_at(d, o)? as usize;
        o = o.checked_add(2 + 33 * n_acc).ok_or(BAD_ACCOUNT)?;
        let program = d.get(o..o + 32).ok_or(BAD_ACCOUNT)?;
        o += 32;
        let len = u16_at(d, o)? as usize;
        o += 2;
        let data = d.get(o..o + len).ok_or(BAD_ACCOUNT)?;
        if program != COMPUTE_BUDGET_PROGRAM {
            continue;
        }
        let rd32 = |x: &[u8]| frontier_abi::bytes::rd_u32(x, 1).ok_or(BAD_ACCOUNT);
        match data.first().copied() {
            Some(cb::SET_COMPUTE_UNIT_LIMIT) if !limit => {
                bid.limit = rd32(data)?;
                limit = true;
            }
            Some(cb::SET_COMPUTE_UNIT_PRICE) if !price => {
                bid.price = frontier_abi::bytes::rd_u64(data, 1).ok_or(BAD_ACCOUNT)?;
                price = true;
            }
            Some(cb::SET_LOADED_ACCOUNTS_DATA_SIZE_LIMIT) if !loaded => {
                bid.loaded = rd32(data)?;
                loaded = true;
            }
            Some(cb::REQUEST_HEAP_FRAME) if !heap => {
                bid.heap = rd32(data)?;
                heap = true;
            }
            _ => {}
        }
    }
    Ok(bid)
}

/// Reads the bid from the instructions sysvar account (its key must be the
/// sysvar's, `BadAccount`) and the landing slot from the Clock.
#[cfg(feature = "program")]
pub fn read(
    ix_sysvar: &solana_program::account_info::AccountInfo,
    slot: u64,
) -> Result<Evidence, Error> {
    if ix_sysvar.key.to_bytes() != frontier_abi::prologue::ids::INSTRUCTIONS_SYSVAR {
        return Err(BAD_ACCOUNT);
    }
    let d = ix_sysvar.try_borrow_data()?;
    Ok(Evidence::new(slot, parse(&d)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// Serializes instructions as `solana-instructions-sysvar` does.
    fn sysvar(ixs: &[([u8; 32], usize, Vec<u8>)]) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&(ixs.len() as u16).to_le_bytes());
        d.resize(2 + 2 * ixs.len(), 0);
        for (i, (program, n_acc, data)) in ixs.iter().enumerate() {
            let at = d.len() as u16;
            d[2 + 2 * i..4 + 2 * i].copy_from_slice(&at.to_le_bytes());
            d.extend_from_slice(&(*n_acc as u16).to_le_bytes());
            for k in 0..*n_acc {
                d.push(3);
                d.extend_from_slice(&[k as u8; 32]);
            }
            d.extend_from_slice(program);
            d.extend_from_slice(&(data.len() as u16).to_le_bytes());
            d.extend_from_slice(data);
        }
        d.extend_from_slice(&(ixs.len() as u16 - 1).to_le_bytes());
        d
    }

    fn cbix(tag: u8, v: u64, w: usize) -> ([u8; 32], usize, Vec<u8>) {
        let mut d = Vec::from([tag]);
        d.extend_from_slice(&v.to_le_bytes()[..w]);
        (COMPUTE_BUDGET_PROGRAM, 0, d)
    }

    #[test]
    fn reads_every_compute_budget_field() {
        let d = sysvar(&[
            cbix(cb::SET_COMPUTE_UNIT_LIMIT, 26_000, 4),
            cbix(cb::SET_COMPUTE_UNIT_PRICE, 123_456_789, 8),
            cbix(cb::SET_LOADED_ACCOUNTS_DATA_SIZE_LIMIT, 1_048_576, 4),
            cbix(cb::REQUEST_HEAP_FRAME, 262_144, 4),
            ([9; 32], 18, Vec::from([0x51u8; 136])),
        ]);
        assert_eq!(
            parse(&d).unwrap(),
            Bid {
                price: 123_456_789,
                limit: 26_000,
                loaded: 1_048_576,
                heap: 262_144
            }
        );
        let e = Evidence::new(77, parse(&d).unwrap());
        assert_eq!(
            (e.slot, e.price, e.limit, e.loaded),
            (77, 123_456_789, 26_000, 1_048_576)
        );
    }

    #[test]
    fn absent_fields_are_zero_and_garbage_is_refused() {
        let d = sysvar(&[([9; 32], 3, Vec::from([1u8, 2, 3]))]);
        assert_eq!(parse(&d).unwrap(), Bid::default());
        assert!(parse(&[]).is_err());
        assert!(parse(&d[..d.len() - 6]).is_err());
        let mut bad = sysvar(&[cbix(cb::SET_COMPUTE_UNIT_LIMIT, 1, 2)]);
        assert!(parse(&bad).is_err(), "limit shorter than u32");
        bad[2] = 0xff; // offset out of range
        assert!(parse(&bad).is_err());
    }
}
