//! V3 — randomness (contract §5.1, §5.8, §8.5; CL-19/20/24, I-30, I-44).
//!
//! Every signature the season used is re-verified off chain with blstrs
//! against the **pinned** key (the season's `quicknet_pk_hash` must be that
//! key's), and every round is the rule's:
//!
//! - GENESIS_SEED: `round = genesis_seed_round(t_create_min)` (= the
//!   SEASON_CREATED round), `genesis_ts = round_time(round) + 600`, `seed =
//!   seed_of(round, sig)`;
//! - RING_OPEN of a genesis ring `d ≤ g`: `seed = sha256("PSF-RING" ‖
//!   genesis_seed ‖ le16(d))`, round 0; of a later ring: `round =
//!   ring_seed_round(t_open)` with `t_open` the landing Clock; RING_SEED:
//!   that round, `seed = seed_of(round, sig)`;
//! - ANCHOR: one per `(bell, region)` for the whole season (after an
//!   archive too), `round = T(bell)`, `A` and `slot` = the landing Clock and
//!   slot, the anchor at its canonical address;
//! - SEED: THE anchor's `A`, `round = S(bell, r) = first_round_from(A +
//!   W(bell) + Δ)`, `seed = seed_of(round, sig)`, the cache canonical;
//! - BEACON: the signature verifies;
//! - ARCHIVE: `a_off = A − bell_end(bell)`, the entry's seed is THE seed and
//!   its signature THE anchor's.
//!
//! Codes: `BeaconSigInvalid`, `DuplicateAnchor`, `NonCanonicalAddress`,
//! `SeedRoundRule`, `GenesisSeedRule`, `RingSeedRule`.

use frontier_abi::layout::beacon::{anchor_archive as AA, archive_entry as AE};
use frontier_abi::layout::world::ring_seed as RS;
use frontier_abi::log::Kind;
use frontier_abi::tags::Ix;
use permutation_rules::frontier::beacon;
use permutation_rules::hash::sha256;

use super::Ctx;
use crate::codes::*;
use crate::world::Rec;

const V: &str = "V3";

/// `seed_of(round, sig)` of a compressed signature.
pub fn seed_of48(round: u64, sig: &[u8; 48]) -> Option<[u8; 32]> {
    fclient::beacon::decompress_sig(sig).map(|s| fclient::beacon::seed_of(round, &s))
}

/// The signature for `round` in record `r`'s own transaction, from an
/// instruction of one of `tags`.
fn tx_sig(cx: &Ctx, r: &Rec, round: u64, tags: &[Ix]) -> Option<[u8; 48]> {
    cx.f.sigs
        .get(&round)?
        .iter()
        .find(|s| s.tx == r.tx && tags.contains(&s.ix))
        .map(|s| s.sig)
}

/// Verifies `sig` for `round`; a FAIL if it does not verify.
fn check_sig(
    cx: &mut Ctx,
    r: &Rec,
    round: u64,
    sig: Option<[u8; 48]>,
    what: &str,
) -> Option<[u8; 48]> {
    let Some(sig) = sig else {
        cx.missing(
            V,
            what.to_string(),
            r.bell,
            Some(r.tx),
            format!("the transaction carries no signature of round {round}"),
        );
        return None;
    };
    let pk = cx.f.pk;
    if !cx.sigs.ok(round, &sig, &pk) {
        cx.fail(
            V,
            BEACON_SIG_INVALID,
            what.to_string(),
            r.bell,
            Some(r.tx),
            format!("the signature of round {round} does not verify under the pinned key"),
        );
        return None;
    }
    Some(sig)
}

