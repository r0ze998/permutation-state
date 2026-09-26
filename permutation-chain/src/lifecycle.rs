//! Liveness escape hatches (WP14): when a season may be aborted, and what
//! members and the operator get back after an abort, as pure functions so
//! the program, the gateway, the web client and the verifier compute the
//! same deadlines and amounts. Always compiled (no `program` feature).
//!
//! | Status | Who may abort | When | Escrow |
//! |---|---|---|---|
//! | Registering | the operator | any time (no seed exists yet) | returned |
//! | Registering | anyone | `start_by + ABORT_GRACE` | returned |
//! | Seeding, Genesis, Seating | anyone (the operator too) | `stage_at + ABORT_GRACE` | forfeited |
//! | Running | anyone | never fully delegated and idle a day; or past `running_deadline` and not finishable; or a week after it | forfeited |
//! | Finalized, Aborted | nobody | – | – |

use crate::error::ChainError;
use crate::state::*;

/// What `Abort` sees of the world on the base layer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldView {
    /// Every world chunk is owned by the program on base (none delegated).
    pub all_home: bool,
    /// Chunk 0 is owned by the program on base.
    pub chunk0_home: bool,
    /// Chunk 0 carries `WORLD_MAGIC` (genesis done), and its meta's
    /// `finished` and `deadline`.
    pub is_world: bool,
    pub finished: bool,
    pub deadline: i64,
    /// `FinishSeason`'s precheck passes on this world (every chunk home,
    /// finished, not rolled back, consistent root, the season's rules).
    pub precheck_ok: bool,
}

/// Latest time a Running season is expected to be finalized by: tick 0's
/// grace, every tick at twice its length plus a minute, the roster grace
/// and a day. `stage_at` is when the government opened.
pub fn running_deadline(season: &Season, ticks_per_season: u16) -> i64 {
    let per_tick = 2 * season.tick_seconds as i64 + TICK_OVERHEAD_SECONDS;
    season
        .stage_at
        .saturating_add(TICK0_GRACE_SECONDS)
        .saturating_add(ticks_per_season as i64 * per_tick)
        .saturating_add(ROSTER_GRACE_SECONDS)
        .saturating_add(ABORT_GRACE_SECONDS)
}

/// Whether the season's way out is `FinishSeason`.
pub fn finishable(_season: &Season, w: &WorldView) -> bool {
    w.precheck_ok
}

/// Whether `Abort` may run now. `operator`: the caller is the admin or the
/// crank. `world` is needed only while Running.
pub fn check_abort(
    season: &Season,
    ticks_per_season: u16,
    world: Option<&WorldView>,
    operator: bool,
    now: i64,
) -> Result<(), ChainError> {
    let due = |at: i64| {
        if now >= at {
            Ok(())
        } else {
            Err(ChainError::TooEarly)
        }
    };
    match season.status {
        SeasonStatus::Registering if operator => Ok(()),
        SeasonStatus::Registering => due(season.start_by.saturating_add(ABORT_GRACE_SECONDS)),
        // Once a seed is requested or exists, the operator has no early
        // abort: it could cancel after seeing the oracle's answer in flight.
        SeasonStatus::Seeding | SeasonStatus::Genesis | SeasonStatus::Seating => {
            due(season.stage_at.saturating_add(ABORT_GRACE_SECONDS))
        }
        SeasonStatus::Running => {
            let w = world.ok_or(ChainError::WorldTooSmall)?;
            // (i) Delegation never completed (chunk 0 goes last) and the
            // world has not moved for a day.
            if season.delegated & 1 == 0
                && w.chunk0_home
                && w.is_world
                && !w.finished
                && now
                    >= season
                        .stage_at
                        .max(w.deadline)
                        .saturating_add(ABORT_GRACE_SECONDS)
            {
                return Ok(());
            }
            let rd = running_deadline(season, ticks_per_season);
            // (iii) Long overdue: whatever state the world is in.
            if now >= rd.saturating_add(FINISH_GRACE_SECONDS) {
                return Ok(());
            }
            // (ii) Overdue, and FinishSeason is not available.
            if now >= rd && !finishable(season, w) {
                return Ok(());
            }
            Err(ChainError::TooEarly)
        }
        SeasonStatus::Finalized | SeasonStatus::Aborted => Err(ChainError::WrongStatus),
    }
}

/// The operator's escrow: the AI bounties and the bond.
pub fn escrow(season: &Season) -> u64 {
    season
        .bounty_each
        .saturating_mul(season.ai_count as u64)
        .saturating_add(season.bond)
}

