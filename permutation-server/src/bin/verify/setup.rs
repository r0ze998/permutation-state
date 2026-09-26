//! Before the first tick: the season account, genesis, the seating of the
//! members and the first election.

use crate::chain::{fields, season_members};
use crate::report::Report;
use crate::Ctx;
use borsh::BorshDeserialize;
use permutation_chain::seat::{seat_member, Seat};
use permutation_chain::state::{MemberAccount, Season};
use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::{self, NOBODY};
use permutation_rules::state::WorldState;
use permutation_rules::Ruleset;
use permutation_server::chainlink::ChainLink;
use permutation_server::codec::{from_hex, hex};
use serde_json::Value;

/// A `PS_SEAT` record: the number of members seated once it was logged, the
/// state root then, and the members it seated (`seat::Seat`: nation, the key
/// seated, candidacy and votes).
type SeatRecord = (usize, Vec<u8>, Vec<Seat>);

/// Every nation's officers (member ids, -1 for a vacant office), as printed.
pub fn officers(state: &WorldState) -> String {
    format!(
        "{:?}",
        state
            .nations
            .iter()
            .map(|n| n.offices.map(|m| if m == NOBODY { -1 } else { m as i64 }))
            .collect::<Vec<_>>()
    )
}

/// 1. The Season account, read from the base layer, and the check that it is
///    the season address of the program whose logs are read.
pub fn read_season(
    base: &ChainLink,
    program_id: &str,
    season_addr: &str,
    r: &mut Report,
) -> Season {
    let data = base
        .account_data(season_addr)
        .expect("base RPC")
        .expect("season account exists");
    let season = Season::deserialize(&mut &data[..]).expect("decode season");
    // The program whose logs are read is the one that owns this season: its
    // address is that program's PDA for the season id.
    r.check(
        "the season account is the program's season address (logs are read from that program only)",
        permutation_chain::state::season_address(program_id, season.season_id).as_deref()
            == Some(season_addr),
        program_id.to_string(),
    );
    r.check(
        format!(
            "season {} read from the base layer ({})",
            season.season_id, season_addr
        ),
        true,
        format!(
            "{} nations, {} members, entry fee {}",
            season.nations, season.member_count, season.entry_fee
        ),
    );
    season
}

/// 2. Genesis, rebuilt from the season's nations, seeds and treasuries and
///    checked against `PS_GENESIS`. Returns the genesis world.
pub fn genesis(ctx: &mut Ctx) -> WorldState {
    let (season, rules) = (&ctx.season, &ctx.rules);
    let mut entries = nation_entries(season.nations as usize);
    for (e, t) in entries.iter_mut().zip(&season.treasury) {
        e.treasury = *t;
    }
    let state = new_season(rules, &season.world_seed, &season.season_seed, &entries)
        .expect("genesis recomputes");
    let genesis_root = state.state_root().unwrap();
    match ctx.info["genesis"]["root"].as_str().map(from_hex) {
        Some(claimed) => {
            let on_chain = fields(
                &ctx.base,
                ctx.info["genesis"]["signature"].as_str(),
                b"PS_GENESIS",
            )
            .ok()
            .flatten()
            .map(|f| f[1].clone());
            let ok =
                claimed == genesis_root.to_vec() && on_chain.as_ref().is_none_or(|c| *c == claimed);
            ctx.report.check(
                "genesis recomputed from the season's nations, seeds and treasuries",
                ok,
                format!(
                    "root {} {}",
                    &hex(&genesis_root)[..16],
                    if on_chain.is_some() {
                        "(matched the PS_GENESIS log on chain)"
                    } else {
                        ""
                    }
                ),
            );
        }
        None => ctx.report.check(
            "genesis record available",
            false,
            "the gateway has no PS_GENESIS record for this season",
        ),
    }
    state
}

/// The `PS_SEAT` records the gateway indexed, re-read from the chain, in
/// order.
fn seat_records(ctx: &Ctx) -> Vec<SeatRecord> {
    let mut n = 0usize;
    ctx.info["seating"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|rec| {
            let f = fields(&ctx.base, rec["signature"].as_str(), b"PS_SEAT")
                .ok()
                .flatten()?;
            let seated: Vec<Seat> = Vec::try_from_slice(f.get(2)?).ok()?;
            n += seated.len();
            Some((n, f.get(1)?.clone(), seated))
        })
        .collect()
}

