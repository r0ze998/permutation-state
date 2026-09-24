//! Human-readable chronicle events derived by diffing two consecutive states.
//! The engine itself only hash-chains events; this is presentation.

use permutation_rules::state::{CivId, Relation, WorldState};
use permutation_rules::tech::TECHS;

fn rel_code(r: Relation) -> char {
    match r {
        Relation::Peace => 'P',
        Relation::War { .. } => 'W',
        Relation::Nap { .. } => 'N',
        Relation::Alliance { .. } => 'A',
    }
}

/// `kind|text` lines for what changed between `prev` and `next`.
pub fn diff_events(prev: &WorldState, next: &WorldState, names: &[&str]) -> Vec<String> {
    let mut ev = Vec::new();
    let n = next.civs.len() as CivId;
    let nm = |c: CivId| names[c as usize];
    for a in 0..n {
        for b in a + 1..n {
            let (r0, r1) = (prev.relation(a, b), next.relation(a, b));
            if rel_code(r0) == rel_code(r1) {
                continue;
            }
            ev.push(match r1 {
                Relation::War {
                    declared_by,
                    casus_belli,
                    ..
                } => {
                    let other = if declared_by == a { b } else { a };
                    let why = if matches!(r0, Relation::Nap { .. }) {
                        " by breaking their pact"
                    } else if casus_belli {
                        " (casus belli)"
                    } else {
                        ""
                    };
                    format!(
                        "war|{} declares war on {}{}",
                        nm(declared_by),
                        nm(other),
                        why
                    )
                }
                Relation::Peace => match r0 {
                    Relation::War { .. } => format!("peace|{} and {} make peace", nm(a), nm(b)),
                    Relation::Nap { .. } => {
                        format!("diplo|The pact between {} and {} expires", nm(a), nm(b))
                    }
                    _ => format!("diplo|{} and {} end their alliance", nm(a), nm(b)),
                },
                Relation::Nap { .. } => {
                    format!("diplo|{} and {} sign a non-aggression pact", nm(a), nm(b))
                }
                Relation::Alliance { .. } => {
                    format!("ally|{} and {} form an alliance", nm(a), nm(b))
                }
            });
        }
    }
    for c in &next.cities {
        let before = prev.cities.get(c.id as usize);
        match before {
            None if c.founder != u16::MAX && c.captured_tick.is_none() => ev.push(format!(
                "found|{} founds a new city",
                nm(c.owner.unwrap_or(c.founder))
            )),
            None => ev.push(format!(
                "capture|{} conquers a city-state",
                nm(c.owner.unwrap_or(0))
            )),
            Some(b) if b.alive && !c.alive => ev.push("raze|A city is razed to a ruin".to_string()),
            Some(b) if b.alive && c.alive && b.owner != c.owner => {
                ev.push(match (b.owner, c.owner) {
                    (Some(o), Some(nw)) => {
                        format!("capture|{} captures a city of {}", nm(nw), nm(o))
                    }
                    (Some(o), None) => {
                        format!("revolt|A city of {} revolts and becomes free", nm(o))
                    }
                    (None, Some(nw)) => format!("capture|{} captures a free city", nm(nw)),
                    _ => continue,
                })
            }
            _ => {}
        }
        if let Some(b) = before {
            let (s0, s1) = (
                b.buildings.star_gate_stages(),
                c.buildings.star_gate_stages(),
            );
            if s1 > s0 {
                ev.push(format!(
                    "science|{} completes Star Gate stage {}",
                    nm(c.owner.unwrap_or(0)),
                    s1
                ));
            }
        }
    }
    for (i, cs) in next.city_states.iter().enumerate() {
        let before = &prev.city_states[i];
        if cs.suzerain != before.suzerain {
            if let Some(z) = cs.suzerain {
                ev.push(format!(
                    "diplo|{} becomes suzerain of city-state {}",
                    nm(z),
                    i + 1
                ));
            }
        }
    }
    for c in &next.civs {
        let p = &prev.civs[c.id as usize];
        for t in TECHS {
            if c.techs.has(t.tech) && !p.techs.has(t.tech) && t.era >= 3 {
                ev.push(format!("tech|{} discovers {:?}", nm(c.id), t.tech));
            }
        }
    }
    ev
}

const ROLE_NAMES: [&str; 4] = ["general", "steward", "science officer", "diplomat"];
const PATHS: [&str; 4] = ["Hegemony", "Prosperity", "Science", "Concord"];

/// Governance and achievement announcements (V5 §5.7, §6.3): office
/// changes (elections and recalls), milestones reached or lost, new eras.
/// `members[m]` is a member's display name.
pub fn diff_gov_events(prev: &WorldState, next: &WorldState, names: &[&str], members: &[String]) -> Vec<String> {
    use permutation_rules::gov::NOBODY;
    let mut ev = Vec::new();
    let who = |m: u32| if m == NOBODY { "the acting official".to_string() } else { members.get(m as usize).cloned().unwrap_or_else(|| format!("member {m}")) };
    for (c, (a, b)) in prev.nations.iter().zip(&next.nations).enumerate() {
        let nm = names[c];
        for (r, role) in ROLE_NAMES.iter().enumerate() {
            if a.offices[r] != b.offices[r] {
                let kind = if prev.tick > 0 && next.tick % 30 != 0 { "recall" } else { "gov" };
                ev.push(format!("{kind}|{nm}: {} becomes {role} (was {})", who(b.offices[r]), who(a.offices[r])));
            }
        }
        if b.adopted > a.adopted {
            ev.push(format!("gov|{nm} adopts {} proposal{}", b.adopted - a.adopted, if b.adopted - a.adopted > 1 { "s" } else { "" }));
        }
    }
    for (c, (a, b)) in prev.civs.iter().zip(&next.civs).enumerate() {
        let nm = names[c];
        for (p, path) in PATHS.iter().enumerate() {
            let (t0, t1) = (a.achievements.tiers[p], b.achievements.tiers[p]);
            if t1 > t0 {
                ev.push(format!("milestone|{nm} reaches {path} {t1}"));
            } else if t1 < t0 {
                ev.push(format!("milestone|{nm} loses {path} {t0}"));
            }
        }
        if b.achievements.era != a.achievements.era {
            ev.push(format!("era|{nm} enters era {}", b.achievements.era));
        }
    }
    ev
}
