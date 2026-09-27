//! The relay stand-in's checks (§8.3) over HTTP on `127.0.0.1:0`, on a
//! chain without the program (fast; the day run covers the chain side):
//! a well-shaped sponsored transaction passes the shape checks and reaches
//! the drain guard's simulation; every shape violation is refused before
//! co-signing with the relay's code.

use fclient::addr::Addresses;
use fclient::ix::{self, HoldingRef, Player};
use fclient::{Address, Instruction, Keypair, Signer};
use frontier_bots::txb::{self, RelayInfo};
use itest::relay::{self, RelayState};
use localnet::{Config, InProcess};
use serde_json::{json, Value};

const PROGRAM: [u8; 32] = [0x5F; 32];

async fn relay() -> relay::Running {
    let program = Address::new_from_array(PROGRAM);
    let ip = InProcess::new(Config::default(), Some(program));
    relay::serve(RelayState::new(ip, program, 7, &[0x52; 32]))
        .await
        .expect("relay")
}

async fn info(r: &relay::Running) -> RelayInfo {
    let a = fclient::http::get(&format!("{}/f/relay", r.base()))
        .await
        .unwrap();
    let v: Value = serde_json::from_slice(&a.body).unwrap();
    let ans = frontier_bots::ports::Answer::new(a.status, v);
    RelayInfo::from_answer(&ans).expect("GET /f/relay")
}

async fn post(r: &relay::Running, path: &str, body: &Value) -> (u16, Value) {
    let a = fclient::http::post_json(&format!("{}{path}", r.base()), body)
        .await
        .unwrap();
    (
        a.status,
        serde_json::from_slice(&a.body).unwrap_or(Value::Null),
    )
}

fn harvest(a: &Addresses, session: &Keypair, wallet: &Keypair, payer: Address) -> Instruction {
    let p = Player {
        actor: session.pubkey(),
        payer,
        wallet: wallet.pubkey(),
    };
    ix::harvest(
        a,
        &p,
        HoldingRef {
            p: 2,
            q: 0,
            site: 1,
        },
    )
}

