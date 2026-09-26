//! Before the first tick: the season account, genesis, the seating of the
//! members and the first election, each checked against the base layer's
//! records bound to this season (never against the gateway's claims).

use crate::chain::{season_members, PrefixData};
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
use permutation_server::codec::hex;

/// A `PS_SEAT` record: the state root it was logged at, the members it
/// seated (`seat::Seat`: nation, the key seated, candidacy and votes), and
/// whether it is bound to this season (logged by a SeatMembers that read
/// this season's account).
type SeatRecord = (Vec<u8>, Vec<Seat>, bool);

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

/// 1. The Season account at `address` (derived from the pinned program and
///    season id), read from the base layer. `None` after a ✗ or ? line.
pub fn read_season(
    base: &ChainLink,
    address: &str,
    season_id: u64,
    r: &mut Report,
) -> Option<Season> {
    let what = format!("season {season_id} read from the base layer ({address})");
    let data = match base.account_data(address) {
        Ok(Some(d)) => d,
        Ok(None) => {
            r.check(what, false, "no account at the season's address");
            return None;
        }
        Err(e) => {
            r.incomplete(what, format!("could not read the account: {e}"));
            return None;
        }
    };
    let Ok(season) = Season::deserialize(&mut &data[..]) else {
        r.check(what, false, "the account does not decode as a Season");
        return None;
    };
    r.check(
        what,
        season.season_id == season_id,
        format!(
            "{} nations, {} members, entry fee{}",
            season.nations,
            season.member_count,
            if season.season_id == season_id {
                format!(" {}", season.entry_fee)
            } else {
                format!(
                    " {}; the account names season {}",
                    season.entry_fee, season.season_id
                )
            }
        ),
    );
    (season.season_id == season_id).then_some(season)
}

/// The genesis line: the `PS_GENESIS` records selected by this season's
/// seed (the seed mixes the season id in) against the recomputed root.
fn genesis_line(prefix: &PrefixData, seed: &[u8; 32], root: &[u8; 32]) -> (Option<bool>, String) {
    let ours: Vec<_> = prefix.genesis.iter().filter(|g| g.1 == *seed).collect();
    if ours.is_empty() {
        return (
            None,
            "no PS_GENESIS for this season in the base history".into(),
        );
    }
    match ours.iter().find(|g| g.0 != *root) {
        None => (
            Some(true),
            format!("root {} (seed matches)", &hex(root)[..16]),
        ),
        Some(g) => (
            Some(false),
            format!(
                "PS_GENESIS with this season's seed has root {}, recomputed {}",
                &hex(&g.0)[..16],
                &hex(root)[..16]
            ),
        ),
    }
}

/// 2. Genesis, rebuilt from the season's nations, seeds and treasuries and
///    checked against the season's `PS_GENESIS`. Returns the genesis world.
pub fn genesis(ctx: &mut Ctx, prefix: &PrefixData) -> WorldState {
    let (season, rules) = (&ctx.season, &ctx.rules);
    let mut entries = nation_entries(season.nations as usize);
    for (e, t) in entries.iter_mut().zip(&season.treasury) {
        e.treasury = *t;
    }
    let state = new_season(rules, &season.world_seed, &season.season_seed, &entries)
        .expect("genesis recomputes");
    let (ok, detail) = genesis_line(prefix, &season.season_seed, &state.state_root().unwrap());
    let what = "genesis recomputed from the season's nations, seeds and treasuries matches PS_GENESIS on the base layer";
    match ok {
        Some(ok) => ctx.report.check(what, ok, detail),
        None => ctx.report.incomplete(what, detail),
    }
    state
}

/// The `PS_SEAT` records read from the base layer.
fn seat_records(prefix: &PrefixData) -> Vec<SeatRecord> {
    prefix
        .seats
        .iter()
        .map(|(root, seats, _, bound)| (root.to_vec(), seats.clone(), *bound))
        .collect()
}

