//! One simulated season: setup, the tick loop with its timeline metrics,
//! and the settlement (plain and V5 §18).

use crate::env::Env;
use crate::{fractions, seed_bytes, tick_vrf, ENTRY_FEE};
use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::MemberId;
use permutation_rules::invariants;
use permutation_rules::mapgen::rotate_world;
use permutation_rules::payout::{settle, settle_with, Extras, Settlement};
use permutation_rules::roster::{bounties, home_city};
use permutation_rules::scoring::{captured_held, nation_scores};
use permutation_rules::state::{CivId, WorldState};
use permutation_rules::tech::Tech;
use permutation_rules::tick::{run_phase, TickInput};
use permutation_rules::Ruleset;
use permutation_server::bots::{city_count, Persona, PERSONAS};
use permutation_server::driver::{seat_ai_members, AiSeason, Planner};
use permutation_server::ledger::Ledger;
use std::collections::BTreeSet;

pub struct Season {
    pub persona: Vec<Persona>,
    pub members: Vec<usize>,
    pub tiers: Vec<[u8; 4]>,
    pub era: Vec<u8>,
    pub points: Vec<u64>,
    pub share: Vec<f64>,
    pub cities: Vec<usize>,
    pub captured: Vec<u32>,
    pub budget_use: f64,
    /// Payout per member: (nation, officer ever, merit total, usdc).
    pub member_pay: Vec<(CivId, bool, u64, u64)>,
    pub adopted: u32,
    pub recalls: u32,
    pub merit_by_path: [u64; 5],
    pub violations: usize,
    /// First tick each nation stood in era 1, 2 and 3 (provisional score).
    pub era_tick: Vec<[Option<u16>; 3]>,
    /// Leader among nations with members at tick 120, and at the end.
    pub leader_t120: Option<usize>,
    pub leader_end: Option<usize>,
    pub wars: u32,
    pub captures: u32,
    /// Nations that held a trade hub at some tick.
    pub hub_holders: usize,
    /// Largest share of ticks one nation held one hub.
    pub hub_hold_max: f64,
    pub chivalry_t150: Vec<bool>,
    /// Skipped orders per civ, by `Blocked` code.
    pub skips: Vec<[u32; 64]>,
    pub s18: Sec18,
}

/// V5 §18: operator AI members (the first `SIM_AI` members of every nation),
/// their bounties, the redistribution of their payouts, and contracts.
#[derive(Default)]
pub struct Sec18 {
    pub ais: u32,
    /// AIs whose home city was conquered (bounty paid), and conquered by a
    /// recent pact partner (no bounty).
    pub homes_paid: u32,
    pub homes_voided: u32,
    pub bounty_total: u64,
    pub redistributed: u64,
    /// Nation shares with bounties added (fractions of what was paid).
    pub share: Vec<f64>,
    /// People's payout with and without the AI roster.
    pub people_with: u64,
    pub people_without: u64,
    pub contracts_offered: u32,
    pub contracts_accepted: u32,
    pub contract_paid: u64,
}

/// The operator's AI members: the roster flag per member, and each AI's
/// nation and home-city salt.
struct Roster {
    roster: Vec<bool>,
    ais: Vec<(CivId, [u8; 32])>,
}

