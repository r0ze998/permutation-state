//! Balance measurement for Game Design V5: many Blitz seasons of AI-run
//! nations, one line per season and a summary against the calibration
//! targets of V5 §6.5.
//!
//!     cargo run --release --bin sim -- [seeds=20] [members=3,3,2,2,1,0]
//!
//! Every nation is run by hosted AI members (`driver`): they stand for
//! office, vote, propose, and the elected officers order within their
//! office. A nation with 0 members is run by the acting official alone and
//! takes no share of the pool. Personas rotate over the start positions
//! with the seed. Every decision is made from the full state (perfect
//! information).

use permutation_rules::genesis::{nation_entries, new_season, start_order};
use permutation_rules::gov::MemberId;
use permutation_rules::invariants;
use permutation_rules::payout::settle;
use permutation_rules::rng::Seed;
use permutation_rules::scoring::{nation_scores, PATH_NAMES};
use permutation_rules::state::{CivId, WorldState};
use permutation_rules::tech::Tech;
use permutation_rules::tick::TickInput;
use permutation_rules::{Preset, Ruleset};
use permutation_server::bots::{city_count, Persona, PERSONAS};
use permutation_server::driver::{seat_ai_members, AiSeason, Planner};
use permutation_server::ledger::Ledger;
use std::collections::BTreeMap;

const ENTRY_FEE: u64 = 10_000_000;

struct Season {
    persona: Vec<Persona>,
    members: Vec<usize>,
    tiers: Vec<[u8; 4]>,
    era: Vec<u8>,
    points: Vec<u64>,
    share: Vec<f64>,
    cities: Vec<usize>,
    captured: Vec<u32>,
    budget_use: f64,
    /// Payout per member: (nation, officer ever, merit total, usdc).
    member_pay: Vec<(CivId, bool, u64, u64)>,
    adopted: u32,
    recalls: u32,
    merit_by_path: [u64; 5],
    violations: usize,
    /// First tick each nation stood in era 1, 2 and 3 (provisional score).
    era_tick: Vec<[Option<u16>; 3]>,
    /// Leader among nations with members at tick 120, and at the end.
    leader_t120: Option<usize>,
    leader_end: Option<usize>,
    wars: u32,
    captures: u32,
    /// Nations that held a trade hub at some tick.
    hub_holders: usize,
    /// Largest share of ticks one nation held one hub.
    hub_hold_max: f64,
    chivalry_t150: Vec<bool>,
    /// Skipped orders per civ, by `Blocked` code.
    skips: Vec<[u32; 64]>,
}

/// Leader by points among nations with members (ties: lowest id).
fn leader(points: &[u64], members: &[usize]) -> Option<usize> {
    (0..points.len())
        .filter(|c| members[*c] > 0)
        .max_by_key(|c| (points[*c], std::cmp::Reverse(*c)))
}

fn seed_bytes(tag: &str, i: u32) -> Seed {
    let mut s = [0u8; 32];
    let t = format!("permutation-state/{tag}/sim-{i:04}");
    s[..t.len().min(32)].copy_from_slice(&t.as_bytes()[..t.len().min(32)]);
    s
}