/// What replaying the seating found: whether every member was seated and
/// every record of this season was reached, the first mismatch, the members
/// seated with a substitute key (a session key registered twice), and how
/// many records were matched or ignored (not this season's).
#[derive(Debug, Default)]
struct Replayed {
    ok: bool,
    detail: String,
    substituted: Vec<u32>,
    matched: usize,
    ignored: usize,
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
/// Records are matched by root, in any order: a record whose root the
/// replay reaches must list the members just seated with their keys; a
/// record of this season whose root is never reached fails; others are
/// ignored. A missing record only lowers the count (`PS_OPEN` anchors the
/// result, see `first_election`).
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
    let mut matched = vec![false; records.len()];
    for m in members {
        let Ok(seat) = seat_member(state, rules, m) else {
            out.fail(format!("member {} could not be seated", m.index));
            break;
        };
        if seat.1 != m.session {
            out.substituted.push(m.index);
        }
        seats.push(seat);
        let root = state.state_root().unwrap().to_vec();
        for (i, (_, listed, _)) in records.iter().enumerate().filter(|(_, r)| r.0 == root) {
            matched[i] = true;
            let n = seats.len();
            if listed.len() > n || seats[n - listed.len()..] != listed[..] {
                out.fail(format!(
                    "PS_SEAT at root {} lists other members or keys",
                    &hex(&root)[..16]
                ));
            }
        }
    }
    for (r, _) in records.iter().zip(&matched).filter(|(_, m)| !**m) {
        if r.2 {
            out.fail(format!(
                "PS_SEAT root {} is never reached by the replay",
                &hex(&r.0)[..16]
            ));
        } else {
            out.ignored += 1;
        }
    }
    out.matched = matched.iter().filter(|m| **m).count();
    out
}

/// 3a. The seating of the members. They come from their own accounts on the
///     base layer (final once registration closed), in registration order;
///     the `PS_SEAT` records read are matched by root. Returns the members;
///     `None` (seating incomplete) when their accounts cannot be read.
pub fn seating(
    ctx: &mut Ctx,
    state: &mut WorldState,
    prefix: &PrefixData,
) -> Option<Vec<MemberAccount>> {
    let records = seat_records(prefix);
    let (season, rules) = (&ctx.season, &ctx.rules);
    let members = match season_members(&ctx.base, &ctx.program_id, season.season_id) {
        Ok(m) => m,
        Err(e) => {
            ctx.report.incomplete(
                "seating",
                format!("the member accounts could not be read: {e}"),
            );
            return None;
        }
    };
    let mut r = replay_seating(state, rules, &members, &records);
    if members.len() as u32 != season.member_count {
        r.ok = false;
        r.detail = format!(
            "{} member accounts found for {} registered",
            members.len(),
            season.member_count
        );
    }
    if r.ok {
        let mut notes = vec![format!("{} PS_SEAT records matched by root", r.matched)];
        if r.ignored > 0 {
            notes.push(format!("{} of other seasons ignored", r.ignored));
        }
        if !r.substituted.is_empty() {
            notes.push(format!(
                "member(s) {:?} seated with a substitute key (session key registered twice)",
                r.substituted
            ));
        }
        r.detail = notes.join("; ");
    }
    ctx.report.check(
        format!(
            "{} members seated in registration order, with their candidacy and votes",
            state.members.len()
        ),
        r.ok && state.members.len() as u32 == season.member_count,
        r.detail,
    );
    Some(members)
}

/// The first-election line: a `PS_OPEN` at the recomputed root, or a
/// contradiction if one of this season's has another root.
fn election_line(prefix: &PrefixData, root: &[u8; 32]) -> (Option<bool>, String) {
    if prefix.open.iter().any(|o| o.0 == *root) {
        return (Some(true), format!("root {}", &hex(root)[..16]));
    }
    match prefix.open.iter().find(|o| o.2) {
        Some(o) => (
            Some(false),
            format!(
                "this season's PS_OPEN has root {}, recomputed {}",
                &hex(&o.0)[..16],
                &hex(root)[..16]
            ),
        ),
        None => (
            None,
            "no PS_OPEN with the recomputed root in the base history".into(),
        ),
    }
}

