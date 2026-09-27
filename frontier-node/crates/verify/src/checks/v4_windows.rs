//! V4 — reveal windows (contract §5.1, §5.11 Reveal steps 4–5, §8.5; I-07).
//!
//! Every landed Reveal of an arrival at `(P, Q, b)`: once THE anchor of
//! `(b, region)` has landed, the Reveal's Clock is before `A + W(b)`; after
//! the bell is archived no Reveal lands; and it precedes the province-bell's
//! first GATHER, its CLASH and any SKIP over `b` (the latch). A Reveal in the
//! last 16 game seconds of its window is a liveness warning.
//!
//! Codes: `RevealAfterClose`, `RevealAfterLatch`; warn `RevealNearClose`.

use std::collections::HashMap;

use frontier_abi::log::Kind;

use super::Ctx;
use crate::codes::*;
use crate::facts::Facts;

const V: &str = "V4";

/// Per province, its SKIP runs: `(first bell, end bell, record)`.
type Skips = HashMap<(i32, i32), Vec<(u32, u32, usize)>>;
/// The keeper's ε before the close (§8.2: 2 slots at 20×, 8 game s each).
pub const NEAR_CLOSE_SECS: i64 = 16;

pub fn run(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    // The first latch event per (P, Q, bell): GATHER or CLASH of the bell,
    // or a SKIP whose run covers it.
    let mut latched: HashMap<(i32, i32, u32), usize> = HashMap::new();
    let mut skips: Skips = HashMap::new();
    for (n, r) in w.recs.iter().enumerate() {
        match r.kind {
            Kind::GATHER | Kind::CLASH => {
                let (p, q) = r.pq();
                latched.entry((p, q, r.ku32("bell"))).or_insert(n);
            }
            Kind::SKIP => {
                let (p, q) = r.pq();
                let b0 = r.pu32("b0");
                skips
                    .entry((p, q))
                    .or_default()
                    .push((b0, b0 + r.pu8("n") as u32, n));
            }
            _ => {}
        }
    }
    for (n, r) in w
        .recs
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == Kind::REVEAL)
    {
        let (p, q) = r.pq();
        let b = r.ku32("arrive");
        let region = Facts::region(p, q);
        let t = w.txs[r.tx].time;
        let what = format!("reveal ({p}, {q}) bell {b} host {}", r.pu64("host_id"));
        if let Some(a) = f.anchor(b, region).filter(|a| a.rec < n) {
            let close = f.close(b, a.a);
            if t >= close {
                cx.fail(
                    V,
                    REVEAL_AFTER_CLOSE,
                    what.clone(),
                    b,
                    Some(r.tx),
                    format!("landed at {t}, the window closed at A + W = {close}"),
                );
            } else if t >= close - NEAR_CLOSE_SECS {
                cx.liveness.reveals_near_close += 1;
                cx.warn(
                    V,
                    REVEAL_NEAR_CLOSE,
                    what.clone(),
                    b,
                    Some(r.tx),
                    format!("landed {} s before the close", close - t),
                );
            }
        }
        if let Some((_, _, ar)) = f.archived.get(&(b, region)) {
            if *ar < n {
                cx.fail(
                    V,
                    REVEAL_AFTER_CLOSE,
                    what.clone(),
                    b,
                    Some(r.tx),
                    "landed after the bell was archived",
                );
            }
        }
        let latch = latched.get(&(p, q, b)).copied().into_iter().chain(
            skips
                .get(&(p, q))
                .into_iter()
                .flatten()
                .filter(|(b0, b1, _)| *b0 <= b && b < *b1)
                .map(|x| x.2),
        );
        if let Some(l) = latch.filter(|&l| l < n).min() {
            cx.fail(
                V,
                REVEAL_AFTER_LATCH,
                what,
                b,
                Some(r.tx),
                format!(
                    "landed after the province-bell's {} record",
                    w.recs[l].kind.name()
                ),
            );
        }
    }
}