/// What replaying the seating found: whether every member was seated and
/// every record matched, the first mismatch, and the members seated with a
/// substitute key (a session key registered twice).
#[derive(Debug, Default)]
struct Replayed {
    ok: bool,
    detail: String,
    substituted: Vec<u32>,
}

impl Replayed {
    fn fail(&mut self, why: String) {
        if self.ok {
            self.ok = false;
            self.detail = why;
        }
    }
}

/// Seat `members` (registration order) with `seat::seat_member`, the function
/// `SeatMembers` runs, so a duplicate session key gets the same substitute.
/// Each record is checked where it ends: the root, and the members it lists
/// with the key each was seated with.
fn replay_seating(
    state: &mut WorldState,
    rules: &Ruleset,
    members: &[MemberAccount],
    records: &[SeatRecord],
) -> Replayed {
    let mut out = Replayed {
        ok: true,
        ..Default::default()
    };
    let mut seats: Vec<Seat> = Vec::new();
    for m in members {
        let Ok(seat) = seat_member(state, rules, m) else {
            out.fail(format!("member {} could not be seated", m.index));
            break;
        };
        if seat.1 != m.session {
            out.substituted.push(m.index);
        }
        seats.push(seat);
        let n = state.members.len();
        if let Some((_, root, listed)) = records.iter().find(|(end, _, _)| *end == n) {
            if state.state_root().unwrap().to_vec() != *root {
                out.fail(format!(
                    "root after seating {n} members differs from PS_SEAT"
                ));
            } else if seats[n - listed.len()..] != listed[..] {
                out.fail(format!(
                    "PS_SEAT ending at member {} lists other members or keys",
                    n - 1
                ));
            }
        }
    }
    out
}

/// 3a. The seating of the members. They come from their own accounts on the
///     base layer (final once registration closed), in registration order;
///     each `PS_SEAT` record, when the gateway indexed it, is checked too
///     (its root and the keys it seated), and tick 0's pre-state root anchors
///     the result (see `first_election`). Returns the members.
pub fn seating(ctx: &mut Ctx, state: &mut WorldState) -> Vec<MemberAccount> {
    let records = seat_records(ctx);
    let (season, rules) = (&ctx.season, &ctx.rules);
    let members = season_members(&ctx.base, &ctx.program_id, season.season_id);
    let mut r = replay_seating(state, rules, &members, &records);
    if members.len() as u32 != season.member_count {
        r.ok = false;
        r.detail = format!(
            "{} member accounts found for {} registered",
            members.len(),
            season.member_count
        );
    }
    if r.ok && !r.substituted.is_empty() {
        r.detail = format!(
            "member(s) {:?} seated with a substitute key (session key registered twice)",
            r.substituted
        );
    }
    ctx.report.check(
        format!(
            "{} members seated in registration order, with their candidacy and votes",
            state.members.len()
        ),
        r.ok && state.members.len() as u32 == season.member_count,
        r.detail,
    );
    members
}