/// Leader by points among nations with members (ties: lowest id).
fn leader(points: &[u64], members: &[usize]) -> Option<usize> {
    (0..points.len())
        .filter(|c| members[*c] > 0)
        .max_by_key(|c| (points[*c], std::cmp::Reverse(*c)))
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

/// Genesis with the seated members, run by the bots. `SIM_PERSONA` makes
/// every nation play that persona (for isolating effects); otherwise
/// personas rotate with the seed. `SIM_TREASURY` is every nation's opening
/// treasury, and `SIM_ROTATE` turns the genesis world by k × 60° (the rules
/// and bots should not care which way the world faces).
fn start(i: u32, members: &[usize], env: &Env) -> (AiSeason, Vec<Persona>) {
    let rules = env.rules();
    let n = members.len();
    let persona: Vec<Persona> = (0..n)
        .map(|c| {
            env.persona
                .unwrap_or(PERSONAS[(c + i as usize) % PERSONAS.len()])
        })
        .collect();
    let mut entries = nation_entries(n);
    for e in &mut entries {
        e.treasury = env.treasury;
    }
    let mut s: WorldState = new_season(
        &rules,
        &seed_bytes("world", i),
        &seed_bytes("season", i),
        &entries,
    )
    .expect("genesis");
    if let Some(k) = env.rotate {
        rotate_world(&mut s, k);
    }
    seat_ai_members(&mut s, &rules, members).expect("registration and first election");
    let season = AiSeason::new(
        rules,
        s,
        Planner::with_personas(&persona),
        Ledger::seeded(&seed_bytes("ledger", i)),
    );
    (season, persona)
}

/// V5 §18: the first `SIM_AI` members of each nation are the operator's AI
/// members, each with a home-city salt drawn from the seed.
fn pick_ais(s: &WorldState, i: u32, n: usize, ai_k: usize) -> Roster {
    let mut roster = vec![false; s.members.len()];
    let mut ais: Vec<(CivId, [u8; 32])> = Vec::new();
    for c in 0..n {
        for id in (0..s.members.len())
            .filter(|x| s.members[*x].civ as usize == c)
            .take(ai_k)
        {
            roster[id] = true;
            let mut salt = seed_bytes("salt", i);
            salt[28..].copy_from_slice(&(id as u32).to_le_bytes());
            ais.push((c as CivId, salt));
        }
    }
    Roster { roster, ais }
}

/// `SIM_HOMEAWARE=1`: every nation's armies know the others' AI home
/// cities once they are drawn (a leak) and march on them first.
fn aim_at_ai_homes(season: &mut AiSeason, ais: &[(CivId, [u8; 32])]) {
    let homes: Vec<(CivId, Option<u32>)> = ais
        .iter()
        .map(|(c, salt)| (*c, home_city(&season.state, *c, salt)))
        .collect();
    for bot in &mut season.planner.bots {
        bot.targets = homes
            .iter()
            .filter(|(c, _)| *c != bot.civ)
            .filter_map(|(_, h)| *h)
            .collect();
    }
}

/// The state a tick's metrics compare against, taken before it resolves.
struct Before {
    holders: Vec<[MemberId; 4]>,
    war: Vec<bool>,
    owner: Vec<(bool, Option<CivId>)>,
}

/// Metrics gathered tick by tick (not part of the rules).
struct Timeline {
    n: usize,
    officers_ever: Vec<bool>,
    spent: u64,
    available: u64,
    recalls: u32,
    violations: usize,
    era_tick: Vec<[Option<u16>; 3]>,
    leader_t120: Option<usize>,
    wars: u32,
    captures: u32,
    chivalry_t150: Vec<bool>,
    /// Ticks each nation held each hub.
    hub_ticks: Vec<Vec<u32>>,
    ticks: u32,
    skips: Vec<[u32; 64]>,
    accepted_ids: BTreeSet<u32>,
}

impl Timeline {
    fn new(s: &WorldState, n: usize) -> Timeline {
        Timeline {
            n,
            officers_ever: vec![false; s.members.len()],
            spent: 0,
            available: 0,
            recalls: 0,
            violations: 0,
            era_tick: vec![[None; 3]; n],
            leader_t120: None,
            wars: 0,
            captures: 0,
            chivalry_t150: vec![false; n],
            hub_ticks: vec![vec![0u32; n]; s.hubs.len()],
            ticks: 0,
            skips: vec![[0u32; 64]; n],
            accepted_ids: BTreeSet::new(),
        }
    }

    /// Marks the members in office at the start of the tick.
    fn note_officers(&mut self, s: &WorldState) {
        for nat in &s.nations {
            for m in nat.offices {
                if let Some(o) = self.officers_ever.get_mut(m as usize) {
                    *o = true;
                }
            }
        }
    }

    /// Counts the planned orders' cost against the budget and snapshots
    /// what `after` compares.
    fn before(&mut self, s: &WorldState, input: &TickInput) -> Before {
        for b in &input.batches {
            self.spent += b.orders.iter().map(|o| o.cost() as u64).sum::<u64>();
        }
        for c in 0..self.n {
            self.available += s.civs[c].tick_budget as u64;
        }
        Before {
            holders: s.nations.iter().map(|x| x.offices).collect(),
            war: war_pairs(s, self.n),
            owner: s.cities.iter().map(|c| (c.alive, c.owner)).collect(),
        }
    }

    /// Records the resolved tick; reports invariant violations on stderr.
    fn after(&mut self, i: u32, s: &WorldState, rules: &Ruleset, b: &Before, members: &[usize]) {
        for (c, before) in b.holders.iter().enumerate() {
            let now = s.nations[c].offices;
            if s.tick % rules.term_ticks != 0 {
                self.recalls += before.iter().zip(now).filter(|(a, b)| **a != *b).count() as u32;
            }
        }
        let v = invariants::check(s, rules);
        if !v.is_empty() {
            self.violations += v.len();
            eprintln!("seed {i}: invariant violated at tick {}: {v:?}", s.tick);
        }
        for k in &s.last_skipped {
            self.skips[k.civ as usize][k.reason as usize % 64] += 1;
        }
        for c in s.contracts.iter().filter(|c| c.accepted.is_some()) {
            self.accepted_ids.insert(c.id);
        }
        self.wars += war_pairs(s, self.n)
            .iter()
            .zip(&b.war)
            .filter(|(now, before)| **now && !**before)
            .count() as u32;
        self.captures += s
            .cities
            .iter()
            .zip(&b.owner)
            .filter(|(c, (alive, owner))| {
                *alive && c.alive && owner.is_some() && c.owner.is_some() && c.owner != *owner
            })
            .count() as u32;
        let hubs = self.hub_ticks.len();
        for (h, hex) in s.hubs.iter().enumerate().take(hubs) {
            if let Some(c) = s.territory_owner(*hex) {
                self.hub_ticks[h][c as usize] += 1;
            }
        }
        self.ticks += 1;
        let scores = nation_scores(s, rules);
        for (c, sc) in scores.iter().enumerate().take(self.n) {
            for e in 1..=3u8 {
                if sc.era >= e && self.era_tick[c][e as usize - 1].is_none() {
                    self.era_tick[c][e as usize - 1] = Some(s.tick);
                }
            }
        }
        if s.tick == 120 {
            let pts: Vec<u64> = scores.iter().map(|x| x.total()).collect();
            self.leader_t120 = leader(&pts, members);
        }
        if s.tick == 150 {
            for (c, has) in self.chivalry_t150.iter_mut().enumerate() {
                *has = s.civs[c].techs.has(Tech::Chivalry);
            }
        }
    }
}

pub fn play(i: u32, members: &[usize], env: &Env) -> Season {
    let n = members.len();
    let (mut season, persona) = start(i, members, env);
    let mut tl = Timeline::new(&season.state, n);
    let ai = pick_ais(&season.state, i, n, env.ai);
    while !season.over() {
        tl.note_officers(&season.state);
        let input = season.plan(tick_vrf(season.state.tick, i));
        let before = tl.before(&season.state, &input);
        if env.debug == Some((i, season.state.tick)) {
            debug_tick(&mut season.state, &season.rules, &input);
            season.fog.update(&season.state);
        } else {
            season.resolve(&input).expect("tick");
        }
        tl.after(i, &season.state, &season.rules, &before, members);
        if env.home_aware && season.state.tick == season.rules.ai_home_tick + 1 {
            aim_at_ai_homes(&mut season, &ai.ais);
        }
    }
    let (s, rules) = (&season.state, &season.rules);
    let pool = ENTRY_FEE * s.members.len() as u64 * 8 / 10;
    let p = settle(s, rules, pool, ENTRY_FEE);
    let s18 = sec18(s, rules, pool, &p, &ai, env.bounty, tl.accepted_ids.len());
    let mut merit_by_path = [0u64; 5];
    for m in &s.members {
        for (k, x) in m.merit.iter().enumerate() {
            merit_by_path[k] += *x as u64;
        }
    }
    let points: Vec<u64> = p.scores.iter().map(|x| x.total()).collect();
    let hub_holders = (0..n)
        .filter(|c| tl.hub_ticks.iter().any(|h| h[*c] > 0))
        .count();
    let hub_hold_max = tl
        .hub_ticks
        .iter()
        .flatten()
        .map(|t| *t as f64 / tl.ticks.max(1) as f64)
        .fold(0.0, f64::max);
    Season {
        leader_end: leader(&points, members),
        hub_holders,
        hub_hold_max,
        persona,
        members: members.to_vec(),
        tiers: p.scores.iter().map(|x| x.tiers).collect(),
        era: p.scores.iter().map(|x| x.era).collect(),
        points,
        share: fractions(&p.nation_share),
        cities: (0..n).map(|c| city_count(s, c as CivId)).collect(),
        captured: (0..n).map(|c| captured_held(s, c as CivId)).collect(),
        budget_use: if tl.available == 0 {
            0.0
        } else {
            tl.spent as f64 / tl.available as f64
        },
        member_pay: s
            .members
            .iter()
            .enumerate()
            .map(|(id, m)| {
                let officer = tl.officers_ever[id];
                (m.civ, officer, m.merit_total(), p.per_member[id])
            })
            .collect(),
        adopted: s.nations.iter().map(|x| x.adopted).sum::<u32>(),
        recalls: tl.recalls,
        merit_by_path,
        violations: tl.violations,
        era_tick: tl.era_tick,
        leader_t120: tl.leader_t120,
        wars: tl.wars,
        captures: tl.captures,
        chivalry_t150: tl.chivalry_t150,
        skips: tl.skips,
        s18,
    }
}

/// V5 §18 settlement: the AI roster's bounties (`bounty_each` per conquered
/// AI home) and redistributed payouts, against the plain settlement `p`.
fn sec18(
    s: &WorldState,
    rules: &Ruleset,
    pool: u64,
    p: &Settlement,
    ai: &Roster,
    bounty_each: u64,
    contracts_accepted: usize,
) -> Sec18 {
    let b = bounties(s, &ai.ais, bounty_each);
    let p18 = settle_with(
        s,
        rules,
        pool + b.unpaid,
        ENTRY_FEE,
        &Extras {
            roster: &ai.roster,
            bounty: &b.by_civ,
        },
    );
    let voided = b
        .homes
        .iter()
        .filter(|h| {
            h.and_then(|c| s.cities.get(c as usize))
                .and_then(|c| c.first_conquest)
                .is_some_and(|q| !q.bounty)
        })
        .count() as u32;
    let people = |x: &Settlement| -> u64 {
        (0..s.members.len())
            .filter(|i| !ai.roster[*i])
            .map(|i| x.per_member[i])
            .sum()
    };
    Sec18 {
        ais: ai.ais.len() as u32,
        homes_paid: (b.by_civ.iter().sum::<u64>() / bounty_each.max(1)) as u32,
        homes_voided: voided,
        bounty_total: p18.bounty.iter().sum(),
        redistributed: p18.redistributed,
        share: fractions(&p18.nation_share),
        people_with: people(&p18),
        people_without: people(p),
        contracts_offered: s.next_contract,
        contracts_accepted: contracts_accepted as u32,
        contract_paid: s.civs.iter().map(|c| c.contract_income).sum(),
    }
}

/// `SIM_DEBUG="seed:tick"`: resolve the open tick phase by phase, stopping
/// the simulation at the first phase that breaks an invariant (with the
/// units and city involved).
fn debug_tick(s: &mut WorldState, rules: &Ruleset, input: &TickInput) {
    let tick = s.tick;
    while s.tick == tick {
        let (p, before) = (s.phase_cursor, s.clone());
        run_phase(s, rules, input, p).expect("phase");
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
