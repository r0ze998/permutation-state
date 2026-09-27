//! The operator's AI members (V5 §18.2), revealed on the base layer after
//! the season; and the members' messages, anchored on the ER (§18.7).

use permutation_rules::roster::{roster_commit, roster_link, roster_tag};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_error::ProgramError,
    pubkey::Pubkey,
};

use super::accounts::*;
use crate::error::ChainError;
use crate::state::*;

/// `RevealRoster`: the next AIs of the roster, in the committed order
/// (WP09). The operator only (admin or crank), once the season is over
/// with the final world back on base (chunk 0 program-owned, `finished`),
/// before FinishSeason. Each salt must turn its member's wallet into the
/// tag the wallet registered with; the revealed tags extend `roster_acc`.
/// `from == 0` restarts the reveal (an operator mistake is redone, never
/// stuck); otherwise `from` is the count revealed so far. The batch that
/// completes the roster must satisfy `roster_commit(blind, acc) ==
/// season.roster_commit` and stores the blind; a complete roster is final.
pub(super) fn reveal_roster(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    from: u16,
    salts: Vec<[u8; 32]>,
    blind: [u8; 32],
) -> ProgramResult {
    let [authority, season_info, roster_info, chunk0, members @ ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer(authority)?;
    let mut season = load_season(program_id, season_info)?;
    require_operator(&season, authority)?;
    if season.status != SeasonStatus::Running
        || season.ai_count == 0
        || season.roster_revealed == season.ai_count
    {
        return Err(ChainError::WrongStatus.into());
    }
    // The final world, back on base: nobody reveals while the season can
    // still be played.
    if chunk0.owner != program_id || chunk0.data_len() != CHUNK {
        return Err(ChainError::WrongWorld.into());
    }
    let id = season.season_id.to_le_bytes();
    expect_pda(program_id, chunk0, &[WORLD_SEED, &id, &[0]])?;
    if chunk0.try_borrow_data()?[..8] != WORLD_MAGIC {
        return Err(ChainError::WrongWorld.into());
    }
    let meta = WorldMeta::from_chunk0(&chunk0.try_borrow_data()?)?;
    if meta.season_id != season.season_id || !meta.finished {
        return Err(ChainError::SeasonNotOver.into());
    }
    let mut roster = load_roster(program_id, roster_info, season.season_id)?;
    if from == 0 {
        season.roster_acc = [0; 32];
        season.roster_revealed = 0;
        roster.entries.clear();
    } else if from != season.roster_revealed {
        return Err(ChainError::InvalidParams.into());
    }
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
    if season.roster_revealed == season.ai_count {
        if roster_commit(&blind, &season.roster_acc) != season.roster_commit {
            return Err(ChainError::RosterMismatch.into());
        }
        season.roster_blind = blind;
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
        return Err(ProgramError::NotEnoughAccountKeys);
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
