//! The archive duty (§5.8, I-44, I-49) over `localnet` in process: 48 h
//! after each anchor, ArchiveAnchors (≤ 8 bells per transaction) moves the
//! region-day's anchors into its AnchorArchive — tombstone and archived bits,
//! `{a_off, seed, sig}` — and closes each anchor **to the keeper payer that
//! created it**; the seed caches then close to theirs. Checked by walking
//! every transaction of the run: each ArchiveAnchors credits each anchor's
//! `rent_to` with exactly the anchor's rent (I-49: the pool refills).
//!
//! The instructions are W4-B's; this runs against the native model (or a
//! `.so` named by `PSF_FRONTIER_SO` that has them).

mod common;
mod model;

use std::collections::HashMap;

use fclient::abi::{size, tag, BELL_SECS};
use fclient::decode::AnchorArchive;
use fclient::ports::{ChainPort, Cursor};
use fclient::{Address, Signer};
use keeper_core::journal::Journal;
use keeper_core::Keeper;

use common::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn archive_closes_anchors_to_the_payer_that_paid() {
    let t0 = std::time::Instant::now();
    let w = world().await;
    println!("program: {}", w.which.label());
    let genesis_ts = w.season().genesis_ts;
    let mut k = Keeper::new(
        keeper_config(&w),
        w.ip.clone(),
        w.drand.clone(),
        &[0x52u8; 32],
        Some(Journal::open(std::path::Path::new(":memory:")).unwrap()),
    )
    .unwrap();
    fund(&w, &k.payers);
    k.start().await.unwrap();
    // Day 0 (bells 0–143) is archivable 48 h after its last anchor.
    let end = genesis_ts + 144 * BELL_SECS + 172_800 + 1_800;
    while w.now() < end {
        k.tick().await.unwrap();
        w.step();
    }
    println!(
        "ran {:.1} game days in {:.1} s",
        (w.now() - genesis_ts) as f64 / 86_400.0,
        t0.elapsed().as_secs_f64()
    );
    if k.archive.unsupported {
        println!("the program answers NotImplemented for ArchiveAnchors (W4-B's)");
        return;
    }

    // ---- day 0 archived, anchors and caches closed
    let sc = fclient::clock::SeasonClock::from_season(&w.season());
    for r in 0..16u8 {
        let a =
            w.ip.lock()
                .account(&w.addrs.archive(r, 0))
                .expect("archive");
        let arch = AnchorArchive::decode(&a.data).unwrap();
        assert!(k.payers.delay.addresses().contains(&arch.rent_to));
        for b in 0..144u32 {
            assert!(
                arch.tombstoned(b) && arch.is_archived(b),
                "bell {b} region {r}"
            );
            let e = &arch.entries[b as usize];
            let t = sc.tlock_round(b);
            assert_eq!(
                e.sig,
                w.drand.key.sign(t),
                "THE anchor's signature, for late settlements"
            );
            let a_ts = fclient::clock::bell_end(genesis_ts, b) + e.a_off as i64;
            let s_round = sc.seed_round(b, a_ts);
            let sig = fclient::beacon::decompress_sig(&w.drand.key.sign(s_round)).unwrap();
            assert_eq!(e.seed, fclient::beacon::seed_of(s_round, &sig));
            assert!(
                w.ip.lock().account(&w.addrs.anchor(b, r)).is_none(),
                "anchor closed"
            );
        }
    }
    println!(
        "archived bells {}, caches closed {}",
        k.archive.archived_bells, k.archive.caches_closed
    );
    assert!(k.archive.archived_bells >= 144 * 16);
    assert!(
        k.archive.caches_closed >= 144 * 16 - 16,
        "caches follow their anchors"
    );

    // ---- every ArchiveAnchors credits each anchor's rent_to its rent
    let all = localnet::InProcess::from_chain(w.ip.chain.clone(), None);
    let mut feed = vec![];
    loop {
        let got = all
            .feed(Cursor(
                feed.last().map_or(0, |r: &fclient::ports::TxRecord| r.seq),
            ))
            .await
            .unwrap();
        if got.is_empty() {
            break;
        }
        feed.extend(got);
    }
    let mut bal: HashMap<Address, u64> = HashMap::new();
    for a in k.payers.delay.addresses() {
        bal.insert(a, 2_000_000_000);
    }
    for f in &k.payers.funders.keys {
        bal.insert(f.pubkey(), 40_000_000_000);
    }
    let (mut checked, mut credited) = (0u64, 0u64);
    let mut seen_archives: std::collections::HashSet<Address> = Default::default();
    for r in &feed {
        if r.tx.is_empty() {
            continue;
        }
        let t = fclient::tx::from_wire(&r.tx).unwrap();
        let m = &t.message;
        let archive_ix = m.instructions.iter().find(|ci| {
            m.account_keys[ci.program_id_index as usize] == w.program
                && ci.data.first() == Some(&tag::ARCHIVE_ANCHORS)
        });
        if let (Some(ci), None) = (archive_ix, &r.err) {
            let n = ci.data[6] as usize;
            let mut want: HashMap<Address, u64> = HashMap::new();
            for j in 0..n {
                let ben = m.account_keys[ci.accounts[6 + 3 * j] as usize];
                *want.entry(ben).or_default() += fclient::abi::rent(size::BELL_ANCHOR);
            }
            let payer = m.account_keys[0];
            let day = u32::from_le_bytes(ci.data[2..6].try_into().unwrap());
            let archive = w.addrs.archive(ci.data[1], day);
            // The first batch of a region-day also pays the archive's rent.
            let created_archive = seen_archives.insert(archive);
            for (ben, credit) in want {
                let before = *bal.get(&ben).expect("a pool payer");
                let (fee, archive_rent) = if ben == payer {
                    (
                        r.fee,
                        if created_archive {
                            fclient::abi::rent(size::ANCHOR_ARCHIVE)
                        } else {
                            0
                        },
                    )
                } else {
                    (0, 0)
                };
                let after = r
                    .post
                    .iter()
                    .find(|(k2, _)| *k2 == ben)
                    .and_then(|(_, a)| a.as_ref())
                    .map(|a| a.lamports)
                    .expect("the beneficiary was written");
                assert_eq!(
                    after,
                    before + credit - fee - archive_rent,
                    "ArchiveAnchors credits the creating payer the anchors' rent"
                );
                checked += 1;
                credited += credit;
            }
        }
        for (a, acct) in &r.post {
            if bal.contains_key(a) {
                bal.insert(*a, acct.as_ref().map_or(0, |x| x.lamports));
            }
        }
    }
    println!(
        "{checked} refunds checked, {credited} lamports of anchor rent returned to their payers"
    );
    assert!(
        checked > 0 && credited >= 144 * 16 * fclient::abi::rent(size::BELL_ANCHOR),
        "checked {checked}, credited {credited}"
    );
}
