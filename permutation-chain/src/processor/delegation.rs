//! The Ephemeral Rollup lifecycle: delegating the world and nation
//! accounts, committing them back to the base layer and undelegating them.
//!
//! * `Delegate` takes each target once (`Season::delegated`), world chunk 0
//!   last, to the validator the season was created with.
//! * `CommitPart` (the crank's) and `UndelegatePart` (anyone's, after the
//!   last tick) carry one world chunk or 1..=3 nations per intent: larger
//!   intents were measured on devnet to exceed the committor's limits.
//! * `UndelegatePart` follows `undelegation_order` (chunks 1..19, the
//!   nations, chunk 0 last), counted in chunk 0's header, one step per
//!   transaction; a target that already left the ER is skipped.

use ephemeral_rollups_sdk::{
    consts::{DELEGATION_PROGRAM_ID, MAGIC_CONTEXT_ID, MAGIC_PROGRAM_ID},
    cpi::{delegate_account, undelegate_account, DelegateAccounts, DelegateConfig},
    ephem::{FoldableIntentBuilder, MagicIntentBundleBuilder},
};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};
use solana_system_interface::program as system_program;

use super::accounts::*;
use super::ER_COMMIT_FREQUENCY_MS;
use crate::error::ChainError;
use crate::state::*;

pub(super) fn delegate(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    target: u16,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let authority = next_account_info(it)?;
    let system = next_account_info(it)?;
    let season_info = next_account_info(it)?;
    let pda = next_account_info(it)?;
    let owner_program = next_account_info(it)?;
    let buffer = next_account_info(it)?;
    let record = next_account_info(it)?;
    let metadata = next_account_info(it)?;
    let delegation_program = next_account_info(it)?;
    let validator = next_account_info(it)?;
    signer(authority)?;
    let mut season = load_season(program_id, season_info)?;
    if season.status != SeasonStatus::Running {
        return Err(ChainError::WrongStatus.into());
    }
    require_operator(&season, authority)?;
    require_rules(&season)?;
    if owner_program.key != program_id || system.key != &system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    if delegation_program.key != &DELEGATION_PROGRAM_ID {
        return Err(ChainError::WrongDelegationProgram.into());
    }
    let id = season.season_id.to_le_bytes();
    let civ_bytes;
    let chunk;
    let seeds: Vec<&[u8]> = if (target as usize) < WORLD_CHUNKS {
        chunk = [target as u8];
        vec![WORLD_SEED, &id, &chunk]
    } else if target >= NATION_TARGET && target - NATION_TARGET < season.nations as u16 {
        civ_bytes = (target - NATION_TARGET).to_le_bytes();
        vec![NATION_SEED, &id, &civ_bytes]
    } else {
        return Err(ChainError::InvalidParams.into());
    };
    expect_pda(program_id, pda, &seeds)?;
    let bit = target_bit(target).ok_or(ChainError::InvalidParams)?;
    // Once only: a target that came back must never go to the ER again (a
    // chunk sent back after chunk 0 left would be stranded, and one sent
    // back after the season would hold FinishSeason up).
    if season.delegated & bit != 0 {
        return Err(ChainError::AlreadyDelegated.into());
    }
    // Chunk 0 last: while it is on base, nothing can play on the ER.
    if target == 0 && (season.delegated | 1) != all_targets(season.nations) {
        return Err(ChainError::DelegationOrder.into());
    }
    // The validator the season was created with (the ER trust root it named).
    if validator.key.as_ref() != season.validator {
        return Err(ChainError::WrongValidator.into());
    }
    season.delegated |= bit;
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
    delegate_account(
        DelegateAccounts {
            payer: authority,
            pda,
            owner_program,
            buffer,
            delegation_record: record,
            delegation_metadata: metadata,
            delegation_program,
            system_program: system,
        },
        &seeds,
        DelegateConfig {
            commit_frequency_ms: ER_COMMIT_FREQUENCY_MS,
            validator: Some(*validator.key),
        },
    )?;
    msg!("PS delegated {} target {}", pda.key, target);
    Ok(())
}

/// What the delegation program may hand back to one of our PDAs (`seeds`):
/// a world chunk of full size (chunk 0 with a world or genesis header), or a
/// nation account. The magic *families* are accepted, not this build's
/// versions, so an account delegated under an older layout still comes
/// home; the instructions that read it refuse it (exact magic). Only helps
/// intents scheduled before an upgrade: anything older is recovered by
/// `RollbackUndelegation`.
pub fn undelegated_content_ok(seeds: &[&[u8]], data: &[u8]) -> bool {
    match seeds.first().copied() {
        Some(s) if s == WORLD_SEED => {
            data.len() == CHUNK
                && (seeds.get(2) != Some(&&[0u8][..])
                    || data.starts_with(WORLD_FAMILY)
                    || data.starts_with(GENESIS_FAMILY))
        }
        Some(s) if s == NATION_SEED => data.starts_with(NATION_FAMILY),
        _ => false,
    }
}

