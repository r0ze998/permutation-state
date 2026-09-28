//! Bot policies (M1 contract §8.6; offchain design §9): heuristics
//! re-implemented from the simulator's `sim.rs` (`session`, `economy`,
//! `expand`, `war`, `camp_raid`, `pick_stance`) over an [`Observation`],
//! plus the thirteen adversarial personas.
//!
//! [`decide`] is a pure function of (seed, agent, observation, memory): it
//! draws from `Rng::fork(seed, index ‖ bell)`, so the same inputs give the
//! same [`Intent`]s (the determinism test). The runner (`frontier-bots`)
//! turns intents into transactions, sends them through the relay and
//! updates the [`Memory`] from what came back.
//!
//! Order of a decision (the simulator's `session`: defence and duties
//! first, then the economy, then war):
//! 1. not joined → Join (from the agent's join bell);
//! 2. duties on own marches: owner reveal in the arrival bell (a
//!    profile-dependent share, `1 − withhold`), SettleTransit when due;
//! 3. no holding → FileTicket (sites in the own wedge's outer rings);
//! 4. a final holding, in a session: harvest, build, train, muster, explore
//!    and its settlement, then a march (camp raid or war) with the
//!    profile's aggression; personas change or add intents here.

use std::collections::{BTreeMap, BTreeSet};

use fclient::abi::layout::{citizen as lc, entry as le, holding as lh, site as ls};
use fclient::decode::{Entry, Holding, Province};
use fclient::ix::{HoldingRef, RevealArgs, SeedSource, SettleTransitArgs, Site};
use fclient::seal::{Plain, PLAIN_LEN};
use fclient::Address;
use permutation_rules::frontier::catalog;
use permutation_rules::frontier::doctrine::DOCTRINES;
use permutation_rules::frontier::geometry::ProvinceCoord;
use permutation_rules::frontier::holding::{Accrual, Tier, RESOURCES};

use crate::obs::{site_state, Observation};
use crate::path::{self, Path};
use crate::persona::Persona;
use crate::profile::{AgentSpec, Profile};
use crate::rng::Rng;

/// The Scout (explores).
pub const SCOUT: u8 = 6;
/// Smallest host (`host::MIN_HOST_TROOPS`, whole troops).
pub const MIN_HOST: u32 = 100;
pub const MAX_HOST: u32 = 30_000;
/// Wall-clock-free pacing: a march is planned to leave this many game
/// seconds after the decision (the relay, the seal and one slot).
pub const DEPART_SLACK_SECS: i64 = 60;

/// How a march's seal is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SealKind {
    /// `tlock` to `tlock_round(arrive)` over a valid plaintext.
    Honest,
    /// 165 random bytes (byte 0 a compressed-G2 flag, so Depart's syntax
    /// check passes) with a valid commitment to a valid plaintext.
    Garbage,
    /// A valid seal over an invalid plaintext (seal code 5).
    BadPlaintext,
}

/// Which way a transaction goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// Sponsored by the relay (`/f/relay`, `/f/join`) or, for a reveal, the
    /// keeper through `/f/reveal`.
    Relay,
    /// Sent by the bot itself with its own funded key (personas only).
    Direct,
}

/// A march to send.
#[derive(Clone, Debug, PartialEq)]
pub struct DepartPlan {
    pub h: HoldingRef,
    /// Where the host stands.
    pub host_at: (i16, i16),
    pub host_id: u64,
    pub transit_slot: u8,
    pub plain: Plain,
    pub tip: u64,
    pub seal: SealKind,
    pub route: Route,
    /// Path provinces other than the destination (for a later Reveal).
    pub path_others: Vec<(i32, i32)>,
    /// What the march is for (report only).
    pub why: &'static str,
}

impl DepartPlan {
    pub fn dest(&self) -> (i32, i32) {
        (self.plain.dest_p as i32, self.plain.dest_q as i32)
    }
}

/// How a reveal is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevealRoute {
    /// `POST /f/reveal` (the keeper sends it with its own payer).
    Keeper,
    /// The bot's own Reveal transaction, `beneficiary` = its wallet.
    DirectSelf,
    /// The bot's own Reveal naming a non-canonical anchor (forger).
    DirectForged,
}

/// One thing a bot wants done.
#[derive(Clone, Debug)]
pub enum Intent {
    Join {
        faction: u8,
    },
    FileTicket {
        sites: Vec<Site>,
    },
    Harvest {
        h: HoldingRef,
    },
    Build {
        h: HoldingRef,
        item: u8,
        walls: bool,
    },
    Train {
        h: HoldingRef,
        unit: u8,
        n: u32,
    },
    Muster {
        h: HoldingRef,
        unit: u8,
        troops: u32,
        tile: u8,
    },
    Explore {
        h: HoldingRef,
        at: (i16, i16),
        host_id: u64,
        tiles: Vec<u8>,
    },
    SettleExplore {
        h: HoldingRef,
        bell: u32,
        region: u8,
        src: SeedSource,
    },
    Depart(Box<DepartPlan>),
    /// Reveal of a journalled march (`MarchMemo` key).
    Reveal {
        key: MarchKey,
        route: RevealRoute,
        args: Option<Box<RevealArgs>>,
        /// Sent after `A + W` or after the first gather (late_revealer).
        late: bool,
    },
    SettleTransit {
        key: MarchKey,
        args: Box<SettleTransitArgs>,
        /// A non-canonical slot key (forger).
        forged: bool,
    },
    /// settle_racer: Depart the host of a march still in transit (the
    /// program refuses `HostInTransit`).
    Redepart {
        key: MarchKey,
    },
    /// prefunder: lamports to future addresses (nothing must be blocked).
    Prefund {
        targets: Vec<Address>,
        lamports: u64,
    },
    /// ticket_holder: SettleTicket sent by the owner itself.
    SettleOwnTicket {
        k: u8,
        site: Site,
        ticket_bell: u32,
        sites: Vec<Site>,
        src: SeedSource,
    },
    /// ticket_holder: hold these accounts at a D-capped priority for some
    /// bells (`frontier_hold` on the local chain).
    Hold {
        keys: Vec<Address>,
        priority_milli: u32,
        bells: u32,
    },
    /// spammer: `n` sponsored Harvests back to back.
    Spam {
        h: HoldingRef,
        n: u32,
    },
}

