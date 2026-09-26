//! Elections (V5 §5.3) after WP06: ballots live on the members, the tally is
//! one pass, and the winner and runner-up of every office are exactly what
//! the v8 algorithm (rescan every vote per standing candidate, stable sort,
//! filter by office cap) chose.

use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::{self, GovAction, GovEntry, MemberId, Role, NOBODY, NO_VOTE};
use permutation_rules::state::WorldState;
use permutation_rules::{Preset, Ruleset};

/// xorshift64*: deterministic, no dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn world(nations: usize, members: u32, rng: &mut Rng) -> (Ruleset, WorldState) {
    let mut rules = Ruleset::new(Preset::Blitz);
    // The algorithm is compared beyond the season's member cap (48) too.
    rules.max_members = 256;
    let mut s = new_season(&rules, &[1; 32], &[2; 32], &nation_entries(nations)).unwrap();
    for i in 0..members {
        let mut key = [9u8; 32];
        key[..4].copy_from_slice(&i.to_le_bytes());
        // Skewed: nation 0 gets about half of everyone.
        let civ = if rng.below(2) == 0 {
            0
        } else {
            rng.below(nations as u64) as u16
        };
        gov::join(&mut s, &rules, civ, key).unwrap();
    }
    (rules, s)
}

/// The v8 election (terms.rs at 96a3464), over the ballots as a vote list.
fn reference(s: &WorldState, rules: &Ruleset, start: u16) -> Vec<([MemberId; 4], [MemberId; 4])> {
    let seed = permutation_rules::hash::sha256(&[
        b"PS/election",
        &s.season_seed,
        &s.tick_seed,
        &start.to_le_bytes(),
    ]);
    let mut out = Vec::new();
    for civ in 0..s.nations.len() {
        // The nation's votes: its own members' ballots.
        let votes: Vec<(MemberId, Role, MemberId)> = s
            .members
            .iter()
            .enumerate()
            .filter(|(_, m)| m.civ as usize == civ)
            .flat_map(|(v, _)| {
                Role::ALL
                    .into_iter()
                    .filter(move |r| s.vote(v as MemberId, *r) != NOBODY)
                    .map(move |r| (v as MemberId, r, s.vote(v as MemberId, r)))
            })
            .collect();
        let mut new_offices = [NOBODY; 4];
        let mut runners = [NOBODY; 4];
        for role in Role::ALL {
            let mut ranked: Vec<(u32, u64, MemberId)> = s
                .members
                .iter()
                .enumerate()
                .filter(|(_, m)| m.civ as usize == civ && m.standing_for & role.bit() != 0)
                .map(|(id, _)| {
                    let id = id as MemberId;
                    let n = votes.iter().filter(|v| v.1 == role && v.2 == id).count() as u32;
                    let key = ((civ as u64) << 40) | ((role as u64) << 32) | id as u64;
                    (n, permutation_rules::rng::tie_key(&seed, key), id)
                })
                .collect();
            ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            let eligible: Vec<MemberId> = ranked
                .into_iter()
                .map(|x| x.2)
                .filter(|id| {
                    new_offices.iter().filter(|o| *o == id).count()
                        < rules.max_offices_per_member as usize
                })
                .collect();
            new_offices[role.index()] = eligible.first().copied().unwrap_or(NOBODY);
            runners[role.index()] = eligible.get(1).copied().unwrap_or(NOBODY);
        }
        out.push((new_offices, runners));
    }
    out
}

