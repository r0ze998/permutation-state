//! drand-replay over HTTP on 127.0.0.1:0: the SP-V2 fixture archive, the
//! `--test-key` mode through `fclient::beacon::HttpDrand`, and the gate on a
//! live localnet game clock.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use drand_replay::{spawn_chain_clock, ClockSource, Replay, Source};
use fclient::beacon::{self, HttpDrand, TestKey};
use fclient::ports::DrandPort;
use fclient::rpc::RpcPort;

#[tokio::test]
async fn archive_mode_serves_verified_fixture_rounds_by_the_game_clock() {
    let src = Source::archive(&beacon::fixture_dir(), beacon::quicknet_info()).unwrap();
    let rounds: Vec<u64> = match &src {
        Source::Archive { rounds, .. } => rounds.keys().copied().collect(),
        _ => unreachable!(),
    };
    let r = rounds[10];
    let info = beacon::quicknet_info();
    let now = info.round_time(r) + 2;
    let node = drand_replay::start(
        Arc::new(Replay {
            source: src,
            clock: ClockSource::Fixed(now),
            delay_ms: 1_000,
        }),
        0,
    )
    .await
    .unwrap();
    let chain = beacon::QUICKNET_CHAIN_HASH;
    let got = fclient::http::get(&format!("{}/{chain}/info", node.url()))
        .await
        .unwrap();
    assert_eq!(got.status, 200);
    let i = beacon::parse_info_json(&serde_json::from_slice(&got.body).unwrap()).unwrap();
    assert_eq!(i, info);
    let d = HttpDrand::new(vec![node.url()], info.clone());
    let b = d.round(r).await.unwrap().expect("published and recorded");
    assert!(beacon::verify(r, &b.sig48, &info.public_key));
    assert_eq!(d.round(r + 1).await.unwrap(), None, "425 before its time");
    let early = fclient::http::get(&format!("{}/{chain}/public/{}", node.url(), rounds[31]))
        .await
        .unwrap();
    assert_eq!(early.status, 425);
    let latest = fclient::http::get(&format!("{}/public/latest", node.url()))
        .await
        .unwrap();
    let lb = beacon::parse_beacon_json(&serde_json::from_slice(&latest.body).unwrap()).unwrap();
    assert_eq!(lb.round, r);
    let wrong = fclient::http::get(&format!("{}/{}/public/{r}", node.url(), "00".repeat(32)))
        .await
        .unwrap();
    assert_eq!(wrong.status, 404);
    node.stop();
}

#[tokio::test]
async fn test_key_mode_signs_on_demand() {
    let k = TestKey::new();
    let info = k.info();
    let now = info.round_time(50_000_000) + 5;
    let node = drand_replay::start(
        Arc::new(Replay {
            source: Source::TestKey(TestKey::new()),
            clock: ClockSource::Fixed(now),
            delay_ms: 800,
        }),
        0,
    )
    .await
    .unwrap();
    let d = HttpDrand::new(vec![node.url()], info.clone());
    let b = d.round(50_000_000).await.unwrap().unwrap();
    assert!(
        beacon::verify(50_000_000, &b.sig48, &k.pk96),
        "test-key rounds verify against the test key"
    );
    assert!(
        !beacon::verify(50_000_000, &b.sig48, &beacon::quicknet_info().public_key),
        "and never against quicknet"
    );
    assert_eq!(
        d.round(50_000_002).await.unwrap(),
        None,
        "425 for the future"
    );
    // A seal to a test-key round opens with the served signature.
    let p = fclient::seal::Plain {
        version: 1,
        host_id: 5,
        arrive_bell: 9,
        stance: 0,
        ..Default::default()
    };
    let s = fclient::seal::seal(&fclient::seal::pack(&p), &info.public_key, 50_000_000).unwrap();
    assert_eq!(
        fclient::seal::judge(&s.seal, &s.commit, &b.sig48, 5, 9).0,
        0
    );
    node.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gates_on_a_live_localnet_clock() {
    let info = TestKey::new().info();
    // The chain starts 20 game seconds before round R; at 20× (8 s per
    // 400-ms slot) R is published about one wall second later.
    let r = 60_000_000u64;
    let g0 = info.round_time(r) - 20;
    let chain = localnet::Chain::new(localnet::Config {
        scale: 20.0,
        g0,
        ..Default::default()
    });
    let ln = localnet::server::start(Arc::new(Mutex::new(chain)), 0)
        .await
        .unwrap();
    let (gc, poller) = spawn_chain_clock(
        RpcPort::localnet(ln.url(), fclient::addr::system_program()),
        Duration::from_millis(100),
    );
    let node = drand_replay::start(
        Arc::new(Replay {
            source: Source::TestKey(TestKey::new()),
            clock: ClockSource::Chain { clock: gc },
            delay_ms: 1_000,
        }),
        0,
    )
    .await
    .unwrap();
    let d = HttpDrand::new(vec![node.url()], info);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(d.round(r).await.unwrap(), None, "not yet on the game clock");
    let mut got = None;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(b) = d.round(r).await.unwrap() {
            got = Some(b);
            break;
        }
    }
    assert!(
        got.is_some(),
        "published once the chain's Clock passes round_time + delay"
    );
    poller.abort();
    node.stop();
    ln.stop();
}
