//! Herald fold (contract §8.4, gate W3 "herald fold determinism"): the
//! same archive gives byte-identical files — folded in one pass, in
//! batches through findex, and across restarts from a checkpoint or a
//! crash before one; per-bell files, overviews, bell-region records and
//! clash reports as the contract states them; a tampered digest is a
//! published `MISMATCH`; failed transactions are not events.

mod common;

use std::sync::Arc;

use common::{tmp, tree, VecSource};
use fclient::decode::{ClashInputs, Province};
use herald_fold::files::Out;
use herald_fold::fixture::{self, PROVINCES};
use herald_fold::fold::{Fold, FoldCfg};
use herald_fold::runner::{Ingest, IngestCfg};
use herald_fold::{checkpoint, clash::Provisional, views};
use serde_json::Value;
use tokio::sync::broadcast;

const BELLS: u32 = 12;

fn cfg() -> FoldCfg {
    FoldCfg {
        program: fixture::program(),
        season_id: fixture::SEASON_ID,
        builder: Arc::new(Provisional),
        exact_post: true,
    }
}

fn fold_all(dir: &std::path::Path, txs: &[fclient::ports::TxRecord]) -> Fold {
    let mut f = Fold::new(cfg(), Out::new(dir));
    for t in txs {
        f.apply(t);
    }
    f
}