#[test]
fn the_single_pass_election_elects_exactly_what_v8_did() {
    let mut rng = Rng(0x5eed_0006);
    for case in 0..400 {
        let nations = 2 + rng.below(5) as usize;
        let members = rng.below(80) as u32;
        let (rules, mut s) = world(nations, members, &mut rng);
        // Few distinct vote counts so ties are common.
        for i in 0..members as usize {
            s.members[i].standing_for = rng.below(16) as u8;
            for r in 0..4 {
                let c = match rng.below(8) {
                    0 | 1 => NOBODY,
                    2 => members + rng.below(5) as u32, // not a member (pre-season votes are unchecked)
                    3 => u32::MAX - 1,
                    _ => rng.below(members.max(1) as u64) as u32, // any member, maybe of another nation
                };
                s.cast(i as MemberId, Role::ALL[r], c);
            }
        }
        let want = reference(&s, &rules, 30);
        gov::run_election(&mut s, &rules, 30);
        for (civ, (offices, runners)) in want.iter().enumerate() {
            assert_eq!(
                s.nations[civ].offices, *offices,
                "case {case} nation {civ} offices"
            );
            assert_eq!(
                s.nations[civ].runner_up, *runners,
                "case {case} nation {civ} runners-up"
            );
        }
        assert!(
            s.ballots.iter().all(|b| *b == [NO_VOTE; 4]),
            "every ballot is spent"
        );
    }
}

#[test]
fn a_new_vote_replaces_the_last_one_for_that_office() {
    let mut rng = Rng(1);
    let (rules, mut s) = world(2, 4, &mut rng);
    s.tick = rules.term_ticks - 1; // inside the vote window
    let civ0: Vec<MemberId> = (0..4)
        .filter(|i| s.members[*i as usize].civ == s.members[0].civ)
        .collect();
    let key = s.members[0].key;
    let vote = |c| GovEntry {
        member: 0,
        signer: key,
        action: GovAction::Vote {
            role: Role::Science,
            candidate: c,
        },
    };
    gov::apply_actions(
        &mut s,
        &rules,
        &[vote(civ0[0]), vote(*civ0.last().unwrap())],
    );
    assert_eq!(
        s.ballots[0],
        [NO_VOTE, NO_VOTE, *civ0.last().unwrap() as u16, NO_VOTE]
    );
    // A vote for a member of another nation is refused in season and leaves the ballot alone.
    if let Some(other) = (0..4).find(|i| s.members[*i as usize].civ != s.members[0].civ) {
        gov::apply_actions(&mut s, &rules, &[vote(other)]);
        assert_eq!(s.vote(0, Role::Science), *civ0.last().unwrap());
    }
}

/// The largest allowed nation: every member of a full season in one nation,
/// all standing for every office, four votes each. The election touches each
/// ballot once and each (member, office) once: count the work through the
/// tally size and check the result is sane (the CU bound itself is measured
/// on the SBF build by permutation-chain/svm-tests).
#[test]
fn a_full_season_in_one_nation_elects_in_one_pass() {
    let rules = Ruleset::new(Preset::Blitz);
    let mut s = new_season(&rules, &[1; 32], &[2; 32], &nation_entries(6)).unwrap();
    let n = rules.max_members;
    for i in 0..n {
        let mut key = [9u8; 32];
        key[..4].copy_from_slice(&i.to_le_bytes());
        gov::join(&mut s, &rules, 0, key).unwrap();
        s.members[i as usize].standing_for = 0x0f;
    }
    for i in 0..n as usize {
        let i32 = i as u32;
        s.cast(i32, Role::General, (i32 + 1) % n);
        s.cast(i32, Role::Steward, (i32 + 2) % n);
        s.cast(i32, Role::Science, (i32 + 3) % n);
        s.cast(i32, Role::Diplomat, 7);
    }
    let t = gov::tally(&s);
    assert_eq!(t.len(), n as usize);
    assert_eq!(t[7][3], n, "everyone voted 7 for diplomat");
    let want = reference(&s, &rules, 0);
    gov::first_election(&mut s, &rules).unwrap();
    assert_eq!(s.nations[0].offices, want[0].0);
    assert_eq!(s.nations[0].holder(Role::Diplomat), 7);
}

