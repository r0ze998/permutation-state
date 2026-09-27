//! The contention emulator (`frontier_hold`, §8.7): a transaction that
//! writes a held key lands only when its priority exceeds the hold's; ties
//! lose; readers are unaffected; the filler takes the account's 40M and the
//! same CU of the block; holds expire, can be released and report their
//! notional price; the RPC form works.

use std::sync::{Arc, Mutex};

use fclient::ports::ChainPort;
use fclient::rpc::RpcPort;
use fclient::{addr, fees, tx, AccountMeta, Address, Keypair, Signer};
use localnet::chain::{ACCOUNT_CU, BLOCK_CU};
use localnet::{server, Chain, Config};

fn funded(c: &mut Chain, n: u8) -> Keypair {
    let k = Keypair::new_from_array([n; 32]);
    c.airdrop(&k.pubkey(), 10_000_000_000).unwrap();
    k
}

fn bid(payer: &Keypair, dest: Address, price: u64, bh: &fclient::Hash) -> fclient::Transaction {
    tx::build(
        &[tx::transfer(payer.pubkey(), dest, 1_000_000)],
        &tx::TxBudget {
            cu_limit: 50_000,
            cu_price: price,
            loaded_limit: 65_536,
            heap: None,
        },
        &[payer],
        bh,
    )
    .unwrap()
}

fn prio(t: &fclient::Transaction) -> u64 {
    tx::priority(&t.message).0
}

#[test]
fn a_held_key_admits_only_higher_priorities_and_ties_lose() {
    let mut c = Chain::new(Config::default());
    let (lo, hi, tie, reader) = (
        funded(&mut c, 1),
        funded(&mut c, 2),
        funded(&mut c, 3),
        funded(&mut c, 4),
    );
    let held = Address::new_from_array([0x77; 32]);
    let (bh, _) = c.latest_blockhash();
    let t_lo = bid(&lo, held, 0, &bh);
    let t_hi = bid(&hi, held, 5_000_000, &bh);
    let p_hold = 1_000;
    assert!(prio(&t_lo) < p_hold && prio(&t_hi) > p_hold);
    // A tie: a price that lands exactly on the hold's priority.
    let cost = tx::priority(&bid(&tie, held, 0, &bh).message).1;
    let fee = (0..1_000_000u64)
        .find(|f| fees::priority_milli(*f, cost) == p_hold)
        .unwrap();
    let t_tie = bid(&tie, held, fees::cu_price_micro(fee, 50_000), &bh);
    assert_eq!(prio(&t_tie), p_hold, "a bid exactly at the hold's priority");
    // A reader of the held key (read-only extra account on a transfer).
    let mut ix = tx::transfer(
        reader.pubkey(),
        Address::new_from_array([0x78; 32]),
        1_000_000,
    );
    ix.accounts.push(AccountMeta::new_readonly(held, false));
    let t_read = tx::build(
        &[ix],
        &tx::TxBudget {
            cu_limit: 50_000,
            cu_price: 0,
            loaded_limit: 65_536,
            heap: None,
        },
        &[&reader],
        &bh,
    )
    .unwrap();

    let h = c.hold(vec![held], p_hold, 3).unwrap();
    assert_eq!((h.from_slot, h.until_slot), (c.slot() + 1, c.slot() + 3));
    let s_lo = c.submit(&tx::wire(&t_lo)).unwrap();
    let s_hi = c.submit(&tx::wire(&t_hi)).unwrap();
    let s_tie = c.submit(&tx::wire(&t_tie)).unwrap();
    let s_read = c.submit(&tx::wire(&t_read)).unwrap();

    let r = c.produce_block();
    assert_eq!((r.landed, r.deferred), (2, 2), "{r:?}");
    assert!(c.status(&s_hi).is_some(), "above the hold: lands");
    assert!(c.status(&s_read).is_some(), "readers are not held");
    assert!(c.status(&s_lo).is_none() && c.status(&s_tie).is_none());
    // The filler took the rest of the account's 40M (minus the winner's
    // 50k) and that much of the block.
    assert_eq!(r.hold_cu, ACCOUNT_CU - 50_000);
    for _ in 0..2 {
        let r = c.produce_block();
        assert_eq!((r.landed, r.deferred), (0, 2), "still held: {r:?}");
    }
    // The hold ends after its third slot: both land at the next.
    let r = c.produce_block();
    assert_eq!((r.landed, r.deferred, r.hold_cu), (2, 0, 0), "{r:?}");
    assert!(c.status(&s_lo).is_some() && c.status(&s_tie).is_some());
    // The hold is gone; its totals were reported while it ran.
    assert!(c.holds().is_empty());
}