/// 3b. The first election, checked against `PS_OPEN` or, failing that, the
///     pre-state root of the first tick record (`ticks`: the gateway's
///     `/ticks` index, if it answered).
pub fn first_election(ctx: &mut Ctx, state: &mut WorldState, ticks: Option<&Value>) {
    let _ = gov::first_election(state, &ctx.rules);
    let open_root = ctx.info["open"]["root"].as_str().map(from_hex);
    let open_chain = fields(
        &ctx.base,
        ctx.info["open"]["signature"].as_str(),
        b"PS_OPEN",
    )
    .ok()
    .flatten()
    .map(|f| f[1].clone());
    let opened = open_chain.or(open_root);
    let first_pre = ticks
        .and_then(|v| v["records"].as_array().and_then(|r| r.first().cloned()))
        .and_then(|r| r["preRoot"].as_str().map(from_hex));
    let anchor = opened.or(first_pre);
    ctx.report.check(
        "first election recomputed (PS_OPEN, or tick 0's pre-state root)",
        anchor.as_ref() == Some(&state.state_root().unwrap().to_vec()),
        format!("officers {}", officers(state)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use permutation_chain::seat::substitute_key;
    use permutation_chain::state::MEMBER_MAGIC;
    use permutation_rules::gov::{GovAction, GovEntry, Role};
    use permutation_rules::Preset;

    const SEASON: u64 = 7;

    fn member(index: u32, civ: u16, wallet: u8, session: [u8; 32], stand: u8) -> MemberAccount {
        MemberAccount {
            magic: MEMBER_MAGIC,
            season_id: SEASON,
            bump: 255,
            index,
            civ,
            wallet: [wallet; 32],
            session,
            kind: 2,
            name: format!("m{index}"),
            attestation: [0; 32],
            stand,
            votes: [NOBODY; 4],
            shares: 0,
            claimed: false,
            tag: [0; 32],
        }
    }

    /// A genesis world and three members, the second registering the first's
    /// session key.
    fn season() -> (Ruleset, WorldState, Vec<MemberAccount>) {
        let rules = Ruleset::new(Preset::Blitz);
        let world = new_season(&rules, &[1; 32], &[2; 32], &nation_entries(2)).unwrap();
        let members = vec![
            member(0, 0, 10, [1; 32], Role::Steward.bit()),
            member(1, 0, 11, [1; 32], Role::General.bit()),
            member(2, 1, 12, [2; 32], 0x0f),
        ];
        (rules, world, members)
    }

    /// The records a program logs that seats `keys` (members 0–1, then 2),
    /// built with the rules alone.
    fn logged(rules: &Ruleset, world: &WorldState, keys: [[u8; 32]; 3]) -> Vec<SeatRecord> {
        let (_, _, members) = season();
        let mut s = world.clone();
        let mut out = Vec::new();
        for range in [0..2, 2..3] {
            let mut seats = Vec::new();
            for m in &members[range] {
                let key = keys[m.index as usize];
                let id = gov::join(&mut s, rules, m.civ, key).unwrap();
                let stand = GovEntry {
                    member: id,
                    signer: key,
                    action: GovAction::Stand { roles: m.stand },
                };
                gov::apply_pre_season(&mut s, rules, &[stand]).unwrap();
                seats.push((m.civ, key, m.stand, m.votes));
            }
            out.push((s.members.len(), s.state_root().unwrap().to_vec(), seats));
        }
        out
    }

    #[test]
    fn a_duplicate_session_key_replays_with_its_substitute() {
        let (rules, world, members) = season();
        let keys = [[1; 32], substitute_key(SEASON, &[11; 32]), [2; 32]];
        let records = logged(&rules, &world, keys);
        let mut s = world.clone();
        let r = replay_seating(&mut s, &rules, &members, &records);
        assert!(r.ok, "{}", r.detail);
        assert_eq!(r.substituted, vec![1]);
        assert_eq!(s.members.iter().map(|m| m.key).collect::<Vec<_>>(), keys);
        assert_eq!(s.members[1].standing_for, Role::General.bit());
        // Without records (none indexed), the replay alone still seats all.
        let r = replay_seating(&mut world.clone(), &rules, &members, &[]);
        assert!(r.ok);
    }

    #[test]
    fn a_record_with_another_key_or_root_fails() {
        let (rules, world, members) = season();
        let keys = [[1; 32], substitute_key(SEASON, &[11; 32]), [2; 32]];
        // The record lists the session key the member registered, not the
        // key it was seated with.
        let mut records = logged(&rules, &world, keys);
        records[0].2[1].1 = [1; 32];
        let r = replay_seating(&mut world.clone(), &rules, &members, &records);
        assert!(!r.ok);
        assert!(r.detail.contains("keys"), "{}", r.detail);
        // A program that seated another substitute: the root differs.
        let other = [[1; 32], substitute_key(SEASON + 1, &[11; 32]), [2; 32]];
        let records = logged(&rules, &world, other);
        let r = replay_seating(&mut world.clone(), &rules, &members, &records);
        assert!(!r.ok);
        assert!(r.detail.contains("root"), "{}", r.detail);
    }
}
