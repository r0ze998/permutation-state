//! The wind-up after the last tick (WP02; contract §1.3 tags 8, 9, 12, 20).
//!
//! Undelegation is open to anyone, but only in small intents (one world
//! chunk, or up to three nations), only in the fixed order
//! `undelegation_order` (chunks 1..19, the nations, chunk 0 last) and one
//! step per transaction; the whole-season intents (tags 8 and 9) are
//! retired; `CommitPart` has the same shape limit and stops once the
//! wind-up began; a target that already left the ER out of order is
//! skipped. These flip the audit repros `undelegate_open_to_anyone.rs`
//! (wf-60, wf-27) and `undelegation_intent_shape.rs` (wf-30).
//!
//! Native `process`: off chain the Magic program CPI is the default syscall
//! stub, which returns Ok, so `Ok(())` means every check of this program
//! passed (the LiteSVM suite, `svm-tests/tests/undelegation.rs`, records the
//! intents themselves). The ER's view after an intent (the scheduled
//! accounts owned by the delegation program) is simulated by changing their
//! owner.

use ephemeral_rollups_sdk::consts::{DELEGATION_PROGRAM_ID, MAGIC_CONTEXT_ID, MAGIC_PROGRAM_ID};
use permutation_chain::error::ChainError;
use permutation_chain::instruction::ChainInstruction;
use permutation_chain::processor::{process, NATION_TARGET};
use permutation_chain::state::*;
use permutation_rules::gov::{GovAction, GovEntry, Role};
#[allow(deprecated)]
use solana_program::sysvar::instructions::{construct_instructions_data, BorrowedInstruction};
use solana_program::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey};

const COMPUTE_BUDGET: Pubkey =
    solana_program::pubkey!("ComputeBudget111111111111111111111111111111");

/// The Instructions sysvar of a transaction whose top-level instructions go
/// to `programs` (no accounts, no data: only the program ids are checked),
/// executing index `current`.
#[allow(deprecated)]
fn sysvar_data(programs: &[Pubkey], current: u16) -> Vec<u8> {
    let borrowed: Vec<BorrowedInstruction> = programs
        .iter()
        .map(|p| BorrowedInstruction {
            program_id: p,
            accounts: Vec::new(),
            data: &[],
        })
        .collect();
    let mut d = construct_instructions_data(&borrowed);
    let n = d.len();
    d[n - 2..].copy_from_slice(&current.to_le_bytes());
    d
}

const ID: u64 = 7;

struct Acc {
    key: Pubkey,
    owner: Pubkey,
    lamports: u64,
    data: Vec<u8>,
}

fn acc(key: Pubkey, owner: Pubkey, data: Vec<u8>) -> Acc {
    Acc {
        key,
        owner,
        lamports: 1_000_000,
        data,
    }
}

/// A season's 20 world chunks and its nation accounts, as the ER holds them
/// (program-owned while delegated).
struct Env {
    program: Pubkey,
    crank: Pubkey,
    civs: u16,
    world: Vec<Acc>,
    nations: Vec<Acc>,
}

impl Env {
    fn new(civs: u16, finished: bool) -> Self {
        let program = Pubkey::new_unique();
        let crank = Pubkey::new_unique();
        let id = ID.to_le_bytes();
        let world = (0..WORLD_CHUNKS as u8)
            .map(|k| {
                let (pda, _) = Pubkey::find_program_address(&[WORLD_SEED, &id, &[k]], &program);
                let mut d = vec![0u8; CHUNK];
                if k == 0 {
                    d[..8].copy_from_slice(&WORLD_MAGIC);
                    let meta = WorldMeta {
                        season_id: ID,
                        civs: civs as u8,
                        tick_seconds: 30,
                        finished,
                        ..Default::default()
                    };
                    meta.write_chunk0(&mut d).unwrap();
                }
                acc(pda, program, d)
            })
            .collect();
        let nations = (0..civs)
            .map(|c| {
                let (pda, bump) =
                    Pubkey::find_program_address(&[NATION_SEED, &id, &c.to_le_bytes()], &program);
                let mut d = vec![0u8; NATION_SPACE];
                store(
                    &mut d,
                    &NationAccount::new(ID, c, bump, 0, true, crank.to_bytes()),
                )
                .unwrap();
                acc(pda, program, d)
            })
            .collect();
        Env {
            program,
            crank,
            civs,
            world,
            nations,
        }
    }

