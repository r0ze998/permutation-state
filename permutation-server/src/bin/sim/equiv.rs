//! `SIM_EQUIV=1` (diagnostic): rotational equivariance of the rules.

use crate::env::Env;
use crate::{seed_bytes, tick_vrf};
use permutation_rules::checks::BLOCKED_NAMES;
use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::GovAction;
use permutation_rules::mapgen::{rotate, rotate_by, rotate_world};
use permutation_rules::orders::{AttackTarget, Order};
use permutation_rules::scoring::nation_scores;
use permutation_rules::state::WorldState;
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_server::bots::{Persona, PERSONAS};
use permutation_server::driver::{seat_ai_members, AiSeason, Planner};
use permutation_server::ledger::Ledger;

/// Play seed `i` with the bots and record every tick's input; then replay
/// the same inputs, turned by `SIM_EQUIV_K` × 60°, in the world turned the
/// same way. The rules are fair to every position of a symmetric map only
/// if each nation ends with the same score both times.
pub fn equivariance(i: u32, members: &[usize], env: &Env) -> bool {
    let rules = env.rules();
    let n = members.len();
    let persona: Vec<Persona> = (0..n)
        .map(|c| PERSONAS[(c + i as usize) % PERSONAS.len()])
        .collect();
    let start = || {
        let mut s = new_season(
            &rules,
            &seed_bytes("world", i),
            &seed_bytes("season", i),
            &nation_entries(n),
        )
        .expect("genesis");
        seat_ai_members(&mut s, &rules, members).expect("seating");
        s
    };
    let mut season = AiSeason::new(
        rules.clone(),
        start(),
        Planner::with_personas(&persona),
        Ledger::seeded(&seed_bytes("ledger", i)),
    );
    let mut inputs = Vec::new();
    let mut history = vec![season.state.clone()];
    while !season.over() {
        // The seed is not part of the randomness here.
        let input = season.plan(tick_vrf(season.state.tick, 0));
        season.resolve(&input).expect("tick");
        inputs.push(input);
        history.push(season.state.clone());
    }
    let a = nation_scores(&season.state, &rules);
    let mut t = start();
    let asym = t
        .map
        .tiles
        .iter()
        .filter(|x| {
            let y = t.map.tile(rotate(x.hex)).unwrap();
            (y.terrain, y.river, y.resource) != (x.terrain, x.river, x.resource)
        })
        .count();
    if asym > 0 {
        eprintln!(
            "seed {i}: the genesis map is not symmetric: {asym} tiles differ from their 60° turn"
        );
    }
    let k = env.equiv_k;
    rotate_world(&mut t, k);
    for mut input in inputs {
        for b in &mut input.batches {
            b.orders.iter_mut().for_each(|o| o.rotate(k));
        }
        for e in &mut input.gov {
            if let GovAction::Propose { orders, .. } = &mut e.action {
                orders.iter_mut().for_each(|o| o.rotate(k));
            }
        }
        let before = t.clone();
        resolve_tick(&mut t, &rules, &input).expect("tick");
        if env.equiv_debug {
            if report_divergence(&history, &before, &t, &input, k)
                || report_impassable(&before, &t, &input)
            {
                std::process::exit(0);
            }
            if t.tick < 40 {
                print_skips(&t);
            }
        }
    }
    let b = nation_scores(&t, &rules);
    let same = a
        .iter()
        .zip(&b)
        .all(|(x, y)| x.total() == y.total() && x.tiers == y.tiers);
    println!(
        "seed {i}: {} {:?} vs {:?}",
        if same { "same" } else { "DIFFERENT" },
        a.iter().map(|x| x.total()).collect::<Vec<_>>(),
        b.iter().map(|x| x.total()).collect::<Vec<_>>()
    );
    same
}

