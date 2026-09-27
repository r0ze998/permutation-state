//! V8 — the lag witness (contract §5.11 SettleDeparture and GatherClash,
//! §8.5; design §8.4 "lag only waits").
//!
//! - Every DEPARTURE_SETTLED lands after its origin resolved the departure
//!   bell (a CLASH of that bell or a SKIP over it), and its values are that
//!   resolve's: the host as the origin Province held it before the resolve,
//!   its result of the replayed clash applied (`Host::apply_clash`), its
//!   departure settled (`Host::settle`), then `Host::march_values`.
//! - Every gathered arrival's troops and stamina are its DEPARTURE_SETTLED
//!   values — never a later bell's.
//!
//! Code: `OriginValueMismatch`.

use std::collections::HashMap;

use frontier_abi::entry::{find_entry, read_entry};
use frontier_abi::log::Kind;
use permutation_rules::frontier::host::Host;

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

pub fn run(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    let mut settled: HashMap<u64, Vec<(usize, u32, u16)>> = HashMap::new();
    let res = resolutions(cx);
    for (n, r) in w.recs.iter().enumerate() {
        match r.kind {
            Kind::DEPARTURE_SETTLED => {
                let host = r.ku64("host_id");
                let what = format!("departure of host {host}");
                let (ta, sa) = (r.pu32("troops_after"), r.pu16("stamina_after"));
                settled.entry(host).or_default().push((n, ta, sa));
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
