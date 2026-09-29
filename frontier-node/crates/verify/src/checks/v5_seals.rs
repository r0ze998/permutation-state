//! V5 — seals (contract §5.11, §8.5; I-06, I-27, I-28, I-44).
//!
//! Every DEPART's seal is opened with the **stock `tlock` crate** (the
//! round `T(arrive)` signature some anchor transaction carried, verified
//! under the pinned key) and the PS-KS body, its commitment recomputed and
//! its plaintext validated (`fclient::seal::judge`, independent of the
//! program's opener). Then:
//!
//! - DEPART's `seal_root = sha256(commit ‖ sha256(seal))`;
//! - every Reveal's plaintext and salt give DEPART's commitment and root,
//!   and a valid seal's revealed plaintext is the opened one;
//! - SettleTransit judged the logged pair (its commit and seal = DEPART's);
//! - **every TRANSIT_SETTLED seal code is the stock opener's** (exactly;
//!   codes 1 and 3 are not separable off chain, integ-W4 review);
//! - **a bad seal whose transit settled with another outcome is a FAIL**.
//!
//! A seal the verifier cannot open — no transaction carried a verified
//! signature of `T(arrive)` — is `MissingData` wherever a verdict needs it:
//! a Reveal, a settlement, an unsettled march whose arrival bell ended a
//! bell before the archive does (W5-D, the integ-W4 review).
//!
//! Warnings (liveness, E5 gates them): a valid seal never revealed that
//! settled `ROUTED` (`ValidSealUnrevealed`, with the signatures of the
//! failed Reveal attempts of that march, `(host, arrive)`; W6T-3: not every
//! failed Reveal of the host), a bad seal still unsettled at the end
//! (`BadSealUnsettled`). A valid seal never revealed that the rules explain
//! is listed in `liveness.unrevealed_by_rule` with its reason instead:
//! settled with another outcome (`bounced`, W6-C), or settled `ROUTED`
//! because §5.11 Reveal step 6 refuses it (`path`, `arrival-bell`,
//! `shielded-dest`, `shielded-own`), judged on the archived post-states and
//! never from the refusals' codes ([`rule_refusal`], W6T-3: w6-s7's 27).
//!
//! W6T-3 (w6-s7 criterion 1): a landed DEPART whose arrival bell is at or
//! after `end_bell` is a FAIL (`ArrivalAfterEnd`, §5.11 Depart step 4
//! v1.12): no anchor, gather, resolve or settlement exists for it.
//!
//! Codes: `RevealCommitMismatch`, `VerdictDisagreesWithTlock`,
//! `BadSealSurvived`, `ArrivalAfterEnd`; warn `ValidSealUnrevealed`,
//! `BadSealUnsettled`.

use std::collections::{HashMap, HashSet};

use fclient::seal::{self, Plain, SEAL_LEN};
use frontier_abi::ix as aix;
use frontier_abi::log::{transit_outcome, Kind};
use frontier_abi::tags::Ix;

use super::Ctx;
use crate::codes::*;
use crate::world::{Rec, World};

const V: &str = "V5";

/// The stock judgement of one DEPART.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Judged {
    pub code: u8,
    pub plain: Option<Plain>,
}

/// Judges the seal of DEPART record `d` (`None`: no verified signature of
/// `T(arrive)` in the archive).
pub fn judge(cx: &mut Ctx, d: &Rec) -> Option<Judged> {
    let f = cx.f;
    let arrive = d.pu32("arrive_bell");
    let sig = f.good_sig(f.tlock_round(arrive), &mut cx.sigs)?;
    let seal: [u8; SEAL_LEN] = d.p("seal").try_into().ok()?;
    let (code, plain) = seal::judge(&seal, &d.p32("commit"), &sig, d.ku64("host_id"), arrive);
    Some(Judged { code, plain })
}

/// The Reveal instruction of a REVEAL record's transaction.
pub fn reveal_ix(w: &World, r: &Rec) -> Option<aix::Reveal> {
    w.txs[r.tx]
        .ix(Ix::Reveal.tag())
        .and_then(|i| aix::Reveal::decode(&i.data).ok())
}