/// The vote-application path: random Vote/Stand sequences through
/// `apply_pre_season` and `apply_actions` (pre-season, in-season inside and
/// outside the vote window, re-votes, refused votes, wrong signers, unknown
/// members, ids past `NO_VOTE`) against a v8-style model that keeps each
/// nation's votes as a list (`retain` the voter's last vote for the office,
/// then `push`). The per-(candidate, office) counts of `gov::tally` must equal
/// the model's counts, and each ballot slot must hold the model's last vote.
#[test]
fn votes_applied_through_apply_match_the_v8_vote_list() {
    let mut rng = Rng(0x5eed_0006);
    for case in 0..300 {
        let nations = 2 + rng.below(5) as usize;
        let members = 1 + rng.below(60) as u32;
        let (rules, mut s) = world(nations, members, &mut rng);
        // v8 model: per nation, (voter, role, candidate) in list order.
        let mut model: Vec<Vec<(MemberId, Role, MemberId)>> = vec![Vec::new(); nations];
        let entries = |s: &WorldState, rng: &mut Rng, n: usize| -> Vec<GovEntry> {
            (0..n)
                .map(|_| {
                    let member = match rng.below(20) {
                        0 => members + rng.below(3) as u32, // not a member
                        _ => rng.below(members as u64) as u32,
                    };
                    let signer = match (s.members.get(member as usize), rng.below(15)) {
                        (Some(m), k) if k > 0 => m.key,
                        _ => [0xEE; 32], // wrong signer: ignored
                    };
                    let action = if rng.below(6) == 0 {
                        GovAction::Stand {
                            roles: rng.below(16) as u8,
                        }
                    } else {
                        let candidate = match rng.below(10) {
                            0 => NOBODY,
                            1 => NO_VOTE as u32,
                            2 => NO_VOTE as u32 + 1 + rng.below(5) as u32,
                            3 => members + rng.below(4) as u32,
                            _ => rng.below(members as u64) as u32,
                        };
                        GovAction::Vote {
                            role: Role::ALL[rng.below(4) as usize],
                            candidate,
                        }
                    };
                    GovEntry {
                        member,
                        signer,
                        action,
                    }
                })
                .collect()
        };
        let record = |s: &WorldState,
                      es: &[GovEntry],
                      pre: bool,
                      model: &mut Vec<Vec<(MemberId, Role, MemberId)>>| {
            for e in es {
                let Some(m) = s.members.get(e.member as usize) else {
                    continue;
                };
                if m.key != e.signer {
                    continue;
                }
                if let GovAction::Vote { role, candidate } = e.action {
                    let member_of = s
                        .members
                        .get(candidate as usize)
                        .is_some_and(|c| c.civ == m.civ);
                    if pre || (gov::vote_open(&rules, s.tick) && member_of) {
                        let list = &mut model[m.civ as usize];
                        list.retain(|v| !(v.0 == e.member && v.1 == role));
                        list.push((e.member, role, candidate));
                    }
                }
            }
        };
        let check = |s: &WorldState, model: &Vec<Vec<(MemberId, Role, MemberId)>>, when: &str| {
            let mut want = vec![[0u32; 4]; s.members.len()];
            for (civ, list) in model.iter().enumerate() {
                for (_, role, c) in list {
                    if s.members
                        .get(*c as usize)
                        .is_some_and(|x| x.civ as usize == civ)
                    {
                        want[*c as usize][role.index()] += 1;
                    }
                }
                for (v, role, c) in list {
                    let kept = if *c < NO_VOTE as u32 { *c } else { NOBODY };
                    assert_eq!(s.vote(*v, *role), kept, "case {case} {when}: ballot slot");
                }
            }
            let got = gov::tally(s);
            assert_eq!(got, want, "case {case} {when}: tally");
        };
        // Registration: every vote is taken.
        let k = rng.below(120) as usize;
        let es = entries(&s, &mut rng, k);
        record(&s, &es, true, &mut model);
        gov::apply_pre_season(&mut s, &rules, &es).unwrap();
        check(&s, &model, "pre-season");
        gov::first_election(&mut s, &rules).unwrap();
        model.iter_mut().for_each(|l| l.clear());
        // In season: a few ticks, inside and outside the vote window.
        for _ in 0..6 {
            s.tick = rng.below(rules.ticks_per_season as u64 - 1) as u16;
            let k = rng.below(80) as usize;
            let es = entries(&s, &mut rng, k);
            record(&s, &es, false, &mut model);
            gov::apply_actions(&mut s, &rules, &es);
            check(&s, &model, "in season");
        }
    }
}
