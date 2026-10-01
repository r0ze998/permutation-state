//! The herald origin over 127.0.0.1:0 (contract §8.4): `/h/*` answers and
//! their cache headers, gzip siblings, ETag/304, security headers and the
//! two CSPs, `/frontier/*` static files, the `/gw/*` proxy (headers passed
//! and dropped, `X-Forwarded-For`), and `WS /h/ws` (subscribe, diffs in
//! sequence, heartbeat, a slow socket's gap and drop).

mod common;

use std::io::Read;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{tmp, VecSource};
use fclient::ports::TxRecord;
use herald_fold::fixture;
use herald_fold::fold::{Diff, Scope};
use herald_fold::runner::{Ingest, IngestCfg};
use herald_fold::server::{self, App, API_CSP, WEB_CSP};
use herald_fold::views::SeasonStatic;
use herald_fold::ws::{self, WsCfg};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;

struct Stack {
    addr: SocketAddr,
    ing: Ingest,
    rest: Vec<TxRecord>,
    relay_seen: Arc<Mutex<Vec<String>>>,
    dir: std::path::PathBuf,
}

/// A fake relay: records each raw request, answers 201 with a cookie the
/// herald must not pass on.
async fn fake_relay() -> (String, Arc<Mutex<Vec<String>>>) {
    let l = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = l.local_addr().unwrap().to_string();
    let seen = Arc::new(Mutex::new(vec![]));
    let s2 = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else {
                return;
            };
            let seen = s2.clone();
            tokio::spawn(async move {
                let mut buf = vec![];
                let mut b = [0u8; 4096];
                loop {
                    let n = s.read(&mut b).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&b[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(i) = text.find("\r\n\r\n") {
                        let len = text[..i]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if buf.len() >= i + 4 + len {
                            break;
                        }
                    }
                }
                seen.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf).to_string());
                let body = r#"{"ok":true,"from":"relay"}"#;
                let ans = format!(
                    "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nSet-Cookie: s=1\r\nX-Relay: yes\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(ans.as_bytes()).await;
            });
        }
    });
    (addr, seen)
}

async fn stack(name: &str, bells: u32, fold_first: usize, diffs_cap: usize) -> Stack {
    let dir = tmp(name);
    let txs = fixture::mini_season(bells);
    let (diffs, _) = broadcast::channel(diffs_cap);
    let cfg = IngestCfg::new(&dir, fixture::program(), fixture::SEASON_ID);
    let mut ing = Ingest::open(cfg.clone(), diffs.clone()).unwrap();
    let n = fold_first.min(txs.len());
    let mut src = VecSource::new(txs[..n].to_vec(), 10_000);
    while ing.step(&mut src).await.unwrap() > 0 {}
    let web = dir.join("web");
    std::fs::create_dir_all(web.join("map")).unwrap();
    std::fs::write(web.join("index.html"), "<!doctype html><title>F</title>").unwrap();
    std::fs::write(web.join("app.mjs"), "export const x = 1;\n".repeat(50)).unwrap();
    std::fs::write(
        web.join("app.mjs.gz"),
        herald_fold::files::gzip("export const x = 1;\n".repeat(50).as_bytes()),
    )
    .unwrap();
    std::fs::write(web.join("map/fmap.1a2b3c4d.mjs"), "export {};").unwrap();
    std::fs::write(dir.join("secret.txt"), "no").unwrap();
    let (relay, relay_seen) = fake_relay().await;
    let mut app = App::new(
        ing.fold.clone(),
        herald_fold::files::Out::new(cfg.files_dir()),
        SeasonStatic {
            cluster: "localnet".into(),
            drand: fclient::beacon::TestKey::new().info(),
            quotas: json!({"perDay": 40, "burst": 60}),
        },
        diffs,
    );
    app.web = Some(web);
    app.relay = Some(relay);
    app.index = Some(cfg.index_path());
    app.ws = WsCfg {
        heartbeat: Duration::from_secs(5),
        max_lags: 1,
        max_sockets: 64,
    };
    let l = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(server::serve(l, Arc::new(app)));
    Stack {
        addr,
        ing,
        rest: txs[n..].to_vec(),
        relay_seen,
        dir,
    }
}

struct Answer {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Answer {
    fn h(&self, k: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(x, _)| x.eq_ignore_ascii_case(k))
            .map(|(_, v)| v.as_str())
    }
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&self.body)))
    }
}