pub(super) fn undelegate_callback(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    seeds: Vec<Vec<u8>>,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let pda = next_account_info(it)?;
    let buffer = next_account_info(it)?;
    let payer = next_account_info(it)?;
    let system = next_account_info(it)?;
    if buffer.owner != &DELEGATION_PROGRAM_ID {
        return Err(ChainError::WrongDelegationProgram.into());
    }
    // Only our own PDAs, with the content they are supposed to hold.
    let refs: Vec<&[u8]> = seeds.iter().map(Vec::as_slice).collect();
    expect_pda(program_id, pda, &refs)?;
    if !undelegated_content_ok(&refs, &buffer.try_borrow_data()?) {
        return Err(ChainError::WrongPda.into());
    }
    undelegate_account(pda, program_id, buffer, payer, system, seeds)?;
    msg!("PS undelegated {}", pda.key);
    Ok(())
}

/// The shape of one intent: one world chunk alone, or 1..=3 distinct nation
/// accounts. Larger intents were measured on devnet to exceed the
/// committor's limits (64 keys, finalize compute) and leave accounts stuck
/// mid-undelegation.
fn check_intent_shape(targets: &[u16]) -> ProgramResult {
    let chunks = targets
        .iter()
        .filter(|&&t| (t as usize) < WORLD_CHUNKS)
        .count();
    let ok = match chunks {
        0 => (1..=MAX_NATIONS_PER_INTENT).contains(&targets.len()),
        1 => targets.len() == 1,
        _ => false,
    };
    let distinct = targets
        .iter()
        .enumerate()
        .all(|(i, t)| !targets[..i].contains(t));
    if ok && distinct {
        Ok(())
    } else {
        Err(ChainError::InvalidParams.into())
    }
}

