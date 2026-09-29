//! The adversary schedule (M1 contract §13.4, §8.8; offchain design §11.4):
//! `frontier_hold` scripts in the SP-FEE drill shapes, spread over the play
//! window. Each hold's keys are chosen from the chain's **pending work** at
//! the moment it starts (wave-5 review of W5-B: a hold that locks nothing
//! anyone wants exercises nothing): the slot holds take a Province that
//! has arrivals today (its ArrivalDay exists), the lag hold the origin of a
//! march in flight (a departed entry awaiting its settle), the ticket hold
//! a Province with an open cohort and lasts until that cohort's 24 bells
//! are over; a kind with nothing pending waits (retried every bell) until
//! its deadline. Every hold is written to `events.jsonl` with its keys,
//! price and slot window, and the report counts, per hold, the program
//! transactions that wrote a held key inside the window and when the
//! first one landed after it (the hold's effect), and lists the windows
//! where a liveness finding is expected (criterion 4: `ValidSealUnrevealed`
//! only where the hold was above the keeper cap). W6-C: the
//! `frontier-fund` hold waits for a ring opening (a ring with provinces
//! still to open, or a crowding ring due by the folded values:
//! `fclient::land::ring_opening`) and holds Frontier and the ProvinceFund
//! of the wedge paying for it; the `defence-pool` hold waits for an open
//! defence claim (a late Reveal's refund inside its grace:
//! `fclient::play::open_claim`, the keeper's own rule) whose grace outlasts
//! the hold by a bell, so claims wait and none is lost. Both are armed over
//! the whole play window like the ticket hold. W6T-4: `slots-below` and
//! `lag` are armed to the end of play as well (w6-s7 skipped both after
//! their one-hour deadline, and `defence-pool` for want of the claim a
//! below-cap slot hold creates): `slots-below` from the first hour (by 60%
//! of play at the latest), `lag` on the origin of a transit in flight (a
//! Holding transit in state 1-2; a Province entry in state 3 lasts only
//! until its SettleDeparture). `slots-above` is armed to the end of play
//! too, and both slot holds aim at a province the fleet sealed marches to
//! for the held bell (the bots' marchbooks; a seal hides the destination on
//! chain), falling back to a province with arrivals today. A
//! `hold-skipped` hold makes the run not exit-grade (report row E).
//!
//! | kind | keys | price (milli) | length | expected |
//! |---|---|---|---|---|
//! | `slots-below` | one province-bell's ArrivalSlots + ArrivalDay (armed over play) | 1,500 (< P_def 2.0) | through the close | reveals land (keepers escalate past it; a late one opens a claim) |
//! | `slots-above` | the same, another bell | 3,000 (> P_def) | through the close | reveals wait; late ones routed and flagged |
//! | `anchor` | one region's next BellAnchor | 1,000 (> P_delay 0.5) | ⅓ bell | the window stays open longer (delay only) |
//! | `keeper-payers` | 20 of keeper A's reveal payers | 3,000 | 1 bell | the keeper draws other payers (effective N ≥ 150 − 20) |
//! | `lag` | the origin Province of transits in flight + its region's next anchor (armed over play) | 1,000 | 2 bells | the destination's result is unchanged (G7 in play) |
//! | `ticket` | a Province with an open ticket cohort | 1,000 | 2 bells | finality waits for the cohort; the higher score still wins |
//! | `frontier-fund` | Frontier + the ProvinceFund of the wedge a ring opening draws on (during the opening) | 1,000 | 1 bell | ring openings and folds wait (delay) |
//! | `defence-pool` | the DefencePool, while a defence claim is open (≥ 4 bells of its grace left) | 1,000 | 3 bells (half the claim grace) | claims and sweeps wait; none lost inside the grace |
//! | `relay-payers` | 20 of the relay pool's payers | 3,000 | 1 bell | the relay draws other payers |

use fclient::addr::Addresses;
use fclient::decode::Province;
use fclient::Address;
use serde_json::{json, Value};

pub const KINDS: &[&str] = &[
    "slots-below",
    "slots-above",
    "anchor",
    "keeper-payers",
    "lag",
    "ticket",
    "frontier-fund",
    "defence-pool",
    "relay-payers",
];

#[derive(Clone, Debug, PartialEq)]
pub struct Planned {
    pub kind: &'static str,
    /// Game time the hold starts.
    pub at: i64,
    pub priority_milli: u64,
    /// Length in game seconds (converted to slots at the run's scale).
    pub game_secs: i64,
    /// Skipped if nothing to hold by then (the ticket hold waits for an
    /// open cohort over the whole play window; the others an hour).
    pub deadline: i64,
}