impl Intent {
    pub fn name(&self) -> &'static str {
        match self {
            Intent::Join { .. } => "join",
            Intent::FileTicket { .. } => "file_ticket",
            Intent::Harvest { .. } => "harvest",
            Intent::Build { .. } => "build",
            Intent::Train { .. } => "train",
            Intent::Muster { .. } => "muster",
            Intent::Explore { .. } => "explore",
            Intent::SettleExplore { .. } => "settle_explore",
            Intent::Depart(_) => "depart",
            Intent::Reveal { .. } => "reveal",
            Intent::SettleTransit { .. } => "settle_transit",
            Intent::Redepart { .. } => "redepart",
            Intent::Prefund { .. } => "prefund",
            Intent::SettleOwnTicket { .. } => "settle_own_ticket",
            Intent::Hold { .. } => "hold",
            Intent::Spam { .. } => "spam",
        }
    }
}

/// A march's identity in the journal: (host id, depart bell).
pub type MarchKey = (u64, u32);

/// What a bot remembers of a march it sent (the marchbook: plaintext and
/// salt are journalled before the Depart is signed, §9.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarchMemo {
    pub key: MarchKey,
    pub h: HoldingRef,
    pub transit_slot: u8,
    pub arrive_bell: u32,
    pub dest: (i32, i32),
    pub path_others: Vec<(i32, i32)>,
    pub plain: [u8; PLAIN_LEN],
    pub salt: [u8; 32],
    pub commit: [u8; 32],
    pub seal: Vec<u8>,
    pub ct_hash: [u8; 32],
    pub tip: u64,
    pub kind: SealKind,
    /// The Depart landed (the relay answered ok).
    pub sent: bool,
    pub reveal_tries: u8,
    pub revealed: bool,
    pub late_done: bool,
    pub settled: bool,
    pub redeparted: bool,
}

/// A bot's memory between decisions (updated by the runner).
#[derive(Clone, Debug, Default)]
pub struct Memory {
    pub marches: Vec<MarchMemo>,
    /// Game day of the last spam burst.
    pub spam_day: Option<u32>,
    pub prefunded: BTreeSet<(u32, (i32, i32))>,
    pub forged: u32,
    pub zero_tip_days: BTreeSet<u32>,
    pub own_ticket_settled: Option<u32>,
    pub held: Option<(i16, i16)>,
    /// Refusal codes seen per intent name (report and back-off).
    pub refusals: BTreeMap<String, u32>,
    /// Bell of the last FileTicket sent (a refile waits a bell).
    pub ticket_sent_bell: Option<u32>,
}

impl Memory {
    pub fn march(&self, key: MarchKey) -> Option<&MarchMemo> {
        self.marches.iter().find(|m| m.key == key)
    }
    pub fn march_mut(&mut self, key: MarchKey) -> Option<&mut MarchMemo> {
        self.marches.iter_mut().find(|m| m.key == key)
    }
}

/// What a decision needs besides the observation.
pub struct Ctx<'a> {
    pub spec: &'a AgentSpec,
    pub seed: u64,
    pub wallet: Address,
    pub mem: &'a Memory,
    /// `L(reveal)` from the budgets table (for the tip minimum).
    pub reveal_loaded_limit: u32,
    /// Whether the runner can send direct transactions (`--rpc`).
    pub direct: bool,
    /// Whether this call is a session (economy and war) or a duty wake-up
    /// (reveals, settlements, tickets only).
    pub session: bool,
}

/// Stores of a holding at `now` (kernel `Accrual::settle` on a copy).
pub fn stores_at(h: &Holding, now: i64) -> [i64; RESOURCES] {
    core::array::from_fn(|r| {
        let s = &h.stores[r];
        let mut a = Accrual {
            value: s.value,
            rate: s.rate,
            cap: s.cap,
            t0: s.t0,
            frac: s.frac,
        };
        a.settle(now);
        a.value
    })
}

fn affordable(stores: &[i64; RESOURCES], cost: &[i64; RESOURCES]) -> bool {
    stores.iter().zip(cost).all(|(s, c)| s >= c)
}

pub fn tier_of(t: u8) -> Tier {
    match t {
        0 => Tier::Hamlet,
        1 => Tier::Town,
        2 => Tier::City,
        _ => Tier::Stronghold,
    }
}

/// Copies of building `item` a holding has (estimated from its production
/// above the tier's base; the Holding does not store the counts) plus the
/// queued ones: the `n − 1` of the next `catalog::building(item, n)`.
pub fn copies(h: &Holding, item: u8, now: i64) -> u32 {
    let Some(b) = catalog::BUILDINGS.get(item as usize) else {
        return 0;
    };
    let r = b.resource as usize;
    let base = catalog::base_production(tier_of(h.tier))[r];
    let per = b.per_hour * 1_000;
    let built = if per > 0 && h.production[r] > base {
        ((h.production[r] - base) / per) as u32
    } else {
        0
    };
    // Every Production item of the building's resource: a running one is
    // queued; a finished one the program's settle has already added to
    // `production` (the herald's bytes predate that settle). The item's
    // `arg` is the resource, not the building (W3-B P1); matching the
    // building index mispriced Builds (`Insufficient`, W4-F's in-process
    // day). `now` is kept for the callers' symmetry.
    let _ = now;
    let queued = h
        .queue
        .iter()
        .filter(|q| q.kind == QUEUE_PRODUCTION && q.arg as usize == r)
        .count() as u32;
    built + queued
}

/// Queue item kind `Production{resource = arg}` (W3-B P1; the program's
/// `queue_kind::PRODUCTION`, not yet exported by `frontier-abi`).
pub const QUEUE_PRODUCTION: u8 = 1;

/// Whether the build queue has room at `now`: items still running
/// (`kind ≠ 0`, `done_at > now`; a finished item is freed by the next
/// settle) below the tier's slots (Hamlet 2, Town 3, City and Stronghold
/// 4: `holding::Tier::queue_slots`). Counting the four stored items alone
/// sent Builds a Hamlet refuses `QueueFull` (W4-F's in-process day).
pub fn queue_free(h: &Holding, now: i64) -> bool {
    use permutation_rules::frontier::holding::Tier;
    let slots = [Tier::Hamlet, Tier::Town, Tier::City, Tier::Stronghold]
        .get(h.tier as usize)
        .map_or(2, |t| t.queue_slots());
    let busy = h
        .queue
        .iter()
        .filter(|q| q.kind != 0 && q.done_at > now)
        .count();
    busy < slots
}

