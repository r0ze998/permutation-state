//! The verifier's command line against a stub gateway and stub base/ER RPCs
//! (plain HTTP in this process, on ephemeral ports): `--er` is required, an
//! empty index never verifies a running season, a pinned season must be the
//! gateway's, and a stuffed history ends INCOMPLETE, never FAILED.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Output};
use std::sync::Arc;

use permutation_chain::state::{
    season_address, Season, SeasonStatus, CHUNK, SEASON_MAGIC, WORLD_CHUNKS, WORLD_HEADER,
    WORLD_MAGIC,
};
use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov;
use permutation_rules::tick::{run_phase, TickInput};
use permutation_rules::{Preset, Ruleset};
use permutation_server::codec::base64_encode;
use serde_json::{json, Value};

const PROGRAM: &str = "J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n";
const SEASON: u64 = 7;

/// Answers a GET path, or a JSON-RPC method with its params.
type Handler = Arc<dyn Fn(&str, &Value) -> Value + Send + Sync>;

/// Serve `h` on an ephemeral port; returns its URL.
fn serve(h: Handler) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for conn in l.incoming().flatten() {
            let h = h.clone();
            std::thread::spawn(move || answer(conn, &h));
        }
    });
    url
}

fn answer(conn: TcpStream, h: &Handler) {
    let mut r = BufReader::new(conn.try_clone().unwrap());
    let mut first = String::new();
    r.read_line(&mut first).unwrap();
    let mut len = 0;
    loop {
        let mut l = String::new();
        r.read_line(&mut l).unwrap();
        if l.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                len = v.trim().parse().unwrap();
            }
        }
    }
    let mut body = vec![0; len];
    r.read_exact(&mut body).unwrap();
    let mut words = first.split_whitespace();
    let (method, path) = (words.next().unwrap(), words.next().unwrap());
    let reply = if method == "POST" {
        let req: Value = serde_json::from_slice(&body).unwrap();
        let method = req["method"].as_str().unwrap();
        json!({"jsonrpc": "2.0", "id": 1, "result": h(method, &req["params"])})
    } else {
        h(path, &Value::Null)
    }
    .to_string();
    let mut w = conn;
    let _ = write!(
        w,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    );
}

/// A Running 2-nation Blitz season with no members.
fn season() -> Season {
    Season {
        magic: SEASON_MAGIC,
        season_id: SEASON,
        bump: 255,
        vault_bump: 255,
        admin: [3; 32],
        crank: [4; 32],
        usdc_mint: [5; 32],
        usdc_decimals: 6,
        preset: 0,
        nations: 2,
        entry_fee: 0,
        tick_seconds: 30,
        market: false,
        status: SeasonStatus::Running,
        world_seed: [1; 32],
        season_seed: [2; 32],
        member_count: 0,
        nation_members: vec![0, 0],
        seated: 0,
        pool: 0,
        ops: 0,
        ops_withdrawn: false,
        treasury: vec![0, 0],
        treasury_final: vec![],
        payouts: vec![],
        final_root: [0; 32],
        prev_season_id: 0,
        prev_history_root: [0; 32],
        history_root: [0; 32],
        ai_count: 0,
        roster_chain: [0; 32],
        bounty_each: 0,
        bond: 0,
        roster_acc: [0; 32],
        roster_revealed: 0,
        roster_outcome: 0,
        bounty_paid: vec![],
    }
}

/// The season's world after its first election and `ticks` ticks, as the
/// 20 chunk accounts' data.
fn world_chunks(ticks: u16) -> Vec<Vec<u8>> {
    let rules = Ruleset::new(Preset::Blitz);
    let mut s = new_season(&rules, &[1; 32], &[2; 32], &nation_entries(2)).unwrap();
    gov::first_election(&mut s, &rules).unwrap();
    for _ in 0..ticks {
        for p in 0..12 {
            run_phase(&mut s, &rules, &TickInput::default(), p).unwrap();
        }
    }
    assert_eq!(s.tick, ticks);
    let body = borsh::to_vec(&s).unwrap();
    let mut all = vec![0u8; WORLD_CHUNKS * CHUNK];
    all[..8].copy_from_slice(&WORLD_MAGIC);
    all[8..12].copy_from_slice(&(body.len() as u32).to_le_bytes());
    all[WORLD_HEADER..WORLD_HEADER + body.len()].copy_from_slice(&body);
    all.chunks(CHUNK).map(|c| c.to_vec()).collect()
}

fn account(owner: &str, data: &[u8]) -> Value {
    json!({"data": [base64_encode(data), "base64"], "owner": owner, "lamports": 1, "executable": false})
}