/// One planned hold per kind, spread evenly over `[start, end)` (the
/// first after an hour of play, so there are residents and marches).
pub fn plan(start: i64, end: i64) -> Vec<Planned> {
    let span = (end - start - 3_600).max(0);
    let n = KINDS.len() as i64;
    KINDS
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let (prio, secs) = match *k {
                "slots-below" => (1_500, 1_500),
                "slots-above" => (3_000, 1_500),
                "anchor" => (1_000, 200),
                "keeper-payers" => (3_000, 600),
                "lag" => (1_000, 1_200),
                "ticket" => (1_000, 1_200),
                "frontier-fund" => (1_000, 600),
                "defence-pool" => (1_000, 1_800),
                _ => (3_000, 600),
            };
            // Tickets are filed as bots join, early in play: the ticket hold
            // is armed from the first bell and fires at the first open
            // cohort it sees. W6-C: the ring-opening and claim-grace holds
            // are armed likewise (from the first hour) and fire when their
            // situation arises. W6T-4: `slots-below` and `lag` are armed to
            // the end of play too (w6-s7 skipped both after one game hour
            // with nothing pending), and `slots-below` starts by 60% of play
            // so the escalation past it can open a claim `defence-pool`
            // holds. `slots-above` needs arrivals as well and is armed the
            // same way (the U4 R3 run skipped it after its one hour, bell 16:
            // the 300-bot fleet's first arrivals came at bell 76).
            let armed = matches!(
                *k,
                "ticket" | "frontier-fund" | "defence-pool" | "slots-below" | "slots-above" | "lag"
            );
            let at = match *k {
                "ticket" => start + 600,
                "frontier-fund" | "defence-pool" => start + 3_600,
                "slots-below" => start + 3_600.min((end - start) * 6 / 10),
                _ => start + 3_600 + span * i as i64 / n,
            };
            let deadline = if armed { end } else { at + 3_600 };
            Planned {
                kind: k,
                at,
                priority_milli: prio,
                game_secs: secs,
                deadline,
            }
        })
        .collect()
}

/// Slots for `game_secs` at `scale` (a slot is 0.4 × scale game seconds).
pub fn slots_for(game_secs: i64, scale: f64) -> u64 {
    ((game_secs as f64 / (0.4 * scale)).ceil() as u64).max(1)
}

/// Residents in a Province (entries in state 1, resident).
pub fn residents(p: &Province) -> usize {
    p.entries.iter().filter(|e| e.state == 1).count()
}

/// The province with the most residents (ties: most entries, then order).
pub fn busiest(ps: &[Province]) -> Option<&Province> {
    ps.iter()
        .max_by_key(|p| (residents(p), p.n_entries, -(p.p as i32), -(p.q as i32)))
}

/// A province with an open ticket cohort (filed > settled), most open first.
pub fn open_cohort(ps: &[Province]) -> Option<&Province> {
    ps.iter()
        .filter(|p| p.cohorts.iter().any(|c| c.filed > c.settled))
        .max_by_key(|p| {
            p.cohorts
                .iter()
                .map(|c| c.filed.saturating_sub(c.settled) as u32)
                .sum::<u32>()
        })
}

/// A province-bell's ArrivalSlots (every faction × `transit_slots`) and its
/// ArrivalDay.
pub fn slot_keys(a: &Addresses, p: &Province, bell: u32, transit_slots: u8) -> Vec<Address> {
    let (pp, qq) = (p.p as i32, p.q as i32);
    let mut v = vec![];
    for f in 0..fclient::abi::FACTIONS {
        for i in 0..transit_slots {
            v.push(a.arrival_slot(pp, qq, bell, f, i));
        }
    }
    v.push(a.arrival_day(pp, qq, bell / 144));
    v.truncate(64);
    v
}

/// What the hold of `kind` locks, from the chain's state now. `None` when
/// the state has nothing to hold (e.g. no open cohort yet).
#[allow(clippy::too_many_arguments)]
pub fn keys_for(
    kind: &str,
    a: &Addresses,
    ps: &[Province],
    bell_now: u32,
    transit_slots: u8,
    keeper_reveal_payers: &[Address],
    relay_payers: &[Address],
) -> Option<(Vec<Address>, Value)> {
    keys_for_pending(
        kind,
        a,
        ps,
        &Pending::default(),
        bell_now,
        transit_slots,
        keeper_reveal_payers,
        relay_payers,
    )
}