fn href(h: &Holding) -> HoldingRef {
    HoldingRef {
        p: h.p,
        q: h.q,
        site: h.site,
    }
}

/// Whether a host id belongs to holding `h` (same site and generation).
pub fn host_of(h: &Holding, id: u64) -> bool {
    fclient::addr::host_parts(id).is_ok_and(|(p, q, site, gen, _)| {
        (p, q, site, gen) == (h.p as i32, h.q as i32, h.site, h.gen)
    })
}

/// The holding's hosts in the observed provinces: (province, entry).
pub fn own_hosts(obs: &Observation, h: &Holding) -> Vec<((i16, i16), Entry)> {
    let mut out = vec![];
    for (&k, v) in &obs.provinces {
        for e in &v.province.entries {
            if e.state != le::STATE_FREE && host_of(h, e.id) {
                out.push((k, *e));
            }
        }
    }
    out
}

fn in_transit(h: &Holding, id: u64) -> bool {
    h.transit_of(id).is_some()
}

/// A host that can leave now: roster, settled, no pending op, not in
/// transit, not a Scout (unless `scout`).
fn ready_host(h: &Holding, e: &Entry, bell: u32, scout: bool) -> bool {
    e.state == le::STATE_ROSTER
        && e.from_bell <= bell
        && e.pend_op == 0
        && e.ready_bell <= bell
        && (e.unit == SCOUT) == scout
        && !in_transit(h, e.id)
}

/// The sim's `pick_stance`: with probability `q` the faction's drilled
/// stance, else a uniform one.
pub fn pick_stance(faction: u8, q: f64, rng: &mut Rng) -> u8 {
    let d = &DOCTRINES[(faction % 6) as usize];
    if let Some((s, _)) = d.drill {
        if rng.chance(q) {
            return s as u8;
        }
    }
    rng.below(4) as u8
}

/// Retreat ratio (bps) from decision quality: careful players set one,
/// the rest fight to the end (0 = never).
pub fn pick_retreat(q: f64, rng: &mut Rng) -> u16 {
    if rng.chance(q) {
        (5_000 + rng.below(25_001)) as u16
    } else {
        0
    }
}

/// Tip for a sponsored march: the persona's, else a preset by quality
/// (careful players pay more so contested reveals still land).
pub fn pick_tip(presets: [u64; 3], persona: Option<Persona>, q: f64, rng: &mut Rng) -> u64 {
    match persona {
        Some(Persona::MinTip) => presets[0],
        Some(Persona::SelfTip) => presets[2],
        Some(Persona::ZeroTip) => 0,
        _ => {
            if rng.chance(q * 0.5) {
                presets[2]
            } else if rng.chance(0.5) {
                presets[1]
            } else {
                presets[0]
            }
        }
    }
}

/// Ticket sites (§5.9 FileTicket): free sites of opened provinces in ring
/// ≥ 2 of the faction's own wedge (the overview's view), up to three, the
/// outermost ring first (crowding sends newcomers outward), shuffled.
pub fn ticket_sites(obs: &Observation, faction: u8, rng: &mut Rng) -> Vec<Site> {
    let mut cands: Vec<(u32, Site)> = vec![];
    for r in obs.overview_recs() {
        let pc = ProvinceCoord::new(r.p as i32, r.q as i32);
        let ring = pc.ring();
        if ring < 2 || pc.wedge() != Some(faction % 6) {
            continue;
        }
        let count = obs.province(r.p, r.q).map(|p| p.site_count).unwrap_or(12);
        for s in r.free_sites(count) {
            cands.push((
                ring,
                Site {
                    p: r.p,
                    q: r.q,
                    site: s,
                },
            ));
        }
    }
    if cands.is_empty() {
        return vec![];
    }
    // Shuffle, then a stable sort by ring descending: random inside a ring.
    for i in (1..cands.len()).rev() {
        let j = rng.below(i as u64 + 1) as usize;
        cands.swap(i, j);
    }
    cands.sort_by_key(|c| std::cmp::Reverse(c.0));
    let mut out: Vec<Site> = vec![];
    let mut provs: Vec<(i16, i16)> = vec![];
    for (_, s) in cands {
        if out.len() == 3 {
            break;
        }
        if !provs.contains(&(s.p, s.q)) && provs.len() == 3 {
            continue;
        }
        if !provs.contains(&(s.p, s.q)) {
            provs.push((s.p, s.q));
        }
        out.push(s);
    }
    out
}

/// A march target: a province, a tile and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub p: i16,
    pub q: i16,
    pub tile: u8,
    pub why: &'static str,
}

fn free_passable_tile(pv: &Province, avoid_sites: bool) -> Option<u8> {
    let sites: Vec<u8> = pv.sites[..pv.site_count.min(12) as usize].to_vec();
    (0..61u8).find(|&i| {
        pv.passable_mask >> i & 1 == 1
            && !(avoid_sites && sites.contains(&i))
            && pv.camp.tile != i
            && !pv
                .entries
                .iter()
                .any(|e| e.state != le::STATE_FREE && e.tile == i)
    })
}

/// Targets in view: camps (the sim's `camp_raid`), other factions'
/// unshielded holdings (the sim's `war`), and, for a "stays" march, a free
/// tile of an observed province without a camp or another faction's host.
pub fn targets(obs: &Observation, faction: u8, arrive_hint: u32) -> Vec<Target> {
    let mut out = vec![];
    for (&(p, q), v) in &obs.provinces {
        let pv = &v.province;
        if pv.camp.state != 0 && pv.passable_mask >> pv.camp.tile & 1 == 1 {
            out.push(Target {
                p,
                q,
                tile: pv.camp.tile,
                why: "camp",
            });
        }
        for s in 0..pv.site_count.min(12) as usize {
            let m = &pv.site_mirror[s];
            if m.state == ls::STATE_HOLDING
                && m.faction != faction
                && m.shield_until_bell < arrive_hint
                && pv.passable_mask >> pv.sites[s] & 1 == 1
            {
                out.push(Target {
                    p,
                    q,
                    tile: pv.sites[s],
                    why: "war",
                });
            }
        }
    }
    out
}