/// Ruleset with calibration overrides from `SIM_SET`, e.g.
/// `SIM_SET="science_techs=4,9,13;prosperity_pop=12,22,32,42,55"`.
fn rules() -> Ruleset {
    let mut r = Ruleset::new(Preset::Blitz);
    let spec = std::env::var("SIM_SET").unwrap_or_default();
    for part in spec.split(';').filter(|p| !p.trim().is_empty()) {
        let (k, v) = part.split_once('=').expect("key=values");
        let v: Vec<u32> = v
            .split(',')
            .map(|x| x.trim().parse().expect("number"))
            .collect();
        let set = |dst: &mut [u32]| dst.copy_from_slice(&v[..dst.len()]);
        match k.trim() {
            "hegemony_tiles" => set(&mut r.hegemony_tiles),
            "hegemony_cities" => set(&mut r.hegemony_cities),
            "prosperity_pop" => set(&mut r.prosperity_pop),
            "prosperity_wealth" => set(&mut r.prosperity_wealth),
            "science_techs" => set(&mut r.science_techs),
            "concord_trade" => set(&mut r.concord_trade),
            "concord_partners" => set(&mut r.concord_partners),
            "concord_suzerains" => set(&mut r.concord_suzerains),
            "tier_points" => set(&mut r.tier_points),
            "conquest_min_pop" => r.conquest_min_pop = v[0],
            "peace_pact_min_war" => r.peace_pact_min_war = v[0] as u16,
            "eureka_cost_bps" => r.eureka_cost_bps = v[0],
            "crisis_targets" => r.crisis_targets = v[0] as u8,
            "crisis_pop" => r.crisis_pop = v[0],
            "crisis_loyalty" => r.crisis_loyalty = v[0] as i32,
            "crisis_interval" => r.crisis_interval = v[0] as u16,
            "dark_age_share_bps" => r.dark_age_share_bps = v[0],
            "dark_age_research_bps" => r.dark_age_research_bps = v[0],
            "dark_age_budget" => r.dark_age_budget = v[0] as u16,
            "stalemate_floor_bps" => r.stalemate_floor_bps = v[0],
            "tech_cost_per_city_bps" => r.tech_cost_per_city_bps = v[0],
            "suzerain_lock_ticks" => r.suzerain_lock_ticks = v[0] as u16,
            other => panic!("unknown SIM_SET key {other}"),
        }
    }
    r
}

