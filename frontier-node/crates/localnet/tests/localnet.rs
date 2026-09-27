//! localnet MVP tests: the SIMD-0186 control (I-45), real 400-ms slots with
//! scaled game seconds (I-54), the BLS feature set, priority ordering, the
//! feed, and the RPC subset through `fclient`'s `RpcPort` on 127.0.0.1:0.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fclient::ports::{ChainPort, Cursor};
use fclient::rpc::RpcPort;
use fclient::{addr, fees, tx, Address, Instruction, Keypair, Signer};
use localnet::{server, Chain, Config, InProcess};

const MEMO: &str = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";

fn memo_elf(c: &Chain) -> Vec<u8> {
    c.account(&addr::address(MEMO))
        .expect("LiteSVM ships SPL Memo")
        .data
}

fn memo_ix(program: Address) -> Instruction {
    Instruction {
        program_id: program,
        accounts: vec![],
        data: b"psf".to_vec(),
    }
}

fn funded(c: &mut Chain, n: u8) -> Keypair {
    let k = Keypair::new_from_array([n; 32]);
    c.airdrop(&k.pubkey(), 10_000_000_000).unwrap();
    k
}

fn budget(loaded: u32) -> tx::TxBudget {
    tx::TxBudget {
        cu_limit: 50_000,
        cu_price: 1_000,
        loaded_limit: loaded,
        heap: None,
    }
}

/// A ≈ 0.6 MB LoaderV3 program: SPL Memo's ELF deployed with a 602,112-B
/// max_len (the size SP-V2's 480,512-B `.so` deploys at, §10.1).
fn big_memo(c: &mut Chain) -> Address {
    let elf = memo_elf(c);
    let p = Address::new_from_array([0x3A; 32]);
    c.deploy(p, &elf, fees::deploy_max_len(480_512) as usize, None)
        .unwrap();
    p
}

#[test]
fn loaded_data_control_one_page_below_fails_at_the_need_passes() {
    let mut c = Chain::new(Config::default());
    let prog = big_memo(&mut c);
    let payer = funded(&mut c, 1);
    let (bh, _) = c.latest_blockhash();
    // The need of this transaction shape, by SIMD-0186 accounting.
    let probe = tx::build(
        &[memo_ix(prog)],
        &budget(fees::RUNTIME_MAX_LOADED),
        &[&payer],
        &bh,
    )
    .unwrap();
    let need = c.loaded_size(&probe.message);
    assert!(
        need > 602_112,
        "ProgramData counts toward the limit: need {need}"
    );
    // The total, independently of `loaded_size` (integ-W1 review: the
    // control used to take `need` from the code under test only):
    // SIMD-0186 charges each loaded account its data + 64 B. ProgramData
    // = 45-B LoaderV3 metadata + the 602,112-B max_len, + 64; the program
    // account 36 + 64; the payer 0 + 64; the ComputeBudget builtin (its
    // 22-B name "compute_budget_program") + 64. No instructions sysvar in
    // this message.
    let max_len = fees::deploy_max_len(480_512) as u64;
    assert_eq!(max_len, 602_112);
    let expect = (45 + max_len + 64) + (36 + 64) + 64 + (22 + 64);
    assert_eq!(expect, 602_471);
    assert_eq!(need, expect, "loaded_size against the SIMD-0186 sum");
    let at_need = fees::round_up_page(need) as u32;
    let below = at_need - fees::PAGE;

    let fail = tx::build(&[memo_ix(prog)], &budget(below), &[&payer], &bh).unwrap();
    let before = c.balance(&payer.pubkey());
    let s_fail = c.submit(&tx::wire(&fail)).unwrap();
    c.produce_block();
    let st = c
        .status(&s_fail)
        .expect("the failure lands and is recorded");
    assert_eq!(st.err.as_deref(), Some("MaxLoadedAccountsDataSizeExceeded"));
    let fee = tx::fee_lamports(&fail.message);
    assert_eq!(
        before - c.balance(&payer.pubkey()),
        fee,
        "the fee is charged on failure (SIMD-0186)"
    );

    // The same transaction at the need passes.
    let (bh2, _) = c.latest_blockhash();
    let pass = tx::build(&[memo_ix(prog)], &budget(at_need), &[&payer], &bh2).unwrap();
    let s_pass = c.submit(&tx::wire(&pass)).unwrap();
    c.produce_block();
    let st = c.status(&s_pass).unwrap();
    assert_eq!(
        st.err,
        None,
        "{:?}",
        c.transaction(&s_pass).map(|l| l.logs.clone())
    );
    assert!(need <= at_need as u64 && need > below as u64);
    eprintln!("loaded-data control: need {need} B; limit {below} B fails (fee {fee} charged), {at_need} B passes");
}