/// Quiet destinations where an arrival would stay (settle_racer), nearest
/// first: a free passable non-site tile of an observed province without a
/// camp or another faction's host.
pub fn stay_targets(obs: &Observation, faction: u8, near: (i16, i16)) -> Vec<Target> {
    // Camp-free provinces first; provinces with a camp (a free tile away
    // from it) only after them: every opened province starts with a camp,
    // so on day 0 a camp-free province is rare and the settle racer never
    // marched (W4-F's in-process day).
    let mut v: Vec<(bool, u32, Target)> = vec![];
    for (&(p, q), pv) in obs.provinces.iter().map(|(k, v)| (k, &v.province)) {
        if pv
            .entries
            .iter()
            .any(|e| e.state != le::STATE_FREE && e.faction != faction)
        {
            continue;
        }
        let Some(tile) = free_passable_tile(pv, true) else {
            continue;
        };
        let d = ProvinceCoord::new(p as i32, q as i32)
            .distance(ProvinceCoord::new(near.0 as i32, near.1 as i32));
        v.push((
            pv.camp.state != 0,
            d,
            Target {
                p,
                q,
                tile,
                why: "stay",
            },
        ));
    }
    v.sort_by_key(|x| (x.0, x.1, x.2.p, x.2.q));
    v.into_iter().map(|x| x.2).collect()
}

/// The Depart plan for `host` at `at` to `t`: path over the observed
/// provinces, arrival bell = the earliest possible (leaving
/// `DEPART_SLACK_SECS` from now) plus `extra` bells, at most 72 bells after
/// the departure bell. None when no path or no free transit slot.
#[allow(clippy::too_many_arguments)]
pub fn plan_march(
    obs: &Observation,
    h: &Holding,
    at: (i16, i16),
    host: &Entry,
    t: Target,
    extra: u32,
    stance: u8,
    retreat: u16,
) -> Option<(Path, Plain, u8)> {
    let transit_slot = h.transit.iter().position(|x| x.state == 0)? as u8;
    let provs: BTreeMap<(i16, i16), &Province> = obs
        .provinces
        .iter()
        .map(|(&k, v)| (k, &v.province))
        .collect();
    let path = path::plan(
        &provs,
        (at.0 as i32, at.1 as i32),
        host.tile,
        (t.p as i32, t.q as i32),
        t.tile,
        host.unit,
    )?;
    let depart_ts = obs.now + DEPART_SLACK_SECS;
    let dep_bell = obs.season.bell_at(depart_ts);
    let earliest = path::earliest_arrival(obs.season.genesis_ts, depart_ts, path.secs);
    let arrive = (earliest + 1 + extra).min(dep_bell + 72);
    if arrive < earliest {
        return None;
    }
    let plain = Plain {
        version: 1,
        host_id: host.id,
        arrive_bell: arrive,
        dest_p: t.p,
        dest_q: t.q,
        dest_tile: t.tile,
        stance,
        retreat_bps: retreat,
        path_len: path.dirs.len() as u8,
        path: path.packed(),
        reserved: [0; 3],
    };
    Some((path, plain, transit_slot))
}

/// Everything a bot wants to do now.
pub fn decide(obs: &Observation, cx: &Ctx) -> Vec<Intent> {
    let spec = cx.spec;
    let prof = spec.profile();
    let bell = obs.bell();
    let mut rng = Rng::fork(cx.seed, (spec.index as u64) << 32 | bell as u64);
    let mut out: Vec<Intent> = vec![];
    if bell >= obs.season.end_bell() {
        return out;
    }
    // 1. Join.
    let Some((_, cz)) = &obs.me.citizen else {
        if bell >= spec.join_bell && bell < obs.season.join_close_bell() {
            out.push(Intent::Join {
                faction: spec.faction,
            });
        }
        return out;
    };
    let faction = cz.faction;
    // 2. Duties on own marches.
    duties(obs, cx, &prof, &mut rng, &mut out);
    // 3. Tickets.
    let finals: Vec<&Holding> = obs
        .me
        .holdings
        .iter()
        .map(|(_, h)| h)
        .filter(|h| final_by_rule(obs, h))
        .collect();
    let provisional: Vec<&Holding> = obs
        .me
        .holdings
        .iter()
        .map(|(_, h)| h)
        .filter(|h| h.state == lh::STATE_PROVISIONAL && !final_by_rule(obs, h))
        .collect();
    if finals.is_empty() {
        tickets(obs, cx, cz, &provisional, &mut rng, &mut out);
        return out;
    }
    if !cx.session {
        return out;
    }
    // 4. A session.
    let mut budget = prof.actions as i64;
    if spec.persona == Some(Persona::Spammer) {
        let day = bell / crate::profile::BELLS_PER_DAY;
        if cx.mem.spam_day != Some(day) {
            out.push(Intent::Spam {
                h: href(finals[0]),
                n: 120,
            });
        }
    }
    for h in &finals {
        if budget <= 0 {
            break;
        }
        budget -= economy(obs, cx, &prof, h, &mut rng, &mut out);
    }
    if budget > 2 {
        military(obs, cx, &prof, finals[0], faction, &mut rng, &mut out);
    }
    out
}

/// Whether `h` is final **by rule** (I-29, I-47, §5.6 step 5): stored
/// final, or provisional with `now ≥ final_ts` and its Province's cohort of
/// `ticket_bell` closed (every ticket settled, or 24 bells passed). The flip
/// is lazy: the owner's first resident action (Muster, Dissolve, Garrison,
/// Explore, Depart: they carry the Province) makes it, so a bot must act on
/// such a holding rather than wait for the herald to show `final`. Without
/// the Province in view the holding counts as provisional.
pub fn final_by_rule(obs: &Observation, h: &Holding) -> bool {
    if h.state == lh::STATE_FINAL {
        return true;
    }
    if h.state != lh::STATE_PROVISIONAL || obs.now < h.final_ts {
        return false;
    }
    obs.province(h.p, h.q)
        .is_some_and(|pv| fclient::land::cohort_closed(&pv.cohorts, h.ticket_bell, obs.bell()))
}

