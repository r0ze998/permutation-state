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