/// The chain's pending work a hold is aimed at.
#[derive(Clone, Debug, Default)]
pub struct Pending {
    /// Provinces with an ArrivalDay for today (arrivals are due there).
    pub arrivals_today: Vec<(i16, i16)>,
    /// Whether the holds need their situation (false only in unit tests of
    /// the key shapes).
    pub require: bool,
    /// Game time of the probe.
    pub now: i64,
    /// The ring opening under way or due (W6-C).
    pub ring_opening: Option<fclient::land::RingOpening>,
    /// Open defence claims: `(ArrivalSlot, grace end)` (W6-C).
    pub open_claims: Vec<(Address, i64)>,
    /// Transits in flight (W6T-4): each Holding transit in state 1 (DEPART
    /// seen) or 2 (departure settled), not yet settled at its destination.
    pub in_flight: Vec<InFlight>,
    /// Marches the fleet sealed to arrive at the next bell, by destination
    /// (W6T-4, from the bots' marchbooks: a seal hides the destination on
    /// chain, so only the harness knows where a Reveal will land):
    /// `((P, Q), count)`, most first.
    pub planned_next: Vec<((i16, i16), usize)>,
    /// Provinces a slot hold of this probe already holds (W6T-4: the other
    /// slot hold takes another one, so a 3.0 hold never masks the 1.5 one).
    pub held: Vec<(i16, i16)>,
}

/// The marches sealed to arrive at `bell`, by destination, most first
/// (W6T-4): the `ev: "sealed"` lines of the bots' marchbook JSONL.
pub fn planned_from_marchbook(text: &str, bell: u32) -> Vec<((i16, i16), usize)> {
    let mut n: std::collections::BTreeMap<(i16, i16), usize> = std::collections::BTreeMap::new();
    for l in text.lines() {
        if !l.contains("\"sealed\"") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(l) else {
            continue;
        };
        if v["ev"] != "sealed" || v["arrive_bell"].as_u64() != Some(bell as u64) {
            continue;
        }
        let (Some(p), Some(q)) = (v["dest"][0].as_i64(), v["dest"][1].as_i64()) else {
            continue;
        };
        *n.entry((p as i16, q as i16)).or_default() += 1;
    }
    let mut v: Vec<((i16, i16), usize)> = n.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

/// [`planned_from_marchbook`] over every `marchbook-*.jsonl` of a bots
/// directory.
pub fn planned_arrivals(bots_dir: &std::path::Path, bell: u32) -> Vec<((i16, i16), usize)> {
    let mut text = String::new();
    if let Ok(rd) = std::fs::read_dir(bots_dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with("marchbook-") && n.ends_with(".jsonl") {
                if let Ok(t) = std::fs::read_to_string(e.path()) {
                    text.push_str(&t);
                    text.push('\n');
                }
            }
        }
    }
    let mut m: std::collections::BTreeMap<(i16, i16), usize> = std::collections::BTreeMap::new();
    for (k, c) in planned_from_marchbook(&text, bell) {
        *m.entry(k).or_default() += c;
    }
    let mut v: Vec<((i16, i16), usize)> = m.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

/// One transit in flight (a Holding's transit record in state 1 or 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InFlight {
    pub origin: (i16, i16),
    pub arrive_bell: u32,
    pub state: u8,
}

/// Length of the `defence-pool` hold, game seconds (half the claim grace).
pub const DEFENCE_HOLD_SECS: i64 = 1_800;

/// The ring opening and the open defence claims, read from the chain (W6-C;
/// only when a `frontier-fund` or `defence-pool` hold is waiting).
#[allow(clippy::too_many_arguments)]
pub async fn probe_land_and_claims(
    chain: &crate::chain::Chain,
    a: &Addresses,
    program: &Address,
    season: &fclient::decode::Season,
    ps: &[Province],
    now: i64,
    want_ring: bool,
    want_claims: bool,
) -> (Option<fclient::land::RingOpening>, Vec<(Address, i64)>) {
    let mut ring = None;
    if want_ring {
        let mut keys = vec![a.frontier()];
        keys.extend(a.province_funds());
        if let Ok(got) = chain.accounts(&keys).await {
            let fr = got
                .first()
                .and_then(|x| x.as_ref())
                .filter(|x| x.owner == *program)
                .and_then(|x| fclient::decode::Frontier::decode(&x.data).ok());
            let mut funds = [0u64; 6];
            for (w, x) in got.iter().skip(1).take(6).enumerate() {
                funds[w] = x.as_ref().map_or(0, |x| x.lamports);
            }
            let on: std::collections::BTreeSet<(i32, i32)> =
                ps.iter().map(|p| (p.p as i32, p.q as i32)).collect();
            if let Some(fr) = fr {
                ring = fclient::land::ring_opening(season, &fr, &funds, &on, now);
            }
        }
    }
    let mut claims = vec![];
    if want_claims {
        let v = chain
            .call(
                "getProgramAccounts",
                json!([program.to_string(), {"encoding": "base64",
                    "filters": [{"dataSize": fclient::abi::size::ARRIVAL_SLOT}]}]),
            )
            .await
            .unwrap_or(Value::Null);
        let rows = v
            .as_array()
            .or_else(|| v.get("value").and_then(|x| x.as_array()))
            .cloned()
            .unwrap_or_default();
        let mut slots = vec![];
        for r in rows {
            let Some(key) = r["pubkey"].as_str().and_then(|k| k.parse::<Address>().ok()) else {
                continue;
            };
            if let Ok(Some(acc)) = fclient::rpc::parse_account(&r["account"]) {
                if let Ok(sl) = fclient::decode::ArrivalSlot::decode(&acc.data) {
                    if sl.season_id == season.h.season_id && sl.claimed == 0 && sl.ev_slot > 0 {
                        slots.push((key, sl));
                    }
                }
            }
        }
        let anchors: Vec<Address> = slots
            .iter()
            .map(|(_, sl)| a.anchor(sl.bell, fclient::ix::region_of(sl.p as i32, sl.q as i32)))
            .collect();
        let got = chain.accounts(&anchors).await.unwrap_or_default();
        for ((key, sl), an) in slots.iter().zip(got.iter()) {
            let Some(an) = an
                .as_ref()
                .filter(|x| x.owner == *program)
                .and_then(|x| fclient::decode::BellAnchor::decode(&x.data).ok())
            else {
                continue;
            };
            if let Some(end) = fclient::play::open_claim(sl, an.slot, an.a, season, now) {
                claims.push((*key, end));
            }
        }
    }
    (ring, claims)
}