    fn meta(&self) -> WorldMeta {
        WorldMeta::from_chunk0(&self.world[0].data).unwrap()
    }

    fn target(&mut self, t: u16) -> &mut Acc {
        if (t as usize) < WORLD_CHUNKS {
            &mut self.world[t as usize]
        } else {
            &mut self.nations[(t - NATION_TARGET) as usize]
        }
    }

    /// Run `ix` with `payer` signing, then the Magic program and context,
    /// then `tail` (world chunk k = k, nation civ = NATION_TARGET + civ),
    /// then the Instructions sysvar with `sysvar` as its data (if any).
    fn run_with(
        &mut self,
        payer: Pubkey,
        tail: &[u16],
        ix: ChainInstruction,
        sysvar: Option<Vec<u8>>,
    ) -> Result<(), ProgramError> {
        let data = borsh::to_vec(&ix).unwrap();
        let program = self.program;
        let (mut l0, mut l1, mut l2) = (1u64, 1u64, 1u64);
        let (mut d0, mut d1, mut d2) = (Vec::<u8>::new(), Vec::<u8>::new(), vec![0u8; 64]);
        let (magic, ctx, system) = (MAGIC_PROGRAM_ID, MAGIC_CONTEXT_ID, Pubkey::default());
        let mut infos = vec![
            AccountInfo::new(&payer, true, true, &mut l0, &mut d0, &system, false),
            AccountInfo::new(&magic, false, false, &mut l1, &mut d1, &system, true),
            AccountInfo::new(&ctx, false, true, &mut l2, &mut d2, &magic, false),
        ];
        // Hand out each account once (AccountInfo borrows mutably).
        let mut world: Vec<Option<&mut Acc>> = self.world.iter_mut().map(Some).collect();
        let mut nations: Vec<Option<&mut Acc>> = self.nations.iter_mut().map(Some).collect();
        for &t in tail {
            let slot = if (t as usize) < WORLD_CHUNKS {
                &mut world[t as usize]
            } else {
                &mut nations[(t - NATION_TARGET) as usize]
            };
            let a = slot.take().expect("each account once per call");
            infos.push(AccountInfo::new(
                &a.key,
                false,
                true,
                &mut a.lamports,
                &mut a.data,
                &a.owner,
                false,
            ));
        }
        let sysvar_key = solana_program::sysvar::instructions::ID;
        let (mut l3, mut d3) = (0u64, sysvar.unwrap_or_default());
        if !d3.is_empty() {
            infos.push(AccountInfo::new(
                &sysvar_key,
                false,
                false,
                &mut l3,
                &mut d3,
                &system,
                false,
            ));
        }
        process(&program, &infos, &data)
    }

    fn run(
        &mut self,
        payer: Pubkey,
        tail: &[u16],
        ix: ChainInstruction,
    ) -> Result<(), ProgramError> {
        self.run_with(payer, tail, ix, None)
    }

    /// `undelegatePart`: chunk 0, then the targets except 0, then the
    /// Instructions sysvar (alone in its transaction).
    fn undelegate(&mut self, payer: Pubkey, targets: &[u16]) -> Result<(), ProgramError> {
        let program = self.program;
        self.undelegate_in(payer, targets, Some(sysvar_data(&[program], 0)))
    }

    fn undelegate_in(
        &mut self,
        payer: Pubkey,
        targets: &[u16],
        sysvar: Option<Vec<u8>>,
    ) -> Result<(), ProgramError> {
        let mut tail = vec![0u16];
        tail.extend(targets.iter().copied().filter(|&t| t != 0));
        self.run_with(
            payer,
            &tail,
            ChainInstruction::UndelegatePart {
                targets: targets.to_vec(),
            },
            sysvar,
        )
    }

    /// `commitPart`: chunk 0, a nation that records the crank (one that is
    /// not a target here), then the targets except 0.
    fn commit(&mut self, payer: Pubkey, targets: &[u16]) -> Result<(), ProgramError> {
        let witness = (0..self.civs)
            .map(|c| NATION_TARGET + c)
            .find(|t| !targets.contains(t))
            .unwrap();
        let mut tail = vec![0u16, witness];
        tail.extend(targets.iter().copied().filter(|&t| t != 0));
        self.run(
            payer,
            &tail,
            ChainInstruction::CommitPart {
                targets: targets.to_vec(),
            },
        )
    }

