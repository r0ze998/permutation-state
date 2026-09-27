//! The program's thin wrapper over `frontier_abi::prologue` (M1 contract
//! §5.6, I-55): turns `AccountInfo`s into `AccountView`s and calls the
//! shared account-list tables and checks, so the program, the off-chain
//! builders and the relay's shape allowlist agree on every account list.
//!
//! Every handler starts with [`check_accounts`] (exact count, signers,
//! writability, fixed program ids: DESIGN §8.6 rule 6, within ≈ 1.5k CU),
//! then the season step of its prologue ([`season`], [`keeper`] or the
//! player prologue of later waves), then its own checks in the order the
//! contract lists them.

use alloc::vec::Vec;

use solana_program::{
    account_info::AccountInfo, clock::Clock, instruction::get_stack_height, pubkey::Pubkey,
    sysvar::Sysvar,
};

use frontier_abi::layout::AccountKind;
use frontier_abi::prologue::{self as ap, Acc, AccountView, SeasonHdr, Spec, Wr};
use frontier_abi::tags::Ix;

use crate::error::BAD_ACCOUNT;
use crate::{FrontierError, R};

pub use frontier_abi::prologue::ids;

/// The Clock values a handler uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Now {
    /// `unix_timestamp`: every rule time (§5.1).
    pub ts: i64,
    pub slot: u64,
}

/// Reads the Clock sysvar.
pub fn now() -> R<Now> {
    let c = Clock::get()?;
    Ok(Now {
        ts: c.unix_timestamp,
        slot: c.slot,
    })
}

/// The 32 raw bytes of a key.
pub fn key(ai: &AccountInfo) -> [u8; 32] {
    ai.key.to_bytes()
}

/// A view of an account with its data borrowed by the caller.
pub fn view<'a>(ai: &'a AccountInfo, data: &'a [u8]) -> AccountView<'a> {
    AccountView {
        key: ai.key.as_array(),
        owner: ai.owner.as_array(),
        lamports: ai.lamports(),
        data,
        is_signer: ai.is_signer,
        is_writable: ai.is_writable,
    }
}

const NO_SPEC: Spec = Spec {
    name: "",
    acc: Acc::Any,
    signer: false,
    wr: Wr::R,
};

/// Checks the account list against the instruction's table: the exact
/// count (`TooManyAccounts`), missing signatures (`Auth`), writability
/// (`BadAccount`) and the fixed program ids (System, instructions sysvar,
/// incinerator). `counts` gives the repeat count of every group; `None`
/// derives it from the account count (one varying group at most).
pub fn check_accounts(ix: Ix, accounts: &[AccountInfo], counts: Option<&[u8]>) -> R<()> {
    let mut c = [0u8; 8];
    let ng = match counts {
        Some(given) => {
            let n = given.len().min(c.len());
            c[..n].copy_from_slice(&given[..n]);
            given.len()
        }
        None => {
            ap::counts_from_len(ix, accounts.len(), &mut c).ok_or(FrontierError::TooManyAccounts)?
        }
    };
    let (_, hi) = ap::count_bounds(ix);
    if accounts.len() > hi {
        return Err(FrontierError::TooManyAccounts.into());
    }
    let mut specs = alloc::vec![NO_SPEC; hi];
    let n = ap::resolve(
        ix,
        c.get(..ng).ok_or(FrontierError::TooManyAccounts)?,
        &mut specs,
    )
    .ok_or(FrontierError::TooManyAccounts)?;
    if n != accounts.len() {
        return Err(FrontierError::TooManyAccounts.into());
    }
    // Flags only: the data is not needed by `check_flags`.
    let views: Vec<AccountView> = accounts.iter().map(|a| view(a, &[])).collect();
    ap::check_flags(&specs[..n], &views)?;
    Ok(())
}

/// Instructions marked top-level only refuse CPI (`NotTopLevel`).
pub fn top_level(ix: Ix) -> R<()> {
    ap::check_top_level(ix, get_stack_height())?;
    Ok(())
}

/// The Season, structurally and by status (§5.6 step 1): owner, magic,
/// size (`BadAccount`); the address recomputed from the stored id and bump
/// (`BadAddress`); effective status in `allowed` (`WrongStatus`), then the
/// ruleset when `ruleset` is given (`RulesetMismatch`).
///
/// The address is recomputed (§3.3 "recompute every keyed address a reader
/// trusts"; `frontier_abi::prologue::read_season` leaves it to the caller):
/// every with-seed address the handler derives hangs off this key, and a
/// program-owned copy of a Season at another address must never pass for
/// it (§13.2 G3).
pub fn season(
    ai: &AccountInfo,
    program: &Pubkey,
    ruleset: Option<&[u8; 32]>,
    allowed: &[u8],
    now: i64,
) -> R<SeasonHdr> {
    let d = ai.try_borrow_data()?;
    let v = view(ai, &d);
    canonical_season(&v, program, now)?;
    Ok(ap::check_season(
        &v,
        program.as_array(),
        ruleset,
        allowed,
        now,
    )?)
}

/// `read_season` (owner, magic, size: `BadAccount`), then the key must be
/// the Season PDA of the stored id and bump (`BadAddress`).
fn canonical_season(v: &AccountView, program: &Pubkey, now: i64) -> R<()> {
    let s = ap::read_season(v, program.as_array(), now)?;
    if crate::addr::season_pda(s.id, s.bump, program.as_array()) != *v.key {
        return Err(FrontierError::BadAddress.into());
    }
    Ok(())
}

/// The keeper-write prologue (§5.6): `[fee_payer s,w] [season]`, the
/// Season structurally and at its recomputed PDA (as [`season`]), the
/// season's status in `allowed`, then the ruleset. The fee payer's flags
/// were checked by [`check_accounts`] before this runs.
pub fn keeper(
    accounts: &[AccountInfo],
    program: &Pubkey,
    allowed: &[u8],
    now: i64,
) -> R<SeasonHdr> {
    let [fee_payer, season_ai, ..] = accounts else {
        return Err(FrontierError::TooManyAccounts.into());
    };
    let d = season_ai.try_borrow_data()?;
    let views = [view(fee_payer, &[]), view(season_ai, &d)];
    canonical_season(&views[1], program, now)?;
    Ok(ap::keeper_prologue(
        &views,
        program.as_array(),
        &crate::RULESET_HASH,
        allowed,
        now,
    )?)
}

/// The account must be at `expected` (`BadAddress`).
pub fn expect_key(ai: &AccountInfo, expected: &[u8; 32]) -> R<()> {
    if ai.key.as_array() == expected {
        Ok(())
    } else {
        Err(FrontierError::BadAddress.into())
    }
}

/// Present (program-owned, full size, magic, season) → `true`; absent →
/// `false`; anything else `BadAccount`. The caller has checked the address.
pub fn presence(ai: &AccountInfo, program: &Pubkey, kind: AccountKind, season_id: u64) -> R<bool> {
    let d = ai.try_borrow_data()?;
    Ok(ap::presence(
        &view(ai, &d),
        program.as_array(),
        kind,
        season_id,
    )?)
}

/// Present, else `BadAccount` (absent included).
pub fn present(ai: &AccountInfo, program: &Pubkey, kind: AccountKind, season_id: u64) -> R<()> {
    if presence(ai, program, kind, season_id)? {
        Ok(())
    } else {
        Err(BAD_ACCOUNT)
    }
}