fn play(i: u32, members: &[usize]) -> Season {
    let rules = rules();
    let n = members.len();
    // SIM_PERSONA=Builder (etc.): every nation plays that persona (for
    // isolating effects); otherwise personas rotate with the seed.
    let only = std::env::var("SIM_PERSONA").ok();
    let persona: Vec<Persona> = (0..n)
        .map(|c| {
            only.as_deref()
                .and_then(|name| PERSONAS.iter().copied().find(|p| p.name() == name))
                .unwrap_or(PERSONAS[(c + i as usize) % PERSONAS.len()])
        })
        .collect();
    let mut s: WorldState = new_season(
        &rules,
        &seed_bytes("world", i),
        &seed_bytes("season", i),
        &nation_entries(n),
    )
    .expect("genesis");
    // SIM_ROTATE=k (diagnostic): turn the whole genesis world by k × 60°. The
    // rules and bots should not care which way the world faces.
    if let Some(k) = std::env::var("SIM_ROTATE")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
    {
        permutation_rules::mapgen::rotate_world(&mut s, k);
    }
    seat_ai_members(&mut s, &rules, members).expect("registration and first election");
    let mut officers_ever = vec![false; s.members.len()];
    let mut season = AiSeason::new(
        rules,
        s,
        Planner::with_personas(&persona),
        Ledger::seeded(&seed_bytes("ledger", i)),
    );
    let (mut spent, mut available) = (0u64, 0u64);
    let (mut adopted, mut recalls, mut violations) = (0u32, 0u32, 0usize);
    let mut era_tick = vec![[None; 3]; n];
    let (mut leader_t120, mut wars, mut captures) = (None, 0u32, 0u32);
    let mut chivalry_t150 = vec![false; n];
    let hubs = season.state.hubs.len();
    let mut hub_ticks = vec![vec![0u32; n]; hubs];
    let mut ticks = 0u32;
    let mut skips = vec![[0u32; 64]; n];
    // SIM_DEBUG="seed:tick": check the invariants after every phase of that tick.
    let debug = std::env::var("SIM_DEBUG").ok().and_then(|d| {
        let (a, b) = d.split_once(':')?;
        Some((a.parse::<u32>().ok()?, b.parse::<u16>().ok()?))
    });
    while !season.over() {
        for nat in &season.state.nations {
            for m in nat.offices {
                if let Some(o) = officers_ever.get_mut(m as usize) {
                    *o = true;
                }
            }
        }
        let mut vrf = [0u8; 32];
        vrf[..2].copy_from_slice(&season.state.tick.to_le_bytes());
        vrf[2..6].copy_from_slice(&i.to_le_bytes());
        let input = season.plan(vrf);
        for b in &input.batches {
            spent += b.orders.iter().map(|o| o.cost() as u64).sum::<u64>();
        }
        for c in 0..n {
            available += season.state.civs[c].tick_budget as u64;
        }
        let holders_before: Vec<[MemberId; 4]> =
            season.state.nations.iter().map(|x| x.offices).collect();
        let war_before: Vec<bool> = war_pairs(&season.state, n);
        let owner_before: Vec<(bool, Option<CivId>)> = season
            .state
            .cities
            .iter()
            .map(|c| (c.alive, c.owner))
            .collect();
        if debug == Some((i, season.state.tick)) {
            debug_tick(&mut season.state, &season.rules, &input);
            season.fog.update(&season.state);
        } else {
            season.resolve(&input).expect("tick");
        }
        let (s, rules) = (&season.state, &season.rules);
        for (c, before) in holders_before.iter().enumerate() {
            let now = s.nations[c].offices;
            if s.tick % rules.term_ticks != 0 {
                recalls += before.iter().zip(now).filter(|(a, b)| **a != *b).count() as u32;
            }
        }
        let v = invariants::check(s, rules);
        if !v.is_empty() {
            violations += v.len();
            eprintln!("seed {i}: invariant violated at tick {}: {v:?}", s.tick);
        }
        for k in &s.last_skipped {
            skips[k.civ as usize][k.reason as usize % 64] += 1;
        }
        // Timeline metrics (not part of the rules).
        wars += war_pairs(s, n)
            .iter()
            .zip(&war_before)
            .filter(|(now, before)| **now && !**before)
            .count() as u32;
        captures += s
            .cities
            .iter()
            .zip(&owner_before)
            .filter(|(c, (alive, owner))| {
                *alive && c.alive && owner.is_some() && c.owner.is_some() && c.owner != *owner
            })
            .count() as u32;
        for (h, hex) in s.hubs.iter().enumerate().take(hubs) {
            if let Some(c) = s.territory_owner(*hex) {
                hub_ticks[h][c as usize] += 1;
            }
        }
        ticks += 1;
        let scores = nation_scores(s, rules);
        for (c, sc) in scores.iter().enumerate().take(n) {
            for e in 1..=3u8 {
                if sc.era >= e && era_tick[c][e as usize - 1].is_none() {
                    era_tick[c][e as usize - 1] = Some(s.tick);
                }
            }
        }
        if s.tick == 120 {
            let pts: Vec<u64> = scores.iter().map(|x| x.total()).collect();
            leader_t120 = leader(&pts, members);
        }
        if s.tick == 150 {
            for (c, has) in chivalry_t150.iter_mut().enumerate() {
                *has = s.civs[c].techs.has(Tech::Chivalry);
            }
        }
    }
    let (s, rules) = (&season.state, &season.rules);
    let pool = ENTRY_FEE * s.members.len() as u64 * 8 / 10;
    let p = settle(s, rules, pool, ENTRY_FEE);
    let total: u64 = p.nation_share.iter().sum();
    let mut merit_by_path = [0u64; 5];
    for m in &s.members {
        for (k, x) in m.merit.iter().enumerate() {
            merit_by_path[k] += *x as u64;
        }
    }
    adopted += s.nations.iter().map(|x| x.adopted).sum::<u32>();
    let points: Vec<u64> = p.scores.iter().map(|x| x.total()).collect();
    let hub_holders = (0..n)
        .filter(|c| hub_ticks.iter().any(|h| h[*c] > 0))
        .count();
    let hub_hold_max = hub_ticks
        .iter()
        .flatten()
        .map(|t| *t as f64 / ticks.max(1) as f64)
        .fold(0.0, f64::max);
    Season {
        era_tick,
        leader_t120,
        leader_end: leader(&points, members),
        wars,
        captures,
        hub_holders,
        hub_hold_max,
        chivalry_t150,
        skips,
        persona,
        members: members.to_vec(),
        tiers: p.scores.iter().map(|x| x.tiers).collect(),
        era: p.scores.iter().map(|x| x.era).collect(),
        points,
        share: p
            .nation_share
            .iter()
            .map(|x| {
                if total == 0 {
                    0.0
                } else {
                    *x as f64 / total as f64
                }
            })
            .collect(),
        cities: (0..n).map(|c| city_count(s, c as CivId)).collect(),
        captured: (0..n)
            .map(|c| permutation_rules::scoring::captured_held(s, c as CivId))
            .collect(),
        budget_use: if available == 0 {
            0.0
        } else {
            spent as f64 / available as f64
        },
        member_pay: s
            .members
            .iter()
            .enumerate()
            .map(|(id, m)| (m.civ, officers_ever[id], m.merit_total(), p.per_member[id]))
            .collect(),
        adopted,
        recalls,
        merit_by_path,
        violations,
    }
}