/// 3b. The first election, checked against `PS_OPEN` (selected by root).
pub fn first_election(ctx: &mut Ctx, state: &mut WorldState, prefix: &PrefixData) {
    let _ = gov::first_election(state, &ctx.rules);
    let (ok, detail) = election_line(prefix, &state.state_root().unwrap());
    let what = "first election recomputed matches PS_OPEN on the base layer";
    let detail = format!("{detail}; officers {}", officers(state));
    match ok {
        Some(ok) => ctx.report.check(what, ok, detail),
        None => ctx.report.incomplete(what, detail),
    }
}

/// The season seed's derivation is not checked (WP11 checks it).
pub fn seed_not_checked(ctx: &mut Ctx) {
    ctx.report.not_checked(
        "season seed derivation",
        "the season seed is taken from the Season account; how it was drawn is not checked (see DESIGN.md, Trust model)",
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
    /// built with the rules alone (bound to this season).
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
            out.push((s.state_root().unwrap().to_vec(), seats, true));
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
        records[0].1[1].1 = [1; 32];
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

    #[test]
    fn seating_tolerates_a_missing_record() {
        let (rules, world, members) = season();
        let keys = [[1; 32], substitute_key(SEASON, &[11; 32]), [2; 32]];
        let records = logged(&rules, &world, keys);
        let r = replay_seating(&mut world.clone(), &rules, &members, &records[1..]);
        assert!(r.ok, "{}", r.detail);
        assert_eq!(r.matched, 1);
        // A record of this season with a root the replay never reaches fails...
        let mut stray = records.clone();
        stray.push((vec![9; 32], Vec::new(), true));
        let r = replay_seating(&mut world.clone(), &rules, &members, &stray);
        assert!(!r.ok);
        assert!(r.detail.contains("never reached"), "{}", r.detail);
        // ...one of another season (not bound) is ignored and counted.
        stray.last_mut().unwrap().2 = false;
        let r = replay_seating(&mut world.clone(), &rules, &members, &stray);
        assert!(r.ok, "{}", r.detail);
        assert_eq!((r.matched, r.ignored), (2, 1));
    }

    #[test]
    fn genesis_requires_the_chain_record() {
        let (seed, root) = ([2; 32], [5; 32]);
        // No PS_GENESIS: `?` (the gateway's claimed root is never used).
        let mut p = PrefixData::default();
        assert_eq!(genesis_line(&p, &seed, &root).0, None);
        // Another season's genesis (another seed) first, then this one's.
        p.genesis.push(([6; 32], [3; 32], "other".into()));
        assert_eq!(genesis_line(&p, &seed, &root).0, None);
        p.genesis.push((root, seed, "ours".into()));
        let (ok, detail) = genesis_line(&p, &seed, &root);
        assert_eq!(ok, Some(true));
        assert!(detail.contains("seed matches"));
        // This season's seed with another root: a contradiction.
        p.genesis.push(([7; 32], seed, "bad".into()));
        assert_eq!(genesis_line(&p, &seed, &root).0, Some(false));
    }

    #[test]
    fn first_election_requires_ps_open() {
        let root = [5; 32];
        let mut p = PrefixData::default();
        assert_eq!(election_line(&p, &root).0, None);
        // Another season's PS_OPEN (not bound) with another root: still `?`.
        p.open.push(([6; 32], "foreign".into(), false));
        assert_eq!(election_line(&p, &root).0, None);
        p.open.push((root, "ours".into(), true));
        assert_eq!(election_line(&p, &root).0, Some(true));
        // This season's PS_OPEN at another root: a contradiction.
        let mut p = PrefixData::default();
        p.open.push(([6; 32], "ours".into(), true));
        assert_eq!(election_line(&p, &root).0, Some(false));
    }
}
