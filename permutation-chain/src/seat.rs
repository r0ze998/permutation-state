//! Seating a registered member in the world (`SeatMembers`), as pure
//! functions so the replay verifier seats exactly the same.
//!
//! `Register` cannot compare a session key with the other members' (each is
//! in its own account), and every session key is public in its Member
//! account, so two members can register the same one: by accident (one
//! browser, two wallets) or on purpose (a copied key). The rules seat a key
//! only once (`gov::join` refuses it with `AlreadyMember`), members are
//! seated in registration order, and `OpenGovernment` needs every member
//! seated, so one duplicate used to stop the season in `Seating` for good,
//! with every entry fee locked (there is no refund).
//!
//! Now the first member to register a key is seated with it, and a later one
//! with a substitute nobody can sign with:
//! `sha256("permutation-state/duplicate-session" ‖ season_id u64le ‖ wallet)`.
//! That member cannot sign orders or governance this season (its wallet
//! still claims whatever the season pays it); its registered candidacy and
//! votes still count, signed with the key it was seated with. If even the
//! substitute is taken (only a member who registered that very value as its
//! session key can hold it), the next candidate is
//! `sha256(domain ‖ season_id ‖ wallet ‖ previous)`, until one is free.

use permutation_rules::gov::{self, GovAction, GovEntry, Role, NOBODY};
use permutation_rules::state::WorldState;
use permutation_rules::{RulesError, Ruleset};
use solana_program::hash::hashv;

use crate::state::MemberAccount;

/// Domain of the substitute key for a duplicate session key.
pub const DUPLICATE_SESSION: &[u8] = b"permutation-state/duplicate-session";

/// One member as a `PS_SEAT` record lists it: nation, the key it was seated
/// with (its session key, or the substitute), candidacy and first-election
/// votes.
pub type Seat = (u16, [u8; 32], u8, [u32; 4]);

/// The substitute for a duplicate session key of `wallet`'s member.
pub fn substitute_key(season_id: u64, wallet: &[u8; 32]) -> [u8; 32] {
    hashv(&[DUPLICATE_SESSION, &season_id.to_le_bytes(), wallet]).to_bytes()
}

fn is_seated(state: &WorldState, key: &[u8; 32]) -> bool {
    state.members.iter().any(|x| x.key == *key)
}

/// The key `m` is seated with, given the members seated before it: its
/// session key unless one of them holds it already, else the substitute.
pub fn seat_key(state: &WorldState, m: &MemberAccount) -> [u8; 32] {
    if !is_seated(state, &m.session) {
        return m.session;
    }
    let id = m.season_id.to_le_bytes();
    let mut key = substitute_key(m.season_id, &m.wallet);
    // Each step lands on a seated key only if a member registered exactly
    // that value; the candidates differ (short of a sha256 cycle), so after
    // one step per seated key the next one is free.
    for _ in 0..state.members.len() {
        if !is_seated(state, &key) {
            break;
        }
        key = hashv(&[DUPLICATE_SESSION, &id, &m.wallet, &key]).to_bytes();
    }
    key
}