pub fn run(cx: &mut Ctx) {
    let w = cx.w;
    let f = cx.f;
    // The season pins the key the verifier was given.
    if let Some(c) = w.of(Kind::SEASON_CREATED).next() {
        if c.p32("quicknet_pk_hash") != fclient::beacon::pk_hash(&f.pk) {
            cx.fail(
                V,
                BEACON_SIG_INVALID,
                "Season",
                c.bell,
                Some(c.tx),
                "the season pins another drand key than the verifier's",
            );
        }
        if let Some(a) = w.of(Kind::ANNOUNCE).next() {
            let want = beacon::genesis_seed_round(&f.clock, a.pi64("t_create_min"), f.margin);
            if c.pu64("genesis_round") != want {
                cx.fail(
                    V,
                    GENESIS_SEED_RULE,
                    "Season",
                    c.bell,
                    Some(c.tx),
                    format!(
                        "genesis round {} but the rule gives {want}",
                        c.pu64("genesis_round")
                    ),
                );
            }
            if c.pi64("genesis_ts") != beacon::genesis_ts(&f.clock, c.pu64("genesis_round")) {
                cx.fail(
                    V,
                    GENESIS_SEED_RULE,
                    "Season",
                    c.bell,
                    Some(c.tx),
                    "genesis_ts is not round_time(genesis_round) + 600",
                );
            }
        }
    }
    for r in w.of(Kind::GENESIS_SEED) {
        let round = r.pu64("round");
        if round != f.genesis_round {
            cx.fail(
                V,
                GENESIS_SEED_RULE,
                "Season",
                r.bell,
                Some(r.tx),
                format!(
                    "genesis seed from round {round}, the season's genesis round is {}",
                    f.genesis_round
                ),
            );
        }
        let sig = tx_sig(cx, r, round, &[Ix::ConsumeGenesisSeed]);
        if let Some(sig) = check_sig(cx, r, round, sig, "genesis seed") {
            if seed_of48(round, &sig) != Some(r.p32("seed")) {
                cx.fail(
                    V,
                    GENESIS_SEED_RULE,
                    "Season",
                    r.bell,
                    Some(r.tx),
                    "the genesis seed is not seed_of(round, sig)",
                );
            }
        }
    }
    // Rings.
    let g = f.params.map(|p| p.genesis_ring as u16).unwrap_or(0);
    let mut ring_rounds = std::collections::BTreeMap::new();
    for r in w.of(Kind::RING_OPEN) {
        let d = u16::from_le_bytes([r.k("d")[0], r.k("d")[1]]);
        let round = r.pu64("round");
        if d <= g {
            let want = f
                .genesis_seed
                .map(|s| sha256(&[RS::GENESIS_RING_DOMAIN, &s, &d.to_le_bytes()]));
            if round != 0 || want != Some(r.p32("seed")) {
                cx.fail(
                    V,
                    RING_SEED_RULE,
                    format!("ring {d}"),
                    r.bell,
                    Some(r.tx),
                    "a genesis ring's seed is not sha256(\"PSF-RING\" ‖ genesis_seed ‖ le16(d))",
                );
            }
        } else {
            let t_open = r.pi64("t_open");
            let want = beacon::ring_seed_round(&f.clock, t_open, f.margin);
            if round != want || t_open != w.txs[r.tx].time {
                cx.fail(
                    V,
                    RING_SEED_RULE,
                    format!("ring {d}"),
                    r.bell,
                    Some(r.tx),
                    format!("ring seed round {round}, the rule gives {want} for t_open {t_open}"),
                );
            }
            ring_rounds.insert(d, round);
        }
    }
    for r in w.of(Kind::RING_SEED) {
        let d = u16::from_le_bytes([r.k("d")[0], r.k("d")[1]]);
        let round = r.pu64("round");
        if ring_rounds.get(&d) != Some(&round) {
            cx.fail(
                V,
                RING_SEED_RULE,
                format!("ring {d}"),
                r.bell,
                Some(r.tx),
                format!("ring {d} seeded from round {round}, not its RING_OPEN round"),
            );
        }
        let sig = tx_sig(cx, r, round, &[Ix::ConsumeRingSeed]);
        if let Some(sig) = check_sig(cx, r, round, sig, "ring seed") {
            if seed_of48(round, &sig) != Some(r.p32("seed")) {
                cx.fail(
                    V,
                    RING_SEED_RULE,
                    format!("ring {d}"),
                    r.bell,
                    Some(r.tx),
                    "the ring seed is not seed_of(round, sig)",
                );
            }
        }
    }
    // Anchors.
    let mut max_delay = 0.0f64;
    for ((bell, region), list) in f.anchors.iter() {
        let (bell, region) = (*bell, *region);
        let what = format!("anchor ({bell}, {region})");
        if list.len() > 1 {
            let r = &w.recs[list[1].rec];
            cx.fail(
                V,
                DUPLICATE_ANCHOR,
                what.clone(),
                bell,
                Some(r.tx),
                format!("{} ANCHOR records for one (bell, region)", list.len()),
            );
        }
        for a in list {
            let r = &w.recs[a.rec];
            let t = &w.txs[r.tx];
            let want = f.tlock_round(bell);
            if a.round != want {
                cx.fail(
                    V,
                    SEED_ROUND_RULE,
                    what.clone(),
                    bell,
                    Some(r.tx),
                    format!("anchor round {} but T(b) = {want}", a.round),
                );
            }
            if a.a != t.time || a.slot != t.slot {
                cx.fail(
                    V,
                    SEED_ROUND_RULE,
                    what.clone(),
                    bell,
                    Some(r.tx),
                    format!(
                        "anchor A {} slot {} but it landed at {} in slot {}",
                        a.a, a.slot, t.time, t.slot
                    ),
                );
            }
            if region >= 16 || !t.writable(&f.ctx.bell_anchor(bell, region)) {
                cx.fail(
                    V,
                    NON_CANONICAL_ADDRESS,
                    what.clone(),
                    bell,
                    Some(r.tx),
                    "the anchor was not written at an‖(bell, region)",
                );
            }
            let sig = tx_sig(cx, r, a.round, &[Ix::PostAnchor, Ix::PostAnchorMulti]);
            check_sig(cx, r, a.round, sig, &what);
            let delay = (a.a - f.round_time(a.round)) as f64;
            if delay > max_delay {
                max_delay = delay;
            }
        }
    }
    cx.liveness.max_anchor_delay_s = max_delay;
    // Caches.
    for ((bell, region), list) in f.seeds.iter() {
        let (bell, region) = (*bell, *region);
        let what = format!("seed ({bell}, {region})");
        for s in list {
            let r = &w.recs[s.rec];
            let Some(an) = f.anchor(bell, region).filter(|a| a.rec < s.rec) else {
                cx.fail(
                    V,
                    SEED_ROUND_RULE,
                    what.clone(),
                    bell,
                    Some(r.tx),
                    "a seed cache without THE anchor before it",
                );
                continue;
            };
            let want = f.seed_round(bell, an.a);
            if s.a != an.a || s.round != want {
                cx.fail(
                    V,
                    SEED_ROUND_RULE,
                    what.clone(),
                    bell,
                    Some(r.tx),
                    format!(
                        "seed round {} for A {}; the rule gives {want} for THE anchor's A {}",
                        s.round, s.a, an.a
                    ),
                );
            }
            if !w.txs[r.tx].writable(&f.ctx.seed_cache(bell, region, s.nonce)) {
                cx.fail(
                    V,
                    NON_CANONICAL_ADDRESS,
                    what.clone(),
                    bell,
                    Some(r.tx),
                    "the cache was not written at sd‖(bell, region, nonce)",
                );
            }
            let sig = tx_sig(cx, r, s.round, &[Ix::PostSeed]);
            if let Some(sig) = check_sig(cx, r, s.round, sig, &what) {
                if seed_of48(s.round, &sig) != Some(s.seed) {
                    cx.fail(
                        V,
                        SEED_ROUND_RULE,
                        what.clone(),
                        bell,
                        Some(r.tx),
                        "the cached seed is not seed_of(round, sig)",
                    );
                }
            }
        }
    }
    for r in w.of(Kind::BEACON) {
        let round = r.pu64("round");
        let sig = tx_sig(cx, r, round, &[Ix::PostBeacon]);
        check_sig(cx, r, round, sig, "beacon");
    }
    // Archives.
    for r in w.of(Kind::ARCHIVE) {
        let (bell, region) = (r.pu32("bell"), r.ku8("region"));
        let what = format!("archive ({bell}, {region})");
        let Some(an) = f.anchor(bell, region) else {
            cx.fail(
                V,
                SEED_ROUND_RULE,
                what,
                bell,
                Some(r.tx),
                "an archive entry of a bell with no anchor",
            );
            continue;
        };
        let off = an.a - beacon::bell_end(f.genesis_ts, bell);
        let seed = f
            .seeds
            .get(&(bell, region))
            .and_then(|v| {
                v.iter()
                    .find(|s| s.a == an.a && s.round == f.seed_round(bell, an.a))
            })
            .map(|s| s.seed);
        if off != r.pu32("a_off") as i64 || seed != Some(r.p32("seed")) {
            cx.fail(
                V,
                SEED_ROUND_RULE,
                what.clone(),
                bell,
                Some(r.tx),
                "the archive entry's a_off or seed is not THE anchor's",
            );
        }
        let akey = f.ctx.anchor_archive(region, AA::part_of(bell));
        if let Some(d) = w.txs[r.tx].post_data(&akey) {
            let o = AA::entry(bell) + AE::SIG;
            let esig: Option<[u8; 48]> = d.get(o..o + 48).and_then(|s| s.try_into().ok());
            let anchor_sig = tx_sig_any(cx, an.round);
            if esig.is_none() || esig != anchor_sig {
                cx.fail(
                    V,
                    BEACON_SIG_INVALID,
                    what,
                    bell,
                    Some(r.tx),
                    "the archive entry's signature is not THE anchor's",
                );
            }
        }
    }
}

/// THE verified signature of `round`, from any transaction.
fn tx_sig_any(cx: &mut Ctx, round: u64) -> Option<[u8; 48]> {
    let f = cx.f;
    f.good_sig(round, &mut cx.sigs)
}
