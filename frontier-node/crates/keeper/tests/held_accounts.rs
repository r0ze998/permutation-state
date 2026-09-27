//! Held accounts over `localnet`'s real block builder (40M CU per writable
//! account per block, priority order): an adversary fills one region's
//! anchor address above the keeper's P_delay cap, then one seed-cache
//! address. Expected (§8.2, offchain design §6.4, SP-FEE D5):
//!
//! - the combined anchor of that region's chunk waits; the per-region
//!   fallbacks of the chunk's other regions land one slot later (the
//!   peacetime lag); the held region lands once the hold ends; the bell is
//!   marked contested (not landed 2 slots after a bid ≥ p_tip);
//! - the held cache nonce is abandoned after `nonce_switch_slots`; the cache
//!   lands at the next random nonce while the hold is still up, and after
//!   the stale version's blockhash expires there is exactly one cache.

mod common;
mod model;

use fclient::abi::BELL_SECS;
use fclient::decode::{BellAnchor, SeedCache};
use fclient::ports::ChainPort;
use fclient::{tx, Address, Keypair, Signer};
use keeper_core::journal::Journal;
use keeper_core::Keeper;

use common::*;

/// 29 transfers of 0 lamports into `target` at priority 1.0 (above the
/// keeper's P_delay 0.5): 28 × 1.4M + 700k = 39.9M CU on one account, so no
/// 345k keeper write that locks it fits the block.
async fn hold(w: &World, target: Address, attackers: &[Keypair]) {
    let (bh, _) = w.ip.blockhash().await.unwrap();
    for (i, a) in attackers.iter().enumerate() {
        let limit = if i == 28 { 700_000 } else { 1_400_000 };
        let b = tx::TxBudget {
            cu_limit: limit,
            cu_price: 1_000_000,
            loaded_limit: 32_768,
            heap: None,
        };
        let t = tx::build(&[tx::transfer(a.pubkey(), target, 0)], &b, &[a], &bh).unwrap();
        w.ip.send(&tx::wire(&t)).await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn held_anchor_falls_back_and_held_cache_switches_nonce() {
    let w = world().await;
    println!("program: {}", w.which.label());
    let genesis_ts = w.season().genesis_ts;
    let attackers: Vec<Keypair> = (0..29u8)
        .map(|i| Keypair::new_from_array([0xC0 ^ i; 32]))
        .collect();
    for a in &attackers {
        w.airdrop(&a.pubkey(), 100_000_000_000);
    }
    let cfg = keeper_config(&w);
    let mut k = Keeper::new(
        cfg,
        w.ip.clone(),
        w.drand.clone(),
        &[0x31u8; 32],
        Some(Journal::open(std::path::Path::new(":memory:")).unwrap()),
    )
    .unwrap();
    fund(&w, &k.payers);
    k.start().await.unwrap();

    // Bell 2's anchor of region 5 is held from 3 slots before the bell ends
    // for 12 slots; bell 4's cache of region 9 at its first nonce for 160
    // slots (past the stale version's 150-slot blockhash).
    let (hb, hr) = (2u32, 5u8);
    let (sb, sr) = (4u32, 9u8);
    let hold_from = fclient::clock::bell_end(genesis_ts, hb) - 3 * 8;
    let mut hold_slots = 0u32;
    let mut seed_hold: Option<(Address, u8, u64)> = None;
    let end = fclient::clock::bell_end(genesis_ts, sb) + 900 + 170 * 8;
    while w.now() < end {
        k.tick().await.unwrap();
        if w.now() >= hold_from && hold_slots < 12 {
            hold(&w, w.addrs.anchor(hb, hr), &attackers).await;
            hold_slots += 1;
        }
        if seed_hold.is_none() {
            if let Some(ns) = k.beacon.seed_nonces(sb, sr) {
                // The keeper just sent its first version at nonces[0].
                seed_hold = Some((w.addrs.seed_cache(sb, sr, ns[0]), ns[0], w.slot()));
            }
        }
        if let Some((addr, _, from)) = seed_hold {
            if w.slot() < from + 160 {
                hold(&w, addr, &attackers).await;
            }
        }
        w.step();
    }

    // ---- the held anchor
    let an =
        BellAnchor::decode(&w.ip.lock().account(&w.addrs.anchor(hb, hr)).unwrap().data).unwrap();
    let others: Vec<BellAnchor> = (0..7u8)
        .filter(|&r| r != hr)
        .map(|r| {
            BellAnchor::decode(&w.ip.lock().account(&w.addrs.anchor(hb, r)).unwrap().data).unwrap()
        })
        .collect();
    let chunk2 =
        BellAnchor::decode(&w.ip.lock().account(&w.addrs.anchor(hb, 7)).unwrap().data).unwrap();
    let j = k.journal.as_ref().unwrap();
    let paid_by = |key: String, payer: Address| {
        j.attempts_of(&key)
            .unwrap()
            .iter()
            .any(|a| a.status == "landed" && a.payer == payer.to_string())
    };
    println!(
        "bell {hb}: unheld chunk lands at slot {}, fallbacks of the held chunk at {:?}, held region {hr} at {}",
        chunk2.slot,
        others.iter().map(|a| a.slot).collect::<Vec<_>>(),
        an.slot
    );
    for (i, o) in others.iter().enumerate() {
        let r = if i < hr as usize {
            i as u8
        } else {
            i as u8 + 1
        };
        assert!(
            paid_by(format!("anchor:{hb}:{r}"), o.rent_to),
            "region {r} landed through its fallback"
        );
        assert_eq!(o.slot, chunk2.slot + 1, "one slot of peacetime lag");
    }
    assert!(
        an.slot >= chunk2.slot + 8,
        "held while the adversary held it (9 of its 12 slots after the round)"
    );
    assert!(
        k.beacon.contested_bells.contains(&hb),
        "bell {hb} contested"
    );
    assert!(k.alerts.iter().any(|a| a.1 == "contested"));

    // ---- the held cache
    let (held_addr, held_nonce, _) = seed_hold.expect("the seed write was seen");
    let info = k.beacon.anchors.get(&(sb, sr)).unwrap();
    let c = info.cache.expect("cache landed");
    assert_ne!(c.nonce, held_nonce, "switched away from the held nonce");
    let an4 =
        BellAnchor::decode(&w.ip.lock().account(&w.addrs.anchor(sb, sr)).unwrap().data).unwrap();
    let cache = SeedCache::decode(
        &w.ip
            .lock()
            .account(&w.addrs.seed_cache(sb, sr, c.nonce))
            .unwrap()
            .data,
    )
    .unwrap();
    let sc = fclient::clock::SeasonClock::from_season(&w.season());
    assert_eq!(cache.round, sc.seed_round(sb, an4.a));
    assert!(
        w.ip.lock()
            .account(&held_addr)
            .is_none_or(|a| a.data.is_empty()),
        "the stale version expired unlanded"
    );
    let s_avail = sc.drand.round_time(cache.round);
    println!(
        "bell {sb} region {sr}: cache at nonce {} (held {held_nonce}) landed at slot {} (S public at {} s after genesis)",
        c.nonce,
        cache.slot,
        s_avail - genesis_ts
    );
    assert!(w.now() > fclient::clock::bell_end(genesis_ts, sb) + BELL_SECS);
}

/// A hold longer than the version cap plus blockhash expiry (240 slots of
/// 64 versions + 151): the capped write ends, is re-planned with a fresh
/// escalation and the anchor lands once the hold ends; newer bells keep
/// being anchored meanwhile (integ-W2 review of W2-F: before the fix the
/// write stayed pending for good and the anchor never landed).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn anchor_held_past_the_version_cap_lands_after_the_hold() {
    let w = world().await;
    println!("program: {}", w.which.label());
    let genesis_ts = w.season().genesis_ts;
    let attackers: Vec<Keypair> = (0..29u8)
        .map(|i| Keypair::new_from_array([0xA0 ^ i; 32]))
        .collect();
    for a in &attackers {
        w.airdrop(&a.pubkey(), 1_000_000_000_000);
    }
    let cfg = keeper_config(&w);
    let mut k = Keeper::new(
        cfg,
        w.ip.clone(),
        w.drand.clone(),
        &[0x32u8; 32],
        Some(Journal::open(std::path::Path::new(":memory:")).unwrap()),
    )
    .unwrap();
    fund(&w, &k.payers);
    k.start().await.unwrap();
    let (hb, hr) = (2u32, 5u8);
    let hold_from = fclient::clock::bell_end(genesis_ts, hb) - 3 * 8;
    let hold_len = 240u32;
    let mut held = 0u32;
    let mut hold_end_slot = None;
    let mut landed_at = None;
    // Run until the anchor lands, at most 600 slots after the hold ends.
    loop {
        k.tick().await.unwrap();
        if w.now() >= hold_from && held < hold_len {
            hold(&w, w.addrs.anchor(hb, hr), &attackers).await;
            held += 1;
            if held == hold_len {
                hold_end_slot = Some(w.slot());
            }
        }
        w.step();
        if let Some(e) = hold_end_slot {
            if landed_at.is_none() {
                // (The lock is released before `w.slot()` locks again.)
                let present =
                    w.ip.lock()
                        .account(&w.addrs.anchor(hb, hr))
                        .is_some_and(|a| !a.data.is_empty());
                if present {
                    landed_at = Some(w.slot());
                }
            }
            if landed_at.is_some() || w.slot() > e + 600 {
                break;
            }
        }
    }
    let e = hold_end_slot.unwrap();
    let at = landed_at.expect("the held anchor lands after the hold ends");
    let an =
        BellAnchor::decode(&w.ip.lock().account(&w.addrs.anchor(hb, hr)).unwrap().data).unwrap();
    println!(
        "held anchor ({hb}, {hr}): hold ended at slot {e}, landed at {} (seen {at})",
        an.slot
    );
    assert!(an.slot >= e.saturating_sub(1), "not while held");
    let expired = k.journal.as_ref().unwrap().alerts("write-expired").unwrap();
    println!("write-expired alerts: {expired:?}");
    assert!(
        expired.iter().any(|a| a.1.contains(&format!(":{hb}:"))),
        "the capped write was ended and alerted"
    );
    // Newer bells were anchored in every region while it was held.
    let sc = fclient::clock::SeasonClock::from_season(&w.season());
    let b_now = sc.bell_at(w.now()).unwrap();
    for b in hb + 1..b_now.saturating_sub(1) {
        for r in 0..16u8 {
            assert!(
                w.ip.lock()
                    .account(&w.addrs.anchor(b, r))
                    .is_some_and(|a| !a.data.is_empty()),
                "bell {b} region {r} anchored"
            );
        }
    }
}
