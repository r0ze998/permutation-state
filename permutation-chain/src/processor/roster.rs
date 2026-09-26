//! The operator's AI members (V5 §18.2), revealed on the base layer after
//! the season; and the members' messages, anchored on the ER (§18.7).

use permutation_rules::roster::{roster_link, roster_tag};
use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, msg, pubkey::Pubkey};

use super::accounts::*;
use crate::error::ChainError;
use crate::state::*;

/// `RevealRoster`: the next AIs of the roster, in the committed order. Each
/// salt must turn its member's wallet into the tag the wallet registered
/// with; the revealed tags extend `roster_acc`, which must equal
/// `roster_commit` once all `ai_count` are in. Before the season is
/// finalized; anyone holding the salts may send it.
pub(super) fn reveal_roster(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    from: u16,
    salts: Vec<[u8; 32]>,
    blind: [u8; 32],
) -> ProgramResult {
    // The operator-only, restartable reveal with the blinded commitment
    // (`from`, `blind`) lands with unit P3.
    let _ = (from, blind);
    let [season_info, roster_info, members @ ..] = accounts else {
        return Err(solana_program::program_error::ProgramError::NotEnoughAccountKeys);
    };
    let mut season = load_season(program_id, season_info)?;
    if season.status == SeasonStatus::Finalized {
        return Err(ChainError::WrongStatus.into());
    }
    let mut roster = load_roster(program_id, roster_info, season.season_id)?;
    if salts.is_empty()
        || members.len() != salts.len()
        || season.roster_revealed as usize + salts.len() > season.ai_count as usize
    {
        return Err(ChainError::InvalidParams.into());
    }
    for (member_info, salt) in members.iter().zip(&salts) {
        let m = load_member(program_id, member_info, season.season_id)?;
        if roster_tag(season.season_id, &m.wallet, salt) != m.tag
            || roster.entries.iter().any(|e| e.member == m.index)
        {
            return Err(ChainError::RosterMismatch.into());
        }
        season.roster_acc = roster_link(&season.roster_acc, &m.tag);
        season.roster_revealed += 1;
        roster.entries.push(RosterEntry {
            member: m.index,
            civ: m.civ,
            salt: *salt,
            shares: m.shares,
        });
    }
    if season.roster_revealed == season.ai_count && season.roster_acc != season.roster_commit {
        return Err(ChainError::RosterMismatch.into());
    }
    store(&mut roster_info.try_borrow_mut_data()?, &roster)?;
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
    msg!(
        "PS roster {}/{} revealed, season {}",
        season.roster_revealed,
        season.ai_count,
        season.season_id
    );
    Ok(())
}

/// `AnchorTalk`: log a tick's message root (crank only).
pub(super) fn anchor_talk(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    tick: u16,
    count: u32,
    root: [u8; 32],
) -> ProgramResult {
    let [crank, nation_info, ..] = accounts else {
        return Err(solana_program::program_error::ProgramError::NotEnoughAccountKeys);
    };
    signer(crank)?;
    let n = load_nation_at(program_id, nation_info)?;
    require_crank(program_id, nation_info, n.season_id, crank)?;
    solana_program::log::sol_log_data(&[
        b"PS_TALK",
        &n.season_id.to_le_bytes(),
        &tick.to_le_bytes(),
        &count.to_le_bytes(),
        &root,
    ]);
    Ok(())
}