#[tokio::test]
async fn shapes_are_checked_before_co_signing() {
    let r = relay().await;
    let a = Addresses::new(Address::new_from_array(PROGRAM), 7);
    let b = txb::budgets();
    let wallet = Keypair::new_from_array([1; 32]);
    let session = Keypair::new_from_array([2; 32]);
    let i = info(&r).await;
    assert!(r.state.pool.iter().any(|k| k.pubkey() == i.fee_payer));

    // Well shaped: passes to the drain guard, whose simulation fails here
    // (no program on this chain): 409 with a code, nothing sent.
    let t = txb::sponsored(
        harvest(&a, &session, &wallet, i.fee_payer),
        &b,
        &i,
        &[&session],
    )
    .unwrap();
    let (s, v) = post(&r, "/f/relay", &json!({"tx": txb::wire_b64(&t)})).await;
    assert_eq!(s, 409, "{v}");
    assert_eq!(v["ok"], false);
    assert!(r.state.stats.lock().unwrap().sent.is_empty());

    // A fee payer outside the pool.
    let stranger = RelayInfo {
        fee_payer: Keypair::new_from_array([9; 32]).pubkey(),
        ..i.clone()
    };
    let t = txb::sponsored(
        harvest(&a, &session, &wallet, stranger.fee_payer),
        &b,
        &stranger,
        &[&session],
    )
    .unwrap();
    let (s, v) = post(&r, "/f/relay", &json!({"tx": txb::wire_b64(&t)})).await;
    assert_eq!((s, v["code"].as_str()), (400, Some("RelayRejected")), "{v}");

    // A priced transaction (SetComputeUnitPrice ≠ 0).
    let bud = fclient::tx::TxBudget::from_budget(b.get(fclient::abi::tag::HARVEST), 5);
    let msg = fclient::tx::message(
        &[harvest(&a, &session, &wallet, i.fee_payer)],
        &bud,
        &i.fee_payer,
        &i.blockhash,
    );
    let mut t = fclient::Transaction::new_unsigned(msg);
    t.try_partial_sign(&[&session], i.blockhash).unwrap();
    let (s, v) = post(&r, "/f/relay", &json!({"tx": txb::wire_b64(&t)})).await;
    assert_eq!((s, v["code"].as_str()), (400, Some("RelayRejected")), "{v}");

    // A Reveal goes to /f/reveal.
    let rv = Instruction {
        program_id: a.program,
        accounts: vec![fclient::AccountMeta::new_readonly(session.pubkey(), true)],
        data: vec![fclient::abi::tag::REVEAL],
    };
    let t = txb::sponsored(rv, &b, &i, &[&session]).unwrap();
    let (s, v) = post(&r, "/f/relay", &json!({"tx": txb::wire_b64(&t)})).await;
    assert_eq!(
        (s, v["code"].as_str()),
        (400, Some("UseRevealRoute")),
        "{v}"
    );

    // A Join only on /f/join, and /f/join only a Join.
    let j = ix::join(
        &a,
        wallet.pubkey(),
        i.fee_payer,
        0,
        &session.pubkey(),
        0,
        None,
    );
    let t = txb::sponsored(j, &b, &i, &[&wallet]).unwrap();
    let (s, v) = post(&r, "/f/relay", &json!({"tx": txb::wire_b64(&t)})).await;
    assert_eq!((s, v["code"].as_str()), (400, Some("RelayRejected")), "{v}");
    let t = txb::sponsored(
        harvest(&a, &session, &wallet, i.fee_payer),
        &b,
        &i,
        &[&session],
    )
    .unwrap();
    let (s, v) = post(&r, "/f/join", &json!({"tx": txb::wire_b64(&t)})).await;
    assert_eq!((s, v["code"].as_str()), (400, Some("RelayRejected")), "{v}");

    // A signature that does not verify (signed over another blockhash).
    let mut t = txb::sponsored(
        harvest(&a, &session, &wallet, i.fee_payer),
        &b,
        &i,
        &[&session],
    )
    .unwrap();
    t.message.recent_blockhash = fclient::Hash::new_from_array([3; 32]);
    let (s, v) = post(&r, "/f/relay", &json!({"tx": txb::wire_b64(&t)})).await;
    assert_eq!((s, v["code"].as_str()), (400, Some("BadSignature")), "{v}");

    // A Depart whose tip is not a preset (no season here: every tip is).
    let d = ix::depart(
        &a,
        &Player {
            actor: session.pubkey(),
            payer: i.fee_payer,
            wallet: wallet.pubkey(),
        },
        HoldingRef {
            p: 2,
            q: 0,
            site: 1,
        },
        (2, 0),
        &ix::DepartArgs {
            host_id: 1,
            commit: [0; 32],
            seal: [0; 165],
            arrive_bell: 10,
            tip: 12_345,
            transit_slot: 0,
        },
    );
    let t = txb::sponsored(d, &b, &i, &[&session]).unwrap();
    let (s, v) = post(&r, "/f/relay", &json!({"tx": txb::wire_b64(&t)})).await;
    assert_eq!((s, v["code"].as_str()), (400, Some("TipNotPreset")), "{v}");

    // A settle shape with an authority signature.
    let st = Instruction {
        program_id: a.program,
        accounts: vec![fclient::AccountMeta::new_readonly(session.pubkey(), true)],
        data: vec![fclient::abi::tag::SETTLE_TRANSIT],
    };
    let t = txb::sponsored(st, &b, &i, &[&session]).unwrap();
    let (s, v) = post(&r, "/f/relay", &json!({"tx": txb::wire_b64(&t)})).await;
    assert_eq!((s, v["code"].as_str()), (400, Some("RelayRejected")), "{v}");

    // No keeper linked: /f/reveal is unavailable, not accepted.
    let (s, v) = post(&r, "/f/reveal", &json!({"holding": "x"})).await;
    assert_eq!(
        (s, v["code"].as_str()),
        (503, Some("KeeperUnavailable")),
        "{v}"
    );

    // The quota answer.
    let q = fclient::http::get(&format!(
        "{}/f/quota?citizen={}",
        r.base(),
        a.citizen(&wallet.pubkey())
    ))
    .await
    .unwrap();
    let v: Value = serde_json::from_slice(&q.body).unwrap();
    assert_eq!(v["left"], relay::QUOTA_PER_DAY);
    r.stop();
}