// A persona arm must not fall through to the default arm when its own
// condition fails (a late revealer never reveals in-bell), so the ifs stay
// inside the arms.
#[allow(clippy::collapsible_match)]
fn duties(obs: &Observation, cx: &Ctx, prof: &Profile, rng: &mut Rng, out: &mut Vec<Intent>) {
    let bell = obs.bell();
    let persona = cx.spec.persona;
    for m in &cx.mem.marches {
        if !m.sent || m.settled {
            continue;
        }
        let holding = obs
            .me
            .holdings
            .iter()
            .map(|(_, h)| h)
            .find(|h| href(h) == m.h);
        let transit = holding.and_then(|h| h.transit_of(m.key.0).map(|(_, t)| *t));
        // settle_racer: Depart the same host again from where it now
        // stands — the destination, once that province resolved the arrival
        // bell (a Stays host is in its roster) — and before its settlement
        // (§8.6, G12: `HostInTransit`). Earlier, at the origin, the entry is
        // busy or departed and the program answers `HostBusy`/`NotResident`
        // (W4-F's in-process day), which tests nothing.
        let arrived = obs
            .province(m.dest.0 as i16, m.dest.1 as i16)
            .is_some_and(|pv| pv.resolved_next > m.arrive_bell);
        if persona == Some(Persona::SettleRacer) && !m.redeparted && transit.is_some() && arrived {
            out.push(Intent::Redepart { key: m.key });
        }
        // Reveals: from the arrival bell's start, one bell long for owners.
        let in_bell = bell >= m.arrive_bell && bell <= m.arrive_bell + 1;
        let mut mr = Rng::fork(cx.seed, m.key.0 ^ (m.key.1 as u64) << 40);
        let wants = !mr.chance(prof.withhold);
        match persona {
            Some(Persona::MinTip) => {}
            Some(Persona::LateRevealer) => {
                // After `A + W` by THE anchor when the bell file is in view,
                // else two bells after the arrival bell (past the window and
                // the first gather at W = 1 bell).
                let region = fclient::ix::region_of(m.dest.0, m.dest.1);
                let closed = match obs
                    .bells
                    .get(&(m.arrive_bell, region))
                    .and_then(|b| b.close(obs.season.w))
                {
                    Some(close) => obs.now >= close && bell > m.arrive_bell,
                    None => bell >= m.arrive_bell + 2,
                };
                if !m.late_done && closed {
                    out.push(Intent::Reveal {
                        key: m.key,
                        route: RevealRoute::Keeper,
                        args: None,
                        late: true,
                    });
                    if cx.direct {
                        if let Some(a) = reveal_args(obs, cx, m, RevealRoute::DirectSelf) {
                            out.push(Intent::Reveal {
                                key: m.key,
                                route: RevealRoute::DirectSelf,
                                args: Some(Box::new(a)),
                                late: true,
                            });
                        }
                    }
                }
            }
            Some(Persona::SelfTip) if in_bell && !m.revealed && m.reveal_tries < 2 => {
                if cx.direct {
                    if let Some(a) = reveal_args(obs, cx, m, RevealRoute::DirectSelf) {
                        out.push(Intent::Reveal {
                            key: m.key,
                            route: RevealRoute::DirectSelf,
                            args: Some(Box::new(a)),
                            late: false,
                        });
                    }
                } else {
                    out.push(keeper_reveal(m));
                }
            }
            Some(Persona::GarbageSeal) if in_bell && !m.revealed && m.reveal_tries < 1 => {
                if favourable(obs, cx, m) {
                    out.push(keeper_reveal(m));
                }
            }
            Some(Persona::Forger) if in_bell && !m.revealed && m.reveal_tries < 1 => {
                if cx.direct {
                    if let Some(a) = reveal_args(obs, cx, m, RevealRoute::DirectForged) {
                        out.push(Intent::Reveal {
                            key: m.key,
                            route: RevealRoute::DirectForged,
                            args: Some(Box::new(a)),
                            late: false,
                        });
                    }
                }
                out.push(keeper_reveal(m));
            }
            _ if in_bell && !m.revealed && m.reveal_tries < 2 && wants => {
                out.push(keeper_reveal(m));
            }
            _ => {}
        }
        // Settlement once due (the keeper settles too; the first lands).
        if let (Some(h), Some(t)) = (holding, transit) {
            if t.state >= 2 {
                let due = settle_args(obs, cx, h, m);
                let eager = persona == Some(Persona::SettleRacer) || rng.chance(prof.q);
                if let (Some(a), true) = (due, eager) {
                    out.push(Intent::SettleTransit {
                        key: m.key,
                        args: Box::new(a.clone()),
                        forged: false,
                    });
                    if persona == Some(Persona::Forger) {
                        out.push(Intent::SettleTransit {
                            key: m.key,
                            args: Box::new(a),
                            forged: true,
                        });
                    }
                }
            }
        }
    }
}

fn keeper_reveal(m: &MarchMemo) -> Intent {
    Intent::Reveal {
        key: m.key,
        route: RevealRoute::Keeper,
        args: None,
        late: false,
    }
}

/// garbage_seal's rule: reveal only when the province-bell looks
/// favourable (no other faction has revealed more arrivals there).
fn favourable(obs: &Observation, cx: &Ctx, m: &MarchMemo) -> bool {
    let Some(v) = obs.provinces.get(&(m.dest.0 as i16, m.dest.1 as i16)) else {
        return true;
    };
    let f = cx.spec.faction;
    let mut mine = 0;
    let mut theirs = [0u32; 6];
    for s in v.slots.iter().filter(|s| s.bell == m.arrive_bell) {
        if s.faction == f {
            mine += 1;
        } else {
            theirs[(s.faction % 6) as usize] += 1;
        }
    }
    theirs.iter().all(|&t| t <= mine)
}