/// Whether each pair of civs (a < b) is at war now.
fn war_pairs(s: &WorldState, n: usize) -> Vec<bool> {
    let mut v = Vec::new();
    for a in 0..n {
        for b in a + 1..n {
            v.push(s.at_war(a as CivId, b as CivId));
        }
    }
    v
}

fn median_tick(v: &mut [u16]) -> Option<u16> {
    v.sort();
    v.get(v.len() / 2).copied()
}

/// Resolve the open tick phase by phase, stopping the simulation at the
/// first phase that breaks an invariant (with the units and city involved).
fn debug_tick(s: &mut WorldState, rules: &Ruleset, input: &TickInput) {
    let tick = s.tick;
    while s.tick == tick {
        let (p, before) = (s.phase_cursor, s.clone());
        permutation_rules::tick::run_phase(s, rules, input, p).expect("phase");
        let v = invariants::check(s, rules);
        if v.is_empty() {
            continue;
        }
        eprintln!("phase {p} broke {v:?}");
        for viol in &v {
            let Some(u) = s.units.get(viol.id as usize) else {
                continue;
            };
            for x in s.units.iter().filter(|x| x.alive && x.hex == u.hex) {
                let b = &before.units[x.id as usize];
                eprintln!("  unit {} {:?} owner {:?} troops {} at {:?} (before: at {:?} alive {} path {:?}) standing {:?}", x.id, x.unit_type, x.owner, x.troops, x.hex, b.hex, b.alive, b.path, x.standing);
            }
            let city = s.cities.iter().find(|c| c.hex == u.hex);
            eprintln!(
                "  city here: {:?}",
                city.map(|c| (c.id, c.owner, c.alive, before.cities[c.id as usize].owner))
            );
        }
        std::process::exit(1);
    }
}