/// Commit (the crank, during play and right after the last tick) or commit
/// and undelegate (anyone, after the last tick, in `undelegation_order`) the
/// `targets` in one small intent.
pub(super) fn commit_part(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    targets: Vec<u16>,
    undelegate: bool,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let payer = next_account_info(it)?;
    let magic_program = next_account_info(it)?;
    let magic_context = next_account_info(it)?;
    let chunk0 = next_account_info(it)?;
    signer(payer)?;
    if magic_program.key != &MAGIC_PROGRAM_ID || magic_context.key != &MAGIC_CONTEXT_ID {
        return Err(ChainError::WrongMagicProgram.into());
    }
    check_intent_shape(&targets)?;
    // Chunk 0's header says whether the season is over, and how far the
    // undelegation got. It is never skipped: the counter lives there.
    let season_id = world_season_id(chunk0)?;
    let id = season_id.to_le_bytes();
    if chunk0.owner != program_id || chunk0.data_len() != CHUNK {
        return Err(ChainError::WrongWorld.into());
    }
    expect_pda(program_id, chunk0, &[WORLD_SEED, &id, &[0]])?;
    let mut meta = WorldMeta::from_chunk0(&chunk0.try_borrow_data()?)?;
    if undelegate {
        // Anyone may wind a finished season up, but only in the fixed order,
        // so no intent can depend on an account that already left the ER.
        if !meta.finished {
            return Err(ChainError::SeasonNotOver.into());
        }
        let order = undelegation_order(meta.civs);
        let at = meta.undelegated as usize;
        if order.get(at..at + targets.len()) != Some(&targets[..]) {
            return Err(ChainError::UndelegationOrder.into());
        }
    } else {
        // Commits are the crank's: during play, and once more right after
        // the last tick (the final state on base before the wind-up, which
        // a rollback restores); none once the wind-up began (its nations
        // may be gone, so this comes before the crank's nation is read).
        if meta.finished && meta.undelegated != 0 {
            return Err(ChainError::WrongPhase.into());
        }
        require_crank(program_id, next_account_info(it)?, season_id, payer)?;
    }
    let mut list: Vec<AccountInfo> = Vec::with_capacity(targets.len());
    for &t in &targets {
        if t == 0 {
            list.push(chunk0.clone());
            continue;
        }
        let info = next_account_info(it)?;
        // A target that already left the ER out of order (the validator
        // undelegated it: only the delegation program owns it here now) is
        // skipped, not scheduled: nothing can schedule it any more, and the
        // rest of the wind-up must not wait for it. Only the Magic program
        // (for this program's own intents, which are in order) or the
        // validator can make one of our PDAs delegation-owned on the ER, so
        // nobody can skip a target that is still delegated.
        let gone = undelegate && info.owner == &DELEGATION_PROGRAM_ID;
        if (t as usize) < WORLD_CHUNKS {
            if info.owner != program_id && !gone {
                return Err(ChainError::WrongWorld.into());
            }
            expect_pda(program_id, info, &[WORLD_SEED, &id, &[t as u8]])?;
        } else {
            let civ = t
                .checked_sub(NATION_TARGET)
                .filter(|c| *c < meta.civs as u16)
                .ok_or(ChainError::InvalidParams)?;
            if gone {
                expect_pda(program_id, info, &[NATION_SEED, &id, &civ.to_le_bytes()])?;
            } else {
                // The fixed-size front only: no heap, however full the inbox.
                let head = load_nation_head_at(program_id, info)?;
                if head.season_id != season_id || head.civ != civ {
                    return Err(ChainError::MissingNation.into());
                }
                if undelegate {
                    // Nothing of the open tick matters after the last one:
                    // the account goes back as small as it gets (no batches,
                    // no inbox, no stale bytes) and frozen, whatever was
                    // submitted after the end.
                    store(&mut info.try_borrow_mut_data()?, &head.cleared())?;
                }
            }
        }
        if gone {
            msg!("PS skip {} (already left the ER)", t);
        } else {
            list.push(info.clone());
        }
    }
    if undelegate {
        // One step per transaction: every intent stays its own, whatever the
        // committor does with several intents of one transaction.
        require_alone(program_id, it.next())?;
        meta.undelegated += targets.len() as u8;
        meta.write_chunk0(&mut chunk0.try_borrow_mut_data()?)?;
    }
    if !list.is_empty() {
        let builder = MagicIntentBundleBuilder::new(
            payer.clone(),
            magic_context.clone(),
            magic_program.clone(),
        );
        if undelegate {
            builder.commit_and_undelegate(&list).build_and_invoke()?;
        } else {
            builder.commit(&list).build_and_invoke()?;
        }
    }
    msg!(
        "PS {} {:?} season {}",
        if undelegate { "undelegate" } else { "commit" },
        targets,
        season_id
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(magic: &[u8]) -> Vec<u8> {
        let mut d = vec![0u8; CHUNK];
        d[..magic.len()].copy_from_slice(magic);
        d
    }

    /// The callback accepts the magic families, so an account scheduled
    /// under an older layout still comes home (WP15 test 5).
    #[test]
    fn undelegated_content_ok_accepts_older_magics() {
        let id = 7u64.to_le_bytes();
        let world = |k: u8| vec![WORLD_SEED.to_vec(), id.to_vec(), vec![k]];
        let nation = vec![
            NATION_SEED.to_vec(),
            id.to_vec(),
            1u16.to_le_bytes().to_vec(),
        ];
        let ok = |seeds: &Vec<Vec<u8>>, data: &[u8]| {
            let refs: Vec<&[u8]> = seeds.iter().map(Vec::as_slice).collect();
            undelegated_content_ok(&refs, data)
        };
        for magic in [&b"PSWORLD5"[..], b"PSWORLD6", b"PSGENJB1", b"PSGENJB2"] {
            assert!(ok(&world(0), &chunk(magic)), "chunk 0 {magic:?}");
        }
        for magic in [&b"PSNATN06"[..], b"PSNATN07", b"PSNATN08"] {
            assert!(ok(&nation, &chunk(magic)), "nation {magic:?}");
            assert!(ok(&nation, magic), "a nation of any length");
        }
        // Continuation chunks hold raw body bytes.
        assert!(ok(&world(3), &vec![0xa5; CHUNK]));
        // Refused: chunk 0 with other bytes, a chunk of the wrong length, a
        // nation holding a season, unknown seeds.
        assert!(!ok(&world(0), &vec![7; CHUNK]));
        assert!(!ok(&world(0), &chunk(b"PSSEASN8")));
        assert!(!ok(&world(0), &chunk(b"PSWORLD6")[..CHUNK - 1]));
        assert!(!ok(&world(3), &vec![0; CHUNK + 1]));
        assert!(!ok(&nation, &chunk(b"PSSEASN8")));
        assert!(!ok(&nation, b"PSNAT"));
        let season = vec![SEASON_SEED.to_vec(), id.to_vec()];
        assert!(!ok(&season, &chunk(b"PSSEASN8")));
        assert!(!ok(&vec![], &chunk(b"PSWORLD6")));
    }

    #[test]
    fn intent_shapes() {
        let n = NATION_TARGET;
        for ok in [&[0][..], &[1], &[19], &[n], &[n + 5, n, n + 2]] {
            assert_eq!(check_intent_shape(ok), Ok(()), "{ok:?}");
        }
        for bad in [
            &[][..],
            &[1, 2],
            &[0, 1],
            &[1, n],
            &[n, n],
            &[n, n + 1, n + 2, n + 3],
        ] {
            assert_eq!(
                check_intent_shape(bad),
                Err(ChainError::InvalidParams.into()),
                "{bad:?}"
            );
        }
    }
}