/// A direct Reveal's accounts (self_tip, late_revealer, forger): the first
/// absent slot of `(dest, arrive, faction)` as the target, the ArrivalDay
/// writable when its bit is clear.
pub fn reveal_args(
    obs: &Observation,
    cx: &Ctx,
    m: &MarchMemo,
    route: RevealRoute,
) -> Option<RevealArgs> {
    let dest = (m.dest.0 as i16, m.dest.1 as i16);
    let v = obs.provinces.get(&dest);
    let faction = cx.spec.faction;
    let used: Vec<u8> = v
        .map(|v| {
            v.slots
                .iter()
                .filter(|s| s.bell == m.arrive_bell && s.faction == faction)
                .map(|s| s.i)
                .collect()
        })
        .unwrap_or_default();
    let target_i = (0..4u8).find(|i| !used.contains(i)).unwrap_or(0);
    let day_writable = v
        .and_then(|v| v.day.as_ref())
        .is_none_or(|d| !d.has(m.arrive_bell));
    Some(RevealArgs {
        holding: m.h,
        transit_slot: m.transit_slot,
        target_i,
        plain: m.plain,
        salt: m.salt,
        ct_hash: m.ct_hash,
        beneficiary: if route == RevealRoute::DirectForged {
            Address::new_from_array([0x5A; 32])
        } else {
            cx.wallet
        },
        dest: m.dest,
        arrive: m.arrive_bell,
        faction,
        day_writable,
        path_provinces: m.path_others.clone(),
    })
}

/// SettleTransit's accounts when due (§5.11: `now ≥ close + 600` and the
/// destination resolved past the arrival bell), from the destination's
/// envelope (the arrival's slot and its beneficiary, the inputs' resolver)
/// and the arrival bell's region file (anchor or archive).
pub fn settle_args(
    obs: &Observation,
    cx: &Ctx,
    h: &Holding,
    m: &MarchMemo,
) -> Option<SettleTransitArgs> {
    let dest = (m.dest.0 as i16, m.dest.1 as i16);
    let v = obs.provinces.get(&dest)?;
    if v.province.resolved_next <= m.arrive_bell {
        return None;
    }
    let region = fclient::ix::region_of(m.dest.0, m.dest.1);
    let bv = obs.bells.get(&(m.arrive_bell, region))?;
    let close = bv.close(obs.season.w)?;
    if obs.now < close + obs.season.bell_secs as i64 {
        return None;
    }
    let faction = cx.spec.faction;
    // The arrival bell's own envelope (the latest one is a later bell's
    // once the destination resolved past the arrival); none yet: wait.
    let at = obs.province_at(dest, m.arrive_bell)?;
    let slot = at
        .slots
        .iter()
        .find(|s| s.bell == m.arrive_bell && s.host_id == m.key.0);
    let (slot_i, slot_beneficiary) = match slot {
        Some(s) => (s.i, s.beneficiary),
        None => (0, h.rent_payer),
    };
    let resolver = at
        .inputs
        .as_ref()
        .filter(|i| i.bell == m.arrive_bell)
        .map(|i| i.resolver)
        .unwrap_or(h.rent_payer);
    let seal: [u8; 165] = m.seal.clone().try_into().ok()?;
    Some(SettleTransitArgs {
        holding: m.h,
        transit_slot: m.transit_slot,
        commit: m.commit,
        seal,
        beneficiary: cx.wallet,
        dest: m.dest,
        arrive: m.arrive_bell,
        faction,
        slot_i,
        home: (h.p as i32, h.q as i32),
        anchor_present: bv.anchor.is_some() && !bv.archived,
        slot_beneficiary,
        resolver,
        holding_rent_payer: h.rent_payer,
        // v1.7: the program reads the Citizen only when this host earned
        // the camp's Works (I-56); a racing bot lists it always.
        camp_citizen: Some(h.owner_citizen),
    })
}

fn tickets(
    obs: &Observation,
    cx: &Ctx,
    cz: &fclient::decode::Citizen,
    provisional: &[&Holding],
    rng: &mut Rng,
    out: &mut Vec<Intent>,
) {
    let bell = obs.bell();
    let open_ticket = cz.ticket_bell != u32::MAX;
    if cx.spec.persona == Some(Persona::TicketHolder) {
        // Settle its own ticket first (anyone may), then hold the Province.
        if open_ticket && cx.direct && cx.mem.own_ticket_settled != Some(cz.ticket_bell) {
            let k = cz.ticket_next;
            let sites: Vec<Site> = cz
                .ticket_sites
                .iter()
                .filter(|s| !(s.p == 0 && s.q == 0 && s.site == 0))
                .map(|s| Site {
                    p: s.p,
                    q: s.q,
                    site: s.site,
                })
                .collect();
            if let Some(site) = sites.get(k as usize).copied() {
                let region = fclient::ix::region_of(site.p as i32, site.q as i32);
                if let Some(src) = obs
                    .bells
                    .get(&(cz.ticket_bell, region))
                    .and_then(|b| b.seed_source())
                {
                    out.push(Intent::SettleOwnTicket {
                        k,
                        site,
                        ticket_bell: cz.ticket_bell,
                        sites,
                        src,
                    });
                }
            }
        }
        if let Some(h) = provisional.first() {
            if cx.direct && cx.mem.held != Some((h.p, h.q)) {
                let a = fclient::addr::Addresses::new(obs.season.program, obs.season.season_id);
                out.push(Intent::Hold {
                    keys: vec![a.province(h.p as i32, h.q as i32)],
                    priority_milli: 500,
                    bells: 3,
                });
            }
        }
    }
    let provisional_flag = cz.flags & lc::FLAG_PROVISIONAL != 0;
    if open_ticket || provisional_flag || !provisional.is_empty() {
        return;
    }
    if cx.mem.ticket_sent_bell.is_some_and(|b| bell <= b) {
        return;
    }
    let sites = ticket_sites(obs, cz.faction, rng);
    if !sites.is_empty() {
        out.push(Intent::FileTicket { sites });
    }
}