    /// The ER after an intent: its accounts belong to the delegation program.
    fn scheduled(&mut self, targets: &[u16]) {
        for &t in targets {
            self.target(t).owner = DELEGATION_PROGRAM_ID;
        }
    }

    fn all(&self) -> Vec<u16> {
        (0..WORLD_CHUNKS as u16)
            .chain((0..self.civs).map(|c| NATION_TARGET + c))
            .collect()
    }
}

fn err(e: ChainError) -> Result<(), ProgramError> {
    Err(e.into())
}

/// The crank's next group of `order` from `at`: a world chunk alone, or up
/// to three nations.
fn next_group(order: &[u16], at: usize) -> Vec<u16> {
    if (order[at] as usize) < WORLD_CHUNKS {
        return vec![order[at]];
    }
    order[at..]
        .iter()
        .take(MAX_NATIONS_PER_INTENT)
        .take_while(|&&t| t >= NATION_TARGET)
        .copied()
        .collect()
}

/// wf-60 `a_stranger_commits_and_undelegates_all_26…`, wf-27
/// `stranger_commit_and_undelegate_everything`: tags 8 and 9 are retired for
/// everyone, before and after the last tick.
#[test]
fn whole_season_intents_are_retired() {
    for finished in [false, true] {
        let mut env = Env::new(6, finished);
        let all = env.all();
        for payer in [Pubkey::new_unique(), env.crank] {
            for ix in [
                ChainInstruction::CommitAndUndelegate,
                ChainInstruction::Commit,
            ] {
                assert_eq!(env.run(payer, &all, ix), err(ChainError::Retired));
            }
        }
        assert_eq!(env.meta().undelegated, 0);
    }
}

/// wf-60 `a_stranger_undelegates_26_targets…`, wf-27
/// `stranger_undelegate_part_all_26_accounts…`, wf-30: no intent beyond
/// the measured-safe shape, from anyone, the crank included.
#[test]
fn oversized_undelegation_intents_are_refused_for_everyone() {
    let mut env = Env::new(6, true);
    let crank = env.crank;
    let mut all26: Vec<u16> = (1..WORLD_CHUNKS as u16)
        .chain((0..6).map(|c| NATION_TARGET + c))
        .collect();
    all26.push(0);
    for payer in [Pubkey::new_unique(), crank] {
        assert_eq!(
            env.undelegate(payer, &all26),
            err(ChainError::InvalidParams),
            "26 targets"
        );
        for (targets, what) in [
            (&[1, 2, 3][..], "three chunks"),
            (&[1, 2, 3, 4, 5], "five chunks"),
            (&[1, NATION_TARGET], "a chunk with a nation"),
            (&[0, 1], "chunk 0 with another chunk"),
            (
                &[
                    NATION_TARGET,
                    NATION_TARGET + 1,
                    NATION_TARGET + 2,
                    NATION_TARGET + 3,
                ],
                "four nations",
            ),
            (&[], "empty"),
        ] {
            assert_eq!(
                env.undelegate(payer, targets),
                err(ChainError::InvalidParams),
                "{what}"
            );
        }
        let dup = ChainInstruction::UndelegatePart {
            targets: vec![NATION_TARGET, NATION_TARGET],
        };
        assert_eq!(
            env.run(payer, &[0, NATION_TARGET], dup),
            err(ChainError::InvalidParams),
            "duplicates"
        );
    }
    assert_eq!(env.meta().undelegated, 0, "nothing was scheduled");
}

/// wf-60 `undelegating_chunk_0_first_blocks_every_remaining_group`: chunk 0
/// (or a nation, or chunk 2) first is refused; chunk 1 is the first step.
#[test]
fn chunk_0_first_and_nations_first_are_refused() {
    let mut env = Env::new(6, true);
    let stranger = Pubkey::new_unique();
    for first in [&[0][..], &[NATION_TARGET], &[2], &[19]] {
        assert_eq!(
            env.undelegate(stranger, first),
            err(ChainError::UndelegationOrder),
            "{first:?}"
        );
    }
    // The order is checked before the sysvar.
    assert_eq!(
        env.undelegate_in(stranger, &[0], None),
        err(ChainError::UndelegationOrder)
    );
    assert_eq!(env.undelegate(stranger, &[1]), Ok(()));
    assert_eq!(env.meta().undelegated, 1);
}