/// The transits in flight of the season (W6T-4, only while a `lag` hold
/// waits): every Holding's transit records in state 1 or 2.
pub async fn probe_in_flight(
    chain: &crate::chain::Chain,
    program: &Address,
    season_id: u64,
) -> Vec<InFlight> {
    let v = chain
        .call(
            "getProgramAccounts",
            json!([program.to_string(), {"encoding": "base64",
                "filters": [{"dataSize": fclient::abi::size::HOLDING}]}]),
        )
        .await
        .unwrap_or(Value::Null);
    let rows = v
        .as_array()
        .or_else(|| v.get("value").and_then(|x| x.as_array()))
        .cloned()
        .unwrap_or_default();
    let mut out = vec![];
    for r in rows {
        if let Ok(Some(acc)) = fclient::rpc::parse_account(&r["account"]) {
            if acc.owner != *program {
                continue;
            }
            if let Ok(h) = fclient::decode::Holding::decode(&acc.data) {
                if h.h.season_id == season_id {
                    out.extend(in_flight_of(&h.transit));
                }
            }
        }
    }
    out
}

/// The origin of a march in flight: the Province with the most departed
/// entries awaiting their settle (state 3).
pub fn march_origin(ps: &[Province]) -> Option<&Province> {
    ps.iter()
        .filter(|p| p.entries.iter().any(|e| e.state == 3))
        .max_by_key(|p| p.entries.iter().filter(|e| e.state == 3).count())
}

/// The transits in flight among Holding transit records (state 1 or 2).
pub fn in_flight_of(ts: &[fclient::decode::Transit]) -> Vec<InFlight> {
    ts.iter()
        .filter(|t| matches!(t.state, 1 | 2))
        .map(|t| InFlight {
            origin: (t.origin_p, t.origin_q),
            arrive_bell: t.arrive_bell,
            state: t.state,
        })
        .collect()
}

/// The origin Province with the most transits in flight (W6T-4; ties: the
/// lower coordinates), among the opened Provinces.
pub fn origin_in_flight<'a>(ps: &'a [Province], fl: &[InFlight]) -> Option<(&'a Province, usize)> {
    let mut n: std::collections::BTreeMap<(i16, i16), usize> = std::collections::BTreeMap::new();
    for f in fl {
        *n.entry(f.origin).or_default() += 1;
    }
    n.iter()
        .filter_map(|(o, c)| ps.iter().find(|p| (p.p, p.q) == *o).map(|p| (p, *c)))
        .max_by_key(|(p, c)| (*c, -(p.p as i32), -(p.q as i32)))
}