/// Build, train, muster, explore (the sim's `economy`). Returns actions used.
fn economy(
    obs: &Observation,
    cx: &Ctx,
    prof: &Profile,
    h: &Holding,
    rng: &mut Rng,
    out: &mut Vec<Intent>,
) -> i64 {
    let now = obs.now;
    let bell = obs.bell();
    let r = href(h);
    let mut used = 0;
    let mut stores = stores_at(h, now);
    if rng.chance(0.2) {
        out.push(Intent::Harvest { h: r });
        used += 1;
    }
    let doctrine = &DOCTRINES[(h.faction % 6) as usize];
    // Build: the cheapest affordable building (careful players skip one
    // that leaves too little for a garrison).
    if queue_free(h, now) {
        let mut best: Option<(i64, u8)> = None;
        for item in 0..catalog::BUILDINGS.len() as u8 {
            let n = copies(h, item, now) + 1;
            let Some((cost, _, _)) = catalog::building(item, n, doctrine) else {
                continue;
            };
            if !affordable(&stores, &cost) {
                continue;
            }
            let total: i64 = cost.iter().sum();
            if best.is_none_or(|b| total < b.0) {
                best = Some((total, item));
            }
        }
        if let Some((_, item)) = best {
            let n = copies(h, item, now) + 1;
            let (cost, _, _) = catalog::building(item, n, doctrine).expect("priced");
            let keep = catalog::train(0, 100).expect("priced");
            let after: [i64; RESOURCES] = core::array::from_fn(|i| stores[i] - cost[i]);
            if !rng.chance(prof.thrift) || affordable(&after, &keep) {
                out.push(Intent::Build {
                    h: r,
                    item,
                    walls: false,
                });
                stores = after;
                used += 1;
            }
        }
    }
    let hosts = own_hosts(obs, h);
    let combat: Vec<&((i16, i16), Entry)> = hosts.iter().filter(|(_, e)| e.unit != SCOUT).collect();
    let scouts: Vec<&((i16, i16), Entry)> = hosts.iter().filter(|(_, e)| e.unit == SCOUT).collect();
    let unit = doctrine.unit as u8;
    let persona = cx.spec.persona;
    let want_hosts = match persona {
        Some(Persona::DoubleArrival) => 2,
        Some(Persona::Squatter) => 3,
        _ => 1 + (prof.q * 2.0) as usize,
    };
    // Train: combat troops in hundreds (the sim trains in hundreds).
    let k = 1 + (prof.q * 4.0) as u32;
    let reserve = h.reserve[unit as usize];
    if combat.len() < want_hosts && reserve < MIN_HOST * k {
        for kk in (1..=k).rev() {
            let Some(cost) = catalog::train(unit, 100 * kk) else {
                continue;
            };
            if affordable(&stores, &cost) {
                out.push(Intent::Train {
                    h: r,
                    unit,
                    n: 100 * kk,
                });
                stores = core::array::from_fn(|i| stores[i] - cost[i]);
                used += 1;
                break;
            }
        }
    }
    // Scouts: one scout host to explore.
    if scouts.is_empty() && h.reserve[SCOUT as usize] < MIN_HOST {
        if let Some(cost) = catalog::train(SCOUT, MIN_HOST) {
            if affordable(&stores, &cost) && rng.chance(0.5 + prof.q / 2.0) {
                out.push(Intent::Train {
                    h: r,
                    unit: SCOUT,
                    n: MIN_HOST,
                });
                used += 1;
            }
        }
    }
    // Muster from what is in reserve now. A Train of this step is not
    // counted: the relay simulates each transaction against the landed
    // state, so a Muster sent right behind its Train is `Insufficient`
    // (found by W4-F's in-process day); the next step musters them.
    let own_pv = obs.province(h.p, h.q);
    let pending_here = own_pv
        .map(|p| {
            p.entries
                .iter()
                .filter(|e| e.state != le::STATE_FREE && e.faction == h.faction)
                .count()
        })
        .unwrap_or(0);
    let room = pending_here < permutation_rules::frontier::host::FACTION_RESIDENT_CAP;
    let avail = h.reserve[unit as usize];
    if room && combat.len() < want_hosts && avail >= MIN_HOST {
        let troops = if persona == Some(Persona::Squatter) {
            MIN_HOST
        } else {
            (avail.min(MAX_HOST) / 100 * 100).max(MIN_HOST)
        };
        out.push(Intent::Muster {
            h: r,
            unit,
            troops,
            tile: h.tile,
        });
        used += 1;
    }
    let savail = h.reserve[SCOUT as usize];
    if room && scouts.is_empty() && savail >= MIN_HOST {
        out.push(Intent::Muster {
            h: r,
            unit: SCOUT,
            troops: MIN_HOST,
            tile: h.tile,
        });
        used += 1;
    }
    // Explore with a ready scout at home; settle a finished record.
    if h.explore.state == 0 {
        if let Some((at, e)) = scouts.iter().find(|(_, e)| ready_host(h, e, bell, true)) {
            if let Some(pv) = obs.province(at.0, at.1) {
                let tiles: Vec<u8> = path::adjacent_tiles(e.tile)
                    .into_iter()
                    .filter(|&t| pv.explored_mask >> t & 1 == 0)
                    .take(1 + (prof.q > 0.5) as usize)
                    .collect();
                if !tiles.is_empty() {
                    out.push(Intent::Explore {
                        h: r,
                        at: *at,
                        host_id: e.id,
                        tiles,
                    });
                    used += 1;
                }
            }
        }
    } else {
        let region = fclient::ix::region_of(h.explore.p as i32, h.explore.q as i32);
        if let Some(src) = obs
            .bells
            .get(&(h.explore.bell, region))
            .and_then(|b| b.seed_source())
        {
            out.push(Intent::SettleExplore {
                h: r,
                bell: h.explore.bell,
                region,
                src,
            });
        }
    }
    used
}