#[test]
fn before_the_last_tick_nothing_undelegates() {
    let mut env = Env::new(6, false);
    for payer in [Pubkey::new_unique(), env.crank] {
        assert_eq!(env.undelegate(payer, &[1]), err(ChainError::SeasonNotOver));
    }
    assert_eq!(env.meta().undelegated, 0);
}

/// Anyone may wind the season up, in order; whoever sends a step, the next
/// one continues from chunk 0's counter, and chunk 0 goes last.
#[test]
fn a_stranger_and_the_crank_interleave_and_the_season_comes_back_whole() {
    for civs in 2u16..=6 {
        let mut env = Env::new(civs, true);
        let order = undelegation_order(civs as u8);
        assert_eq!(order.len(), WORLD_CHUNKS + civs as usize);
        let stranger = Pubkey::new_unique();
        let crank = env.crank;
        let (mut at, mut step) = (0, 0);
        while at < order.len() {
            let group = next_group(&order, at);
            let payer = if step % 3 == 1 { stranger } else { crank };
            // Chunk 0 is refused until everything else is scheduled.
            if at < order.len() - 1 {
                assert_eq!(
                    env.undelegate(stranger, &[0]),
                    err(ChainError::UndelegationOrder),
                    "civs {civs} at {at}"
                );
            }
            assert_eq!(
                env.undelegate(payer, &group),
                Ok(()),
                "civs {civs} group {group:?}"
            );
            // The loser of a race gets a clear error, not a scheduled intent.
            assert_eq!(
                env.undelegate(crank, &group),
                err(ChainError::UndelegationOrder),
                "civs {civs} replay {group:?}"
            );
            at += group.len();
            assert_eq!(env.meta().undelegated as usize, at);
            if group != [0] {
                env.scheduled(&group);
            }
            step += 1;
        }
        // 19 chunks, the nations in threes, chunk 0.
        assert_eq!(step, 19 + (civs as usize).div_ceil(3) + 1);
        assert_eq!(env.meta().undelegated as usize, order.len());
        assert!(env.meta().finished);
    }
}

#[test]
fn a_repeated_group_is_refused_with_undelegation_order() {
    let mut env = Env::new(6, true);
    let crank = env.crank;
    assert_eq!(env.undelegate(Pubkey::new_unique(), &[1]), Ok(()));
    env.scheduled(&[1]);
    assert_eq!(
        env.undelegate(crank, &[1]),
        err(ChainError::UndelegationOrder)
    );
    assert_eq!(env.undelegate(crank, &[2]), Ok(()));
}

/// A stuffed inbox (what SubmitGov after the last tick could leave), a roll,
/// an open tick and stale bytes past the value: every nation leaves cleared,
/// frozen and zero-padded, read head-only (no heap however full the inbox).
#[test]
fn nations_leave_without_their_inbox_or_stale_bytes() {
    let mut env = Env::new(3, true);
    for c in 0..3usize {
        let mut n: NationAccount = load(&env.nations[c].data).unwrap();
        n.open_tick = 179;
        n.submitted = [179; 4];
        n.committed = [179; 4];
        n.commits = [[c as u8 + 1; 32]; 4];
        n.gov_quota = 5;
        n.roll = (0..20)
            .map(|m| RollSeat {
                member: m,
                key8: [m as u8; 8],
            })
            .collect();
        n.inbox = (0..60)
            .map(|i| GovEntry {
                member: i,
                signer: [i as u8; 32],
                action: GovAction::Vote {
                    role: Role::Steward,
                    candidate: i,
                },
            })
            .collect();
        store(&mut env.nations[c].data, &n).unwrap();
        let len = borsh::to_vec(&n).unwrap().len();
        for (i, b) in env.nations[c].data[len..].iter_mut().enumerate() {
            *b = (i % 251) as u8 | 1;
        }
    }
    let crank = env.crank;
    for k in 1..WORLD_CHUNKS as u16 {
        assert_eq!(env.undelegate(crank, &[k]), Ok(()));
        env.scheduled(&[k]);
    }
    let nations = [NATION_TARGET, NATION_TARGET + 1, NATION_TARGET + 2];
    assert_eq!(&undelegation_order(3)[19..22], &nations);
    assert_eq!(env.undelegate(crank, &nations), Ok(()));
    for c in 0..3usize {
        let n: NationAccount = load(&env.nations[c].data).unwrap();
        assert!(n.inbox.is_empty() && n.roll.is_empty());
        assert!(n.batches.iter().all(Option::is_none));
        assert!(n.frozen, "frozen on base too");
        assert_eq!(
            n.submitted, [NO_TICK; 4],
            "the open tick's data is forgotten"
        );
        assert_eq!((n.commits, n.committed), ([[0; 32]; 4], [NO_TICK; 4]));
        assert_eq!(n.open_tick, 179);
        assert_eq!((n.season_id, n.civ, n.gov_quota), (ID, c as u16, 5));
        let len = borsh::to_vec(&n).unwrap().len();
        assert!(
            env.nations[c].data[len..].iter().all(|&b| b == 0),
            "no stale bytes"
        );
        assert_eq!(&env.nations[c].data[..8], &NATION_MAGIC);
    }
}