/// [`keys_for`] against the chain's pending work.
#[allow(clippy::too_many_arguments)]
pub fn keys_for_pending(
    kind: &str,
    a: &Addresses,
    ps: &[Province],
    pending: &Pending,
    bell_now: u32,
    transit_slots: u8,
    keeper_reveal_payers: &[Address],
    relay_payers: &[Address],
) -> Option<(Vec<Address>, Value)> {
    let next = bell_now + 1;
    match kind {
        "slots-below" | "slots-above" => {
            // W6T-4: a province the fleet sealed marches to for the next
            // bell (its Reveals will need these slots), else the old guess.
            let free = |pq: &(i16, i16)| !pending.held.contains(pq);
            let planned = pending
                .planned_next
                .iter()
                .filter(|(d, _)| free(d))
                .find_map(|(d, n)| ps.iter().find(|p| (p.p, p.q) == *d).map(|p| (p, *n)));
            if let Some((p, n)) = planned {
                return Some((
                    slot_keys(a, p, next, transit_slots),
                    json!({"province": [p.p, p.q], "bell": next, "planned_arrivals": n,
                        "why": "marches sealed to arrive there at this bell (bots' marchbook)"}),
                ));
            }
            let p = if pending.require {
                let with: Vec<Province> = ps
                    .iter()
                    .filter(|p| pending.arrivals_today.contains(&(p.p, p.q)) && free(&(p.p, p.q)))
                    .cloned()
                    .collect();
                busiest(&with).cloned()?
            } else {
                let with: Vec<Province> =
                    ps.iter().filter(|p| free(&(p.p, p.q))).cloned().collect();
                busiest(&with)?.clone()
            };
            Some((
                slot_keys(a, &p, next, transit_slots),
                json!({"province": [p.p, p.q], "bell": next, "why": "arrivals are due there today"}),
            ))
        }
        "anchor" => {
            let p = busiest(ps)?;
            Some((
                vec![a.anchor(next, p.region)],
                json!({"bell": next, "region": p.region}),
            ))
        }
        "keeper-payers" => (!keeper_reveal_payers.is_empty()).then(|| {
            (
                keeper_reveal_payers.iter().take(20).copied().collect(),
                json!({"payers": keeper_reveal_payers.len().min(20)}),
            )
        }),
        "lag" => {
            // W6T-4: the origin of transits in flight (Holding transit state
            // 1-2); Province entries in state 3 last only until the
            // SettleDeparture a few slots later.
            let (p, n) = match origin_in_flight(ps, &pending.in_flight) {
                Some(x) => x,
                None if pending.require => return None,
                None => (march_origin(ps).or_else(|| busiest(ps))?, 0),
            };
            Some((
                vec![a.province(p.p as i32, p.q as i32), a.anchor(next, p.region)],
                json!({"province": [p.p, p.q], "region": p.region, "bell": next, "in_flight": n,
                    "why": "the origin of transits in flight (DEPART seen, not settled)"}),
            ))
        }
        "ticket" => {
            let p = open_cohort(ps)?;
            let open: Vec<Value> = p
                .cohorts
                .iter()
                .filter(|c| c.filed > c.settled)
                .map(|c| json!({"bell": c.bell, "filed": c.filed, "settled": c.settled}))
                .collect();
            // Through the cohort's last bell (24 bells after it opened).
            let until = p
                .cohorts
                .iter()
                .filter(|c| c.filed > c.settled)
                .map(|c| c.bell + 24)
                .max()
                .unwrap_or(next);
            Some((
                vec![a.province(p.p as i32, p.q as i32)],
                json!({"province": [p.p, p.q], "cohorts": open, "until_bell": until}),
            ))
        }
        "frontier-fund" => {
            use fclient::land::RingOpening;
            match &pending.ring_opening {
                Some(RingOpening::InProgress {
                    ring,
                    wedge,
                    missing,
                }) => Some((
                    vec![a.frontier(), a.province_fund(*wedge)],
                    json!({"ring": ring, "wedge": wedge, "missing_provinces": missing,
                        "why": "a ring opening under way (provinces still to open)"}),
                )),
                Some(RingOpening::Due { ring, wedge }) => Some((
                    vec![a.frontier(), a.province_fund(*wedge)],
                    json!({"ring": ring, "wedge": wedge,
                        "why": "a crowding ring due by the folded values"}),
                )),
                None if pending.require => None,
                None => {
                    let w = ps
                        .iter()
                        .max_by_key(|p| p.opened_bell)
                        .map_or(0, |p| p.wedge);
                    Some((vec![a.frontier(), a.province_fund(w)], json!({"wedge": w})))
                }
            }
        }
        "defence-pool" => {
            // A claim whose grace outlasts the hold by a bell (so it can
            // still be claimed after the hold: none lost).
            let live: Vec<i64> = pending
                .open_claims
                .iter()
                .map(|c| c.1)
                .filter(|&end| end - pending.now >= DEFENCE_HOLD_SECS + 600)
                .collect();
            if pending.require && live.is_empty() {
                return None;
            }
            Some((
                vec![a.defence_pool()],
                json!({"open_claims": live.len(), "grace_end_min": live.iter().min(),
                    "why": "defence claims open inside their grace"}),
            ))
        }
        "relay-payers" => (!relay_payers.is_empty()).then(|| {
            (
                relay_payers.iter().take(20).copied().collect(),
                json!({"payers": relay_payers.len().min(20)}),
            )
        }),
        _ => None,
    }
}