/// A gateway serving `/season` for `season_id` and an empty `/ticks`.
fn gateway(season_id: u64) -> String {
    serve(Arc::new(move |path, _| match path {
        "/season" => json!({
            "programId": PROGRAM,
            "season": {"seasonId": season_id.to_string()},
            "accounts": {"season": season_address(PROGRAM, season_id)},
            "genesis": null, "seating": [], "open": null,
        }),
        _ => json!({"records": []}),
    }))
}

/// Base: the Season account, no members, no history; the world is not on
/// base. ER: the world at tick 2 (every chunk the program's), and `junk`
/// successful transactions without records in world chunk 0's history.
fn chain(junk: usize) -> (String, String) {
    let data = borsh::to_vec(&season()).unwrap();
    let base = serve(Arc::new(move |method, params| match method {
        "getAccountInfo" => json!({"context": {"slot": 1}, "value": account(PROGRAM, &data)}),
        "getMultipleAccounts" => {
            json!({"context": {"slot": 1}, "value": vec![Value::Null; params[0].as_array().unwrap().len()]})
        }
        "getProgramAccounts" | "getSignaturesForAddress" => json!([]),
        _ => Value::Null,
    }));
    let chunks = world_chunks(2);
    let er = serve(Arc::new(move |method, _| match method {
        "getMultipleAccounts" => {
            json!({"context": {"slot": 9}, "value": chunks.iter().map(|c| account(PROGRAM, c)).collect::<Vec<_>>()})
        }
        "getSignaturesForAddress" => json!((0..junk)
            .map(|i| json!({"signature": format!("junk{i}"), "slot": 1000 - i as u64, "err": null}))
            .collect::<Vec<_>>()),
        "getTransaction" => json!({
            "slot": 500,
            "meta": {"err": null, "logMessages": ["Program Other111 invoke [1]", "Program Other111 success"]},
            "transaction": {"message": {"accountKeys": ["Payer", "Other111"], "instructions": []}},
        }),
        _ => Value::Null,
    }));
    (base, er)
}

fn verify(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_verify"))
        .args(args)
        .output()
        .unwrap()
}

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

#[test]
fn er_is_required() {
    let (base, _) = chain(0);
    let o = verify(&["--gateway", &gateway(SEASON), "--base", &base]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("pass --er"));
}

#[test]
fn running_season_with_empty_index_is_not_verified() {
    let (base, er) = chain(0);
    let o = verify(&["--gateway", &gateway(SEASON), "--base", &base, "--er", &er]);
    let out = text(&o);
    assert_eq!(o.status.code(), Some(3), "{out}");
    for line in [
        "? genesis recomputed",
        "? first election recomputed",
        "? the replay reaches the live world — the replay stops at tick 0 but the live world is at tick 2",
    ] {
        assert!(out.contains(line), "missing {line:?} in\n{out}");
    }
    assert!(!out.contains("VERIFIED"), "{out}");
    assert!(!out.contains('✗'), "{out}");
    assert!(out.contains("INCOMPLETE"), "{out}");
}

#[test]
fn season_pin() {
    let (base, er) = chain(0);
    let o = verify(&[
        "--gateway",
        &gateway(SEASON),
        "--base",
        &base,
        "--er",
        &er,
        "--season",
        "8",
    ]);
    let out = text(&o);
    assert_eq!(o.status.code(), Some(1), "{out}");
    assert!(out.contains("✗ season identity"), "{out}");
    // Unpinned: a `–` identity line.
    let o = verify(&["--gateway", &gateway(SEASON), "--base", &base, "--er", &er]);
    assert!(text(&o).contains("– season identity pin"), "{}", text(&o));
    // Pinned to the gateway's season: ✓.
    let o = verify(&[
        "--gateway",
        &gateway(SEASON),
        "--base",
        &base,
        "--er",
        &er,
        "--season",
        "7",
    ]);
    assert!(
        text(&o).contains("✓ season identity pinned"),
        "{}",
        text(&o)
    );
    // A program other than the gateway's: ✗.
    let o = verify(&[
        "--gateway",
        &gateway(SEASON),
        "--base",
        &base,
        "--er",
        &er,
        "--program",
        "11111111111111111111111111111111",
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(text(&o).contains("✗ season identity"), "{}", text(&o));
}

#[test]
fn scan_budget() {
    let (base, er) = chain(600);
    let o = verify(&[
        "--gateway",
        &gateway(SEASON),
        "--base",
        &base,
        "--er",
        &er,
        "--max-scan",
        "100",
        "--json",
    ]);
    let out = text(&o);
    assert_eq!(o.status.code(), Some(3), "{out}");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["verdict"], "incomplete");
    let checks = v["checks"].as_array().unwrap();
    assert!(checks.iter().all(|c| c["status"] != "fail"), "{out}");
    assert!(
        checks.iter().any(|c| c["detail"]
            .as_str()
            .unwrap()
            .contains("scan limit reached: 100 transactions")),
        "{out}"
    );
}