/// wf-27 `control_commit_part_is_crank_gated`, extended: `CommitPart` keeps
/// the small shape, is the crank's, and stops once the wind-up began.
#[test]
fn commit_part_keeps_the_small_shape_and_stops_at_the_last_tick() {
    let mut playing = Env::new(6, false);
    let crank = playing.crank;
    let n = NATION_TARGET;
    assert_eq!(
        playing.commit(Pubkey::new_unique(), &[1]),
        err(ChainError::Unauthorized)
    );
    for ok in [&[1][..], &[0], &[n, n + 1, n + 2], &[n + 3, n + 5]] {
        assert_eq!(playing.commit(crank, ok), Ok(()), "{ok:?}");
    }
    for bad in [
        &[1, 2][..],
        &[0, 1],
        &[1, n],
        &[n + 1, n + 2, n + 3, n + 4],
        &[],
    ] {
        assert_eq!(
            playing.commit(crank, bad),
            err(ChainError::InvalidParams),
            "{bad:?}"
        );
    }
    // A chunk that is not delegated (never skipped in a commit).
    playing.world[5].owner = DELEGATION_PROGRAM_ID;
    assert_eq!(playing.commit(crank, &[5]), err(ChainError::WrongWorld));
    // Right after the last tick the crank may still commit (the final state
    // on base, which a rollback would restore); a stranger may not; once the
    // wind-up began nobody may.
    let mut done = Env::new(6, true);
    let crank = done.crank;
    assert_eq!(
        done.commit(Pubkey::new_unique(), &[1]),
        err(ChainError::Unauthorized)
    );
    assert_eq!(done.commit(crank, &[1]), Ok(()));
    assert_eq!(done.commit(crank, &[n + 1, n + 2, n + 3]), Ok(()));
    assert_eq!(done.commit(crank, &[0]), Ok(()));
    assert_eq!(done.meta().undelegated, 0, "commits count nothing");
    assert_eq!(done.undelegate(Pubkey::new_unique(), &[1]), Ok(()));
    done.scheduled(&[1]);
    for payer in [crank, Pubkey::new_unique()] {
        assert_eq!(done.commit(payer, &[2]), err(ChainError::WrongPhase));
        assert_eq!(done.commit(payer, &[0]), err(ChainError::WrongPhase));
    }
}

/// A target that left the ER out of band (the validator undelegated it; on
/// the ER the delegation program owns it) is skipped, so the rest of the
/// wind-up, chunk 0 included, still completes; nothing else can be skipped.
#[test]
fn a_target_that_left_out_of_order_is_skipped_and_the_rest_completes() {
    for gone in [
        vec![NATION_TARGET + 3],
        vec![7],
        vec![7, NATION_TARGET, NATION_TARGET + 5, 19],
        vec![NATION_TARGET, NATION_TARGET + 1, NATION_TARGET + 2],
    ] {
        let mut env = Env::new(6, true);
        let before: Vec<Vec<u8>> = gone.iter().map(|&t| env.target(t).data.clone()).collect();
        let stranger = Pubkey::new_unique();
        env.scheduled(&gone);
        let order = undelegation_order(6);
        let mut at = 0;
        while at < order.len() {
            let group = next_group(&order, at);
            assert_eq!(
                env.undelegate(stranger, &group),
                Ok(()),
                "gone {gone:?} group {group:?}"
            );
            at += group.len();
            assert_eq!(env.meta().undelegated as usize, at, "skips count");
            if group != [0] {
                env.scheduled(&group);
            }
        }
        assert_eq!(env.meta().undelegated as usize, order.len());
        // A skipped account was not touched (not rewritten, not cleared).
        for (i, &t) in gone.iter().enumerate() {
            assert_eq!(env.target(t).data, before[i], "skipped {t} untouched");
        }
    }
}

