//! V12 — explore (contract §5.10 SettleExplore, §8.5; I-56).
//!
//! Every EXPLORE_RESULT equals `explore::roll(S(record.bell, r), P, Q,
//! tile, host, floor)` per tile, with S the seed of the EXPLORE record's
//! bell in the explored province's region and the floor spent tile by tile
//! from the citizen's `EXPLORES_FLOOR` (3, set at JOIN).
//!
//! Code: `ExploreRollMismatch`.

use std::collections::HashMap;

use frontier_abi::layout::player::citizen as C;
use frontier_abi::log::Kind;
use permutation_rules::frontier::explore;
use permutation_rules::frontier::geometry::{region_of, ProvinceCoord};

use super::Ctx;
use crate::codes::*;
use crate::world::{le, Key};

const V: &str = "V12";
const NO_TILE: u8 = 0xFF;

pub fn run(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    let mut floor: HashMap<Key, u8> = HashMap::new();
    let mut pending: HashMap<u64, usize> = HashMap::new();
    for (n, r) in w.recs.iter().enumerate() {
        match r.kind {
            Kind::JOIN => {
                let tag15: [u8; 15] = r.k("citizen_tag15").try_into().unwrap_or([0; 15]);
                floor.insert(f.ctx.citizen_by_tag15(&tag15), C::EXPLORES_FLOOR);
            }
            Kind::EXPLORE => {
                pending.insert(r.ku64("host_id"), n);
            }
            Kind::EXPLORE_RESULT => {
                let host = r.ku64("host_id");
                let what = format!("host {host}");
                let Some(e) = pending.remove(&host).map(|i| &w.recs[i]) else {
                    cx.fail(
                        V,
                        EXPLORE_ROLL_MISMATCH,
                        what,
                        r.bell,
                        Some(r.tx),
                        "an explore result without an explore",
                    );
                    continue;
                };
                let (p, q) = (e.pi32("p"), e.pi32("q"));
                let pc = ProvinceCoord::new(p, q);
                let Some(seed) = f.bell_seed(e.bell, region_of(pc)) else {
                    cx.missing(V, what, r.bell, Some(r.tx), "no seed of the explore's bell");
                    continue;
                };
                let Some(citizen) = f.owner_of_host(host, n).and_then(|o| o.owner) else {
                    cx.missing(
                        V,
                        what,
                        r.bell,
                        Some(r.tx),
                        "the explorer's holding has no owner",
                    );
                    continue;
                };
                let left = floor.entry(citizen).or_insert(C::EXPLORES_FLOOR);
                let tiles = e.p("tiles");
                let mut per = [0u32; 2];
                let (mut total, mut used) = (0u32, 0u8);
                for (i, &t) in tiles.iter().enumerate().take(2) {
                    if t == NO_TILE {
                        continue;
                    }
                    let fl = *left > 0;
                    if fl {
                        *left -= 1;
                        used += 1;
                    }
                    per[i] = explore::roll(&seed, pc, t, host, fl).works;
                    total += per[i];
                }
                let got = r.p("works_per_tile");
                let got = [le(&got[..4]) as u32, le(&got[4..8]) as u32];
                if got != per || r.pu32("works") != total || r.pu8("floor_used") != used {
                    cx.fail(
                        V,
                        EXPLORE_ROLL_MISMATCH,
                        what,
                        r.bell,
                        Some(r.tx),
                        format!(
                            "finds {got:?} (floor {}) but explore::roll gives {per:?} (floor {used})",
                            r.pu8("floor_used")
                        ),
                    );
                }
            }
            _ => {}
        }
    }
}