async fn req(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Answer {
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let mut r = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (k, v) in headers {
        r.push_str(&format!("{k}: {v}\r\n"));
    }
    r.push_str("\r\n");
    s.write_all(r.as_bytes()).await.unwrap();
    s.write_all(body).await.unwrap();
    let mut buf = vec![];
    s.read_to_end(&mut buf).await.unwrap();
    let a = fclient::http::parse_response(&buf).unwrap();
    Answer {
        status: a.status,
        headers: a.headers,
        body: a.body,
    }
}

async fn get(addr: SocketAddr, path: &str) -> Answer {
    req(addr, "GET", path, &[], &[]).await
}

fn security(a: &Answer, csp: &str) {
    assert_eq!(a.h("x-frame-options"), Some("DENY"));
    assert_eq!(a.h("referrer-policy"), Some("no-referrer"));
    assert_eq!(a.h("x-content-type-options"), Some("nosniff"));
    assert_eq!(a.h("content-security-policy"), Some(csp));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h_routes_caching_and_headers() {
    let st = stack("routes", 10, usize::MAX, 4_096).await;
    let a = st.addr;
    // /h/season: the record, its bytes, ETag, 304.
    let s = get(a, "/h/season").await;
    assert_eq!(s.status, 200);
    security(&s, API_CSP);
    assert_eq!(s.h("access-control-allow-origin"), Some("*"));
    assert_eq!(s.h("cache-control"), Some("public, max-age=30"));
    let j = s.json();
    assert_eq!(j["v"], 1);
    assert_eq!(j["season"], fixture::SEASON_ID.to_string());
    assert_eq!(j["programId"], fixture::program().to_string());
    assert_eq!(j["cluster"], "localnet");
    assert_eq!(j["genesisTs"], fixture::GENESIS_TS);
    assert_eq!(j["W"], 600);
    assert_eq!(j["delta"], 60);
    assert_eq!(
        j["drand"]["publicKey"],
        hex::encode(fclient::beacon::TestKey::new().info().public_key)
    );
    assert_eq!(j["rings"].as_array().unwrap().len(), 2);
    assert!(j["bytes_b64"].is_string());
    let etag = s.h("etag").unwrap().to_string();
    let n = req(a, "GET", "/h/season", &[("If-None-Match", &etag)], &[]).await;
    assert_eq!(n.status, 304);
    // Per-bell province file: immutable, gzip sibling on request.
    let p = get(a, "/h/province/2,0/0").await;
    assert_eq!(p.status, 200);
    assert_eq!(p.h("cache-control"), Some(server::IMMUTABLE));
    assert_eq!(p.json()["key"], "pv:2,0");
    let pz = req(
        a,
        "GET",
        "/h/province/2,0/0",
        &[("Accept-Encoding", "gzip, br")],
        &[],
    )
    .await;
    assert_eq!(pz.h("content-encoding"), Some("gzip"));
    let mut plain = vec![];
    flate2::read::GzDecoder::new(&pz.body[..])
        .read_to_end(&mut plain)
        .unwrap();
    assert_eq!(plain, p.body);
    // Latest.
    let l = get(a, "/h/province/2,0/latest").await;
    assert_eq!(l.h("cache-control"), Some("public, max-age=2"));
    assert_eq!(l.json()["bell"], 10);
    assert_eq!(get(a, "/h/province/2,0/999").await.status, 404);
    assert_eq!(get(a, "/h/province/nope/0").await.status, 400);
    // Overviews.
    let o = get(a, "/h/overview/2/3.bin").await;
    assert_eq!(
        (o.status, o.h("cache-control")),
        (200, Some(server::IMMUTABLE))
    );
    assert_eq!(&o.body[..8], herald_fold::OVERVIEW_MAGIC);
    let ol = get(a, "/h/overview/2/latest.bin").await;
    assert_eq!(ol.h("cache-control"), Some("public, max-age=5"));
    assert_eq!(get(a, "/h/overview/9/latest.bin").await.status, 404);
    // Roster: who holds each site (names and faces are derived by the page).
    let rl = get(a, "/h/roster/2/latest.bin").await;
    assert_eq!(rl.status, 200);
    assert_eq!(rl.h("cache-control"), Some("public, max-age=30"));
    assert_eq!(&rl.body[..8], herald_fold::roster::ROSTER_MAGIC);
    let n = u16::from_le_bytes([rl.body[18], rl.body[19]]) as usize;
    assert_eq!(
        rl.body.len(),
        herald_fold::roster::ROSTER_HEADER + n * herald_fold::roster::ROSTER_RECORD
    );
    assert_eq!(get(a, "/h/roster/9/latest.bin").await.status, 404);
    // Clash report.
    let c = get(a, "/h/clash/2,0/0").await;
    assert_eq!(c.json()["heraldCheck"], "match");
    // Bell-region: final (immutable) once archived, short-lived before.
    let b0 = get(a, "/h/bell/0/region/3").await;
    assert_eq!(b0.h("cache-control"), Some(server::IMMUTABLE));
    assert_eq!(b0.json()["final"], true);
    let b9 = get(a, "/h/bell/9/region/3").await;
    assert_eq!(b9.h("cache-control"), Some("public, max-age=5"));
    // /h/me: no-store, the Citizen and Holding bytes; the relay's quota
    // answer (the fake relay answers every path) is passed through.
    let me = get(a, &format!("/h/me/{}", fixture::wallet())).await;
    assert_eq!(me.h("cache-control"), Some("no-store"));
    let mj = me.json();
    assert!(mj["citizen"]["bytes_b64"].is_string());
    assert_eq!(mj["holdings"].as_array().unwrap().len(), 1);
    assert_eq!(mj["quota"]["from"], "relay");
    assert_eq!(get(a, "/h/me/not-a-wallet").await.status, 400);
    // Events: numbered, decoded, raw; the page after the last is empty.
    let e = get(a, "/h/events?after=0").await.json();
    let evs = e["events"].as_array().unwrap();
    assert!(evs.len() > 20);
    assert_eq!(evs[0]["seq"], "1");
    assert_eq!(evs[0]["decoded"]["name"], "SEASON_CREATED");
    assert!(
        evs.iter().all(|x| x["decoded"]["key"]["p"] != 9),
        "no failed-transaction record"
    );
    let next = e["next"].as_str().unwrap().to_string();
    let e2 = get(a, &format!("/h/events?after={next}")).await.json();
    assert!(e2["events"].as_array().unwrap().is_empty());
    assert_eq!(get(a, "/h/events?after=x").await.status, 400);
    // Status.
    let stj = get(a, "/h/status").await.json();
    assert_eq!(stj["alarms"]["clashMismatch"], 0);
    // Unknown paths.
    assert_eq!(get(a, "/h/nothing").await.status, 404);
    let _ = std::fs::remove_dir_all(&st.dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn frontier_static_and_gw_proxy() {
    let st = stack("static", 2, usize::MAX, 64).await;
    let a = st.addr;
    let r = get(a, "/").await;
    assert_eq!((r.status, r.h("location")), (302, Some("/frontier/")));
    let i = get(a, "/frontier/").await;
    assert_eq!(i.status, 200);
    security(&i, WEB_CSP);
    assert_eq!(i.h("content-type"), Some("text/html; charset=utf-8"));
    assert_eq!(i.h("cache-control"), Some("no-cache"));
    let etag = i.h("etag").unwrap().to_string();
    assert_eq!(
        req(
            a,
            "GET",
            "/frontier/index.html",
            &[("If-None-Match", &etag)],
            &[]
        )
        .await
        .status,
        304
    );
    let m = req(
        a,
        "GET",
        "/frontier/app.mjs",
        &[("Accept-Encoding", "gzip")],
        &[],
    )
    .await;
    assert_eq!(m.h("content-type"), Some("text/javascript; charset=utf-8"));
    assert_eq!(m.h("content-encoding"), Some("gzip"));
    let h = get(a, "/frontier/map/fmap.1a2b3c4d.mjs").await;
    assert_eq!(
        h.h("cache-control"),
        Some(server::IMMUTABLE),
        "hashed names are long-lived"
    );
    for bad in [
        "/frontier/../secret.txt",
        "/frontier/%2e%2e/secret.txt",
        "/frontier/.hidden",
        "/frontier/map//x",
    ] {
        assert_eq!(get(a, bad).await.status, 404, "{bad}");
    }
    // /gw: GET with a query, POST with a body; only Content-Type goes on,
    // X-Forwarded-For is the peer (loopback → the forwarded address).
    let g = req(
        a,
        "GET",
        "/gw/f/quota?citizen=abc",
        &[
            ("Cookie", "secret=1"),
            ("Authorization", "Bearer x"),
            ("X-Forwarded-For", "9.9.9.9, 8.8.8.8"),
        ],
        &[],
    )
    .await;
    assert_eq!(g.status, 201);
    assert_eq!(g.json()["from"], "relay");
    assert_eq!(
        g.h("set-cookie"),
        None,
        "only status, type and body come back"
    );
    assert_eq!(g.h("x-relay"), None);
    security(&g, API_CSP);
    let p = req(
        a,
        "POST",
        "/gw/f/relay",
        &[("Content-Type", "application/json")],
        br#"{"tx":"AAAA"}"#,
    )
    .await;
    assert_eq!(p.status, 201);
    let seen = st.relay_seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert!(
        seen[0].starts_with("GET /f/quota?citizen=abc HTTP/1.1\r\n"),
        "{}",
        seen[0]
    );
    assert!(
        seen[0].contains("X-Forwarded-For: 8.8.8.8\r\n"),
        "{}",
        seen[0]
    );
    assert!(!seen[0].to_ascii_lowercase().contains("cookie"));
    assert!(!seen[0].to_ascii_lowercase().contains("authorization"));
    assert!(seen[1].starts_with("POST /f/relay HTTP/1.1\r\n"));
    assert!(seen[1].contains("Content-Type: application/json\r\n"));
    assert!(seen[1].ends_with(r#"{"tx":"AAAA"}"#));
    assert_eq!(req(a, "PUT", "/gw/f/relay", &[], &[]).await.status, 405);
    assert_eq!(get(a, "/gw/f/../x").await.status, 400);
    let big = vec![b'x'; server::GW_MAX_BODY + 1];
    let tb = req(
        a,
        "POST",
        "/gw/f/relay",
        &[("Content-Type", "application/json")],
        &big,
    )
    .await;
    assert!(tb.status == 413, "{}", tb.status);
    let _ = std::fs::remove_dir_all(&st.dir);
}

async fn next_json(c: &mut ws::Client) -> Value {
    let t = tokio::time::timeout(Duration::from_secs(5), c.next_text())
        .await
        .expect("a message")
        .expect("open");
    serde_json::from_str(&t).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ws_subscribe_and_receive_in_sequence() {
    let mut st = stack("ws", 8, 40, 4_096).await;
    let host = st.addr.to_string();
    let mut c = ws::Client::connect(&host, "/h/ws").await.unwrap();
    c.send_text(
        &json!({"op": "sub", "provinces": [[2, 0]], "rings": [2], "bells": false}).to_string(),
    )
    .await
    .unwrap();
    let ack = next_json(&mut c).await;
    assert_eq!(ack["op"], "subscribed");
    assert_eq!(ack["provinces"], 1);
    // A bad subscription is refused and the old one kept.
    c.send_text(r#"{"op":"nope"}"#).await.unwrap();
    assert_eq!(next_json(&mut c).await["code"], "UnknownOp");
    // Fold the rest of the season: diffs flow to the socket.
    let rest = std::mem::take(&mut st.rest);
    let mut src = VecSource::new(rest, 10_000);
    while st.ing.step(&mut src).await.unwrap() > 0 {}
    let mut seqs = vec![];
    let mut kinds = std::collections::BTreeSet::new();
    let mut keys = vec![];
    while let Ok(Some(t)) = tokio::time::timeout(Duration::from_millis(800), c.next_text()).await {
        let v: Value = serde_json::from_str(&t).unwrap();
        seqs.push(v["seq"].as_u64().unwrap());
        kinds.insert(v["kind"].as_str().unwrap().to_string());
        keys.push(v["key"].as_str().unwrap().to_string());
    }
    assert!(seqs.len() > 10, "{} messages", seqs.len());
    assert_eq!(seqs, (1..=seqs.len() as u64).collect::<Vec<_>>(), "no gap");
    assert!(kinds.contains("acct") && kinds.contains("bell") && kinds.contains("event"));
    assert!(
        keys.iter().any(|k| k.starts_with("/h/overview/2/")),
        "ring 2 overviews"
    );
    assert!(keys.iter().any(|k| k.starts_with("/h/province/2,0/")));
    assert!(
        !keys.iter().any(|k| k.starts_with("/h/province/1,1/")),
        "not subscribed"
    );
    assert!(
        !keys.iter().any(|k| k.starts_with("/h/bell/")),
        "bells: false"
    );
    // A non-WebSocket GET is refused.
    assert_eq!(get(st.addr, "/h/ws").await.status, 400);
    let _ = std::fs::remove_dir_all(&st.dir);
}

/// A socket that falls behind the broadcast channel sees a skipped
/// sequence number (resync), and one that stays behind is dropped with
/// close 1013. In memory, on one thread, so the lag is certain.
#[tokio::test(flavor = "current_thread")]
async fn a_slow_socket_sees_a_gap_then_is_dropped() {
    let (tx, _) = broadcast::channel::<Arc<Diff>>(4);
    let (server_io, client_io) = tokio::io::duplex(1 << 20);
    let stats = Arc::new(ws::WsStats::default());
    let cfg = WsCfg {
        heartbeat: Duration::from_secs(60),
        max_lags: 1,
        max_sockets: 8,
    };
    let rx = tx.subscribe();
    let h = tokio::spawn(ws::serve(server_io, rx, cfg, stats.clone()));
    let (mut rd, mut wr) = tokio::io::split(client_io);
    let sub = json!({"op": "sub", "provinces": [[2, 0]]}).to_string();
    wr.write_all(&ws::encode_frame(
        ws::OP_TEXT,
        sub.as_bytes(),
        Some([1, 2, 3, 4]),
    ))
    .await
    .unwrap();
    let f = ws::read_frame(&mut rd, false).await.unwrap();
    assert!(String::from_utf8_lossy(&f.payload).contains("subscribed"));
    let d = |i: u64| {
        Arc::new(Diff {
            kind: "acct",
            key: format!("k{i}"),
            slot: i,
            head: None,
            bytes: vec![],
            scope: Scope::Province(2, 0),
            t_ms: 0,
            wire: Default::default(),
        })
    };
    // Two in order.
    tx.send(d(1)).unwrap();
    tx.send(d(2)).unwrap();
    let mut got = vec![];
    for _ in 0..2 {
        let f = ws::read_frame(&mut rd, false).await.unwrap();
        let v: Value = serde_json::from_slice(&f.payload).unwrap();
        got.push(v["seq"].as_u64().unwrap());
    }
    assert_eq!(got, vec![1, 2]);
    // Ten at once into a channel of four: the socket lags once.
    for i in 3..13 {
        tx.send(d(i)).unwrap();
    }
    let f = ws::read_frame(&mut rd, false).await.unwrap();
    let v: Value = serde_json::from_slice(&f.payload).unwrap();
    assert_eq!(v["seq"], 4, "seq 3 skipped: the client resyncs");
    for _ in 0..3 {
        ws::read_frame(&mut rd, false).await.unwrap();
    }
    // It lags again: past max_lags it is dropped with 1013.
    for i in 13..23 {
        tx.send(d(i)).unwrap();
    }
    let f = ws::read_frame(&mut rd, false).await.unwrap();
    assert_eq!(f.op, ws::OP_CLOSE);
    assert_eq!(f.payload, 1013u16.to_be_bytes());
    h.await.unwrap();
    assert_eq!(
        stats.dropped_slow.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(stats.open.load(std::sync::atomic::Ordering::SeqCst), 0);
}

/// Heartbeat: pings every period; a silent client is closed after three.
#[tokio::test(flavor = "current_thread")]
async fn heartbeat_pings_and_closes_a_silent_socket() {
    let (tx, _) = broadcast::channel::<Arc<Diff>>(4);
    let (server_io, client_io) = tokio::io::duplex(1 << 16);
    let cfg = WsCfg {
        heartbeat: Duration::from_millis(50),
        max_lags: 3,
        max_sockets: 8,
    };
    let h = tokio::spawn(ws::serve(
        server_io,
        tx.subscribe(),
        cfg,
        Arc::new(ws::WsStats::default()),
    ));
    let (mut rd, _wr) = tokio::io::split(client_io);
    let mut pings = 0;
    loop {
        let f = ws::read_frame(&mut rd, false).await.unwrap();
        match f.op {
            ws::OP_PING => pings += 1,
            ws::OP_CLOSE => break,
            op => panic!("unexpected op {op}"),
        }
    }
    assert_eq!(pings, 3);
    h.await.unwrap();
}
