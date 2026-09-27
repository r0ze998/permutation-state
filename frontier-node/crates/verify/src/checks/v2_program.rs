//! V2 — program and rules (contract §8.5).
//!
//! - **Program id:** the season lives under the program the verifier was
//!   told (the Season account at the program's PDA, owned by it); every
//!   archived transaction invokes it.
//! - **`.so` hash per slot range:** every observed deployment hash equals
//!   the pinned one (when one is pinned; an unobserved pin is `MissingData`).
//! - **Ruleset:** SEASON_CREATED's `ruleset_hash` = the expected hash, and
//!   so does the Season account's.
//! - **Announce → create:** one ANNOUNCE and one SEASON_CREATED; the
//!   params hash of CreateSeason's data (`presets::params_hash`) equals
//!   both records'; creation at or after `t_create_min` and inside the
//!   creation window; the created program version is the binary's.
//!
//! Codes: `ProgramMismatch`, `RulesetMismatch`, `AnnounceMismatch`.

use frontier_abi::layout::world::season as S;
use frontier_abi::log::Kind;
use frontier_abi::presets::{self, SEASON_PARAMS_LEN};

use super::Ctx;
use crate::codes::*;

const V: &str = "V2";

pub fn run(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    let season = f.ctx.season;
    // Program id.
    match w.finals.get(&season) {
        Some(Some(a)) if a.owner.to_bytes() != w.program => cx.fail(
            V,
            PROGRAM_MISMATCH,
            "Season",
            0,
            None,
            "the Season account is not owned by the program",
        ),
        Some(Some(a))
            if a.data.get(S::RULESET_HASH..S::RULESET_HASH + 32)
                != Some(&cx.cfg.ruleset_hash[..]) =>
        {
            cx.fail(
                V,
                RULESET_MISMATCH,
                "Season",
                0,
                None,
                "the Season account's ruleset hash is not the expected one",
            );
        }
        _ => {}
    }
    for (i, t) in w.txs.iter().enumerate() {
        if t.ixs.is_empty() && !t.keys.is_empty() {
            // Harmless (its logs count for nothing), but not the archive
            // of this program: reported, not failed.
            cx.warn(
                V,
                PROGRAM_MISMATCH,
                format!("tx#{i}"),
                0,
                Some(i),
                "an archived transaction does not invoke the program",
            );
        }
    }
    if let Some(want) = cx.cfg.program_hash {
        if cx.input.program_hashes.is_empty() {
            cx.missing(
                V,
                "program",
                0,
                None,
                "a .so hash is pinned but no deployment was observed",
            );
        }
        for (slot, h) in &cx.input.program_hashes {
            if *h != want {
                cx.fail(
                    V,
                    PROGRAM_MISMATCH,
                    "program",
                    0,
                    None,
                    format!(
                        "the program deployed from slot {slot} has sha256 {}",
                        hex::encode(h)
                    ),
                );
            }
        }
    }
    // Ruleset and announce → create.
    let announces: Vec<_> = w.of(Kind::ANNOUNCE).collect();
    let created: Vec<_> = w.of(Kind::SEASON_CREATED).collect();
    if announces.len() > 1 || created.len() > 1 {
        cx.fail(
            V,
            ANNOUNCE_MISMATCH,
            "Season",
            0,
            None,
            format!(
                "{} ANNOUNCE and {} SEASON_CREATED records",
                announces.len(),
                created.len()
            ),
        );
    }
    let Some(c) = created.first() else {
        if !w.recs.is_empty() {
            cx.missing(V, "Season", 0, None, "no SEASON_CREATED record");
        }
        return;
    };
    if c.p32("ruleset_hash") != cx.cfg.ruleset_hash {
        cx.fail(
            V,
            RULESET_MISMATCH,
            "Season",
            c.bell,
            Some(c.tx),
            format!(
                "SEASON_CREATED carries ruleset {} but {} is expected",
                hex::encode(c.p32("ruleset_hash")),
                hex::encode(cx.cfg.ruleset_hash)
            ),
        );
    }
    let Some(a) = announces.first() else {
        cx.fail(
            V,
            ANNOUNCE_MISMATCH,
            "Season",
            c.bell,
            Some(c.tx),
            "SEASON_CREATED without an ANNOUNCE",
        );
        return;
    };
    if a.p32("params_hash") != c.p32("params_hash") {
        cx.fail(
            V,
            ANNOUNCE_MISMATCH,
            "Season",
            c.bell,
            Some(c.tx),
            "the created params hash is not the announced one",
        );
    }
    let t_create_min = a.pi64("t_create_min");
    let ct = w.txs[c.tx].time;
    if ct < t_create_min || ct >= t_create_min + presets::CREATE_WINDOW_SECS {
        cx.fail(
            V,
            ANNOUNCE_MISMATCH,
            "Season",
            c.bell,
            Some(c.tx),
            format!("created at {ct}, outside [{t_create_min}, +7 d)"),
        );
    }
    match (&f.params, f.create_tx) {
        (Some(p), Some(i)) => {
            let raw: [u8; SEASON_PARAMS_LEN] = p.to_bytes();
            if presets::params_hash(&raw, &f.payout) != a.p32("params_hash") {
                cx.fail(
                    V,
                    ANNOUNCE_MISMATCH,
                    "Season",
                    c.bell,
                    Some(i),
                    "CreateSeason's parameters do not hash to the announced params hash",
                );
            }
            if p.program_version != c.pu16("program_version") {
                cx.fail(
                    V,
                    PROGRAM_MISMATCH,
                    "Season",
                    c.bell,
                    Some(i),
                    "SEASON_CREATED's program version is not CreateSeason's",
                );
            }
        }
        _ => cx.missing(
            V,
            "Season",
            c.bell,
            Some(c.tx),
            "CreateSeason's instruction data is not in the archive",
        ),
    }
}