pub fn run(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    let mut judged: HashMap<usize, Option<Judged>> = HashMap::new();
    let mut revealed: HashSet<usize> = HashSet::new();
    let mut settled: HashSet<usize> = HashSet::new();
    let last_time = w.txs.iter().map(|t| t.time).max().unwrap_or(0);
    let failed = failed_reveals(w);
    let end_bell = f.params.as_ref().map(|p| p.end_bell);
    for (n, r) in w.recs.iter().enumerate() {
        match r.kind {
            Kind::DEPART => {
                let host = r.ku64("host_id");
                // W6T-3 (w6-s7 criterion 1): §5.11 Depart step 4 (v1.12)
                // bounds the arrival below `end_bell`. No anchor, gather,
                // resolve or settlement exists for a later bell, so such a
                // march could never settle (w6-s7: 9 marches, MissingData).
                let arrive = r.pu32("arrive_bell");
                if let Some(end) = end_bell.filter(|&e| arrive >= e) {
                    cx.fail(
                        V,
                        ARRIVAL_AFTER_END,
                        format!("host {host}"),
                        r.bell,
                        Some(r.tx),
                        format!(
                            "a landed DEPART arriving at bell {arrive}, at or after end_bell {end}"
                        ),
                    );
                }
                let root = seal::seal_root(&r.p32("commit"), &seal::ct_hash(r.p("seal")));
                if root != r.p32("seal_root") {
                    cx.fail(
                        V,
                        REVEAL_COMMIT_MISMATCH,
                        format!("host {host}"),
                        r.bell,
                        Some(r.tx),
                        "DEPART's seal_root is not sha256(commit ‖ sha256(seal))",
                    );
                }
            }
            Kind::REVEAL => {
                let host = r.pu64("host_id");
                let b = r.ku32("arrive");
                let what = format!("host {host} bell {b}");
                let Some(d) = f.depart_of(w, host, n, Some(b)) else {
                    cx.fail(
                        V,
                        REVEAL_COMMIT_MISMATCH,
                        what,
                        b,
                        Some(r.tx),
                        "a reveal of a march with no DEPART",
                    );
                    continue;
                };
                revealed.insert(d);
                let dr = &w.recs[d];
                let Some(x) = reveal_ix(w, r) else {
                    cx.missing(
                        V,
                        what,
                        b,
                        Some(r.tx),
                        "the Reveal's instruction data is not in the archive",
                    );
                    continue;
                };
                let c = seal::commit(&x.plain, &x.salt);
                if c != dr.p32("commit") || seal::seal_root(&c, &x.ct_hash) != dr.p32("seal_root") {
                    cx.fail(
                        V,
                        REVEAL_COMMIT_MISMATCH,
                        what.clone(),
                        b,
                        Some(r.tx),
                        "the revealed plaintext and salt do not give the departure's commitment",
                    );
                    continue;
                }
                let j = *judged.entry(d).or_insert_with(|| judge(cx, dr));
                // Wave-5 review: the same end-of-archive grace as the
                // unsettled-march path — T(arrive) is signed only once the
                // arrival bell's round is out, so a run cut inside that
                // grace is honest, not unverifiable.
                let arrive_end = permutation_rules::frontier::beacon::bell_end(f.genesis_ts, b);
                let in_grace = f.genesis_ts > 0 && arrive_end + 600 > last_time;
                if j.is_none() && in_grace {
                    cx.warn(
                        V,
                        MISSING_DATA,
                        what.clone(),
                        b,
                        Some(r.tx),
                        "T(arrive) not yet in the archive (the run ends inside the arrival bell's grace): the plaintext is not compared",
                    );
                } else if j.is_none() {
                    // W5-D (integ-W4 review): a seal that cannot be opened
                    // is not a seal that was checked.
                    cx.missing(
                        V,
                        what.clone(),
                        b,
                        Some(r.tx),
                        "no verified signature of T(arrive): the revealed plaintext cannot be compared with the opened seal",
                    );
                }
                if let Some(Judged {
                    code: 0,
                    plain: Some(p),
                }) = j
                {
                    if seal::pack(&p) != x.plain {
                        cx.fail(
                            V,
                            REVEAL_COMMIT_MISMATCH,
                            what,
                            b,
                            Some(r.tx),
                            "the revealed plaintext is not the one the seal opens to",
                        );
                    }
                }
            }
            Kind::TRANSIT_SETTLED => {
                let host = r.ku64("host_id");
                let what = format!("host {host}");
                let Some(d) = f.departs.get(&host).and_then(|v| {
                    v.iter()
                        .rev()
                        .copied()
                        .find(|&d| d < n && !settled.contains(&d))
                }) else {
                    cx.fail(
                        V,
                        VERDICT_DISAGREES_WITH_TLOCK,
                        what,
                        r.bell,
                        Some(r.tx),
                        "a settlement of a march with no DEPART",
                    );
                    continue;
                };
                settled.insert(d);
                let dr = &w.recs[d];
                let x = w.txs[r.tx]
                    .ix(Ix::SettleTransit.tag())
                    .and_then(|i| aix::SettleTransit::decode(&i.data).ok());
                match x {
                    Some(x) if x.commit == dr.p32("commit") && x.seal[..] == *dr.p("seal") => {}
                    Some(_) => cx.fail(
                        V,
                        REVEAL_COMMIT_MISMATCH,
                        what.clone(),
                        r.bell,
                        Some(r.tx),
                        "SettleTransit judged another pair than the one DEPART logged",
                    ),
                    None => cx.missing(
                        V,
                        what.clone(),
                        r.bell,
                        Some(r.tx),
                        "SettleTransit's data is not in the archive",
                    ),
                }
                let j = *judged.entry(d).or_insert_with(|| judge(cx, dr));
                let Some(j) = j else {
                    cx.missing(
                        V,
                        what,
                        r.bell,
                        Some(r.tx),
                        "no verified signature of T(arrive) to open the seal",
                    );
                    continue;
                };
                let (code, outcome) = (r.pu8("seal_code"), r.pu8("outcome"));
                // The exact code, but for the documented ambiguity: codes 1
                // (FO check) and 3 are not separable off chain (integ-W4
                // review, W4-D minor: only `> 0` was compared).
                let same = |a: u8, b: u8| a == b || (matches!(a, 1 | 3) && matches!(b, 1 | 3));
                if !same(code, j.code) {
                    cx.fail(
                        V,
                        VERDICT_DISAGREES_WITH_TLOCK,
                        what.clone(),
                        r.bell,
                        Some(r.tx),
                        format!("seal code {code}, the stock opener gives {}", j.code),
                    );
                }
                if j.code > 0 && outcome != transit_outcome::BAD_SEAL {
                    cx.fail(
                        V,
                        BAD_SEAL_SURVIVED,
                        what.clone(),
                        r.bell,
                        Some(r.tx),
                        format!(
                            "a bad seal (stock code {}) settled with outcome {outcome}",
                            j.code
                        ),
                    );
                }
                if j.code == 0 && outcome == transit_outcome::BAD_SEAL {
                    cx.fail(
                        V,
                        VERDICT_DISAGREES_WITH_TLOCK,
                        what.clone(),
                        r.bell,
                        Some(r.tx),
                        "a valid seal destroyed as bad",
                    );
                }
                // W6-C (W5-C F4, E5 criterion 4): the liveness warning is a
                // valid seal that settled ROUTED unrevealed; one that
                // settled with another outcome unrevealed (bounced-unranked:
                // outranked, quota-refused or the citizen's second arrival)
                // lost nothing by rule and is listed apart.
                // W6T-3 (w6-s7 triage): a valid seal whose Reveal §5.11
                // step 6 refuses by rule (Path, ArrivalBell, Shielded) can
                // only settle ROUTED and no keeper could have revealed it; it
                // is listed apart with its reason, judged from the archived
                // post-states (never from the refusals' error codes). The
                // failed attempts are counted per march `(host, arrive)`.
                if j.code == 0 && !revealed.contains(&d) {
                    let arrive = dr.pu32("arrive_bell");
                    let attempts = failed.get(&(host, arrive)).cloned().unwrap_or_default();
                    let reason = if outcome != transit_outcome::ROUTED {
                        Some(reason::BOUNCED)
                    } else {
                        j.plain.and_then(|p| rule_refusal(cx, dr, &p, r.tx))
                    };
                    match reason {
                        Some(reason) => cx.liveness.unrevealed_by_rule.push(crate::ByRule {
                            host,
                            arrive,
                            outcome,
                            reason,
                            failed_attempts: attempts.len(),
                        }),
                        None => {
                            cx.warn(
                                V,
                                VALID_SEAL_UNREVEALED,
                                what,
                                arrive,
                                Some(r.tx),
                                format!(
                                    "a valid seal never revealed ({} failed attempts)",
                                    attempts.len()
                                ),
                            );
                            cx.liveness.valid_unrevealed.push((host, arrive, attempts));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    // Bad seals never settled (liveness; E5 criterion 1).
    let all: Vec<usize> = f.departs.values().flatten().copied().collect();
    for d in all {
        if settled.contains(&d) {
            continue;
        }
        let dr = &w.recs[d];
        let j = judge(cx, dr);
        // W5-D: a march whose arrival bell ended a whole bell before the
        // archive does, and whose T(arrive) no transaction carried, cannot
        // be judged (a bad seal could hide there).
        let arrive = dr.pu32("arrive_bell");
        let ended = permutation_rules::frontier::beacon::bell_end(f.genesis_ts, arrive);
        if j.is_none() && f.genesis_ts > 0 && ended + 600 <= last_time {
            cx.missing(
                V,
                format!("host {}", dr.ku64("host_id")),
                arrive,
                Some(dr.tx),
                "an unsettled march with no verified signature of T(arrive) in the archive",
            );
        }
        if let Some(Judged { code, .. }) = j {
            if code > 0 {
                cx.warn(
                    V,
                    BAD_SEAL_UNSETTLED,
                    format!("host {}", dr.ku64("host_id")),
                    dr.pu32("arrive_bell"),
                    Some(dr.tx),
                    format!("a bad seal (code {code}) still unsettled at the end"),
                );
            }
        }
    }
}

/// Signatures of the failed Reveal transactions per march `(host,
/// arrive)` from the plaintext they carried (W6T-3: the host alone counted
/// every march of the host; w6-s7 showed 6 and 8 for marches of 2 and 3).
pub fn failed_reveals(w: &World) -> HashMap<(u64, u32), Vec<String>> {
    let mut m: HashMap<(u64, u32), Vec<String>> = HashMap::new();
    for t in w.txs.iter().filter(|t| !t.ok) {
        if let Some(x) = t
            .ix(Ix::Reveal.tag())
            .and_then(|i| aix::Reveal::decode(&i.data).ok())
        {
            let p = seal::unpack(&x.plain);
            m.entry((p.host_id, p.arrive_bell))
                .or_default()
                .push(t.sig.clone());
        }
    }
    m
}

/// Reasons of `liveness.unrevealed_by_rule` (W6T-3, §8.5 V5).
pub mod reason {
    /// The host's own Holding is shielded at `bell_start(arrive)` (not
    /// dormant) and the destination is another faction's holding site.
    pub const SHIELDED_OWN: &str = "shielded-own";
    /// The destination is a shielded holding site of another faction.
    pub const SHIELDED_DEST: &str = "shielded-dest";
    /// The path is not a walk from the origin to the destination over
    /// passable hexes of at most 4 provinces (≤ 32 steps).
    pub const PATH: &str = "path";
    /// The path's travel time does not allow the arrival bell.
    pub const ARRIVAL_BELL: &str = "arrival-bell";
    /// Settled with another outcome than ROUTED (no loss by rule).
    pub const BOUNCED: &str = "bounced";
    pub const ALL: &[&str] = &[SHIELDED_OWN, SHIELDED_DEST, PATH, ARRIVAL_BELL, BOUNCED];
}

/// §5.11 step 6 for a valid march `p` of DEPART `dr`, judged independently
/// of the program on the archived post-states the Reveals read: the state
/// before the first transaction at or after the arrival bell's end (a
/// Reveal is only possible before it; sites, shields and terrain change
/// only at resolves), and never after the settlement. In the program's
/// order: the path (`Path`), the travel time (`ArrivalBell`), then the two
/// shield clauses (`Shielded`). `None`: the rules allowed the Reveal (a
/// keeper could have revealed it). A state that is not in the archive
/// gives `None`, so the march stays a liveness warning (fail safe).
pub fn rule_refusal(cx: &Ctx, dr: &Rec, p: &Plain, settle_tx: usize) -> Option<&'static str> {
    use fclient::decode::{Holding, Province};
    use frontier_abi::layout::player::{holding as H, transit as T};
    use frontier_abi::layout::province::site as SM;
    use permutation_rules::frontier::beacon::{bell_end, bell_start};
    use permutation_rules::frontier::geometry::{locate, ProvinceCoord};
    use permutation_rules::frontier::travel::{
        self, CAVALRY_BPS, FLAT_HEX_SECS, MAX_PATH_PROVINCES, MAX_PATH_STEPS, ROAD_BPS,
        ROUGH_HEX_SECS,
    };
    use permutation_rules::hex::{Hex, DIRECTIONS};
    let (w, f) = (cx.w, cx.f);
    let arrive = dr.pu32("arrive_bell");
    let host = dr.ku64("host_id");
    let end = bell_end(f.genesis_ts, arrive);
    let at = w
        .txs
        .partition_point(|t| t.time < end)
        .min(settle_tx)
        .max(dr.tx + 1);
    let hd = w.state_before(&f.ctx.holding_of_host(host)?, at)?;
    let h = Holding::decode(hd).ok()?;
    let tr = h
        .transit
        .iter()
        .find(|t| T::in_transit(t.state) && t.host_id == host && t.arrive_bell == arrive)?;
    let province = |pq: (i32, i32)| -> Option<Province> {
        Province::decode(w.state_before(&f.ctx.province(pq.0, pq.1), at)?).ok()
    };
    // The walk: every step's hex, its province and tile (kernel geometry).
    let n = p.path_len as usize;
    if n == 0 || n > MAX_PATH_STEPS {
        return Some(reason::PATH);
    }
    let c = ProvinceCoord::new(tr.origin_p as i32, tr.origin_q as i32).tile(tr.origin_tile)?;
    let mut hx = c;
    let mut seen: Vec<(i32, i32)> = vec![];
    let mut lands: HashMap<(i32, i32), Option<Province>> = HashMap::new();
    let cav =
        permutation_rules::frontier::catalog::unit_of(tr.unit).is_some_and(travel::is_cavalry);
    let mut secs = 0u64;
    for i in 0..n {
        let d = seal::step(&p.path, i as u8) as usize;
        let Some((dq, dr_)) = DIRECTIONS.get(d) else {
            return Some(reason::PATH);
        };
        hx = Hex::new(hx.q + dq, hx.r + dr_);
        let (pc, tile) = locate(hx);
        let k = (pc.p, pc.q);
        if !seen.contains(&k) {
            seen.push(k);
        }
        let Some(pv) = lands.entry(k).or_insert_with(|| province(k)) else {
            // An unopened province: the program cannot read its masks.
            return Some(reason::PATH);
        };
        let bit = |m: u64| tile < 64 && m >> tile & 1 == 1;
        if !bit(pv.passable_mask) {
            return Some(reason::PATH);
        }
        let mut s = if bit(pv.rough_mask) {
            ROUGH_HEX_SECS
        } else {
            FLAT_HEX_SECS
        } as u64;
        if bit(pv.road_mask) {
            s = s * ROAD_BPS as u64 / 10_000;
        }
        if cav {
            s = s * CAVALRY_BPS as u64 / 10_000;
        }
        secs += s;
    }
    let (pc, tile) = locate(hx);
    if (pc.p, pc.q) != (p.dest_p as i32, p.dest_q as i32)
        || tile != p.dest_tile
        || seen.len() > MAX_PATH_PROVINCES
    {
        return Some(reason::PATH);
    }
    let doctrine = permutation_rules::frontier::doctrine::of_faction(tr.faction)?;
    let secs = u32::try_from(doctrine.travel_secs(secs)).ok()?;
    if travel::check_arrival_bell(f.genesis_ts, tr.depart_ts, secs, arrive).is_err() {
        return Some(reason::ARRIVAL_BELL);
    }
    // The shield clauses: only a holding site of another faction.
    let dest = lands.get(&(pc.p, pc.q))?.as_ref()?;
    let n_sites = (dest.site_count as usize).min(dest.sites.len());
    let k = (0..n_sites).find(|&k| dest.sites[k] == p.dest_tile)?;
    let site = &dest.site_mirror[k];
    let other = site.state == SM::STATE_HOLDING && site.faction != tr.faction;
    if other && site.shield_until_bell > arrive {
        return Some(reason::SHIELDED_DEST);
    }
    let dormant = h.flags & H::FLAG_DORMANT_CACHE != 0;
    if other && !dormant && h.shield_until > bell_start(f.genesis_ts, arrive) {
        return Some(reason::SHIELDED_OWN);
    }
    None
}