#[test]
fn litesvm_alone_undercounts_programdata() {
    // Why the check lives in localnet: raw LiteSVM 0.16 accepts the
    // below-need transaction because it does not count the ProgramData.
    let mut c = Chain::new(Config::default());
    let prog = big_memo(&mut c);
    let payer = funded(&mut c, 2);
    let (bh, _) = c.latest_blockhash();
    let probe = tx::build(
        &[memo_ix(prog)],
        &budget(fees::RUNTIME_MAX_LOADED),
        &[&payer],
        &bh,
    )
    .unwrap();
    let below = fees::round_up_page(c.loaded_size(&probe.message)) as u32 - fees::PAGE;
    let t = tx::build(
        &[memo_ix(prog)],
        &budget(below),
        &[&payer],
        &c.svm.latest_blockhash(),
    )
    .unwrap();
    assert!(
        c.svm.send_transaction(t).is_ok(),
        "raw LiteSVM does not enforce the ProgramData share"
    );
}

#[test]
fn bls_syscalls_and_sbpf_v2_are_in_the_feature_set() {
    assert_eq!(localnet::bls_and_sbpf_v2_active(), (true, true));
}

#[test]
fn blocks_order_by_priority_and_feed_keeps_failures() {
    let ip = InProcess::new(Config::default(), None);
    let (lo, hi) = {
        let mut c = ip.lock();
        (funded(&mut c, 3), funded(&mut c, 4))
    };
    let (bh, _) = ip.lock().latest_blockhash();
    let dest = Address::new_from_array([9; 32]);
    let cheap = tx::build(
        &[tx::transfer(lo.pubkey(), dest, 1_000_000)],
        &tx::TxBudget {
            cu_price: 0,
            ..budget(65_536)
        },
        &[&lo],
        &bh,
    )
    .unwrap();
    let dear = tx::build(
        &[tx::transfer(hi.pubkey(), dest, 1_000_000)],
        &tx::TxBudget {
            cu_price: 5_000_000,
            ..budget(65_536)
        },
        &[&hi],
        &bh,
    )
    .unwrap();
    // A transfer of more than the payer has: lands as a failure.
    let broke = tx::build(
        &[tx::transfer(lo.pubkey(), dest, u64::MAX / 2)],
        &budget(65_536),
        &[&lo],
        &bh,
    )
    .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    rt.block_on(async {
        ip.send(&tx::wire(&cheap)).await.unwrap();
        ip.send(&tx::wire(&dear)).await.unwrap();
        ip.send(&tx::wire(&broke)).await.unwrap();
        assert!(
            ip.send(&tx::wire(&dear)).await.is_err(),
            "duplicate refused"
        );
        ip.step(1);
        let feed = ip.feed(Cursor(0)).await.unwrap();
        let sigs: Vec<_> = feed.iter().map(|r| r.signature).collect();
        let i_dear = sigs
            .iter()
            .position(|s| *s == tx::signature(&dear))
            .unwrap();
        let i_cheap = sigs
            .iter()
            .position(|s| *s == tx::signature(&cheap))
            .unwrap();
        assert!(i_dear < i_cheap, "higher priority first");
        let f = feed
            .iter()
            .find(|r| r.signature == tx::signature(&broke))
            .unwrap();
        assert!(f.err.is_some(), "failed transactions are in the feed");
        assert!(f.post.iter().any(|(k, _)| *k == lo.pubkey()));
        let st = ip
            .statuses(&[tx::signature(&cheap), tx::signature(&broke)])
            .await
            .unwrap();
        assert!(st[0].as_ref().unwrap().err.is_none() && st[1].as_ref().unwrap().err.is_some());
        // The cursor moves past what was read.
        assert!(ip
            .feed(Cursor(feed.last().unwrap().seq))
            .await
            .unwrap()
            .is_empty());
    });
}

/// Distinct funded payers `base..base+n`.
fn payers(c: &mut Chain, base: u8, n: u8) -> Vec<Keypair> {
    (0..n).map(|i| funded(c, base + i)).collect()
}