fn json_file(dir: &std::path::Path, rel: &str) -> Value {
    serde_json::from_slice(&std::fs::read(dir.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}")))
        .unwrap()
}

#[test]
fn fold_determinism_same_archive_same_bytes() {
    let txs = fixture::mini_season(BELLS);
    let d1 = tmp("det1");
    let d2 = tmp("det2");
    let d3 = tmp("det3");
    let f1 = fold_all(&d1, &txs);
    fold_all(&d2, &txs);
    // A fold interrupted at every third of the archive, checkpointed,
    // restored and fed an overlapping remainder (already-folded records
    // are skipped, rewrites are no-ops).
    let mut f3 = Fold::new(cfg(), Out::new(&d3));
    let cut = txs.len() / 3;
    for t in &txs[..cut] {
        f3.apply(t);
    }
    for k in [2 * cut, txs.len()] {
        let saved = checkpoint::encode(&fixture::program().to_bytes(), fixture::SEASON_ID, &f3.st);
        let st =
            checkpoint::decode(&fixture::program().to_bytes(), fixture::SEASON_ID, &saved).unwrap();
        assert_eq!(st, f3.st, "checkpoint round trip");
        let from = (f3.st.folded_through as usize).saturating_sub(5);
        f3 = Fold::with_state(cfg(), Out::new(&d3), st);
        for t in &txs[from..k] {
            f3.apply(t);
        }
    }
    let (t1, t2, t3) = (tree(&d1), tree(&d2), tree(&d3));
    assert!(t1.len() > 50, "{} files", t1.len());
    assert_eq!(t1, t2, "two folds of one archive");
    assert_eq!(t1, t3, "a fold restarted from checkpoints");
    assert_eq!(f1.st, f3.st, "same final state");
    assert_eq!(f1.st.alarms.rewrites, 0);
    assert_eq!(
        f3.st.alarms.rewrites, 0,
        "restarts rewrote nothing differently"
    );
    // Every per-bell file has its gzip sibling.
    for (p, b) in &t1 {
        if let Some(plain) = p.strip_suffix(".gz") {
            let orig = &t1.iter().find(|(q, _)| q == plain).unwrap().1;
            let mut dec = vec![];
            use std::io::Read;
            flate2::read::GzDecoder::new(&b[..])
                .read_to_end(&mut dec)
                .unwrap();
            assert_eq!(&dec, orig, "{p}");
        }
    }
    for d in [d1, d2, d3] {
        let _ = std::fs::remove_dir_all(d);
    }
}

#[tokio::test]
async fn ingest_restart_and_crash_give_the_same_files() {
    restart_and_crash(std::time::Duration::ZERO).await;
}

/// W5-C group commit: a crash loses the archive's uncommitted appends (the
/// durable cursor is older), the source re-delivers them, and the files end
/// byte-identical.
#[tokio::test]
async fn a_crash_inside_a_group_commit_re_ingests_and_gives_the_same_files() {
    restart_and_crash(std::time::Duration::from_secs(3_600)).await;
}

async fn restart_and_crash(commit: std::time::Duration) {
    let txs = fixture::mini_season(BELLS);
    let reference = tmp("ref");
    fold_all(&reference.join("files"), &txs);

    let data = tmp("ingest");
    let (diffs, _) = broadcast::channel(16_384);
    let mut c = IngestCfg::new(&data, fixture::program(), fixture::SEASON_ID);
    c.segment_bytes = 64 * 1024;
    // Never checkpoint during the run: a restart must re-fold from the
    // checkpoint written at open (a crash after files were written).
    c.checkpoint_slots = u64::MAX / 2;
    c.archive_commit = commit;
    let mut src = VecSource::new(txs.clone(), 17);
    {
        let mut ing = Ingest::open(c.clone(), diffs.clone()).unwrap();
        ing.findex.resume(&mut src);
        for _ in 0..5 {
            ing.step(&mut src).await.unwrap();
        }
        // Crash: no checkpoint on the way out.
    }
    {
        let mut ing = Ingest::open(c.clone(), diffs.clone()).unwrap();
        let mut src = VecSource::new(txs.clone(), 23);
        ing.findex.resume(&mut src);
        if commit.is_zero() {
            assert_eq!(
                src.pos,
                5 * 17,
                "the source resumes from the archived cursor"
            );
        } else {
            // Nothing was committed after open: the source starts over.
            assert_eq!(src.pos, 0, "the durable cursor is the committed one");
        }
        ing.step(&mut src).await.unwrap();
        ing.checkpoint().unwrap();
    }
    {
        let mut ing = Ingest::open(c.clone(), diffs.clone()).unwrap();
        let mut src = VecSource::new(txs.clone(), 1_000);
        ing.findex.resume(&mut src);
        while ing.step(&mut src).await.unwrap() > 0 {}
        let f = ing.fold.read().unwrap();
        assert_eq!(f.st.folded_through, txs.len() as u64);
        assert_eq!(ing.findex.archive.verify(), Ok(txs.len() as u64));
        // /h/events numbering = the fold's.
        assert_eq!(ing.findex.index.last_event().unwrap(), f.st.events);
    }
    assert_eq!(tree(&data.join("files")), tree(&reference.join("files")));
    let _ = std::fs::remove_dir_all(&data);
    let _ = std::fs::remove_dir_all(&reference);
}

/// integ-W6t review (R5: the herald's fold lag reached 20–25 s three times
/// without a kill; `Ingest::step` awaited the checkpoint every 150 slots):
/// the ingest does not wait for a checkpoint's save, a checkpoint saved in
/// the background restores like an awaited one, and the files are the
/// same.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checkpoints_save_in_the_background() {
    let txs = fixture::mini_season(BELLS);
    let reference = tmp("bgref");
    fold_all(&reference.join("files"), &txs);
    let data = tmp("bgckpt");
    let (diffs, _) = broadcast::channel(16_384);
    let mut c = IngestCfg::new(&data, fixture::program(), fixture::SEASON_ID);
    c.segment_bytes = 64 * 1024;
    c.checkpoint_slots = 1;
    c.checkpoint_pause = std::time::Duration::from_millis(1_500);
    {
        // `open` checkpoints once (awaited, at start-up): no pause yet.
        let mut c0 = c.clone();
        c0.checkpoint_pause = std::time::Duration::ZERO;
        let mut ing = Ingest::open(c0, diffs.clone()).unwrap();
        ing.cfg = c.clone();
        let mut src = VecSource::new(txs.clone(), 7);
        ing.findex.resume(&mut src);
        let t0 = std::time::Instant::now();
        let mut steps = 0;
        while ing.step(&mut src).await.unwrap() > 0 {
            steps += 1;
        }
        let took = t0.elapsed();
        assert!(steps > 3, "{steps}");
        // Awaited, every step past the first would have paused 1.5 s.
        assert!(
            took < std::time::Duration::from_millis(1_400),
            "the ingest waited for a checkpoint: {steps} steps in {took:?}"
        );
        assert!(ing.checkpoint_in_flight(), "the save is still running");
        // The last checkpoint (on the way out) waits for the one in flight.
        ing.checkpoint_async().await.unwrap();
        assert!(!ing.checkpoint_in_flight());
    }
    {
        let ing = Ingest::open(c.clone(), diffs.clone()).unwrap();
        let f = ing.fold.read().unwrap();
        assert_eq!(f.st.folded_through, txs.len() as u64);
    }
    assert_eq!(tree(&data.join("files")), tree(&reference.join("files")));
    let _ = std::fs::remove_dir_all(&data);
    let _ = std::fs::remove_dir_all(&reference);
}

