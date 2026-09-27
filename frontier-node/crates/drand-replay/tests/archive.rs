//! The archive path end to end on loopback (M1 contract §8.7, I-53): the
//! prefetch tool fetches from a local `drand-replay --test-key` (two
//! endpoints, one of them lying), verifies every round, resumes, packs; the
//! packed archive loads, serves gated by the game clock, and is re-verified
//! in full; the SP-V2 quicknet fixture rounds pack and serve the same way.
//! No public endpoint is ever contacted (O-M1-12).

use std::path::PathBuf;
use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use drand_replay::archive::Archive;
use drand_replay::prefetch::{self, Fetcher, Options, Plan};
use drand_replay::{ClockSource, Replay, Source};
use fclient::beacon::{self, HttpDrand, TestKey};
use fclient::ports::DrandPort;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("psf-drand-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

async fn test_key_server(now: i64) -> drand_replay::Running {
    drand_replay::start(
        Arc::new(Replay {
            source: Source::TestKey(TestKey::new()),
            clock: ClockSource::Fixed(now),
            delay_ms: 1_000,
        }),
        0,
    )
    .await
    .unwrap()
}

/// An endpoint that answers every round with a signature of another round.
async fn liar() -> (String, tokio::task::JoinHandle<()>) {
    let app = Router::new().route(
        "/{chain}/public/{round}",
        get(
            |axum::extract::Path((_c, r)): axum::extract::Path<(String, u64)>| async move {
                let b = TestKey::new().beacon(r + 1);
                axum::Json(serde_json::json!({"round": r, "signature": hex::encode(b.sig48)}))
            },
        ),
    );
    let l = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    let h = tokio::spawn(async move {
        let _ = axum::serve(l, app).await;
    });
    (url, h)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn prefetch_verifies_resumes_packs_and_serves() {
    let k = TestKey::new();
    let info = k.info();
    let first = 70_000_000u64;
    let plan = Plan {
        first,
        last: first + 239,
    };
    let src = test_key_server(info.round_time(plan.last) + 60).await;
    let d = dir("prefetch");

    // A first run stops early (as a crash would), keeping its progress.
    let o = Options {
        endpoints: vec![src.url(), src.url()],
        rate: 400.0,
        max_new: Some(100),
        ..Options::default()
    };
    let r = prefetch::run(&d, &info, plan, &o).await.unwrap();
    assert!(!r.complete && r.fetched >= 100 && r.resumed == 0, "{r:?}");
    assert!(!Archive::is_packed(&d), "no manifest before completion");

    // The second run resumes, with a lying endpoint beside the honest one:
    // every lie is rejected and its round fetched from the other.
    let (lie, lh) = liar().await;
    let o = Options {
        endpoints: vec![src.url(), lie],
        rate: 400.0,
        attempts: 50,
        ..Options::default()
    };
    let r2 = prefetch::run(&d, &info, plan, &o).await.unwrap();
    assert!(r2.complete, "{r2:?}");
    assert_eq!(r2.resumed, r.fetched);
    assert_eq!(r2.resumed + r2.fetched, plan.count());
    assert!(r2.rejected > 0, "the liar was caught");
    lh.abort();

    let a = Archive::load(&d, &info).unwrap();
    assert_eq!(a.segments.len(), 1);
    assert_eq!(a.range(), Some((plan.first, plan.last)));
    assert!(a.verify_all(4).is_empty());
    assert!(
        Archive::load(&d, &beacon::quicknet_info()).is_err(),
        "pinned chain"
    );

    // Serve it, gated by the game clock.
    let mid = first + 120;
    let node = drand_replay::start(
        Arc::new(Replay {
            source: Source::open(&d, info.clone()).unwrap(),
            clock: ClockSource::Fixed(info.round_time(mid) + 1),
            delay_ms: 1_000,
        }),
        0,
    )
    .await
    .unwrap();
    let h = HttpDrand::new(vec![node.url()], info.clone());
    assert_eq!(h.round(mid).await.unwrap().unwrap().sig48, k.sign(mid));
    assert_eq!(h.round(mid + 1).await.unwrap(), None, "425 before its time");
    let latest = fclient::http::get(&format!("{}/public/latest", node.url()))
        .await
        .unwrap();
    assert_eq!(latest.header("cache-control"), Some("no-store"));
    let lb = beacon::parse_beacon_json(&serde_json::from_slice(&latest.body).unwrap()).unwrap();
    assert_eq!(lb.round, mid);
    let one = fclient::http::get(&format!("{}/public/{}", node.url(), first))
        .await
        .unwrap();
    assert!(one.header("cache-control").unwrap().contains("immutable"));
    let before = fclient::http::get(&format!("{}/public/{}", node.url(), first - 1))
        .await
        .unwrap();
    assert_eq!(before.status, 404, "published but not archived");
    node.stop();
    src.stop();
    let _ = std::fs::remove_dir_all(&d);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_curl_fetcher_and_the_public_endpoint_guard() {
    let info = TestKey::new().info();
    let plan = Plan {
        first: 80_000_000,
        last: 80_000_011,
    };
    // Never contacted: refused before any request.
    let e = prefetch::run(
        &dir("guard"),
        &info,
        plan,
        &Options {
            endpoints: vec!["https://api.drand.sh".into()],
            fetcher: Fetcher::Curl,
            ..Options::default()
        },
    )
    .await
    .unwrap_err();
    assert!(e.contains("O-M1-12"), "{e}");
    let have_curl = std::process::Command::new("curl")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !have_curl {
        eprintln!("curl fetcher NOT RUN: no curl on PATH");
        return;
    }
    let src = test_key_server(info.round_time(plan.last) + 10).await;
    let d = dir("curl");
    let r = prefetch::run(
        &d,
        &info,
        plan,
        &Options {
            endpoints: vec![src.url()],
            fetcher: Fetcher::Curl,
            rate: 200.0,
            ..Options::default()
        },
    )
    .await
    .unwrap();
    assert!(r.complete && r.fetched == 12, "{r:?}");
    // An endpoint without the rounds yet (425) fails the run with progress kept.
    let early = test_key_server(info.round_time(plan.first) - 100).await;
    let e = prefetch::run(
        &dir("early"),
        &info,
        plan,
        &Options {
            endpoints: vec![early.url()],
            rate: 500.0,
            attempts: 2,
            ..Options::default()
        },
    )
    .await
    .unwrap_err();
    assert!(e.contains("HTTP 425"), "{e}");
    early.stop();
    src.stop();
    let _ = std::fs::remove_dir_all(&d);
}

#[tokio::test]
async fn the_quicknet_fixture_rounds_pack_and_serve() {
    let q = beacon::quicknet_info();
    let f = beacon::FixtureDrand::load(&beacon::fixture_dir(), q.clone()).unwrap();
    let rounds = f.rounds.iter().map(|(r, b)| (*r, b.sig48)).collect();
    let a = Archive::from_rounds(q.clone(), &rounds);
    assert_eq!(a.rounds(), 32);
    let d = dir("fixture");
    a.write(&d).unwrap();
    let b = Archive::load(&d, &q).unwrap();
    assert!(b.verify_all(2).is_empty());
    let r = *f.rounds.keys().nth(7).unwrap();
    let rep = Replay {
        source: Source::open(&d, q.clone()).unwrap(),
        clock: ClockSource::Fixed(q.round_time(r) + 2),
        delay_ms: 1_000,
    };
    assert_eq!(rep.round(r).unwrap(), f.rounds[&r]);
    assert_eq!(rep.latest().unwrap().round, r);
    let _ = std::fs::remove_dir_all(&d);
}