/// Whether an abort forfeits the operator's escrow to the members: every
/// abort after registration closed. Only a cancel during Registering,
/// before any seed exists, is free.
pub fn escrow_forfeited(season: &Season) -> bool {
    season.aborted_from != SeasonStatus::Registering as u8
}

/// What `Claim` pays after an abort: what `Register` took, plus an equal
/// share of a forfeited escrow (AI members included: nobody can tell them
/// apart before the reveal, as in the roster-forfeit split).
pub fn refund_amount(season: &Season, member: &MemberAccount) -> u64 {
    let share = if escrow_forfeited(season) && season.member_count > 0 {
        escrow(season) / season.member_count as u64
    } else {
        0
    };
    season
        .entry_fee
        .saturating_add(member.shares)
        .saturating_add(share)
}

/// What `WithdrawOps` pays after an abort: the whole escrow after a free
/// cancel, else only the rounding remainder of the split. The operations
/// share of the fees is never paid: the fees are refunded in full.
pub fn ops_after_abort(season: &Season) -> u64 {
    if !escrow_forfeited(season) || season.member_count == 0 {
        escrow(season)
    } else {
        escrow(season) % season.member_count as u64
    }
}

/// `SeatMembers` and `OpenGovernment` are open to anyone: the operator let
/// the stage sit for `TAKEOVER_SECONDS`.
pub fn overdue(season: &Season, now: i64) -> bool {
    now >= season.stage_at.saturating_add(TAKEOVER_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RD_BLITZ: i64 = 500 + 600 + 180 * 120 + 3600 + 86_400;

    fn season(status: SeasonStatus) -> Season {
        Season {
            magic: SEASON_MAGIC,
            season_id: 1,
            bump: 0,
            vault_bump: 0,
            admin: [1; 32],
            crank: [2; 32],
            usdc_mint: [3; 32],
            usdc_decimals: 6,
            preset: 0,
            nations: 6,
            entry_fee: 10_000_000,
            tick_seconds: 30,
            market: true,
            status,
            world_seed: [0; 32],
            season_seed: [0; 32],
            member_count: 0,
            nation_members: vec![0; 6],
            seated: 0,
            pool: 0,
            ops: 0,
            ops_withdrawn: false,
            treasury: vec![0; 6],
            treasury_final: vec![],
            payouts: vec![],
            final_root: [0; 32],
            prev_season_id: 0,
            prev_history_root: [0; 32],
            history_root: [0; 32],
            ai_count: 0,
            roster_commit: [0; 32],
            bounty_each: 0,
            bond: 0,
            roster_acc: [0; 32],
            roster_revealed: 0,
            roster_outcome: 0,
            bounty_paid: vec![],
            delegated: 0,
            roster_blind: [0; 32],
            refund_base: vec![],
            refund_in_payout: vec![],
            seed_state: 0,
            seed_oracle: [0; 32],
            seed_requested_at: 0,
            seed_requests: 0,
            deposit: 0,
            outstanding: 0,
            voided: false,
            start_by: 1_000,
            stage_at: 500,
            rolled_back: 0,
            aborted_from: 0,
            validator: [4; 32],
            rules_version: 0,
            rules_hash: [0; 32],
            logic_version: 0,
            created_slot: 0,
        }
    }

    fn member(shares: u64) -> MemberAccount {
        MemberAccount {
            magic: MEMBER_MAGIC,
            season_id: 1,
            bump: 0,
            index: 0,
            civ: 0,
            wallet: [0; 32],
            session: [0; 32],
            kind: 0,
            name: String::new(),
            attestation: [0; 32],
            stand: 0,
            votes: [0; 4],
            shares,
            claimed: false,
            tag: [0; 32],
        }
    }

    /// A world back on base, finished and consistent.
    fn home(finished: bool, precheck_ok: bool) -> WorldView {
        WorldView {
            all_home: true,
            chunk0_home: true,
            is_world: true,
            finished,
            deadline: 530,
            precheck_ok,
        }
    }

    #[test]
    fn running_deadline_includes_tick_zeros_grace() {
        assert_eq!(
            running_deadline(&season(SeasonStatus::Running), 180),
            RD_BLITZ
        );
        let mut s = season(SeasonStatus::Running);
        s.stage_at = i64::MAX - 10;
        assert_eq!(running_deadline(&s, 180), i64::MAX, "saturates");
    }

    #[test]
    fn registering() {
        let s = season(SeasonStatus::Registering);
        assert_eq!(check_abort(&s, 180, None, true, 0), Ok(()));
        let open = 1_000 + ABORT_GRACE_SECONDS;
        assert_eq!(
            check_abort(&s, 180, None, false, open - 1),
            Err(ChainError::TooEarly)
        );
        assert_eq!(check_abort(&s, 180, None, false, open), Ok(()));
    }

    #[test]
    fn genesis_and_seating_only_after_a_day_even_for_the_operator() {
        for st in [SeasonStatus::Genesis, SeasonStatus::Seating] {
            let s = season(st);
            let at = 500 + ABORT_GRACE_SECONDS;
            assert_eq!(
                check_abort(&s, 180, None, true, at - 1),
                Err(ChainError::TooEarly)
            );
            assert_eq!(check_abort(&s, 180, None, false, at), Ok(()));
        }
    }

    #[test]
    fn seeding_is_like_genesis() {
        let mut s = season(SeasonStatus::Seeding);
        let at = 500 + ABORT_GRACE_SECONDS;
        assert_eq!(
            check_abort(&s, 180, None, true, at - 1),
            Err(ChainError::TooEarly),
            "no early operator abort"
        );
        assert_eq!(check_abort(&s, 180, None, false, at), Ok(()));
        s.status = SeasonStatus::Aborted;
        s.aborted_from = SeasonStatus::Seeding as u8;
        assert!(escrow_forfeited(&s));
    }

    #[test]
    fn running_never_delegated() {
        let s = season(SeasonStatus::Running);
        let w = home(false, false);
        let at = 530 + ABORT_GRACE_SECONDS;
        assert_eq!(
            check_abort(&s, 180, Some(&w), true, at - 1),
            Err(ChainError::TooEarly)
        );
        assert_eq!(check_abort(&s, 180, Some(&w), false, at), Ok(()));
        // A world someone keeps playing on base moves its deadline.
        let moving = WorldView { deadline: at, ..w };
        assert_eq!(
            check_abort(&s, 180, Some(&moving), false, at),
            Err(ChainError::TooEarly)
        );
        // Chunk 0 delegated: only the running deadline.
        let mut d = season(SeasonStatus::Running);
        d.delegated = all_targets(6);
        let away = WorldView {
            all_home: false,
            chunk0_home: false,
            is_world: false,
            ..w
        };
        assert_eq!(
            check_abort(&d, 180, Some(&away), false, at),
            Err(ChainError::TooEarly)
        );
        let rd = running_deadline(&d, 180);
        assert_eq!(
            check_abort(&d, 180, Some(&away), false, rd - 1),
            Err(ChainError::TooEarly)
        );
        assert_eq!(check_abort(&d, 180, Some(&away), false, rd), Ok(()));
        // Running needs the world.
        assert_eq!(
            check_abort(&d, 180, None, false, rd),
            Err(ChainError::WorldTooSmall)
        );
    }

    #[test]
    fn a_finished_world_on_base_is_finished_not_aborted() {
        let mut s = season(SeasonStatus::Running);
        s.delegated = all_targets(6);
        let back = home(true, true);
        let rd = running_deadline(&s, 180);
        assert_eq!(
            check_abort(&s, 180, Some(&back), false, rd),
            Err(ChainError::TooEarly)
        );
        assert_eq!(
            check_abort(&s, 180, Some(&back), false, rd + FINISH_GRACE_SECONDS),
            Ok(())
        );
        // Rolled back: FinishSeason refuses (its precheck fails), so Abort
        // opens at the deadline.
        s.rolled_back = 1 << 7;
        let rolled = home(true, false);
        assert_eq!(check_abort(&s, 180, Some(&rolled), false, rd), Ok(()));
    }

    /// A world whose chunks come from different writes (the root trailer
    /// does not match) is not finishable, even with every chunk home and
    /// `finished`: abortable at the running deadline.
    #[test]
    fn a_mixed_world_is_not_finishable() {
        let mut s = season(SeasonStatus::Running);
        s.delegated = all_targets(6);
        let mixed = home(true, false);
        assert!(!finishable(&s, &mixed));
        let rd = running_deadline(&s, 180);
        assert_eq!(
            check_abort(&s, 180, Some(&mixed), false, rd - 1),
            Err(ChainError::TooEarly)
        );
        assert_eq!(check_abort(&s, 180, Some(&mixed), false, rd), Ok(()));
    }

    #[test]
    fn done_seasons_cannot_abort() {
        for st in [SeasonStatus::Finalized, SeasonStatus::Aborted] {
            assert_eq!(
                check_abort(&season(st), 180, None, true, i64::MAX),
                Err(ChainError::WrongStatus)
            );
        }
    }

    #[test]
    fn longest_running_window() {
        let mut s = season(SeasonStatus::Running);
        s.stage_at = 0;
        s.tick_seconds = MAX_TICK_SECONDS;
        let rd = running_deadline(&s, 180);
        assert!(rd < 62 * 86_400, "{rd}");
        s.tick_seconds = 30;
        let rd = running_deadline(&s, 180);
        assert!(rd < 32 * 3_600, "{rd}");
    }

    #[test]
    fn takeover_after_ten_minutes() {
        let s = season(SeasonStatus::Seating);
        assert!(!overdue(&s, 500 + TAKEOVER_SECONDS - 1));
        assert!(overdue(&s, 500 + TAKEOVER_SECONDS));
    }

    /// Every abort mode pays out exactly what was paid in (vault ends at 0).
    #[test]
    fn abort_refunds_conserve_the_vault() {
        for (from, n, ai, bounty, bond, dep) in [
            (
                SeasonStatus::Registering,
                12u32,
                3u16,
                1_000_000u64,
                6_000_000u64,
                2_000_000u64,
            ),
            (
                SeasonStatus::Seating,
                12,
                3,
                1_000_000,
                6_000_000,
                2_000_000,
            ),
            (SeasonStatus::Seeding, 7, 2, 333_333, 1, 0),
            (SeasonStatus::Running, 48, 47, 1_000_003, 777_777_777, 0),
            (
                SeasonStatus::Running,
                48,
                40,
                999_999,
                12_345_678_901,
                5_000_000,
            ),
        ] {
            let mut s = season(SeasonStatus::Aborted);
            s.aborted_from = from as u8;
            s.member_count = n;
            s.ai_count = ai;
            s.bounty_each = bounty;
            s.bond = bond;
            let paid_in = n as u128 * (s.entry_fee + dep) as u128 + escrow(&s) as u128;
            let out =
                n as u128 * refund_amount(&s, &member(dep)) as u128 + ops_after_abort(&s) as u128;
            assert_eq!(out, paid_in, "{from:?} n={n}");
        }
    }

    /// Operator net (USDC base units) if it aborts after registration
    /// closed, versus the worst Finalized outcome with the roster revealed
    /// (every bounty paid, AI fees in the pool, AI deposits drained, AI
    /// payouts to people): with the bond at `bond_floor` the abort is never
    /// better. Seasons within the member cap, `ai_count < SEASON_MEMBER_CAP`.
    #[test]
    fn a_stall_never_pays_the_operator_with_the_bond_floor() {
        let fee = 10_000_000u64;
        let ops_each = fee * 2000 / 10_000;
        for (n, a, bounty, dep) in [
            (48u32, 47u16, 1_000_000u64, 0u64),
            (48, 40, 5_000_000, 0),
            (48, 47, 0, 3_000_000),
            (12, 3, 1_000_000, 2_000_000),
            (30, 20, 10_000_000, 1_000_000),
            (8, 1, 0, 0),
        ] {
            let mut s = season(SeasonStatus::Aborted);
            s.aborted_from = SeasonStatus::Running as u8;
            s.entry_fee = fee;
            s.member_count = n;
            s.ai_count = a;
            s.bounty_each = bounty;
            s.pool = n as u64 * (fee - ops_each);
            s.treasury = vec![n as u64 * dep, 0, 0, 0, 0, 0];
            s.bond = bond_floor(&s);
            let (a64, n64) = (a as i128, n as i128);
            let paid = a64 * (fee + dep) as i128 + escrow(&s) as i128;
            let abort =
                a64 * refund_amount(&s, &member(dep)) as i128 + ops_after_abort(&s) as i128 - paid;
            let fin_worst = n64 * ops_each as i128 + s.bond as i128 - paid;
            assert!(
                abort <= fin_worst,
                "n={n} a={a}: abort {abort} > finalized {fin_worst}"
            );
        }
    }

    #[test]
    fn a_free_cancel_returns_the_escrow() {
        let mut s = season(SeasonStatus::Aborted);
        s.aborted_from = SeasonStatus::Registering as u8;
        s.member_count = 5;
        s.ai_count = 2;
        s.bounty_each = 3;
        s.bond = 11;
        assert_eq!(refund_amount(&s, &member(4)), s.entry_fee + 4);
        assert_eq!(ops_after_abort(&s), 17);
        s.aborted_from = SeasonStatus::Genesis as u8;
        assert_eq!(refund_amount(&s, &member(4)), s.entry_fee + 4 + 17 / 5);
        assert_eq!(ops_after_abort(&s), 17 % 5);
    }
}