/// `SIM_EQUIV_DEBUG`: prints the first difference between run A (`history`)
/// and the turned run B (`t`, resolved from `before` with `input`): a civ's
/// stores, a city, a tile's owner, or a unit's position. Returns whether
/// it printed one.
fn report_divergence(
    history: &[WorldState],
    before: &WorldState,
    t: &WorldState,
    input: &TickInput,
    k: u8,
) -> bool {
    let a_now = &history[t.tick as usize];
    for (ca, cb) in a_now.civs.iter().zip(&t.civs) {
        if (ca.gold, ca.science_store, ca.influence, ca.iron, ca.horses)
            != (cb.gold, cb.science_store, cb.influence, cb.iron, cb.horses)
        {
            eprintln!("first divergence after tick {}: civ {} gold {} vs {}, science {} vs {}, influence {} vs {}, iron {} vs {}, horses {} vs {}", before.tick, ca.id, ca.gold, cb.gold, ca.science_store, cb.science_store, ca.influence, cb.influence, ca.iron, cb.iron, ca.horses, cb.horses);
            let a0 = &history[t.tick as usize - 1];
            for (x, y) in a0.cities.iter().zip(&before.cities) {
                if x.owner == Some(ca.id) {
                    eprintln!(
                        "  city {} pop {} vs {} focus {:?} tiles A {} B {}",
                        x.id,
                        x.pop,
                        y.pop,
                        x.focus,
                        a0.map
                            .tiles
                            .iter()
                            .filter(|q| q.owner_city == Some(x.id))
                            .count(),
                        before
                            .map
                            .tiles
                            .iter()
                            .filter(|q| q.owner_city == Some(y.id))
                            .count()
                    );
                }
            }
            return true;
        }
    }
    for (ca, cb) in a_now.cities.iter().zip(&t.cities) {
        if (ca.pop, ca.owner, ca.alive, ca.food, ca.prod)
            != (cb.pop, cb.owner, cb.alive, cb.food, cb.prod)
            || ca.queue != cb.queue
        {
            eprintln!("first divergence after tick {}: city {} pop {}/{} food {}/{} prod {}/{} queue {:?}/{:?}", before.tick, ca.id, ca.pop, cb.pop, ca.food, cb.food, ca.prod, cb.prod, ca.queue, cb.queue);
            return true;
        }
    }
    for ta in &a_now.map.tiles {
        let tb = t.map.tile(rotate_by(ta.hex, k)).unwrap();
        if ta.owner_city != tb.owner_city {
            eprintln!(
                "first divergence after tick {}: tile {:?} owner {:?} vs {:?}",
                before.tick, ta.hex, ta.owner_city, tb.owner_city
            );
            return true;
        }
    }
    for (ua, ub) in a_now.units.iter().zip(&t.units) {
        if ua.alive != ub.alive
            || (ua.alive && rotate_by(ua.hex, k) != ub.hex)
            || ua.troops != ub.troops
        {
            let ua0 = &history[t.tick as usize - 1].units.get(ua.id as usize);
            eprintln!("first divergence after tick {}: unit {} {:?} owner {:?}: A {:?} (turned {:?}) alive {} troops {} | B {:?} alive {} troops {}; before A {:?}",
                before.tick, ua.id, ua.unit_type, ua.owner, ua.hex, rotate_by(ua.hex, k), ua.alive, ua.troops, ub.hex, ub.alive, ub.troops, ua0.map(|x| (x.hex, x.path.clone())));
            let orders: Vec<_> = input
                .batches
                .iter()
                .flat_map(|b| b.orders.iter())
                .filter(|o| matches!(o, Order::MoveUnit { unit, .. } | Order::Attack { army: unit, .. } if *unit == ua.id))
                .collect();
            eprintln!("  its orders (turned): {orders:?}");
            let against: Vec<_> = input
                .batches
                .iter()
                .flat_map(|b| b.orders.iter().map(move |o| (b.civ, o)))
                .filter(|(_, o)| matches!(o, Order::Attack { target: AttackTarget::Unit(x), .. } if *x == ua.id))
                .collect();
            eprintln!(
                "  attacks on it: {against:?}; implicit A {:?} B {:?}",
                a_now.implicit, t.implicit
            );
            return true;
        }
    }
    false
}

/// `SIM_EQUIV_DEBUG`: prints the first move skipped as `Impassable` in run
/// B, with the terrain along its path. Returns whether it printed one.
fn report_impassable(before: &WorldState, t: &WorldState, input: &TickInput) -> bool {
    let Some(skip) = t
        .last_skipped
        .iter()
        .find(|k| BLOCKED_NAMES[k.reason as usize] == "Impassable")
    else {
        return false;
    };
    let b = input
        .batches
        .iter()
        .find(|b| b.civ == skip.civ && b.role as u8 == skip.role);
    let Some(Order::MoveUnit { unit, path }) = b.and_then(|b| b.orders.get(skip.index as usize))
    else {
        return false;
    };
    let u = &before.units[*unit as usize];
    eprintln!(
        "tick {}: unit {unit} at {:?} path {:?} terrains {:?}",
        before.tick,
        u.hex,
        path,
        path.iter()
            .map(|h| before.map.tile(*h).map(|x| x.terrain))
            .collect::<Vec<_>>()
    );
    true
}

/// `SIM_EQUIV_DEBUG`: every order run B skipped in the tick just resolved.
fn print_skips(t: &WorldState) {
    for k in &t.last_skipped {
        eprintln!(
            "tick {} civ {} role {} index {} {}",
            t.tick - 1,
            k.civ,
            k.role,
            k.index,
            BLOCKED_NAMES[k.reason as usize]
        );
    }
}
