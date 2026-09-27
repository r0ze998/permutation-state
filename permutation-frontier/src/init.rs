//! Account initialisation, closing and payments (M1 contract §4.2).
//!
//! - [`init_pda`]: the Season (the only PDA). Require absent; top the
//!   address up to its target balance from the payer (System Transfer,
//!   payer signs); `Allocate` + `Assign` signed by the PDA seeds.
//! - [`init_with_seed`]: every other account, at `create_with_seed(season,
//!   seed, program)`. Require absent; top up from the payer; one
//!   `AllocateWithSeed { base: season, seed, space, owner: program }`
//!   signed by the Season PDA.
//! - [`init_funded`]: as `init_with_seed`, but the shortfall moves by
//!   direct lamport arithmetic from a **program-owned** fund (a
//!   ProvinceFund for a Province, the Citizen's ticket escrow for a
//!   Holding), which must stay rent-exempt.
//! - [`close_to`]: `resize(0)`, `assign(System)`, every lamport through
//!   [`pay_or_divert`]; the address is absent again (§4.2 tombstones).
//! - [`pay_or_divert`]: a payment that would leave a non-program recipient
//!   below rent exemption goes to the sink instead and is logged `DIVERT`
//!   (I-20): `Holding.pool_owed` in SettleTransit and every W, D or player
//!   instruction; the DefencePool only in the N-class instructions that
//!   list it (I-48).
//!
//! **Pre-funding (§4.1).** Every creation path is safe on an address that
//! already holds lamports: nothing here uses `CreateAccount*` (which the
//! System program refuses on a funded address), the payer pays only the
//! shortfall, and lamports above the target stay in the account.
//!
//! The Season PDA's signer seeds are `["season", le64(id), [bump]]` with the
//! bump stored by AnnounceSeason; no caller ever supplies a bump.

use alloc::vec;
use alloc::vec::Vec;

use solana_program::{
    account_info::AccountInfo,
    instruction::{AccountMeta, Instruction},
    program::{invoke, invoke_signed},
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};

use frontier_abi::layout::player::holding as H;
use frontier_abi::layout::world::defence_pool as DP;
use frontier_abi::log::{divert_reason, Kind};

use crate::addr::{self, Seed};
use crate::error::{BAD_ACCOUNT, OVERFLOW};
use crate::layout::Rw;
use crate::{events, FrontierError, R};

/// The System program id (all zeros).
pub const SYSTEM: Pubkey = Pubkey::new_from_array(addr::ids::SYSTEM_PROGRAM);

/// System instruction discriminants used here.
mod sys_ix {
    pub const ASSIGN: u32 = 1;
    pub const TRANSFER: u32 = 2;
    pub const ALLOCATE: u32 = 8;
    pub const ALLOCATE_WITH_SEED: u32 = 9;
}

/// The Season PDA's signer seeds.
#[derive(Clone, Copy, Debug)]
pub struct SeasonSigner {
    id: [u8; 8],
    bump: [u8; 1],
}

impl SeasonSigner {
    pub fn new(id: u64, bump: u8) -> SeasonSigner {
        SeasonSigner {
            id: id.to_le_bytes(),
            bump: [bump],
        }
    }

    pub fn seeds(&self) -> [&[u8]; 3] {
        [addr::SEASON_PREFIX, &self.id, &self.bump]
    }
}

/// The rent-exempt minimum of `space` bytes (Rent sysvar).
pub fn rent(space: usize) -> R<u64> {
    Ok(Rent::get()?.minimum_balance(space))
}

/// Absent: owned by the System program with no data (lamports ignored).
pub fn is_absent(a: &AccountInfo) -> bool {
    addr::absent(&a.owner.to_bytes(), a.data_len())
}

fn system_ix(accounts: Vec<AccountMeta>, data: Vec<u8>) -> Instruction {
    Instruction {
        program_id: SYSTEM,
        accounts,
        data,
    }
}

/// System Transfer `from` → `to` (`from` signs; a wallet, never a program
/// account).
pub fn transfer<'a>(from: &AccountInfo<'a>, to: &AccountInfo<'a>, lamports: u64) -> R<()> {
    if lamports == 0 {
        return Ok(());
    }
    let mut d = Vec::with_capacity(12);
    d.extend_from_slice(&sys_ix::TRANSFER.to_le_bytes());
    d.extend_from_slice(&lamports.to_le_bytes());
    invoke(
        &system_ix(
            vec![
                AccountMeta::new(*from.key, true),
                AccountMeta::new(*to.key, false),
            ],
            d,
        ),
        &[from.clone(), to.clone()],
    )?;
    Ok(())
}

/// Tops `target` up to `total` lamports from `payer` (`Insufficient` if the
/// payer cannot cover it; lamports already there count, §4.1).
fn top_up<'a>(payer: &AccountInfo<'a>, target: &AccountInfo<'a>, total: u64) -> R<()> {
    let need = total.saturating_sub(target.lamports());
    if need > payer.lamports() {
        return Err(FrontierError::Insufficient.into());
    }
    transfer(payer, target, need)
}