fn heavy(payer: &Keypair, dest: Address, bh: &fclient::Hash) -> fclient::Transaction {
    tx::build(
        &[tx::transfer(payer.pubkey(), dest, 1_000_000)],
        &tx::TxBudget {
            cu_limit: 1_400_000,
            cu_price: 0,
            loaded_limit: 65_536,
            heap: None,
        },
        &[payer],
        bh,
    )
    .unwrap()
}

#[test]
fn block_caps_40m_per_writable_account_and_100m_per_block() {
    // 30 transactions requesting 1.4M CU each, all writing one account:
    // ⌊40M / 1.4M⌋ = 28 fit in a block, 2 wait for the next.
    let mut c = Chain::new(Config::default());
    let ps = payers(&mut c, 10, 30);
    let (bh, _) = c.latest_blockhash();
    let hot = Address::new_from_array([0xAB; 32]);
    for p in &ps {
        c.submit(&tx::wire(&heavy(p, hot, &bh))).unwrap();
    }
    let r = c.produce_block();
    assert_eq!((r.landed, r.deferred), (28, 2), "{r:?}");
    // The §10.1 cost: 1.4M + 720 (1 signature) + 2 × 300 (write locks) +
    // 8 × 2 (64 KiB loaded).
    assert_eq!(r.cu, 28 * 1_401_336);
    let r = c.produce_block();
    assert_eq!((r.landed, r.deferred), (2, 0), "{r:?}");

    // 75 transactions of 1.4M on distinct accounts: ⌊100M / 1.4M⌋ = 71 fit.
    let ps = payers(&mut c, 100, 75);
    let (bh, _) = c.latest_blockhash();
    for (i, p) in ps.iter().enumerate() {
        let dest = Address::new_from_array([i as u8 + 1; 32]);
        c.submit(&tx::wire(&heavy(p, dest, &bh))).unwrap();
    }
    let r = c.produce_block();
    assert_eq!((r.landed, r.deferred), (71, 4), "{r:?}");
    assert!(r.cu <= localnet::chain::BLOCK_CU);
    let r = c.produce_block();
    assert_eq!((r.landed, r.deferred), (4, 0), "{r:?}");
}

/// The caps count Agave's cost (§10.1: CU limit + 720 per signature + 300
/// per write lock + 8 per 32 KiB loaded), not the CU limit alone
/// (integ-W2 review of W2-C): 40 transfers of 1,000,000 CU into one
/// account fit the 40M account cap by CU, but cost 1,001,328 each, so 39
/// fit and one waits.
#[test]
fn block_caps_count_the_cost_not_the_cu_limit() {
    let mut c = Chain::new(Config::default());
    let ps = payers(&mut c, 40, 40);
    let (bh, _) = c.latest_blockhash();
    let hot = Address::new_from_array([0xAC; 32]);
    let b = tx::TxBudget {
        cu_limit: 1_000_000,
        cu_price: 0,
        loaded_limit: 32_768,
        heap: None,
    };
    for p in &ps {
        let t = tx::build(&[tx::transfer(p.pubkey(), hot, 1_000_000)], &b, &[p], &bh).unwrap();
        assert_eq!(tx::priority(&t.message).1, 1_001_328);
        c.submit(&tx::wire(&t)).unwrap();
    }
    let r = c.produce_block();
    assert_eq!((r.landed, r.deferred), (39, 1), "{r:?}");
    assert_eq!(r.cu, 39 * 1_001_328);
    let r = c.produce_block();
    assert_eq!((r.landed, r.deferred), (1, 0), "{r:?}");
}

#[test]
fn a_deferred_transaction_expires_with_its_blockhash() {
    let mut c = Chain::new(Config::default());
    let p = funded(&mut c, 7);
    let (bh, _) = c.latest_blockhash();
    let hot = Address::new_from_array([0xCD; 32]);
    // Hold the account for longer than a blockhash lives: the transfer
    // never lands and is dropped once its blockhash is 150 slots old.
    c.hold(vec![hot], u64::MAX, 400).unwrap();
    let t = heavy(&p, hot, &bh);
    let s = c.submit(&tx::wire(&t)).unwrap();
    for _ in 0..localnet::chain::BLOCKHASH_SLOTS {
        let r = c.produce_block();
        assert_eq!(r.deferred, 1);
    }
    let r = c.produce_block();
    assert_eq!((r.dropped, r.deferred), (1, 0));
    assert!(c.status(&s).is_none() && c.is_dropped(&s));
    assert_eq!(c.pending(), 0);
    // And a new submission with that blockhash is refused.
    let t2 = tx::build(
        &[tx::transfer(p.pubkey(), hot, 5)],
        &budget(65_536),
        &[&p],
        &bh,
    )
    .unwrap();
    assert!(c.submit(&tx::wire(&t2)).is_err());
}

