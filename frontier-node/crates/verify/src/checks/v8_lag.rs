//! V8 — the lag witness (contract §5.11 SettleDeparture and GatherClash,
//! §8.5; design §8.4 "lag only waits").
//!
//! - Every DEPARTURE_SETTLED lands after its origin resolved the departure
//!   bell (a CLASH of that bell or a SKIP over it), and its values are that
//!   resolve's: the host as the origin Province held it before the resolve,
//!   its result of the replayed clash applied (`Host::apply_clash`), its
//!   departure settled (`Host::settle`), then `Host::march_values`.
//! - Every gathered arrival's troops and stamina are its DEPARTURE_SETTLED
//!   values — never a later bell's — the stamina refilled from
//!   `depart_bell + 1` to the arrival bell (W4-A's pinned rule, v1.6 §22:
//!   the program's `model::arrival_stamina`; integ-W4 review).
//! - A return settle (`DEPARTURE_SETTLED.destroyed = 2`, SettleDeparture
//!   with `transit_slot = 0xFF`, v1.6 §22) freed a `Leave` entry (state 3)
//!   of the host in the Province and credited its whole troops to the
//!   Holding's reserve (integ-W4 review: V8 looked for a DEPART).
//!
//! Code: `OriginValueMismatch`.

use std::collections::HashMap;

use frontier_abi::entry::{find_entry, read_entry, EntryOp};
use frontier_abi::layout::player::holding as HL;
use frontier_abi::layout::province::entry as EN;
use frontier_abi::log::Kind;
use permutation_rules::frontier::host::{Host, Stamina};

use super::Ctx;
use crate::clash_input::{ClashBuilder, ContractBuilder};
use crate::codes::*;
use crate::facts::Facts;

const V: &str = "V8";

/// Per province, its CLASH and SKIP records: `(record, first bell, end
/// bell, is a clash)`.
type Resolutions = HashMap<(i32, i32), Vec<(usize, u32, u32, bool)>>;

fn resolutions(cx: &Ctx) -> Resolutions {
    let mut m: Resolutions = HashMap::new();
    for (i, r) in cx.w.recs.iter().enumerate() {
        match r.kind {
            Kind::CLASH => {
                let b = r.ku32("bell");
                m.entry(r.pq()).or_default().push((i, b, b + 1, true));
            }
            Kind::SKIP => {
                let b0 = r.pu32("b0");
                m.entry(r.pq())
                    .or_default()
                    .push((i, b0, b0 + r.pu8("n") as u32, false));
            }
            _ => {}
        }
    }
    m
}

/// The resolution of `(P, Q)` covering bell `b` before record `n`:
/// `(record, is a clash)`.
fn resolution(res: &Resolutions, p: i32, q: i32, b: u32, n: usize) -> Option<(usize, bool)> {
    res.get(&(p, q))?
        .iter()
        .find(|(i, b0, b1, _)| *i < n && *b0 <= b && b < *b1)
        .map(|x| (x.0, x.3))
}

/// The origin values of `host` departed at `b` from `(p, q)`, resolved by
/// record `res` (`Err`: why they cannot be computed).
pub fn origin_values(
    cx: &Ctx,
    host: u64,
    p: i32,
    q: i32,
    b: u32,
    res: (usize, bool),
) -> Result<(u32, u16), String> {
    let w = cx.w;
    let f = cx.f;
    let r = &w.recs[res.0];
    let pk = f.ctx.province(p, q);
    let pv = w
        .state_before(&pk, r.tx)
        .ok_or("no origin Province state before its resolve")?;
    let i = find_entry(pv, host).ok_or("the host is not in the origin's entries")?;
    let e = read_entry(pv, i).map_err(|e| format!("{e:?}"))?;
    let mut h: Host = e.to_host().map_err(|e| format!("{e:?}"))?;
    if res.1 {
        let seed = f
            .bell_seed(b, Facts::region(p, q))
            .ok_or("no origin seed")?;
        let ci = w
            .state_before(&f.ctx.clash_inputs(p, q, b), r.tx)
            .unwrap_or(&[]);
        let built = ContractBuilder.build(pv, ci, b, &seed)?;
        let o = built.resolve()?;
        if let Some(fr) = o.fighter(host) {
            h.apply_clash(b, fr.troops, fr.stamina, fr.engaged)
                .map_err(|e| format!("{e:?}"))?;
        }
    }
    h.settle(b + 1).map_err(|e| format!("{e:?}"))?;
    h.march_values(b, b + 1).map_err(|e| format!("{e:?}"))
}

