//! The herald over the chain the keeper drives (in process, `localnet` at
//! 20×, the test-beacon key): the program's own Season, anchors, seed
//! caches and beacon logs — not the synthetic fixture — are folded into
//! `/h/season` and the bell-region records, with THE anchor's A and the
//! cache round equal to the chain's and `S(b, A)` by rule; the events the
//! index numbers are the fold's; and a second fold of the archive gives
//! byte-identical files.
//!
//! The program is the test-beacon `.so` when `PSF_FRONTIER_SO` names one,
//! else the keeper's native model (the keeper tests' harness, included by
//! path). Land and clash instructions are W3-A/W4-A's, so provinces and
//! clashes are covered by the fixture tests (`fold.rs`).

mod common;
#[path = "../../keeper/tests/common/mod.rs"]
mod kw;
#[path = "../../keeper/tests/model/mod.rs"]
mod model;

use std::sync::Arc;

use fclient::decode::{BellAnchor, SeedCache};
use findex::LocalnetFeed;
use herald_fold::files::Out;
use herald_fold::fold::{Fold, FoldCfg};
use herald_fold::runner::{Ingest, IngestCfg};
use herald_fold::views::{self, SeasonStatic};
use keeper_core::Keeper;
use localnet::InProcess;
use tokio::sync::broadcast;

const BELLS: u32 = 8;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn folds_the_programs_beacon_records() {
    let w = kw::world().await;
    println!("program: {}", w.which.label());
    let season = w.season();
    let genesis_ts = season.genesis_ts;
    let dir = common::tmp("real");
    let (diffs, _) = broadcast::channel(65_536);
    let cfg = IngestCfg::new(&dir, w.program, kw::SEASON_ID);
    let mut ing = Ingest::open(cfg.clone(), diffs).unwrap();
    let mut src = LocalnetFeed::new(InProcess::from_chain(w.ip.chain.clone(), Some(w.program)));
    let mut k = Keeper::new(
        kw::keeper_config(&w),
        w.ip.clone(),
        w.drand.clone(),
        &[0x78; 32],
        None,
    )
    .unwrap();
    kw::fund(&w, &k.payers);
    k.start().await.unwrap();
    let end = genesis_ts + BELLS as i64 * 600 + 720;
    let mut n = 0u64;
    while w.now() < end {
        k.tick().await.unwrap();
        w.step();
        n += 1;
        if n.is_multiple_of(25) {
            ing.step(&mut src).await.unwrap();
        }
    }
    while ing.step(&mut src).await.unwrap() > 0 {}
    ing.checkpoint().unwrap();
    let f = ing.fold.read().unwrap();
    // /h/season from the program's Season account.
    let sj = views::season_json(
        &f,
        &SeasonStatic {
            cluster: "localnet".into(),
            drand: w.drand.key.info(),
            quotas: serde_json::json!({}),
        },
    )
    .expect("the Season is captured");
    assert_eq!(sj["genesisTs"], genesis_ts);
    assert_eq!(sj["W"], season.reveal_window);
    assert_eq!(sj["headSeq"], f.st.events.to_string());
    assert!(f.st.events > 100, "{} events", f.st.events);
    assert_eq!(
        ing.findex.index.last_event().unwrap(),
        f.st.events,
        "index numbering = fold's"
    );
    assert_eq!(f.st.alarms.bad_records, 0);
    // Every bell × region the keeper anchored and seeded has its record,
    // with the chain's A, T(b) and cache round, and S(b, A) by rule.
    let sc = fclient::clock::SeasonClock::from_season(&season);
    let mut checked = 0;
    for b in 0..BELLS {
        for r in 0..16u8 {
            let chain = w.ip.lock();
            let Some(acct) = chain.account(&w.addrs.anchor(b, r)) else {
                continue;
            };
            let an = BellAnchor::decode(&acct.data).unwrap();
            let Some(info) = k.beacon.anchors.get(&(b, r)) else {
                continue;
            };
            let Some(c) = info.cache else { continue };
            let cache = SeedCache::decode(
                &chain
                    .account(&w.addrs.seed_cache(b, r, c.nonce))
                    .unwrap()
                    .data,
            )
            .unwrap();
            drop(chain);
            let rel = format!("h/bell/{b}/region/{r}.json");
            let v: serde_json::Value = serde_json::from_slice(
                &std::fs::read(cfg.files_dir().join(&rel)).unwrap_or_else(|e| panic!("{rel}: {e}")),
            )
            .unwrap();
            assert_eq!(v["anchor"]["A"], an.a, "{rel}");
            assert_eq!(v["anchor"]["round"], an.round);
            assert_eq!(an.round, sc.tlock_round(b));
            assert_eq!(v["S"], sc.seed_round(b, an.a), "{rel}: S(b, A)");
            let caches = v["caches"].as_array().unwrap();
            assert!(
                caches
                    .iter()
                    .any(|x| x["round"] == cache.round && x["seed"] == hex::encode(cache.seed)),
                "{rel}"
            );
            assert_eq!(
                v["resolved"].as_array().unwrap().len(),
                0,
                "no provinces on this branch's chain"
            );
            checked += 1;
        }
    }
    println!(
        "{checked} bell-region records checked against the chain; {} events folded",
        f.st.events
    );
    assert!(checked >= (BELLS as usize - 1) * 16, "{checked}");
    // A second fold of the same archive: byte-identical files.
    let d2 = common::tmp("real2");
    let mut f2 = Fold::new(
        FoldCfg {
            program: w.program,
            season_id: kw::SEASON_ID,
            builder: Arc::new(herald_fold::clash::Provisional),
            exact_post: true,
        },
        Out::new(&d2),
    );
    ing.findex
        .archive
        .for_each_after(0, |t| {
            f2.apply(&t);
            Ok(())
        })
        .unwrap();
    assert_eq!(common::tree(&cfg.files_dir()), common::tree(&d2));
    assert_eq!(f2.st, f.st);
    drop(f);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&d2);
}