/// Marches: camp raids and war with the profile's aggression; the personas'
/// marches whenever they can.
fn military(
    obs: &Observation,
    cx: &Ctx,
    prof: &Profile,
    h: &Holding,
    faction: u8,
    rng: &mut Rng,
    out: &mut Vec<Intent>,
) {
    let bell = obs.bell();
    let persona = cx.spec.persona;
    let marching = matches!(
        persona,
        Some(
            Persona::MinTip
                | Persona::GarbageSeal
                | Persona::BadPlaintext
                | Persona::SettleRacer
                | Persona::Prefunder
                | Persona::Squatter
                | Persona::LateRevealer
                | Persona::Forger
                | Persona::DoubleArrival
                | Persona::ZeroTip
                | Persona::SelfTip
        )
    );
    if !marching && !rng.chance(prof.aggression) {
        return;
    }
    let presets = obs.season.tip_presets(cx.reveal_loaded_limit);
    let hosts = own_hosts(obs, h);
    let mut ready: Vec<&((i16, i16), Entry)> = hosts
        .iter()
        .filter(|(_, e)| ready_host(h, e, bell, false))
        .collect();
    if ready.is_empty() {
        return;
    }
    ready.sort_by_key(|(_, e)| std::cmp::Reverse(e.troops));
    let seal = match persona {
        Some(Persona::GarbageSeal) | Some(Persona::SettleRacer) => SealKind::Garbage,
        Some(Persona::BadPlaintext) => SealKind::BadPlaintext,
        _ => SealKind::Honest,
    };
    let (at, host) = *ready[0];
    let near = |t: &Target| {
        ProvinceCoord::new(t.p as i32, t.q as i32)
            .distance(ProvinceCoord::new(at.0 as i32, at.1 as i32))
    };
    let mut cands: Vec<Target> = if persona == Some(Persona::SettleRacer) {
        stay_targets(obs, faction, at)
    } else if persona == Some(Persona::Squatter) {
        let mut v: Vec<Target> = busiest_own(obs, faction).into_iter().collect();
        v.extend(stay_targets(obs, faction, at));
        v
    } else {
        let ts = targets(obs, faction, bell + 4);
        let camps: Vec<Target> = ts.iter().copied().filter(|t| t.why == "camp").collect();
        let mut pool = if !camps.is_empty() && (rng.chance(0.6) || ts.len() == camps.len()) {
            camps
        } else {
            ts
        };
        // Random among the near ones: shuffle, then a stable sort by distance.
        for i in (1..pool.len()).rev() {
            let j = rng.below(i as u64 + 1) as usize;
            pool.swap(i, j);
        }
        pool.sort_by_key(near);
        if pool.is_empty() && marching {
            stay_targets(obs, faction, at)
        } else {
            pool
        }
    };
    cands.truncate(8);
    let stance = pick_stance(faction, prof.q, rng);
    let retreat = pick_retreat(prof.q, rng);
    let extra = rng.below(2) as u32;
    let Some((t, (path, plain, slot))) = cands
        .into_iter()
        .find_map(|t| plan_march(obs, h, at, &host, t, extra, stance, retreat).map(|x| (t, x)))
    else {
        return;
    };
    let mut plans = vec![];
    let tip = pick_tip(presets, persona, prof.q, rng);
    let route = Route::Relay;
    plans.push(DepartPlan {
        h: href(h),
        host_at: at,
        host_id: host.id,
        transit_slot: slot,
        plain,
        tip,
        seal,
        route,
        path_others: path.others((t.p as i32, t.q as i32)),
        why: t.why,
    });
    // double_arrival and squatter: more hosts to the same province-bell.
    if matches!(
        persona,
        Some(Persona::DoubleArrival) | Some(Persona::Squatter)
    ) {
        let mut taken = vec![slot];
        for (at2, h2) in ready.iter().skip(1).map(|x| (x.0, &x.1)) {
            let mut hh = h.clone();
            for &s in &taken {
                hh.transit[s as usize].state = 1;
            }
            let Some((p2, mut plain2, slot2)) =
                plan_march(obs, &hh, at2, h2, t, extra, stance, retreat)
            else {
                continue;
            };
            // The same province-bell as the first.
            if plain2.arrive_bell > plain.arrive_bell {
                continue;
            }
            plain2.arrive_bell = plain.arrive_bell;
            taken.push(slot2);
            plans.push(DepartPlan {
                h: href(h),
                host_at: at2,
                host_id: h2.id,
                transit_slot: slot2,
                plain: plain2,
                tip,
                seal,
                route,
                path_others: p2.others((t.p as i32, t.q as i32)),
                why: "double",
            });
            if persona == Some(Persona::DoubleArrival) && plans.len() == 2 {
                break;
            }
        }
    }
    // zero_tip: the same march also goes direct (the relay refuses a
    // non-preset tip with TipNotPreset; the program refuses TipTooLow).
    let day = bell / crate::profile::BELLS_PER_DAY;
    if persona == Some(Persona::ZeroTip) {
        if cx.mem.zero_tip_days.contains(&day) {
            return;
        }
        if cx.direct {
            let mut d = plans[0].clone();
            d.route = Route::Direct;
            plans.push(d);
        }
    }
    // prefunder: lamports to the march's future accounts first.
    if persona == Some(Persona::Prefunder) {
        let key = (plain.arrive_bell, (t.p as i32, t.q as i32));
        if cx.direct && !cx.mem.prefunded.contains(&key) {
            out.push(Intent::Prefund {
                targets: future_addresses(obs, t, plain.arrive_bell, faction),
                lamports: 1_000_000,
            });
        }
    }
    for p in plans {
        out.push(Intent::Depart(Box::new(p)));
    }
}

/// The province in view where the own faction has the most hosts (from the
/// overview; squatters crowd its slots).
fn busiest_own(obs: &Observation, faction: u8) -> Option<Target> {
    let mut best: Option<(u8, (i16, i16))> = None;
    for r in obs.overview_recs() {
        let n = r.hosts[(faction % 6) as usize];
        if n > 0 && obs.provinces.contains_key(&(r.p, r.q)) && best.is_none_or(|b| n > b.0) {
            best = Some((n, (r.p, r.q)));
        }
    }
    let (_, (p, q)) = best?;
    let pv = obs.province(p, q)?;
    Some(Target {
        p,
        q,
        tile: free_passable_tile(pv, true)?,
        why: "squat",
    })
}

/// Addresses a march to `t` at `arrive` will create or read: its four
/// slots, the ArrivalDay, the ClashInputs, THE anchor and three seed-cache
/// nonces (prefunder: pre-funded = absent, nothing may be blocked).
pub fn future_addresses(obs: &Observation, t: Target, arrive: u32, faction: u8) -> Vec<Address> {
    let a = fclient::addr::Addresses::new(obs.season.program, obs.season.season_id);
    let (p, q) = (t.p as i32, t.q as i32);
    let region = fclient::ix::region_of(p, q);
    let mut v: Vec<Address> = (0..4)
        .map(|i| a.arrival_slot(p, q, arrive, faction, i))
        .collect();
    v.push(a.arrival_day(p, q, fclient::addr::day_of(arrive)));
    v.push(a.clash_inputs(p, q, arrive));
    v.push(a.anchor(arrive, region));
    v.extend((0..3).map(|n| a.seed_cache(arrive, region, n)));
    v
}

/// Whether a site of an overview record is free.
pub fn site_free(state: u8) -> bool {
    state == site_state::FREE
}