#[test]
fn chunk_0_is_never_skipped_and_a_skip_needs_the_right_account() {
    let mut env = Env::new(3, true);
    // Chunk 0 gone from the ER: nothing can run (the counter lives there).
    env.world[0].owner = DELEGATION_PROGRAM_ID;
    assert_eq!(
        env.undelegate(Pubkey::new_unique(), &[1]),
        err(ChainError::WrongWorld)
    );
    let mut env = Env::new(3, true);
    // A delegation-owned impostor in chunk 1's place is not chunk 1.
    let real = env.world[1].key;
    env.world[1].key = Pubkey::new_unique();
    env.world[1].owner = DELEGATION_PROGRAM_ID;
    assert_eq!(
        env.undelegate(Pubkey::new_unique(), &[1]),
        err(ChainError::WrongPda)
    );
    env.world[1].key = real;
    // A nation impostor likewise.
    for k in 1..WORLD_CHUNKS as u16 {
        assert_eq!(env.undelegate(env.crank, &[k]), Ok(()));
        env.scheduled(&[k]);
    }
    env.nations[1].key = Pubkey::new_unique();
    env.nations[1].owner = DELEGATION_PROGRAM_ID;
    let nations = [NATION_TARGET, NATION_TARGET + 1, NATION_TARGET + 2];
    assert_eq!(
        env.undelegate(env.crank, &nations),
        err(ChainError::WrongPda)
    );
    assert_eq!(env.meta().undelegated, 19, "the step did not count");
    // Owned by some other program: neither delegated here nor gone.
    let mut env = Env::new(3, true);
    env.world[1].owner = Pubkey::new_unique();
    assert_eq!(
        env.undelegate(Pubkey::new_unique(), &[1]),
        err(ChainError::WrongWorld)
    );
    let mut env = Env::new(3, true);
    for k in 1..WORLD_CHUNKS as u16 {
        env.undelegate(env.crank, &[k]).unwrap();
        env.scheduled(&[k]);
    }
    env.nations[0].owner = Pubkey::new_unique();
    assert_eq!(
        env.undelegate(env.crank, &nations),
        err(ChainError::MissingNation)
    );
}

/// wf-30 control, as fixed: the crank's groups need the sysvar and must be
/// alone in their transaction (besides compute-budget instructions).
#[test]
fn one_undelegation_step_per_transaction() {
    let mut env = Env::new(3, true);
    let (program, stranger) = (env.program, Pubkey::new_unique());
    for (sysvar, what) in [
        (None, "no sysvar"),
        (
            Some(sysvar_data(&[program, program], 0)),
            "two steps, first",
        ),
        (
            Some(sysvar_data(&[program, program], 1)),
            "two steps, second",
        ),
        (
            Some(sysvar_data(&[program, Pubkey::new_unique()], 0)),
            "with another program",
        ),
        (
            Some(sysvar_data(&[Pubkey::new_unique()], 0)),
            "as a CPI of another program",
        ),
    ] {
        assert_eq!(
            env.undelegate_in(stranger, &[1], sysvar),
            err(ChainError::NotAlone),
            "{what}"
        );
    }
    assert_eq!(env.meta().undelegated, 0);
    assert_eq!(
        env.undelegate_in(
            stranger,
            &[1],
            Some(sysvar_data(&[COMPUTE_BUDGET, COMPUTE_BUDGET, program], 2))
        ),
        Ok(()),
        "compute budget is fine"
    );
    assert_eq!(env.meta().undelegated, 1);
}

#[test]
fn undelegation_order_is_chunks_then_nations_then_chunk_0() {
    let o = undelegation_order(6);
    assert_eq!(o.len(), 26);
    assert_eq!(&o[..19], &(1..20).collect::<Vec<u16>>()[..]);
    assert_eq!(&o[19..25], &[1000, 1001, 1002, 1003, 1004, 1005]);
    assert_eq!(o[25], 0);
    let mut sorted = o.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 26);
}

#[test]
fn undelegation_position_matches_the_order() {
    for civs in 1..=8u8 {
        let order = undelegation_order(civs);
        for (i, &t) in order.iter().enumerate() {
            assert_eq!(undelegation_position(civs, t), Some(i));
        }
        assert_eq!(undelegation_position(civs, 20), None);
        assert_eq!(
            undelegation_position(civs, NATION_TARGET + civs as u16),
            None
        );
    }
}
