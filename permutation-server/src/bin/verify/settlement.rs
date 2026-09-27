//! 5–6. After the last tick: the final root, the operator AI roster
//! (V5 §18), the settlement (V5 §7) and the history layer.

use crate::chain::roster_account;
use crate::report::Report;
use crate::Ctx;
use borsh::BorshDeserialize;
use permutation_chain::finalize::{finalize, Roster};
use permutation_chain::state::{MemberAccount, RosterEntry, Season, SeasonStatus};
use permutation_rules::state::WorldState;
use permutation_server::codec::hex;

/// 5. The checks of a finalized season, in order: the final world root, the
///    operator AI roster, every payout and the pool, the operations share,
///    the treasuries and the history root.
pub fn finalized(ctx: &mut Ctx, state: &WorldState, members: &[MemberAccount]) {
    let (season, rules) = (&ctx.season, &ctx.rules);
    ctx.report.check(
        "final world root matches the Season account",
        state.state_root().unwrap() == season.final_root,
        hex(&season.final_root)[..16].to_string(),
    );
    let paid = paid_in(season, rules.ops_share_bps as u64, members);
    let entries = roster_account(&ctx.base, &ctx.program_id, season.season_id).unwrap_or_default();
    let roster = roster(&mut ctx.report, season, state, members, &entries);
    // The sticky `usdc_broken` flag of the replay is unit V's (WP12 §4.7);
    // a voided season's payouts then differ and this check fails.
    let f = finalize(&paid, state, rules, roster, false);
    let p = &f.settlement;
    ctx.report.check(
        "every member's payout recomputed (V5 §7, §18) matches the Season account",
        f.payouts == season.payouts
            && f.pool == season.pool
            && f.bounty_paid == season.bounty_paid,
        format!(
            "pool {} to {} members{}{}",
            f.pool,
            p.per_member.iter().filter(|x| **x > 0).count(),
            if p.redistributed > 0 {
                format!(
                    ", {} redistributed from AI members to people",
                    p.redistributed
                )
            } else {
                String::new()
            },
            if p.refund {
                " (refund: nobody achieved anything)"
            } else {
                ""
            }
        ),
    );
    let ops = f.ops;
    ctx.report.check(
        "operations share = 20% of fees + 20% of in-play income + rounding (+ the bond, returned)",
        ops == season.ops,
        format!("{ops}"),
    );
    ctx.report.check(
        "treasuries returned to depositors match the final world",
        f.treasury_final == season.treasury_final && paid.treasury == season.treasury,
        format!("{:?}", f.treasury_final),
    );
    // The history layer: this season's record, recomputed from the final
    // world, chained onto the root it took over from the season before.
    let record = permutation_rules::history::season_record(state, p);
    let root = permutation_rules::history::history_root(&season.prev_history_root, &record);
    ctx.report.check(
        "history root recomputed from the final world matches the Season account",
        root == season.history_root,
        format!(
            "{} ({} cities, {} ruins){}",
            &hex(&root)[..16],
            record.cities.len(),
            record.ruins.len(),
            if season.prev_season_id == 0 {
                ", the first season of its line".to_string()
            } else {
                format!(", after season {}", season.prev_season_id)
            }
        ),
    );
}

/// The Season account as the members paid into it: the pool and operations
/// totals from the entry fees, and the treasuries from the members'
/// deposits, rather than the account's own totals.
fn paid_in(season: &Season, ops_share_bps: u64, members: &[MemberAccount]) -> Season {
    let fee_ops = season.entry_fee * ops_share_bps / 10_000;
    let mut paid = season.clone();
    paid.pool = season.member_count as u64 * (season.entry_fee - fee_ops);
    paid.ops = season.member_count as u64 * fee_ops;
    paid.treasury = vec![0; season.nations as usize];
    for m in members {
        if let Some(t) = paid.treasury.get_mut(m.civ as usize) {
            *t += m.shares;
        }
    }
    paid
}

/// Operator AI members (V5 §18.2): every revealed salt must turn its
/// member's wallet into the tag the wallet registered with, and the tags
/// chain to the roster committed before registration. Checks the roster's
/// outcome and returns it for the settlement.
fn roster<'a>(
    r: &mut Report,
    season: &Season,
    state: &WorldState,
    members: &[MemberAccount],
    entries: &'a [RosterEntry],
) -> Roster<'a> {
    match season.roster_outcome {
        1 => {
            let tags: Vec<[u8; 32]> = entries
                .iter()
                .filter_map(|e| {
                    let m = members.get(e.member as usize)?;
                    let tag =
                        permutation_rules::roster::roster_tag(season.season_id, &m.wallet, &e.salt);
                    (tag == m.tag && m.civ == e.civ).then_some(tag)
                })
                .collect();
            let ok = tags.len() == entries.len()
                && entries.len() == season.ai_count as usize
                && permutation_rules::roster::roster_chain(&tags) == season.roster_commit;
            let bounties = permutation_rules::roster::bounties(
                state,
                &entries.iter().map(|e| (e.civ, e.salt)).collect::<Vec<_>>(),
                season.bounty_each,
            );
            let taken = bounties
                .homes
                .iter()
                .filter(|h| {
                    h.and_then(|c| state.cities.get(c as usize))
                        .and_then(|c| c.first_conquest)
                        .is_some_and(|q| q.bounty)
                })
                .count();
            r.check(
                "operator AI roster: every revealed member registered with its tag, and the tags chain to the committed roster",
                ok,
                format!(
                    "{} AI members {:?}, {} home cities conquered, bounty {} each",
                    entries.len(),
                    entries.iter().map(|e| e.member).collect::<Vec<_>>(),
                    taken,
                    season.bounty_each
                ),
            );
            Roster::Revealed(entries)
        }
        2 => {
            r.check(
                "operator AI roster was not revealed in time: bounties and bond joined the pool",
                season.roster_revealed < season.ai_count
                    || season.roster_acc != season.roster_commit,
                format!("{}/{} revealed", season.roster_revealed, season.ai_count),
            );
            Roster::Forfeited
        }
        _ => {
            r.check(
                "no operator AI members this season",
                season.ai_count == 0,
                String::new(),
            );
            Roster::None
        }
    }
}

/// 6. The season before, in the history layer: its root is the one this
///    season took over (and mixed into its seed).
pub fn previous_season(ctx: &mut Ctx) {
    let season = &ctx.season;
    if season.prev_season_id == 0 {
        return;
    }
    let prev = permutation_chain::state::season_address(&ctx.program_id, season.prev_season_id)
        .and_then(|a| ctx.base.account_data(&a).ok().flatten())
        .and_then(|d| Season::deserialize(&mut &d[..]).ok());
    ctx.report.check(
        "the season it follows is finalized and its history root is the one taken over",
        prev.as_ref().is_some_and(|x| {
            x.status == SeasonStatus::Finalized
                && x.history_root == season.prev_history_root
                && x.admin == season.admin
        }),
        format!("season {}", season.prev_season_id),
    );
}