#[test]
fn per_bell_files_overviews_and_bell_records() {
    let txs = fixture::mini_season(BELLS);
    let d = tmp("files");
    let f = fold_all(&d, &txs);
    let cx = fixture::ctx();
    // Province envelope of A at bell 0: the Province after the resolve, the
    // slot, the day and the inputs captured before they close.
    let env = json_file(&d, "h/province/2,0/0.json");
    assert_eq!(env["key"], "pv:2,0");
    assert_eq!(env["bell"], 0);
    assert_eq!(env["slots"].as_array().unwrap().len(), 1);
    assert_eq!(env["slots"][0]["key"], "ar:2,0,0,2,0");
    assert_eq!(env["day"]["key"], "ad:2,0,0");
    assert_eq!(env["inputs"]["key"], "ci:2,0,0");
    let pb = base64_decode(env["bytes"].as_str().unwrap());
    let pv = Province::decode(&pb).unwrap();
    assert_eq!(pv.resolved_next, 1);
    assert_eq!(env["seq"], pv.h.event_seq.to_string());
    assert_eq!(env["head"], hex::encode(pv.h.event_head));
    let ci = ClashInputs::decode(&base64_decode(env["inputs"]["bytes"].as_str().unwrap())).unwrap();
    assert!(ci.resolved());
    // B skips two bells at once: both files exist with the same bytes.
    let b0 = json_file(&d, "h/province/1,1/0.json");
    let b1 = json_file(&d, "h/province/1,1/1.json");
    assert_eq!(b0["bytes"], b1["bytes"]);
    assert!(b0["slots"].as_array().unwrap().is_empty());
    // Every clash recomputes to the logged digest.
    let mut clashes = 0;
    for (p, _) in tree(&d)
        .iter()
        .filter(|(p, _)| p.starts_with("h/clash/") && p.ends_with(".json"))
    {
        let r = json_file(&d, p);
        assert_eq!(r["heraldCheck"], "match", "{p}: {r}");
        assert_eq!(r["builder"], "provisional-w3d");
        assert!(r["seed"].is_string() && r["anchor"]["round"].is_u64());
        assert!(!r["decoded"]["fighters"].as_array().unwrap().is_empty());
        // W4-E R2: the Province the resolve read, resolved up to the bell,
        // rebuilds the same outcome with the shared builder.
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;
        let before = b64
            .decode(r["province_before_b64"].as_str().unwrap())
            .unwrap();
        let bell = r["bell"].as_u64().unwrap() as u32;
        assert_eq!(
            Province::decode(&before).unwrap().resolved_next,
            bell,
            "{p}"
        );
        let inputs = b64.decode(r["inputs_b64"].as_str().unwrap()).unwrap();
        let seed: [u8; 32] = hex::decode(r["seed"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let o = fclient::clash_model::build(&before, &inputs, bell, &seed)
            .unwrap()
            .resolve()
            .unwrap();
        assert_eq!(hex::encode(o.digest()), r["outcomeDigest"], "{p}");
        clashes += 1;
    }
    assert!(clashes >= 8, "{clashes} clash reports");
    assert!(
        !d.join("h/clash/9,9/0.json").exists(),
        "a failed transaction's CLASH is not an event"
    );
    // Overviews: ring 2 (A, B, C) and ring 3 (D), bells in order.
    for b in 0..BELLS - 1 {
        let ov = std::fs::read(d.join(format!("h/overview/2/{b}.bin"))).unwrap();
        assert_eq!(&ov[..8], herald_fold::OVERVIEW_MAGIC);
        assert_eq!(u16::from_le_bytes([ov[18], ov[19]]), 3, "three provinces");
        assert_eq!(u32::from_le_bytes(ov[20..24].try_into().unwrap()), b);
        let recs: Vec<(i16, i16)> = (0..3)
            .map(|k| {
                let o = 32 + 24 * k;
                (
                    i16::from_le_bytes([ov[o], ov[o + 1]]),
                    i16::from_le_bytes([ov[o + 2], ov[o + 3]]),
                )
            })
            .collect();
        assert_eq!(recs, vec![(0, 2), (1, 1), (2, 0)], "sorted by (P, Q)");
        // A = (2, 0): clash flag on even bells, the dormant holding, owner
        // 1 on site 0, site 2 released, sites 3..11 absent.
        let a = &ov[32 + 48..32 + 72];
        assert_eq!(a[19] & 1 != 0, b % 2 == 0, "clash flag at {b}");
        assert!(a[19] & 2 != 0, "dormant holding");
        let owners = u64::from_le_bytes([a[4], a[5], a[6], a[7], a[8], 0, 0, 0]);
        assert_eq!(owners & 7, 1);
        assert_eq!((owners >> 6) & 7, 7);
        let sites = u32::from_le_bytes([a[9], a[10], a[11], 0]);
        assert_eq!((sites & 3, (sites >> 4) & 3, (sites >> 6) & 3), (1, 3, 3));
        assert_eq!(a[12 + 1], 1, "one host of faction 1");
        assert!(d.join(format!("h/overview/3/{b}.bin")).exists());
    }
    // The bell-region record of bell 0, region 3: THE anchor, S, the
    // cache, resolved everywhere and archived → final.
    let br = json_file(&d, "h/bell/0/region/3.json");
    let a = fixture::GENESIS_TS + 600 + 2;
    assert_eq!(br["anchor"]["A"], a);
    assert_eq!(br["anchor"]["present"], false, "closed after the archive");
    let s = permutation_rules::frontier::beacon::seed_round(&fixture::CLOCK, a + 600, 60);
    assert_eq!(br["S"], s);
    assert_eq!(br["caches"][0]["round"], s);
    assert_eq!(br["archived"], true);
    assert_eq!(br["tombstoned"], true);
    assert_eq!(br["final"], true);
    assert_eq!(br["resolved"].as_array().unwrap().len(), 2);
    assert!(f.bell_region_final(0, 3));
    // A recent bell is not archived yet.
    let br = json_file(&d, &format!("h/bell/{}/region/4.json", BELLS - 1));
    assert_eq!(br["archived"], false);
    assert_eq!(br["final"], false);
    // /h/me: the wallet's Citizen, its Holding, the resident host.
    let me = views::me_json(&f, &fixture::wallet(), Value::Null);
    assert_eq!(
        me["citizen"]["address"],
        solana_address::Address::new_from_array(cx.citizen(&fixture::wallet().to_bytes()))
            .to_string()
    );
    assert_eq!(me["holdings"].as_array().unwrap().len(), 1);
    assert_eq!(me["hosts"].as_array().unwrap().len(), 1);
    assert_eq!(me["transits"].as_array().unwrap().len(), 1);
    // Latest views.
    let latest = views::province_latest(&f, 2, 0).unwrap();
    assert_eq!(latest["bell"], BELLS);
    assert!(f.overview_latest(2).is_some());
    assert_eq!(PROVINCES.len(), f.provinces.len());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_tampered_outcome_digest_is_a_published_mismatch() {
    let mut txs = fixture::mini_season(4);
    let (p, q, b) = fixture::fixture_tamper(&mut txs).unwrap();
    let d = tmp("tamper");
    let f = fold_all(&d, &txs);
    let r = json_file(&d, &format!("h/clash/{p},{q}/{b}.json"));
    assert_eq!(r["heraldCheck"], "MISMATCH");
    assert_ne!(r["outcomeDigest"], r["recomputedDigest"]);
    assert_eq!(f.st.alarms.clash_mismatch, 1);
    let _ = std::fs::remove_dir_all(&d);
}

/// Wave-3 review (W3-D major): a PS2 line printed by another program in a
/// transaction that lists ours (here a forged CLASH with a flipped digest)
/// is not folded: the files equal the clean archive's, no mismatch.
#[test]
fn a_foreign_programs_ps2_lines_are_ignored() {
    let clean = fixture::mini_season(4);
    let mut tampered = clean.clone();
    fixture::fixture_tamper(&mut tampered).unwrap();
    let other = fclient::Address::new_from_array([0xEE; 32]);
    let mut mixed = clean.clone();
    let mut n = 0;
    for (m, t) in mixed.iter_mut().zip(&tampered) {
        let forged: Vec<String> = t
            .logs
            .iter()
            .filter(|l| !m.logs.contains(l))
            .cloned()
            .collect();
        if !forged.is_empty() {
            n += forged.len();
            m.logs.extend(fclient::log::in_frame(&other, forged));
        }
    }
    assert_eq!(n, 1, "one forged line");
    let (d1, d2) = (tmp("foreign-clean"), tmp("foreign-mixed"));
    let f1 = fold_all(&d1, &clean);
    let f2 = fold_all(&d2, &mixed);
    assert_eq!(tree(&d1), tree(&d2));
    assert_eq!(f2.st.alarms.clash_mismatch, 0);
    assert_eq!(f1.st.events, f2.st.events);
    let _ = std::fs::remove_dir_all(&d1);
    let _ = std::fs::remove_dir_all(&d2);
}

/// With a public RPC's post-state ("state at a slot ≥ s") a differing
/// recomputation is not an alarm: it is published `unchecked`.
#[test]
fn an_inexact_post_state_never_raises_a_false_mismatch() {
    let mut txs = fixture::mini_season(4);
    let (p, q, b) = fixture::fixture_tamper(&mut txs).unwrap();
    let d = tmp("inexact");
    let mut c = cfg();
    c.exact_post = false;
    let mut f = Fold::new(c, Out::new(&d));
    for t in &txs {
        f.apply(t);
    }
    let r = json_file(&d, &format!("h/clash/{p},{q}/{b}.json"));
    assert_eq!(r["heraldCheck"], "unchecked");
    assert_eq!(f.st.alarms.clash_mismatch, 0);
    assert_eq!(
        json_file(&d, "h/clash/2,0/0.json")["heraldCheck"],
        if (p, q, b) == (2, 0, 0) {
            "unchecked"
        } else {
            "match"
        }
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn diffs_carry_heads_and_scopes() {
    let txs = fixture::mini_season(2);
    let d = tmp("diffs");
    let mut f = Fold::new(cfg(), Out::new(&d));
    let mut all = vec![];
    for t in &txs {
        f.apply(t);
        all.extend(f.take_diffs());
    }
    use herald_fold::fold::Scope;
    let pv = all
        .iter()
        .find(|x| x.kind == "acct" && x.key == "pv:2,0")
        .unwrap();
    assert!(
        pv.head.is_some(),
        "a chained account's diff carries its head"
    );
    assert_eq!(pv.scope, Scope::Province(2, 0));
    assert!(all.iter().any(|x| x.kind == "bell"
        && x.key == "/h/province/2,0/0"
        && x.scope == Scope::Province(2, 0)));
    assert!(all
        .iter()
        .any(|x| x.kind == "bell" && x.key == "/h/overview/2/0.bin" && x.scope == Scope::Ring(2)));
    assert!(all
        .iter()
        .any(|x| x.kind == "bell" && x.key == "/h/bell/0/region/3" && x.scope == Scope::Bells));
    assert!(all
        .iter()
        .any(|x| x.kind == "event" && x.key == "CLASH:2,0,0"));
    assert!(all.iter().any(|x| x.kind == "acct"
        && x.key.starts_with("ct:")
        && x.scope == Scope::Wallet(fixture::wallet().to_bytes())));
    assert!(
        !all.iter().any(|x| x.key == "CLASH:9,9,0"),
        "failed transaction"
    );
    let _ = std::fs::remove_dir_all(&d);
}

fn base64_decode(s: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).unwrap()
}

/// K11 (W5-C): the overview's dormant flag (2) follows the kernel's rule
/// at the bell's end — last owner action + `DORMANT_AFTER` — and not only
/// the Holding's cache bit, which the program refreshes on owner writes
/// only (when it is always clear).
#[test]
fn the_dormant_flag_follows_the_last_owner_action() {
    use frontier_abi::layout::player::holding as HL;
    use permutation_rules::frontier::holding::DORMANT_AFTER;
    let base = fixture::mini_season(4);
    let genesis = base
        .iter()
        .flat_map(|t| t.post.iter())
        .find_map(|(_, a)| {
            let a = a.as_ref()?;
            (a.data.len() > 8 && a.data[..8] == *b"PSF1SEAS")
                .then(|| fclient::decode::Season::decode(&a.data).ok())
                .flatten()
                .map(|s| s.genesis_ts)
        })
        .expect("the season");
    // The cache bit clear; the last owner action `ago` seconds before bell
    // 1's end.
    let with = |ago: i64| {
        let mut txs = base.clone();
        for t in txs.iter_mut() {
            for (_, a) in t.post.iter_mut() {
                if let Some(a) = a {
                    if a.data.len() == HL::SIZE && a.data[..8] == HL::MAGIC {
                        a.data[HL::FLAGS] &= !HL::FLAG_DORMANT_CACHE;
                        let last = genesis + 2 * 600 - ago;
                        a.data[HL::LAST_OWNER_ACTION..HL::LAST_OWNER_ACTION + 8]
                            .copy_from_slice(&last.to_le_bytes());
                    }
                }
            }
        }
        let d = tmp("dormant");
        fold_all(&d, &txs);
        let ov = std::fs::read(d.join("h/overview/2/1.bin")).unwrap();
        let _ = std::fs::remove_dir_all(&d);
        // (2, 0) is the third record, sorted by (P, Q).
        ov[32 + 48 + 19] & 2 != 0
    };
    assert!(
        with(DORMANT_AFTER),
        "idle for DORMANT_AFTER at the bell's end"
    );
    assert!(!with(DORMANT_AFTER - 1), "one second short");
}

fn dump(ix: &findex::index::Index) -> Vec<String> {
    ix.dump_rows().unwrap()
}

/// Wave-5 review of W5-C: a crash inside the archive's group commit leaves
/// the SQLite index ahead of the durable archive. The restart rolls the
/// index back to the archive (no rebuild from seq 0): every table is then
/// exactly what indexing the durable records alone gives, and after the
/// source re-delivers the tail, exactly what one uninterrupted ingest
/// gives (event numbering included).
#[tokio::test]
async fn a_crash_inside_a_group_commit_rolls_the_index_back_not_from_zero() {
    let txs = fixture::mini_season(BELLS);
    let open = |d: &std::path::Path| {
        findex::Findex::open(d, fixture::program(), Some(fixture::SEASON_ID), 64 * 1024).unwrap()
    };
    let data = tmp("truncate");
    let mut src = VecSource::new(txs.clone(), 13);
    let durable = {
        let mut fx = open(&data);
        // The herald's group commit (1 s there; never due here).
        fx.archive.commit_every = std::time::Duration::from_secs(3_600);
        for _ in 0..3 {
            fx.ingest(&mut src).await.unwrap();
        }
        fx.commit().unwrap();
        let durable = fx.archive.last_seq();
        for _ in 0..4 {
            fx.ingest(&mut src).await.unwrap();
        }
        assert!(
            fx.index.last_seq().unwrap() > durable,
            "the index ran ahead"
        );
        durable
        // Crash: the last four batches were never committed.
    };
    let mut fx = open(&data);
    assert_eq!(fx.archive.last_seq(), durable);
    assert_eq!(fx.index.rebuilds, 0, "no rebuild from seq 0");
    assert_eq!(fx.index.truncations, 1, "rolled back to the archive");
    assert_eq!(fx.index.last_seq().unwrap(), durable);
    // The same tables as indexing the durable records alone.
    let refd = tmp("truncate-ref");
    let mut r = open(&refd);
    let mut rs = VecSource::new(txs[..durable as usize].to_vec(), 1_000);
    while !r.ingest(&mut rs).await.unwrap().is_empty() {}
    r.commit().unwrap();
    assert_eq!(dump(&fx.index), dump(&r.index));
    // The source re-delivers the tail; the result is one clean ingest's.
    let mut src = VecSource::new(txs.clone(), 13);
    fx.resume(&mut src);
    while !fx.ingest(&mut src).await.unwrap().is_empty() {}
    let mut rs = VecSource::new(txs.clone(), 1_000);
    r.resume(&mut rs);
    while !r.ingest(&mut rs).await.unwrap().is_empty() {}
    assert_eq!(fx.index.last_seq().unwrap(), txs.len() as u64);
    assert_eq!(dump(&fx.index), dump(&r.index));
    assert_eq!(fx.index.rebuilds, 0);
    let _ = std::fs::remove_dir_all(&data);
    let _ = std::fs::remove_dir_all(&refd);
}