/// Creates the Season PDA `target` holding `total` lamports (rent plus
/// whatever the caller escrows, e.g. the creation bond). The caller has
/// checked the address.
pub fn init_pda<'a>(
    payer: &AccountInfo<'a>,
    target: &AccountInfo<'a>,
    signer: &SeasonSigner,
    space: usize,
    total: u64,
    program: &Pubkey,
) -> R<()> {
    if !is_absent(target) {
        return Err(BAD_ACCOUNT);
    }
    top_up(payer, target, total)?;
    let seeds = signer.seeds();
    let mut d = Vec::with_capacity(12);
    d.extend_from_slice(&sys_ix::ALLOCATE.to_le_bytes());
    d.extend_from_slice(&(space as u64).to_le_bytes());
    invoke_signed(
        &system_ix(vec![AccountMeta::new(*target.key, true)], d),
        core::slice::from_ref(target),
        &[&seeds],
    )?;
    let mut d = Vec::with_capacity(36);
    d.extend_from_slice(&sys_ix::ASSIGN.to_le_bytes());
    d.extend_from_slice(program.as_ref());
    invoke_signed(
        &system_ix(vec![AccountMeta::new(*target.key, true)], d),
        core::slice::from_ref(target),
        &[&seeds],
    )?;
    Ok(())
}

fn allocate_with_seed<'a>(
    target: &AccountInfo<'a>,
    season: &AccountInfo<'a>,
    signer: &SeasonSigner,
    seed: &Seed,
    space: usize,
    program: &Pubkey,
) -> R<()> {
    let s = seed.as_bytes();
    let mut d = Vec::with_capacity(4 + 32 + 8 + s.len() + 8 + 32);
    d.extend_from_slice(&sys_ix::ALLOCATE_WITH_SEED.to_le_bytes());
    d.extend_from_slice(season.key.as_ref());
    d.extend_from_slice(&(s.len() as u64).to_le_bytes());
    d.extend_from_slice(s);
    d.extend_from_slice(&(space as u64).to_le_bytes());
    d.extend_from_slice(program.as_ref());
    invoke_signed(
        &system_ix(
            vec![
                AccountMeta::new(*target.key, false),
                AccountMeta::new_readonly(*season.key, true),
            ],
            d,
        ),
        &[target.clone(), season.clone()],
        &[&signer.seeds()],
    )?;
    Ok(())
}

/// Creates the with-seed account `target` (= `with_seed(season, seed,
/// program)`, checked by the caller) holding at least `total` lamports,
/// the shortfall paid by `payer`.
#[allow(clippy::too_many_arguments)]
pub fn init_with_seed<'a>(
    payer: &AccountInfo<'a>,
    target: &AccountInfo<'a>,
    season: &AccountInfo<'a>,
    signer: &SeasonSigner,
    seed: &Seed,
    space: usize,
    total: u64,
    program: &Pubkey,
) -> R<()> {
    if !is_absent(target) {
        return Err(BAD_ACCOUNT);
    }
    crate::heap::trace_checkpoint(100);
    top_up(payer, target, total)?;
    crate::heap::trace_checkpoint(101);
    allocate_with_seed(target, season, signer, seed, space, program)?;
    crate::heap::trace_checkpoint(102);
    Ok(())
}

/// Moves lamports out of a program-owned account by direct arithmetic.
pub fn move_lamports(from: &AccountInfo, to: &AccountInfo, amount: u64) -> R<()> {
    if amount == 0 || from.key == to.key {
        return Ok(());
    }
    let f = from
        .lamports()
        .checked_sub(amount)
        .ok_or(FrontierError::Insufficient)?;
    let t = to.lamports().checked_add(amount).ok_or(OVERFLOW)?;
    **from.try_borrow_mut_lamports()? = f;
    **to.try_borrow_mut_lamports()? = t;
    Ok(())
}

/// As [`init_with_seed`], funded from the program-owned `fund`, which
/// stays rent-exempt (`Insufficient`): moves `max(amount, rent(space) −
/// pre-funded)` so the target ends rent-exempt. Returns the lamports moved.
///
/// Order (W3-A F1, integ-W3 review): **allocate first, then move the
/// lamports.** Moving them first by direct arithmetic leaves the fund's
/// debit outside the AllocateWithSeed CPI (the fund is not a CPI account)
/// while the target's credit is synced into it, and the runtime refuses the
/// caller as `UnbalancedInstruction` [measured, W3-A notes]. Here the CPI
/// runs on untouched accounts and the lamports then move between two
/// program-owned accounts.
#[allow(clippy::too_many_arguments)]
pub fn init_funded<'a>(
    fund: &AccountInfo<'a>,
    target: &AccountInfo<'a>,
    season: &AccountInfo<'a>,
    signer: &SeasonSigner,
    seed: &Seed,
    space: usize,
    program: &Pubkey,
    amount: u64,
) -> R<u64> {
    if fund.owner != program || !is_absent(target) {
        return Err(BAD_ACCOUNT);
    }
    let need = rent(space)?.saturating_sub(target.lamports()).max(amount);
    let floor = rent(fund.data_len())?;
    if fund.lamports().saturating_sub(need) < floor {
        return Err(FrontierError::Insufficient.into());
    }
    allocate_with_seed(target, season, signer, seed, space, program)?;
    move_lamports(fund, target, need)?;
    Ok(need)
}