/// Seat `m`, the next member in registration order: join its nation with
/// `seat_key`, then apply its pre-season candidacy and votes, signed by that
/// key (the rules ignore an entry whose signer is not the member's key).
/// Returns the member as `PS_SEAT` lists it.
pub fn seat_member(
    state: &mut WorldState,
    rules: &Ruleset,
    m: &MemberAccount,
) -> Result<Seat, RulesError> {
    let key = seat_key(state, m);
    let id = gov::join(state, rules, m.civ, key)?;
    let mut entries = vec![GovEntry {
        member: id,
        signer: key,
        action: GovAction::Stand { roles: m.stand },
    }];
    for role in Role::ALL {
        let candidate = m.votes[role.index()];
        if candidate != NOBODY {
            entries.push(GovEntry {
                member: id,
                signer: key,
                action: GovAction::Vote { role, candidate },
            });
        }
    }
    gov::apply_pre_season(state, rules, &entries)?;
    Ok((m.civ, key, m.stand, m.votes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::MEMBER_MAGIC;
    use permutation_rules::genesis::{nation_entries, new_season};
    use permutation_rules::Preset;

    const SEASON: u64 = 7;

    fn world() -> (Ruleset, WorldState) {
        let rules = Ruleset::new(Preset::Blitz);
        let state = new_season(&rules, &[1; 32], &[2; 32], &nation_entries(2)).unwrap();
        (rules, state)
    }

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

    #[test]
    fn the_substitute_is_the_documented_hash() {
        let wallet = [9u8; 32];
        let expected = permutation_rules::hash::sha256(&[
            b"permutation-state/duplicate-session",
            &SEASON.to_le_bytes(),
            &wallet,
        ]);
        assert_eq!(substitute_key(SEASON, &wallet), expected);
        assert_ne!(substitute_key(SEASON + 1, &wallet), expected);
    }

    #[test]
    fn a_fresh_session_key_is_seated_as_is() {
        let (rules, mut s) = world();
        for (i, key) in [[1u8; 32], [2; 32]].into_iter().enumerate() {
            let m = member(i as u32, 0, 10 + i as u8, key, 1);
            let seat = seat_member(&mut s, &rules, &m).unwrap();
            assert_eq!(seat, (0, key, 1, [NOBODY; 4]));
        }
        assert_eq!(s.members[1].key, [2; 32]);
    }

    /// The second member with a key gets the substitute; the first keeps it,
    /// and the second's candidacy and votes count (signed by the substitute).
    #[test]
    fn a_duplicate_session_key_gets_the_substitute() {
        let (rules, mut s) = world();
        let a = member(0, 0, 10, [1; 32], Role::Steward.bit());
        let mut b = member(1, 0, 11, [1; 32], Role::General.bit());
        b.votes[Role::General.index()] = 1;
        seat_member(&mut s, &rules, &a).unwrap();
        // What the program did before: the rules refuse the key twice.
        assert_eq!(
            gov::join(&mut s.clone(), &rules, 0, [1; 32]),
            Err(RulesError::AlreadyMember)
        );
        let seat = seat_member(&mut s, &rules, &b).unwrap();
        let sub = substitute_key(SEASON, &[11; 32]);
        assert_eq!(seat.1, sub);
        assert_eq!(s.members[0].key, [1; 32]);
        assert_eq!(s.members[1].key, sub);
        assert_eq!(s.members[1].standing_for, Role::General.bit());
        assert!(s.nations[0]
            .votes
            .iter()
            .any(|v| v.voter == 1 && v.role == Role::General && v.candidate == 1));
        // A third copy of the key gets its own wallet's substitute.
        let c = member(2, 1, 12, [1; 32], 0);
        assert_eq!(
            seat_member(&mut s, &rules, &c).unwrap().1,
            substitute_key(SEASON, &[12; 32])
        );
        gov::first_election(&mut s, &rules).unwrap();
        assert_eq!(s.nations[0].offices[Role::General.index()], 1);
        assert_eq!(s.nations[0].offices[Role::Steward.index()], 0);
    }

    /// A member who registered another wallet's substitute as its session key
    /// cannot block that wallet's seat: the substitute moves on.
    #[test]
    fn a_taken_substitute_moves_on() {
        let (rules, mut s) = world();
        let squat = substitute_key(SEASON, &[12; 32]);
        let members = [
            member(0, 0, 10, squat, 0),
            member(1, 0, 11, [5; 32], 0),
            member(2, 1, 12, [5; 32], 0),
        ];
        let seats: Vec<Seat> = members
            .iter()
            .map(|m| seat_member(&mut s, &rules, m).unwrap())
            .collect();
        let third = seats[2].1;
        assert_ne!(third, squat);
        assert_ne!(third, [5; 32]);
        assert_eq!(
            third,
            hashv(&[DUPLICATE_SESSION, &SEASON.to_le_bytes(), &[12; 32], &squat]).to_bytes()
        );
        assert_eq!(s.members.len(), 3);
    }
}
