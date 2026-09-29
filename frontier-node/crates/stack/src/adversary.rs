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
//! the whole play window like the ticket hold.
//!
//! | kind | keys | price (milli) | length | expected |
//! |---|---|---|---|---|
//! | `slots-below` | one province-bell's ArrivalSlots + ArrivalDay | 1,500 (< P_def 2.0) | through the close | reveals land (keepers escalate past it) |
//! | `slots-above` | the same, another bell | 3,000 (> P_def) | through the close | reveals wait; late ones routed and flagged |
//! | `anchor` | one region's next BellAnchor | 1,000 (> P_delay 0.5) | ⅓ bell | the window stays open longer (delay only) |
//! | `keeper-payers` | 20 of keeper A's reveal payers | 3,000 | 1 bell | the keeper draws other payers (effective N ≥ 150 − 20) |
//! | `lag` | an origin Province + its region's next anchor | 1,000 | 2 bells | the destination's result is unchanged (G7 in play) |
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
            // situation arises.
            let armed = matches!(*k, "ticket" | "frontier-fund" | "defence-pool");
            let at = match *k {
                "ticket" => start + 600,
                "frontier-fund" | "defence-pool" => start + 3_600,
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
    let _ = ts;
    vec![] // W6T-4 failing-first stub
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
            let p = if pending.require {
                let with: Vec<Province> = ps
                    .iter()
                    .filter(|p| pending.arrivals_today.contains(&(p.p, p.q)))
                    .cloned()
                    .collect();
                busiest(&with).cloned()?
            } else {
                busiest(ps)?.clone()
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
            let p = if pending.require {
                march_origin(ps)?
            } else {
                march_origin(ps).or_else(|| busiest(ps))?
            };
            Some((
                vec![a.province(p.p as i32, p.q as i32), a.anchor(next, p.region)],
                json!({"province": [p.p, p.q], "region": p.region, "bell": next,
                    "why": "the origin of a march in flight (departed entries await their settle)"}),
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
        let (k, _) = keys_for_pending("lag", &ad, &ps2, &need, 10, 4, &[], &[]).unwrap();
        assert_eq!(k[0], ad.province(1, 0), "the origin of the march in flight");
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
