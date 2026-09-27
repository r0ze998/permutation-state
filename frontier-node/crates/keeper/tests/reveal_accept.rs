//! `keeper::reveal_accept_over_localnet` (M1 contract §8.2 `/v1/reveal`,
//! I-24): the accept path of the loopback API against a live `localnet`
//! chain, with THE anchors posted by the keeper's beacon duty.
//!
//! A Holding with one departed transit is written as a fixture (Depart is
//! W3-B's, the reveal pipeline W4-C's); its seal root is built from a
//! plaintext, a salt and a ciphertext hash by the kernel's rules. Then:
//! the material is accepted (`202`, queued once, the same material answers
//! the same track); a wrong salt or ciphertext hash is `409
//! CommitMismatch`; a plaintext for another arrival bell `422 BadPlaintext`;
//! an empty transit slot or a missing Holding `409 TransitState`; a
//! destination resolved past the arrival bell `410 WindowClosed`
//! (LatchClosed); and once THE anchor's window `A + W` passed, `410
//! WindowClosed`.

mod common;
mod model;

use std::sync::Arc;

use base64::Engine as _;
use serde_json::{json, Value};

use fclient::abi::layout as l;
use fclient::seal;
use fclient::{Address, Signer};
use keeper_core::api;
use keeper_core::reveal_accept::{ChainGate, RevealGate};
use keeper_core::Keeper;

use common::*;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

