//! Before the first tick: the season account, genesis, the seating of the
//! members and the first election.

use crate::chain::{fields, season_members};
use crate::report::Report;
use crate::Ctx;
use borsh::BorshDeserialize;
use permutation_chain::state::{MemberAccount, Season};
use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::{self, GovAction, GovEntry, Role, NOBODY};
use permutation_rules::state::WorldState;
use permutation_server::chainlink::ChainLink;
use permutation_server::codec::{from_hex, hex};
use serde_json::Value;

/// One member as a `PS_SEAT` record lists it: nation, session key,
/// candidacy and first-election votes.
type Seat = (u16, [u8; 32], u8, [u32; 4]);

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

/// The state root after each `PS_SEAT` record the gateway indexed, keyed by
/// the number of members seated by then.
fn seat_roots(ctx: &Ctx) -> Vec<(usize, Vec<u8>)> {
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
            Some((n, f.get(1)?.clone()))
        })
        .collect()
}

/// 3a. The seating of the members. They come from their own accounts on the
///     base layer (final once registration closed), in registration order;
///     each `PS_SEAT` root, when the gateway indexed it, is checked too, and
///     tick 0's pre-state root anchors the result (see `first_election`).
///     Returns the members.
pub fn seating(ctx: &mut Ctx, state: &mut WorldState) -> Vec<MemberAccount> {
    let mut seat_ok = true;
    let mut seat_detail = String::new();
    let seat_roots = seat_roots(ctx);
    let (season, rules) = (&ctx.season, &ctx.rules);
    let members = season_members(&ctx.base, &ctx.program_id, season.season_id);
    if members.len() as u32 != season.member_count {
        seat_ok = false;
        seat_detail = format!(
            "{} member accounts found for {} registered",
            members.len(),
            season.member_count
        );
    }
    for m in &members {
        let (civ, key, stand, votes) = (m.civ, m.session, m.stand, m.votes);
        let Ok(id) = gov::join(state, rules, civ, key) else {
            seat_ok = false;
            break;
        };
        let mut e = vec![GovEntry {
            member: id,
            signer: key,
            action: GovAction::Stand { roles: stand },
        }];
        for role in Role::ALL {
            if votes[role.index()] != NOBODY {
                e.push(GovEntry {
                    member: id,
                    signer: key,
                    action: GovAction::Vote {
                        role,
                        candidate: votes[role.index()],
                    },
                });
            }
        }
        seat_ok &= gov::apply_pre_season(state, rules, &e).is_ok();
        if let Some((_, root)) = seat_roots.iter().find(|(n, _)| *n == state.members.len()) {
            if state.state_root().unwrap().to_vec() != *root {
                seat_ok = false;
                seat_detail = format!(
                    "root after seating {} members differs from PS_SEAT",
                    state.members.len()
                );
            }
        }
    }
    ctx.report.check(
        format!(
            "{} members seated in registration order, with their candidacy and votes",
            state.members.len()
        ),
        seat_ok && state.members.len() as u32 == season.member_count,
        seat_detail,
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