/// Whether a hold of this kind at this price is above the keeper's cap for
/// the writes it blocks (so its liveness findings are expected).
pub fn above_cap(kind: &str, priority_milli: u64) -> bool {
    let cap = match kind {
        "slots-below" | "slots-above" | "keeper-payers" | "relay-payers" => {
            fclient::payers::P_DEF_MILLI
        }
        _ => fclient::payers::P_DELAY_MILLI,
    };
    priority_milli > cap
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plan_covers_every_kind_inside_the_window() {
        let p = plan(1_000, 1_000 + 86_400);
        assert_eq!(p.len(), KINDS.len());
        for (x, k) in p.iter().zip(KINDS) {
            assert_eq!(x.kind, *k);
            assert!(x.at >= 1_000 + 600 && x.at < 1_000 + 86_400);
            assert!(x.deadline > x.at);
        }
        let t = p.iter().find(|x| x.kind == "ticket").unwrap();
        assert_eq!(
            (t.at, t.deadline),
            (1_600, 1_000 + 86_400),
            "armed over the whole play"
        );
        for k in ["frontier-fund", "defence-pool"] {
            let h = p.iter().find(|x| x.kind == k).unwrap();
            assert_eq!(
                (h.at, h.deadline),
                (1_000 + 3_600, 1_000 + 86_400),
                "{k}: armed from the first hour to the end"
            );
        }
        let others: Vec<_> = p
            .iter()
            .filter(|x| !matches!(x.kind, "ticket" | "frontier-fund" | "defence-pool"))
            .collect();
        assert!(others.windows(2).all(|w| w[0].at < w[1].at));
        assert!(!above_cap("slots-below", 1_500), "below the W cap 2.0");
        assert!(above_cap("slots-above", 3_000));
        assert!(above_cap("anchor", 1_000), "above the D cap 0.5");
    }

    /// W6T-4: `slots-below` and `lag` are armed over the whole play window
    /// (w6-s7 skipped both: armed for one game hour); `slots-below` starts
    /// by 60% of play, so the keeper escalation it causes can open a claim
    /// that `defence-pool` holds later.
    #[test]
    fn plan_arms_every_hold_over_play() {
        for (start, end) in [(1_000, 1_000 + 86_400), (5_000, 5_000 + 7 * 86_400)] {
            let p = plan(start, end);
            for k in [
                "slots-below",
                "slots-above",
                "lag",
                "ticket",
                "frontier-fund",
                "defence-pool",
            ] {
                let h = p.iter().find(|x| x.kind == k).unwrap();
                assert_eq!(h.deadline, end, "{k}: armed to the end of play");
                assert!(h.at < end, "{k}");
            }
            let sb = p.iter().find(|x| x.kind == "slots-below").unwrap();
            assert!(
                sb.at <= start + (end - start) * 6 / 10,
                "slots-below by 60% of play"
            );
            let dp = p.iter().find(|x| x.kind == "defence-pool").unwrap();
            assert!(sb.at <= dp.deadline);
        }
    }

    /// W6T-4: the lag hold's origin is a transit in flight (state 1-2), not
    /// a Province entry in state 3 (SettleDeparture clears those within
    /// slots, so w6-s7 never saw one at a probe).
    #[test]
    fn lag_uses_transits_in_flight() {
        let ad = Addresses::new(Address::new_from_array([7; 32]), 7);
        let mut a = empty_province(0, 0);
        let mut b = empty_province(1, 0);
        a.region = 3;
        b.region = 5;
        a.entries[0].state = 1;
        let ps = vec![a, b];
        let none = Pending {
            require: true,
            ..Pending::default()
        };
        assert!(keys_for_pending("lag", &ad, &ps, &none, 10, 4, &[], &[]).is_none());
        let fl = |p: i16, q: i16, arrive: u32, state: u8| InFlight {
            origin: (p, q),
            arrive_bell: arrive,
            state,
        };
        let pending = Pending {
            require: true,
            in_flight: vec![
                fl(1, 0, 14, 1),
                fl(1, 0, 15, 2),
                fl(0, 0, 13, 2),
                fl(9, 9, 16, 1),
            ],
            ..Pending::default()
        };
        let (k, d) = keys_for_pending("lag", &ad, &ps, &pending, 10, 4, &[], &[]).unwrap();
        assert_eq!(k, vec![ad.province(1, 0), ad.anchor(11, 5)]);
        assert_eq!(d["province"], json!([1, 0]));
        assert_eq!(d["in_flight"], 2);
        // An origin that is not an opened Province is never picked.
        let only_unknown = Pending {
            require: true,
            in_flight: vec![fl(9, 9, 16, 1)],
            ..Pending::default()
        };
        assert!(keys_for_pending("lag", &ad, &ps, &only_unknown, 10, 4, &[], &[]).is_none());
        // Holding transits: states 1 and 2 are in flight, 0 and 3 are not.
        let mut t = fclient::decode::Transit::decode(&[0u8; 96]);
        t.origin_p = 2;
        t.origin_q = -1;
        t.arrive_bell = 30;
        let got: Vec<InFlight> = [0u8, 1, 2, 3]
            .iter()
            .flat_map(|s| {
                let mut x = t;
                x.state = *s;
                in_flight_of(&[x])
            })
            .collect();
        assert_eq!(got, vec![fl(2, -1, 30, 1), fl(2, -1, 30, 2)]);
    }

    /// W6T-4: the slot holds aim at a province the fleet sealed marches to
    /// for the held bell (the U4 nightlies held slots no Reveal needed:
    /// 0 writes inside or after the `slots-below` window), and fall back
    /// to the ArrivalDay guess without a marchbook.
    #[test]
    fn slot_holds_aim_at_planned_arrivals() {
        let book = [
            r#"{"arrive_bell":11,"bot":1,"depart_bell":8,"dest":[1,0],"ev":"sealed","host":"5"}"#,
            r#"{"arrive_bell":11,"bot":2,"depart_bell":8,"dest":[1,0],"ev":"sealed","host":"6"}"#,
            r#"{"arrive_bell":11,"bot":3,"depart_bell":9,"dest":[0,0],"ev":"sealed","host":"7"}"#,
            r#"{"arrive_bell":12,"bot":4,"depart_bell":9,"dest":[0,0],"ev":"sealed","host":"8"}"#,
            r#"{"bot":1,"depart_bell":8,"ev":"sent","host":"5"}"#,
            "not json",
        ]
        .join("\n");
        let planned = planned_from_marchbook(&book, 11);
        assert_eq!(planned, vec![((1, 0), 2), ((0, 0), 1)]);
        let ad = Addresses::new(Address::new_from_array([7; 32]), 7);
        let mut a = empty_province(0, 0);
        a.entries[0].state = 1;
        a.entries[1].state = 1;
        let ps = vec![a, empty_province(1, 0)];
        let pending = Pending {
            require: true,
            arrivals_today: vec![(0, 0)],
            planned_next: planned,
            ..Pending::default()
        };
        let (k, d) = keys_for_pending("slots-below", &ad, &ps, &pending, 10, 4, &[], &[]).unwrap();
        assert_eq!(k, slot_keys(&ad, &ps[1], 11, 4));
        assert_eq!(d["planned_arrivals"], 2);
        // Without a marchbook: the old guess (arrivals today, busiest).
        let guess = Pending {
            planned_next: vec![],
            ..pending.clone()
        };
        let (_, d) = keys_for_pending("slots-below", &ad, &ps, &guess, 10, 4, &[], &[]).unwrap();
        assert_eq!(d["province"], json!([0, 0]));
        // The other slot hold of the same probe takes another province.
        let taken = Pending {
            held: vec![(1, 0)],
            ..pending.clone()
        };
        let (_, d) = keys_for_pending("slots-above", &ad, &ps, &taken, 10, 4, &[], &[]).unwrap();
        assert_eq!(d["province"], json!([0, 0]), "{d}");
        assert_eq!(d["planned_arrivals"], 1);
        // A planned destination that is not an opened Province is skipped.
        let off = Pending {
            planned_next: vec![((9, 9), 5)],
            ..guess
        };
        let (_, d) = keys_for_pending("slots-above", &ad, &ps, &off, 10, 4, &[], &[]).unwrap();
        assert_eq!(d["province"], json!([0, 0]));
        // The marchbook files of a bots directory.
        let dir = std::env::temp_dir().join(format!("psf-marchbook-{}", crate::run::wall_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("marchbook-1-0.jsonl"), &book).unwrap();
        std::fs::write(dir.join("marchbook-2-0.jsonl"), &book).unwrap();
        std::fs::write(dir.join("report.json"), &book).unwrap();
        assert_eq!(planned_arrivals(&dir, 11), vec![((1, 0), 4), ((0, 0), 2)]);
        assert!(planned_arrivals(std::path::Path::new("/nonexistent"), 11).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slot_counts() {
        assert_eq!(slots_for(600, 100.0), 15);
        assert_eq!(slots_for(600, 20.0), 75);
        assert_eq!(slots_for(1, 100.0), 1);
    }

    #[test]
    fn slot_keys_are_distinct_and_within_the_lock_limit() {
        let a = Addresses::new(Address::new_from_array([7; 32]), 7);
        let p = empty_province(2, -1);
        let k = slot_keys(&a, &p, 30, 4);
        assert_eq!(k.len(), 25);
        let s: std::collections::BTreeSet<_> = k.iter().collect();
        assert_eq!(s.len(), 25);
        assert_eq!(k[0], a.arrival_slot(2, -1, 30, 0, 0));
        assert_eq!(k[24], a.arrival_day(2, -1, 0));
    }

    fn empty_province(p: i16, q: i16) -> Province {
        // A decoded all-zero Province with the right magic/size.
        let mut d = vec![0u8; fclient::abi::size::PROVINCE];
        d[..8].copy_from_slice(fclient::abi::magic::PROVINCE);
        let mut pr = Province::decode(&d).expect("zero province decodes");
        pr.p = p;
        pr.q = q;
        pr
    }

    #[test]
    fn picks() {
        let mut a = empty_province(0, 0);
        let mut b = empty_province(1, 0);
        a.entries[0].state = 1;
        b.entries[0].state = 1;
        b.entries[1].state = 1;
        let ps = vec![a.clone(), b.clone()];
        assert_eq!(busiest(&ps).map(|p| (p.p, p.q)), Some((1, 0)));
        assert!(open_cohort(&ps).is_none());
        a.cohorts[0].filed = 2;
        a.cohorts[0].settled = 1;
        let ps = vec![a, b];
        assert_eq!(open_cohort(&ps).map(|p| (p.p, p.q)), Some((0, 0)));
        let ad = Addresses::new(Address::new_from_array([7; 32]), 7);
        let (k, _) = keys_for("ticket", &ad, &ps, 10, 4, &[], &[]).unwrap();
        assert_eq!(k, vec![ad.province(0, 0)]);
        assert!(keys_for("keeper-payers", &ad, &ps, 10, 4, &[], &[]).is_none());
        let (k, _) = keys_for("defence-pool", &ad, &ps, 10, 4, &[], &[]).unwrap();
        assert_eq!(k, vec![ad.defence_pool()]);
        // Wave-5 review: holds aim at pending work.
        let need = Pending {
            arrivals_today: vec![],
            require: true,
            ..Pending::default()
        };
        assert!(
            keys_for_pending("slots-above", &ad, &ps, &need, 10, 4, &[], &[]).is_none(),
            "no arrivals due: the slot hold waits"
        );
        let due = Pending {
            arrivals_today: vec![(0, 0)],
            require: true,
            ..Pending::default()
        };
        let (_, d) = keys_for_pending("slots-above", &ad, &ps, &due, 10, 4, &[], &[]).unwrap();
        assert_eq!(
            d["province"],
            json!([0, 0]),
            "the province with arrivals, not the busiest"
        );
        assert!(keys_for_pending("lag", &ad, &ps, &need, 10, 4, &[], &[]).is_none());
        let mut ps2 = ps.clone();
        ps2[1].entries[3].state = 3;
        // W6T-4: a state-3 entry alone is no longer the lag hold's
        // situation (it lasts only until the SettleDeparture); a transit in
        // flight is (`lag_uses_transits_in_flight`). Unrequired, the
        // state-3 origin is still the fallback.
        assert!(keys_for_pending("lag", &ad, &ps2, &need, 10, 4, &[], &[]).is_none());
        let (k, _) = keys_for("lag", &ad, &ps2, 10, 4, &[], &[]).unwrap();
        assert_eq!(k[0], ad.province(1, 0), "the fallback origin");
        let (_, d) = keys_for("ticket", &ad, &ps2, 10, 4, &[], &[]).unwrap();
        assert_eq!(d["until_bell"], 24, "through the cohort's 24 bells");
        // W6-C: the ring-opening and claim-grace holds wait for their
        // situation.
        assert!(keys_for_pending("frontier-fund", &ad, &ps, &need, 10, 4, &[], &[]).is_none());
        let opening = Pending {
            ring_opening: Some(fclient::land::RingOpening::InProgress {
                ring: 3,
                wedge: 2,
                missing: 5,
            }),
            ..need.clone()
        };
        let (k, d) =
            keys_for_pending("frontier-fund", &ad, &ps, &opening, 10, 4, &[], &[]).unwrap();
        assert_eq!(k, vec![ad.frontier(), ad.province_fund(2)]);
        assert_eq!(d["ring"], 3);
        let due = Pending {
            ring_opening: Some(fclient::land::RingOpening::Due { ring: 4, wedge: 5 }),
            ..need.clone()
        };
        let (k, _) = keys_for_pending("frontier-fund", &ad, &ps, &due, 10, 4, &[], &[]).unwrap();
        assert_eq!(k[1], ad.province_fund(5));
        assert!(keys_for_pending("defence-pool", &ad, &ps, &need, 10, 4, &[], &[]).is_none());
        let slot = Address::new_from_array([3; 32]);
        let short = Pending {
            now: 10_000,
            open_claims: vec![(slot, 10_000 + DEFENCE_HOLD_SECS + 599)],
            ..need.clone()
        };
        assert!(
            keys_for_pending("defence-pool", &ad, &ps, &short, 10, 4, &[], &[]).is_none(),
            "a grace the hold would outlast: claims could be lost, so not this one"
        );
        let open = Pending {
            now: 10_000,
            open_claims: vec![(slot, 10_000 + 3_000)],
            ..need.clone()
        };
        let (k, d) = keys_for_pending("defence-pool", &ad, &ps, &open, 10, 4, &[], &[]).unwrap();
        assert_eq!(k, vec![ad.defence_pool()]);
        assert_eq!(d["open_claims"], 1);
    }
}