async fn call(addr: std::net::SocketAddr, token: &str, body: Value) -> (u16, Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let b = body.to_string();
    let req = format!(
        "POST /v1/reveal HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
        b.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = vec![];
    s.read_to_end(&mut out).await.unwrap();
    let text = String::from_utf8_lossy(&out).to_string();
    let code: u16 = text[9..12].parse().unwrap();
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
    (code, serde_json::from_str(body).unwrap_or(json!(body)))
}

fn put(d: &mut [u8], o: usize, v: &[u8]) {
    d[o..o + v.len()].copy_from_slice(v);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reveal_accept_over_localnet() {
    let w = world().await;
    println!("program: {}", w.which.label());
    let mut cfg = keeper_config(&w);
    cfg.roles = vec!["beacon".into(), "rings".into()];
    let mut k = Keeper::new(cfg, w.ip.clone(), w.drand.clone(), &[0x51; 32], None).unwrap();
    fund(&w, &k.payers);
    k.start().await.unwrap();
    let genesis_ts = w.season().genesis_ts;
    while w.now() < genesis_ts + 2 * 600 {
        k.tick().await.unwrap();
        w.step();
    }
    let s = w.season();
    let now_bell = fclient::clock::bell_at(s.genesis_ts, w.now()).unwrap();
    let arrive = now_bell + 2;
    // The origin holding (2, 0, site 1) and the destination province (3, -1).
    let (op, oq, site) = (2i32, 0i32, 1u8);
    let (dp, dq) = (3i16, -1i16);
    let host = fclient::addr::host_id(op, oq, site, 0, 1).unwrap();
    let plain = seal::Plain {
        version: 1,
        host_id: host,
        arrive_bell: arrive,
        dest_p: dp,
        dest_q: dq,
        dest_tile: 7,
        stance: 2,
        retreat_bps: 0,
        path_len: 0,
        ..Default::default()
    };
    let pt = seal::pack(&plain);
    let salt = [0x5A; 32];
    let ct = [0xC7; 32];
    let root = seal::seal_root(&seal::commit(&pt, &salt), &ct);
    let hold_addr = w.addrs.holding(op, oq, site);
    {
        let mut h = vec![0u8; fclient::abi::size::HOLDING];
        put(&mut h, 0, fclient::abi::magic::HOLDING);
        put(&mut h, 8, &s.h.season_id.to_le_bytes());
        put(&mut h, 16, &1u16.to_le_bytes());
        put(&mut h, l::holding::P, &(op as i16).to_le_bytes());
        put(&mut h, l::holding::Q, &(oq as i16).to_le_bytes());
        h[l::holding::SITE] = site;
        h[l::holding::STATE] = l::holding::STATE_FINAL;
        let o = l::holding::TRANSIT + l::holding::TRANSIT_STRIDE;
        h[o + l::transit::STATE] = 1;
        h[o + l::transit::FACTION] = 0;
        put(&mut h, o + l::transit::HOST_ID, &host.to_le_bytes());
        put(&mut h, o + l::transit::DEPART_BELL, &now_bell.to_le_bytes());
        put(&mut h, o + l::transit::ARRIVE_BELL, &arrive.to_le_bytes());
        put(&mut h, o + l::transit::SEAL_ROOT, &root);
        w.ip.lock()
            .set_account(
                hold_addr,
                fclient::ports::Account {
                    lamports: fclient::abi::rent(h.len()),
                    data: h,
                    owner: w.program,
                    executable: false,
                },
            )
            .unwrap();
    }
    let token = "k".repeat(32);
    let gate: Arc<dyn RevealGate> = Arc::new(ChainGate {
        port: w.ip.clone(),
        addrs: w.addrs.clone(),
    });
    let srv = api::serve_with(
        "127.0.0.1:0".parse().unwrap(),
        k.shared.clone(),
        token.clone(),
        Some(gate),
    )
    .await
    .unwrap();
    let a = srv.addr;
    let good = json!({
        "holding": hold_addr.to_string(),
        "transit_slot": 1,
        "plain_b64": B64.encode(pt),
        "salt_b64": B64.encode(salt),
        "ct_hash_b64": B64.encode(ct),
    });

    // Accepted, queued once.
    let (c, v) = call(a, &token, good.clone()).await;
    assert_eq!(c, 202, "{v}");
    let track = v["track"].as_str().unwrap().to_string();
    let (c, v) = call(a, &token, good.clone()).await;
    assert_eq!((c, v["track"].as_str()), (202, Some(track.as_str())));
    {
        let sh = k.shared.lock().unwrap();
        assert_eq!(sh.reveals.len(), 1);
        let q = &sh.reveals[0].1;
        assert_eq!(q["checked"], json!(true));
        assert_eq!(q["arrive"], json!(arrive));
        assert_eq!(q["dest"], json!([dp, dq]));
        assert_eq!(q["host_id"], json!(host.to_string()));
    }
    // Commitment.
    let mut bad = good.clone();
    bad["salt_b64"] = json!(B64.encode([0x5B; 32]));
    let (c, v) = call(a, &token, bad).await;
    assert_eq!((c, v["code"].as_str()), (409, Some("CommitMismatch")));
    let mut bad = good.clone();
    bad["ct_hash_b64"] = json!(B64.encode([0; 32]));
    assert_eq!(call(a, &token, bad).await.0, 409);
    // A plaintext for another arrival bell.
    let mut p2 = plain;
    p2.arrive_bell = arrive + 1;
    let mut bad = good.clone();
    bad["plain_b64"] = json!(B64.encode(seal::pack(&p2)));
    let (c, v) = call(a, &token, bad).await;
    assert_eq!((c, v["code"].as_str()), (422, Some("BadPlaintext")));
    // An empty slot, a missing holding.
    let mut bad = good.clone();
    bad["transit_slot"] = json!(0);
    let (c, v) = call(a, &token, bad).await;
    assert_eq!((c, v["code"].as_str()), (409, Some("TransitState")));
    let mut bad = good.clone();
    bad["holding"] = json!(Address::new_from_array([0x77; 32]).to_string());
    assert_eq!(call(a, &token, bad).await.0, 409);
    // The latch: the destination resolved past the arrival bell.
    let pa = w.addrs.province(dp as i32, dq as i32);
    let saved = w.ip.lock().account(&pa);
    if let Some(mut acct) = saved.clone() {
        put(
            &mut acct.data,
            l::province::RESOLVED_NEXT,
            &(arrive + 1).to_le_bytes(),
        );
        w.ip.lock().set_account(pa, acct).unwrap();
        let (c, v) = call(a, &token, good.clone()).await;
        assert_eq!((c, v["code"].as_str()), (410, Some("WindowClosed")));
        assert!(v["detail"].as_str().unwrap().contains("Latch"));
        w.ip.lock().set_account(pa, saved.unwrap()).unwrap();
        assert_eq!(call(a, &token, good.clone()).await.0, 202);
    } else {
        println!("destination province absent (program without land): latch case skipped");
    }
    // THE anchor of (arrive, region(dest)) and its window.
    let r = fclient::ix::region_of(dp as i32, dq as i32);
    let sc = fclient::clock::SeasonClock::from_season(&s);
    loop {
        k.tick().await.unwrap();
        w.step();
        if let Some(an) = k.beacon.anchors.get(&(arrive, r)).copied() {
            let close = sc.reveal_close(arrive, an.a);
            if w.now() < close {
                assert_eq!(
                    call(a, &token, good.clone()).await.0,
                    202,
                    "open before A + W"
                );
            } else {
                break;
            }
        }
    }
    let (c, v) = call(a, &token, good.clone()).await;
    assert_eq!((c, v["code"].as_str()), (410, Some("WindowClosed")));
    println!(
        "accept path: 202 / 409 / 422 / 410 as the program's Reveal; queued {}",
        k.shared.lock().unwrap().reveals.len()
    );
    let _ = w.authority.pubkey();
    srv.stop();
}