#[test]
fn the_filler_takes_block_room_and_multi_key_holds_fill_every_key() {
    let mut c = Chain::new(Config::default());
    let keys: Vec<Address> = (0..20)
        .map(|i| Address::new_from_array([0x90 + i; 32]))
        .collect();
    let h = c.hold(keys.clone(), 2_000, 2).unwrap();
    assert_eq!(h.keys.len(), 20);
    // 50 unrelated 1.4M transactions: the filler's 40M leaves 60M → 42 fit.
    let ps: Vec<Keypair> = (0..50).map(|i| funded(&mut c, 100 + i)).collect();
    let (bh, _) = c.latest_blockhash();
    for (i, p) in ps.iter().enumerate() {
        let t = tx::build(
            &[tx::transfer(
                p.pubkey(),
                Address::new_from_array([i as u8 + 1; 32]),
                1_000_000,
            )],
            &tx::TxBudget {
                cu_limit: 1_400_000,
                cu_price: 0,
                loaded_limit: 65_536,
                heap: None,
            },
            &[p],
            &bh,
        )
        .unwrap();
        c.submit(&tx::wire(&t)).unwrap();
    }
    let r = c.produce_block();
    assert_eq!(r.hold_cu, ACCOUNT_CU);
    assert_eq!((r.landed, r.deferred), (42, 8), "{r:?}");
    assert!(r.cu <= BLOCK_CU);
    // Every held key is full: a bid below the hold on any of them waits.
    let p = funded(&mut c, 200);
    let (bh, _) = c.latest_blockhash();
    let t = bid(&p, keys[13], 0, &bh);
    let s = c.submit(&tx::wire(&t)).unwrap();
    let r = c.produce_block();
    assert!(c.status(&s).is_none(), "{r:?}");
    let totals = c.holds()[0].clone();
    assert_eq!(totals.filled_cu, 2 * ACCOUNT_CU);
    assert_eq!(totals.notional_lamports, 2 * ACCOUNT_CU * 2_000 / 1_000);
    c.produce_block();
    assert!(c.status(&s).is_some(), "lands once the hold ends");
    // Limits: ≤ 64 keys, ≥ 1 slot.
    let many: Vec<Address> = (0..65u8)
        .map(|i| Address::new_from_array([i; 32]))
        .collect();
    assert!(c.hold(many, 1, 1).is_err());
    assert!(c.hold(vec![keys[0]], 1, 0).is_err());
    // Release ends a hold at once.
    let h = c.hold(vec![keys[0]], 5_000, 1_000).unwrap();
    assert!(c.release(h.id).is_some() && c.release(h.id).is_none());
    let r = c.produce_block();
    assert_eq!(r.hold_cu, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn frontier_hold_over_rpc_delays_a_low_bid_until_the_hold_ends() {
    let mut chain = Chain::new(Config {
        scale: 20.0,
        ..Config::default()
    });
    let payer = funded(&mut chain, 9);
    let node = server::start(Arc::new(Mutex::new(chain)), 0).await.unwrap();
    let port = RpcPort::localnet(node.url(), addr::system_program());
    let held = Address::new_from_array([0x55; 32]);
    let h = port
        .rpc
        .call(
            "frontier_hold",
            serde_json::json!([[held.to_string()], 3_000, 5]),
        )
        .await
        .unwrap();
    let until = h["untilSlot"].as_u64().unwrap();
    assert_eq!(h["priorityMilli"], 3_000);
    let (bh, _) = port.blockhash().await.unwrap();
    let t = bid(&payer, held, 0, &bh);
    let sig = port.send(&tx::wire(&t)).await.unwrap();
    let mut landed = None;
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        if let Some(Some(s)) = port.statuses(&[sig]).await.unwrap().into_iter().next() {
            landed = Some(s);
            break;
        }
    }
    let st = landed.expect("lands after the hold");
    assert!(st.slot > until, "landed at {} ≤ hold end {until}", st.slot);
    let holds = port
        .rpc
        .call("frontier_holds", serde_json::json!([]))
        .await
        .unwrap();
    assert!(holds.as_array().unwrap().is_empty(), "{holds}");
    assert!(port
        .rpc
        .call("frontier_hold", serde_json::json!([[], 1, 1]))
        .await
        .is_err());
    node.stop();
}