#[test]
fn virtual_time_scales_the_clock_per_slot() {
    let ip = InProcess::new(
        Config {
            scale: 20.0,
            ..Config::default()
        },
        None,
    );
    let c0 = ip.lock().clock();
    ip.step(10);
    let c1 = ip.lock().clock();
    assert_eq!(c1.slot - c0.slot, 10);
    assert_eq!(
        c1.unix_timestamp - c0.unix_timestamp,
        80,
        "0.4 × 20 = 8 game seconds per slot"
    );
    ip.lock().set_scale(2_000.0);
    ip.step(1);
    assert_eq!(
        ip.lock().clock().unix_timestamp - c1.unix_timestamp,
        800,
        "pre-season scale at the next slot"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rpc_subset_over_http_with_real_400ms_slots() {
    let mut chain = Chain::new(Config {
        scale: 20.0,
        ..Config::default()
    });
    let payer = funded(&mut chain, 5);
    let state = Arc::new(Mutex::new(chain));
    let node = server::start(state.clone(), 0).await.unwrap();
    assert!(node.addr.ip().is_loopback());
    let port = RpcPort::localnet(node.url(), addr::system_program());

    // Real slots: ≈ 2.5 per second; 8 game seconds per slot at 20×.
    let t0 = Instant::now();
    let c0 = port.clock().await.unwrap();
    tokio::time::sleep(Duration::from_millis(2_050)).await;
    let c1 = port.clock().await.unwrap();
    let wall = t0.elapsed().as_secs_f64();
    let slots = c1.slot - c0.slot;
    assert!(
        (slots as f64 - wall / 0.4).abs() <= 1.5,
        "{slots} slots in {wall:.2} s"
    );
    assert_eq!(c1.unix_timestamp - c0.unix_timestamp, 8 * slots as i64);
    let mut gc = fclient::clock::GameClock::new(1.0);
    gc.observe(c0, t0);
    gc.observe(c1, Instant::now());
    assert!(
        (gc.scale() - 20.0).abs() < 4.0,
        "GameClock detects ≈ 20×: {}",
        gc.scale()
    );

    // send → status → getTransaction → feed; simulate with post-state.
    let dest = Address::new_from_array([0x44; 32]);
    let (bh, _) = port.blockhash().await.unwrap();
    let t = tx::build(
        &[tx::transfer(payer.pubkey(), dest, 2_000_000)],
        &budget(65_536),
        &[&payer],
        &bh,
    )
    .unwrap();
    let sim = port.simulate(&tx::wire(&t)).await.unwrap();
    assert_eq!(sim.err, None);
    assert_eq!(sim.post_balance(&dest), Some(2_000_000));
    let sig = port.send(&tx::wire(&t)).await.unwrap();
    let mut landed = None;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if let Some(Some(s)) = port.statuses(&[sig]).await.unwrap().into_iter().next() {
            landed = Some(s);
            break;
        }
    }
    let st = landed.expect("landed within 4 s");
    assert_eq!(st.err, None);
    let accts = port
        .accounts(&[dest, Address::new_from_array([0x45; 32])], st.slot)
        .await
        .unwrap();
    assert_eq!(accts[0].as_ref().unwrap().lamports, 2_000_000);
    assert!(accts[1].is_none(), "absent account");
    let feed = port.feed(Cursor(0)).await.unwrap();
    assert!(feed.iter().any(|r| r.signature == sig && r.slot == st.slot));
    let got = port
        .rpc
        .call(
            "getTransaction",
            serde_json::json!([sig.to_string(), {"encoding": "base64"}]),
        )
        .await
        .unwrap();
    assert_eq!(got["slot"].as_u64(), Some(st.slot));
    assert_eq!(port.rpc.get_balance(&dest).await.unwrap(), 2_000_000);
    // A stale blockhash is refused; a preflight failure is reported.
    let bad = tx::build(
        &[tx::transfer(payer.pubkey(), dest, 1)],
        &budget(65_536),
        &[&payer],
        &fclient::Hash::new_from_array([1; 32]),
    )
    .unwrap();
    assert!(port.send(&tx::wire(&bad)).await.is_err());
    // getVersion, getHealth, getMinimumBalanceForRentExemption (5,080/B incl. 128).
    assert_eq!(
        port.rpc
            .call("getHealth", serde_json::json!([]))
            .await
            .unwrap(),
        "ok"
    );
    let rent = port
        .rpc
        .call(
            "getMinimumBalanceForRentExemption",
            serde_json::json!([160]),
        )
        .await
        .unwrap();
    assert_eq!(rent.as_u64(), Some(fclient::abi::rent(160)));
    // Pause freezes the clock; setScale applies at the next slot.
    port.rpc
        .call("frontier_pause", serde_json::json!([]))
        .await
        .unwrap();
    let p0 = port.clock().await.unwrap();
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(port.clock().await.unwrap(), p0);
    port.rpc
        .call("frontier_setScale", serde_json::json!([2.0]))
        .await
        .unwrap();
    port.rpc
        .call("frontier_resume", serde_json::json!([]))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1_300)).await;
    let p1 = port.clock().await.unwrap();
    assert!(p1.slot > p0.slot);
    assert!(
        (p1.unix_timestamp - p0.unix_timestamp) as f64 <= 0.8 * (p1.slot - p0.slot) as f64 + 1.0,
        "scale 2: 0.8 s per slot"
    );
    // Tamper hooks stay closed without --allow-tamper.
    assert!(port
        .rpc
        .call(
            "frontier_setAccount",
            serde_json::json!([dest.to_string(), null])
        )
        .await
        .is_err());
    node.stop();
}