/// A return settle (`destroyed = 2`): the Leave entry it freed and the
/// reserve it credited (`Err`: what does not hold).
fn check_return(cx: &Ctx, n: usize, host: u64) -> Result<(), String> {
    let w = cx.w;
    let r = &w.recs[n];
    let tx = &w.txs[r.tx];
    let ix = tx
        .ixs
        .first()
        .ok_or("a return settle outside a program instruction")?;
    let (pk, hk) = (
        ix.key(2).ok_or("no province account")?,
        ix.key(3).ok_or("no holding account")?,
    );
    let pv = w
        .state_before(&pk, r.tx)
        .ok_or("no Province state before the return")?;
    let i = find_entry(pv, host).ok_or("no entry of the host before the return")?;
    let e = read_entry(pv, i).map_err(|e| format!("{e:?}"))?;
    if e.state != EN::STATE_DEPARTED || e.op != EntryOp::Leave {
        return Err(format!(
            "the entry is in state {} op {:?}, not a departed Leave",
            e.state, e.op
        ));
    }
    let whole = e.troops / 1_000;
    if r.pu32("troops_after") != whole * 1_000 {
        return Err(format!(
            "logged {} troops, the entry held {} (whole {whole})",
            r.pu32("troops_after"),
            e.troops
        ));
    }
    let at = HL::reserve(e.unit as usize);
    let rd = |d: &[u8]| {
        d.get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    let before = w.state_before(&hk, r.tx).and_then(rd);
    let after = tx.post_data(&hk).and_then(rd);
    match (before, after) {
        (Some(b), Some(a)) if a >= b && a - b >= whole => Ok(()),
        (b, a) => Err(format!(
            "reserve[{}] {b:?} → {a:?}, the return credits {whole}",
            e.unit
        )),
    }
}

pub fn run(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    // host → (record, troops, stamina, depart bell) of its settlements.
    let mut settled: HashMap<u64, Vec<(usize, u32, u16, u32)>> = HashMap::new();
    let res = resolutions(cx);
    for (n, r) in w.recs.iter().enumerate() {
        match r.kind {
            Kind::DEPARTURE_SETTLED if r.pu8("destroyed") == 2 => {
                let host = r.ku64("host_id");
                if let Err(e) = check_return(cx, n, host) {
                    cx.fail(
                        V,
                        ORIGIN_VALUE_MISMATCH,
                        format!("return of host {host}"),
                        r.bell,
                        Some(r.tx),
                        e,
                    );
                }
            }
            Kind::DEPARTURE_SETTLED => {
                let host = r.ku64("host_id");
                let what = format!("departure of host {host}");
                let (ta, sa) = (r.pu32("troops_after"), r.pu16("stamina_after"));
                let Some(d) = f.depart_of(w, host, n, None) else {
                    cx.fail(
                        V,
                        ORIGIN_VALUE_MISMATCH,
                        what,
                        r.bell,
                        Some(r.tx),
                        "no DEPART",
                    );
                    continue;
                };
                let dr = &w.recs[d];
                let (p, q, b) = (
                    dr.pi32("origin_p"),
                    dr.pi32("origin_q"),
                    dr.pu32("depart_bell"),
                );
                settled.entry(host).or_default().push((n, ta, sa, b));
                let Some(res) = resolution(&res, p, q, b, n) else {
                    cx.fail(
                        V,
                        ORIGIN_VALUE_MISMATCH,
                        what,
                        b,
                        Some(r.tx),
                        "settled before its origin resolved the departure bell",
                    );
                    continue;
                };
                if r.pu8("destroyed") != 0 {
                    continue;
                }
                match origin_values(cx, host, p, q, b, res) {
                    Ok((t, s)) if (t, s) == (ta, sa) => {}
                    Ok((t, s)) => cx.fail(
                        V,
                        ORIGIN_VALUE_MISMATCH,
                        what,
                        b,
                        Some(r.tx),
                        format!("settled ({ta}, {sa}), the origin's resolve of bell {b} gives ({t}, {s})"),
                    ),
                    Err(e) => cx.missing(V, what, b, Some(r.tx), e),
                }
            }
            Kind::CLASH => {
                let (p, q) = r.pq();
                let b = r.ku32("bell");
                let Some(ci) = w
                    .state_before(&f.ctx.clash_inputs(p, q, b), r.tx)
                    .and_then(|x| fclient::decode::ClashInputs::decode(x).ok())
                else {
                    continue;
                };
                for a in ci.arrivals.iter().filter(|a| a.present == 1) {
                    let last = settled.get(&a.host_id).and_then(|v| v.last()).copied();
                    // The stamina refilled over the march (v1.6 §22).
                    let last = last.map(|(n, t, s, db)| {
                        let s = Stamina {
                            value: s,
                            bell: db.saturating_add(1),
                        }
                        .at(b);
                        (n, t, s)
                    });
                    match last {
                        Some((_, t, s)) if (t, s) == (a.troops, a.stamina) => {}
                        Some((_, t, s)) => cx.fail(
                            V,
                            ORIGIN_VALUE_MISMATCH,
                            format!("arrival of host {}", a.host_id),
                            b,
                            Some(r.tx),
                            format!(
                                "gathered ({}, {}), its departure settled ({t}, {s})",
                                a.troops, a.stamina
                            ),
                        ),
                        None => cx.fail(
                            V,
                            ORIGIN_VALUE_MISMATCH,
                            format!("arrival of host {}", a.host_id),
                            b,
                            Some(r.tx),
                            "gathered before its departure was settled",
                        ),
                    }
                }
            }
            _ => {}
        }
    }
}