/// SIM_EQUIV=1 (diagnostic): play seed `i` with the bots and record every
/// tick's input; then replay the same inputs, turned by 60°, in the world
/// turned by 60°. The rules are fair to every position of a symmetric map
/// only if each nation ends with the same score both times.
fn equivariance(i: u32, members: &[usize]) -> bool {
    use permutation_rules::gov::GovAction;
    let rules = rules();
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
        let mut vrf = [0u8; 32];
        vrf[..2].copy_from_slice(&season.state.tick.to_le_bytes());
        let input = season.plan(vrf);
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
            let y = t
                .map
                .tile(permutation_rules::mapgen::rotate(x.hex))
                .unwrap();
            (y.terrain, y.river, y.resource) != (x.terrain, x.river, x.resource)
        })
        .count();
    if asym > 0 {
        eprintln!(
            "seed {i}: the genesis map is not symmetric: {asym} tiles differ from their 60° turn"
        );
    }
    let k: u8 = std::env::var("SIM_EQUIV_K")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    permutation_rules::mapgen::rotate_world(&mut t, k);
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
        permutation_rules::tick::resolve_tick(&mut t, &rules, &input).expect("tick");
        if std::env::var("SIM_EQUIV_DEBUG").is_ok() {
            // The first unit whose position differs from run A's turned by k.
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
                    std::process::exit(0);
                }
            }
            for (ca, cb) in a_now.cities.iter().zip(&t.cities) {
                if (ca.pop, ca.owner, ca.alive, ca.food, ca.prod)
                    != (cb.pop, cb.owner, cb.alive, cb.food, cb.prod)
                    || ca.queue != cb.queue
                {
                    eprintln!("first divergence after tick {}: city {} pop {}/{} food {}/{} prod {}/{} queue {:?}/{:?}", before.tick, ca.id, ca.pop, cb.pop, ca.food, cb.food, ca.prod, cb.prod, ca.queue, cb.queue);
                    std::process::exit(0);
                }
            }
            for ta in &a_now.map.tiles {
                let tb = t
                    .map
                    .tile(permutation_rules::mapgen::rotate_by(ta.hex, k))
                    .unwrap();
                if ta.owner_city != tb.owner_city {
                    eprintln!(
                        "first divergence after tick {}: tile {:?} owner {:?} vs {:?}",
                        before.tick, ta.hex, ta.owner_city, tb.owner_city
                    );
                    std::process::exit(0);
                }
            }
            for (ua, ub) in a_now.units.iter().zip(&t.units) {
                if ua.alive != ub.alive
                    || (ua.alive && permutation_rules::mapgen::rotate_by(ua.hex, k) != ub.hex)
                    || ua.troops != ub.troops
                {
                    let ua0 = &history[t.tick as usize - 1].units.get(ua.id as usize);
                    eprintln!("first divergence after tick {}: unit {} {:?} owner {:?}: A {:?} (turned {:?}) alive {} troops {} | B {:?} alive {} troops {}; before A {:?}",
                        before.tick, ua.id, ua.unit_type, ua.owner, ua.hex, permutation_rules::mapgen::rotate_by(ua.hex, k), ua.alive, ua.troops, ub.hex, ub.alive, ub.troops, ua0.map(|x| (x.hex, x.path.clone())));
                    let orders: Vec<_> = input.batches.iter().flat_map(|b| b.orders.iter()).filter(|o| matches!(o, permutation_rules::orders::Order::MoveUnit { unit, .. } | permutation_rules::orders::Order::Attack { army: unit, .. } if *unit == ua.id)).collect();
                    eprintln!("  its orders (turned): {orders:?}");
                    let against: Vec<_> = input.batches.iter().flat_map(|b| b.orders.iter().map(move |o| (b.civ, o))).filter(|(_, o)| matches!(o, permutation_rules::orders::Order::Attack { target: permutation_rules::orders::AttackTarget::Unit(x), .. } if *x == ua.id)).collect();
                    eprintln!(
                        "  attacks on it: {against:?}; implicit A {:?} B {:?}",
                        a_now.implicit, t.implicit
                    );
                    std::process::exit(0);
                }
            }
        }
        if std::env::var("SIM_EQUIV_DEBUG").is_ok() {
            if let Some(k) = t.last_skipped.iter().find(|k| {
                permutation_rules::checks::BLOCKED_NAMES[k.reason as usize] == "Impassable"
            }) {
                let b = input
                    .batches
                    .iter()
                    .find(|b| b.civ == k.civ && b.role as u8 == k.role);
                if let Some(permutation_rules::orders::Order::MoveUnit { unit, path }) =
                    b.and_then(|b| b.orders.get(k.index as usize))
                {
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
                    std::process::exit(0);
                }
            }
        }
        if std::env::var("SIM_EQUIV_DEBUG").is_ok() && t.tick < 40 {
            for k in &t.last_skipped {
                eprintln!(
                    "tick {} civ {} role {} index {} {}",
                    t.tick - 1,
                    k.civ,
                    k.role,
                    k.index,
                    permutation_rules::checks::BLOCKED_NAMES[k.reason as usize]
                );
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if std::env::var("SIM_EQUIV").is_ok() {
        let seeds: u32 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(5);
        let members: Vec<usize> = args
            .get(2)
            .map(|a| a.split(',').filter_map(|x| x.trim().parse().ok()).collect())
            .unwrap_or_else(|| vec![3, 3, 3, 3, 3, 3]);
        let ok = (0..seeds).filter(|i| equivariance(*i, &members)).count();
        println!(
            "{ok}/{seeds} seasons end the same when the world and the orders are turned by 60°"
        );
        return;
    }
    let seeds: u32 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(20);
    let members: Vec<usize> = args
        .get(2)
        .map(|a| a.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![3, 3, 2, 2, 1, 0]);
    let mut eras: Vec<u8> = Vec::new();
    let mut era5 = 0u32;
    let mut era_by_persona: BTreeMap<&str, Vec<u8>> = BTreeMap::new();
    let mut tier_counts = [[0u32; 6]; 4];
    let mut top_share = Vec::new();
    let mut over40 = 0;
    let mut pairs: BTreeMap<(usize, usize), u32> = BTreeMap::new();
    let mut dead_points = Vec::new();
    let (mut off_pay, mut off_n, mut non_pay, mut non_n) = (0u64, 0u64, 0u64, 0u64);
    let mut per_member_nation: BTreeMap<usize, (u64, u64)> = BTreeMap::new();
    let (mut adopted, mut recalls, mut budget, mut violations) = (0u32, 0u32, 0.0, 0usize);
    let mut merit_paths = [0u64; 5];
    // Every pair of paths at tier 3+ in an era-3+ nation (a nation counts
    // for each pair it has), and how many era-3+ nations have science 3+.
    let mut pairs_all: BTreeMap<(usize, usize), u32> = BTreeMap::new();
    let (mut era3, mut era3_sci) = (0u32, 0u32);
    let mut era_ticks: [Vec<u16>; 3] = Default::default();
    let (mut lead_known, mut lead_changed) = (0u32, 0u32);
    let (mut wars, mut captures, mut hub_holders) = (0u32, 0u32, 0usize);
    let mut hub_dominated = 0u32;
    let (mut chivalry, mut counted_n) = (0u32, 0u32);
    let mut slot_points: Vec<(u64, u32)> = vec![(0, 0); members.len()];
    let mut slot_cities: Vec<u64> = vec![0; members.len()];
    let mut slot_skips: Vec<[u64; 64]> = vec![[0; 64]; members.len()];
    println!("members per nation: {members:?}");
    println!("seed | eras | points | pool share % | tiers (H/P/S/C) | cities | captured held");
    let from: u32 = std::env::var("SIM_FROM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    for i in from..seeds {
        let r = play(i, &members);
        let counted: Vec<usize> = (0..r.members.len()).filter(|c| r.members[*c] > 0).collect();
        for &c in &counted {
            eras.push(r.era[c]);
            era5 += (r.era[c] == 5) as u32;
            era_by_persona
                .entry(r.persona[c].name())
                .or_default()
                .push(r.era[c]);
            for (p, counts) in tier_counts.iter_mut().enumerate() {
                counts[r.tiers[c][p] as usize] += 1;
            }
            // Which two paths carried the nation to its era (legacy metric:
            // ties go to the higher path index, one pair per nation).
            let mut t: Vec<(u8, usize)> = r.tiers[c].iter().copied().zip(0..4).collect();
            t.sort_by(|a, b| b.cmp(a));
            if r.era[c] >= 3 {
                let (a, b) = (t[0].1.min(t[1].1), t[0].1.max(t[1].1));
                *pairs.entry((a, b)).or_default() += 1;
                era3 += 1;
                era3_sci += (r.tiers[c][2] >= 3) as u32;
                for a in 0..4 {
                    for b in a + 1..4 {
                        if r.tiers[c][a] >= 3 && r.tiers[c][b] >= 3 {
                            *pairs_all.entry((a, b)).or_default() += 1;
                        }
                    }
                }
            }
            for (k, list) in era_ticks.iter_mut().enumerate() {
                list.extend(r.era_tick[c][k]);
            }
            counted_n += 1;
            chivalry += r.chivalry_t150[c] as u32;
        }
        // By generation slot (nation c starts at slot order[c], §2.4).
        let order = start_order(&seed_bytes("season", i), r.members.len());
        for (c, slot) in order.iter().enumerate() {
            slot_points[*slot].0 += r.points[c];
            slot_points[*slot].1 += 1;
            slot_cities[*slot] += r.cities[c] as u64;
            for (k, x) in r.skips[c].iter().enumerate() {
                slot_skips[*slot][k] += *x as u64;
            }
        }
        if let (Some(a), Some(b)) = (r.leader_t120, r.leader_end) {
            lead_known += 1;
            lead_changed += (a != b) as u32;
        }
        wars += r.wars;
        captures += r.captures;
        hub_holders += r.hub_holders;
        hub_dominated += (r.hub_hold_max > 0.8) as u32;
        for c in 0..r.members.len() {
            if r.cities[c] == 0 {
                dead_points.push(r.points[c]);
            }
        }
        let top = r.share.iter().cloned().fold(0.0, f64::max);
        top_share.push(top);
        if top > 0.4 {
            over40 += 1;
        }
        for (civ, officer, _merit, pay) in &r.member_pay {
            if *officer {
                off_pay += pay;
                off_n += 1;
            } else {
                non_pay += pay;
                non_n += 1;
            }
            let e = per_member_nation
                .entry(r.members[*civ as usize])
                .or_default();
            e.0 += pay;
            e.1 += 1;
        }
        adopted += r.adopted;
        recalls += r.recalls;
        budget += r.budget_use;
        violations += r.violations;
        for (total, m) in merit_paths.iter_mut().zip(r.merit_by_path) {
            *total += m;
        }
        println!(
            "{i:4} | {:?} | {:?} | {:?} | {} | {:?} | {:?}",
            r.era,
            r.points,
            r.share
                .iter()
                .map(|x| (x * 100.0).round() as u32)
                .collect::<Vec<_>>(),
            r.tiers
                .iter()
                .map(|t| format!("{}{}{}{}", t[0], t[1], t[2], t[3]))
                .collect::<Vec<_>>()
                .join(" "),
            r.cities,
            r.captured,
        );
    }
    eras.sort();
    let median = eras.get(eras.len() / 2).copied().unwrap_or(0);
    println!("\n== {seeds} seasons, V5 §6.5 targets ==");
    println!(
        "era of nations with members: median {median} (target 2–3), distribution {:?}",
        (0..=5)
            .map(|e| eras.iter().filter(|x| **x == e).count())
            .collect::<Vec<_>>()
    );
    println!(
        "era 5 per season: {:.2} (target 0–1)",
        era5 as f64 / seeds as f64
    );
    println!(
        "top nation's pool share > 40%: {over40}/{seeds} (target: rare); mean top share {:.0}%",
        100.0 * top_share.iter().sum::<f64>() / top_share.len().max(1) as f64
    );
    let pair_names: Vec<String> = pairs
        .iter()
        .map(|((a, b), n)| format!("{}+{} {}", PATH_NAMES[*a], PATH_NAMES[*b], n))
        .collect();
    println!(
        "path pairs that reached era 3+ (legacy, exclusive): {} (target: every pair possible)",
        if pair_names.is_empty() {
            "none".into()
        } else {
            pair_names.join(", ")
        }
    );
    let pct = |a: u32, b: u32| 100.0 * a as f64 / b.max(1) as f64;
    println!(
        "path pairs at tier 3+ in era-3+ nations (every pair a nation has, % of {era3}): {}",
        (0..4)
            .flat_map(|a| (a + 1..4).map(move |b| (a, b)))
            .map(|(a, b)| {
                let k = pairs_all.get(&(a, b)).copied().unwrap_or(0);
                format!(
                    "{}+{} {k} ({:.0}%)",
                    PATH_NAMES[a],
                    PATH_NAMES[b],
                    pct(k, era3)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "era-3+ nations with science 3+: {:.0}% (target ≤ 70%)",
        pct(era3_sci, era3)
    );
    println!(
        "first tick in era 1/2/3, median: {}",
        era_ticks
            .iter_mut()
            .map(|v| median_tick(v).map_or("-".into(), |t| format!("{t} (n={})", v.len())))
            .collect::<Vec<String>>()
            .join(" / ")
    );
    println!("leader at T120 ≠ final leader: {lead_changed}/{lead_known} seasons (target ≥ 25%)");
    let per = |x: f64| x / seeds.max(1) as f64;
    println!(
        "per season: wars declared {:.1}, cities captured {:.1}, nations that held a hub {:.1}; seasons with one nation holding a hub > 80% of ticks: {hub_dominated}/{seeds}",
        per(wars as f64),
        per(captures as f64),
        per(hub_holders as f64),
    );
    println!(
        "nations with members that had Chivalry by T150: {:.0}%",
        pct(chivalry, counted_n)
    );
    println!(
        "mean points by generation slot (map fairness; nations are shuffled over slots): {}",
        slot_points
            .iter()
            .enumerate()
            .map(|(c, (sum, k))| format!("{c}:{:.0}", *sum as f64 / (*k).max(1) as f64))
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!(
        "mean cities by generation slot: {}",
        slot_cities
            .iter()
            .zip(&slot_points)
            .enumerate()
            .map(|(c, (n, (_, k)))| format!("{c}:{:.2}", *n as f64 / (*k).max(1) as f64))
            .collect::<Vec<_>>()
            .join(" ")
    );
    if std::env::var("SIM_SKIPS").is_ok() {
        for (slot, row) in slot_skips.iter().enumerate() {
            let mut top: Vec<(u64, usize)> = row
                .iter()
                .enumerate()
                .filter(|(_, x)| **x > 0)
                .map(|(k, x)| (*x, k))
                .collect();
            top.sort_by(|a, b| b.cmp(a));
            println!(
                "  slot {slot} skipped orders: {}",
                top.iter()
                    .take(6)
                    .map(|(x, k)| format!(
                        "{} {x}",
                        permutation_rules::checks::BLOCKED_NAMES
                            .get(*k)
                            .unwrap_or(&"?")
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    println!("dead nations' points: {:?} (target ≈ 0)", dead_points);
    for (p, name) in PATH_NAMES.iter().enumerate() {
        println!("  {name:10} tiers 0..5: {:?}", tier_counts[p]);
    }
    for (p, v) in &era_by_persona {
        let avg = v.iter().map(|x| *x as f64).sum::<f64>() / v.len().max(1) as f64;
        println!("  {p:9} mean era {avg:.2} over {} nations", v.len());
    }
    println!(
        "payout per member (USDC): officers {:.2}, others {:.2}; by nation size {}",
        off_pay as f64 / 1e6 / off_n.max(1) as f64,
        non_pay as f64 / 1e6 / non_n.max(1) as f64,
        per_member_nation
            .iter()
            .map(|(k, (sum, n))| format!("{k}:{:.2}", *sum as f64 / 1e6 / *n as f64))
            .collect::<Vec<_>>()
            .join(" "),
    );
    let tm: u64 = merit_paths.iter().sum();
    println!(
        "merit by path: {} ",
        ["hegemony", "prosperity", "science", "concord", "common"]
            .iter()
            .zip(merit_paths)
            .map(|(n, m)| format!("{n} {:.0}%", 100.0 * m as f64 / tm.max(1) as f64))
            .collect::<Vec<_>>()
            .join("  ")
    );
    println!(
        "proposals adopted: {:.1} per season; office changes mid-term (recalls): {:.1} per season",
        adopted as f64 / seeds as f64,
        recalls as f64 / seeds as f64
    );
    println!(
        "order budget used: {:.0}%   invariant violations: {violations}",
        100.0 * budget / seeds.max(1) as f64
    );
}