/// Where a diverted payment goes (I-20, I-48).
pub enum Sink<'a, 'b> {
    /// Stays in (or moves to) the Holding and counts in its `pool_owed`,
    /// swept later by SweepPoolOwed.
    PoolOwed(&'b AccountInfo<'a>),
    /// Credits the DefencePool (`diverted_total`); N-class instructions
    /// that list it only.
    DefencePool(&'b AccountInfo<'a>),
    /// No sink: a payment that would divert is refused (`BadAccount`).
    /// For whole-account closes whose recipient is not a Holding
    /// (CloseProvince → its ProvinceFund, CloseCitizen → its rent payer):
    /// a whole-account refund is at least `rent(0)`, so it never diverts
    /// (§4.2); the variant keeps a wrong-typed `pool_owed` write impossible.
    Never,
}

/// Where a payment went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paid {
    Recipient,
    Diverted,
}

/// The divert rule of `pay_or_divert` for a recipient that is not a
/// program account: the payment would leave it below its rent-exempt
/// minimum (`None` on overflow).
pub const fn diverts(to_lamports: u64, amount: u64, rent_floor: u64) -> Option<bool> {
    match to_lamports.checked_add(amount) {
        Some(after) => Some(after < rent_floor),
        None => None,
    }
}

/// Pays `amount` from the program-owned `from` to `to`, or to the sink if
/// `to` is not program-owned and would stay below rent exemption; a
/// diversion is logged `DIVERT {recipient, amount, reason}` (reasons:
/// `frontier_abi::log::divert_reason`).
pub fn pay_or_divert<'a>(
    program: &Pubkey,
    from: &AccountInfo<'a>,
    to: &AccountInfo<'a>,
    amount: u64,
    sink: &Sink<'a, '_>,
    reason: u8,
    bell: u32,
) -> R<Paid> {
    if amount == 0 {
        return Ok(Paid::Recipient);
    }
    let below = to.owner != program
        && diverts(to.lamports(), amount, rent(to.data_len())?).ok_or(OVERFLOW)?;
    if !below {
        move_lamports(from, to, amount)?;
        return Ok(Paid::Recipient);
    }
    match sink {
        Sink::PoolOwed(h) => {
            move_lamports(from, h, amount)?;
            let mut d = h.try_borrow_mut_data()?;
            Rw(&mut d).add_u64(H::POOL_OWED, amount)?;
        }
        Sink::DefencePool(p) => {
            move_lamports(from, p, amount)?;
            let mut d = p.try_borrow_mut_data()?;
            Rw(&mut d).add_u64(DP::DIVERTED_TOTAL, amount)?;
        }
        Sink::Never => return Err(BAD_ACCOUNT),
    }
    let payload = events::Buf::<9>::new().u64(amount).u8(reason);
    events::emit(Kind::DIVERT, bell, to.key.as_ref(), payload.get()?, &mut [])?;
    Ok(Paid::Diverted)
}

/// Closes `target` (program-owned): data freed, assigned back to System,
/// every lamport to `recipient` through [`pay_or_divert`] (a whole-account
/// refund is at least `rent(0)` for any recipient, so it never diverts in
/// practice). Returns the lamports moved. The caller logs `CLOSE` with the
/// final chain of a chained account before calling this.
pub fn close_to<'a>(
    program: &Pubkey,
    target: &AccountInfo<'a>,
    recipient: &AccountInfo<'a>,
    sink: &Sink<'a, '_>,
    bell: u32,
) -> R<u64> {
    if target.owner != program {
        return Err(BAD_ACCOUNT);
    }
    let lamports = target.lamports();
    target.resize(0)?;
    target.assign(&SYSTEM);
    pay_or_divert(
        program,
        target,
        recipient,
        lamports,
        sink,
        divert_reason::RENT_REFUND,
        bell,
    )?;
    Ok(lamports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diverts_only_below_rent_exemption() {
        let floor = frontier_abi::layout::rent(0);
        // an empty wallet receiving less than rent(0): diverted
        assert_eq!(diverts(0, floor - 1, floor), Some(true));
        // a whole-account refund (≥ rent(0)) never diverts (§4.2)
        assert_eq!(diverts(0, floor, floor), Some(false));
        // a funded wallet takes any amount
        assert_eq!(diverts(floor, 1, floor), Some(false));
        assert_eq!(diverts(u64::MAX, 1, floor), None);
        // every program account's rent clears the whole-account floor
        for k in frontier_abi::layout::AccountKind::ALL {
            assert_eq!(diverts(0, k.rent(), floor), Some(false), "{k:?}");
        }
    }

    #[test]
    fn season_signer_seeds_are_the_pda_seeds() {
        let s = SeasonSigner::new(0x0102, 254);
        let seeds = s.seeds();
        assert_eq!(seeds[0], b"season");
        assert_eq!(seeds[1], &[2, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(seeds[2], &[254]);
        assert_eq!(SYSTEM.to_bytes(), [0u8; 32]);
    }
}