/// SP-V2's BLS program on this node: `Verify` (tag 9) of a fixture quicknet
/// round with `fclient`'s hints returns `fclient::beacon::seed_of`. Needs the
/// SP-V2 build: `PSF_SPV2_SO=…/SP-V2/program/out/plain-v2/spv2.so cargo test -p localnet -- --ignored`.
#[test]
#[ignore = "needs PSF_SPV2_SO (the SP-V2 lab build, not in the repo)"]
fn spv2_bls_program_verifies_a_fixture_round() {
    let so = std::fs::read(std::env::var("PSF_SPV2_SO").expect("PSF_SPV2_SO")).unwrap();
    let mut c = Chain::new(Config::default());
    let prog = Address::new_from_array([0x5B; 32]);
    c.deploy(
        prog,
        &so,
        fees::deploy_max_len(so.len() as u64) as usize,
        None,
    )
    .unwrap();
    let payer = funded(&mut c, 6);
    let f = fclient::beacon::FixtureDrand::load(
        &fclient::beacon::fixture_dir(),
        fclient::beacon::quicknet_info(),
    )
    .unwrap();
    let b = *f.rounds.values().next().unwrap();
    let mut data = vec![9u8];
    data.extend_from_slice(&b.round.to_le_bytes());
    data.push(0);
    data.extend_from_slice(&b.sig48);
    data.extend(fclient::beacon::hints_bytes(b.round));
    let (bh, _) = c.latest_blockhash();
    let probe = tx::build(
        &[Instruction {
            program_id: prog,
            accounts: vec![],
            data: data.clone(),
        }],
        &budget(fees::RUNTIME_MAX_LOADED),
        &[&payer],
        &bh,
    )
    .unwrap();
    let limit = fees::round_up_page(c.loaded_size(&probe.message)) as u32;
    let t = tx::build(
        &[Instruction {
            program_id: prog,
            accounts: vec![],
            data,
        }],
        &tx::TxBudget {
            cu_limit: 400_000,
            cu_price: 0,
            loaded_limit: limit,
            heap: None,
        },
        &[&payer],
        &bh,
    )
    .unwrap();
    let s = c.submit(&tx::wire(&t)).unwrap();
    c.produce_block();
    let l = c.transaction(&s).unwrap().clone();
    assert_eq!(l.err, None, "{:?}", l.logs);
    let sig96 = fclient::beacon::decompress_sig(&b.sig48).unwrap();
    assert_eq!(
        l.return_data,
        fclient::beacon::seed_of(b.round, &sig96).to_vec()
    );
    eprintln!(
        "SP-V2 Verify on localnet: {} CU, loaded limit {limit} B, .so {} B",
        l.units,
        so.len()
    );
}
