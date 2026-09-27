//! The one-time I-45 validator drill (M1 contract §11 W2-B, §12 Gate W2):
//! SIMD-0186 loaded-data accounting on the installed `solana-test-validator
//! 3.1.9`, against a padded non-BLS probe of the release `.so`'s size.
//!
//! Run by `drill/validator-drill.sh` (starts the validator on 41080–41089 +
//! 41100–41140, deploys the probe twice at two `--max-len`s, runs this test
//! with `--ignored`, stops the validator). Needs `PSF_DRILL_RPC`,
//! `PSF_DRILL_PROGRAM`, `PSF_DRILL_PROGRAM_SMALL`, `PSF_DRILL_KEYPAIR`.
//!
//! What it establishes, each by the smallest passing limit (the formula's
//! prediction P must pass and P − 1 must fail):
//! 1. the base transaction: payer + probe program + its ProgramData (not
//!    listed) + the ComputeBudget program;
//! 2. what an **absent** address, a **pre-funded** (lamports, no data)
//!    address, a 10,000-B data account, the **instructions sysvar** and a
//!    listed **System program** each add;
//! 3. that the ProgramData counts: the same ELF deployed at a smaller
//!    `max_len` needs exactly the difference less;
//! 4. that a load failure is **charged**: the transaction lands in a block
//!    with `MaxLoadedAccountsDataSizeExceeded` and the payer loses the fee.
//!
//! The harness's `Chain::loaded_size` implements the rule this drill
//! confirms; `chain_rule_matches_the_drill` (tests/harness.rs) pins the
//! harness to the recorded numbers.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::{json, Value};
use solana_address::Address;
use solana_hash::Hash;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;

struct Rpc {
    host: String,
    id: u64,
}

impl Rpc {
    fn new(url: &str) -> Rpc {
        let host = url
            .strip_prefix("http://")
            .expect("http:// URL")
            .trim_end_matches('/')
            .to_string();
        Rpc { host, id: 0 }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let body = json!({"jsonrpc": "2.0", "id": self.id, "method": method, "params": params})
            .to_string();
        let mut s = TcpStream::connect(&self.host).expect("connect to the drill validator");
        s.set_read_timeout(Some(Duration::from_secs(30))).ok();
        write!(
            s,
            "POST / HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.host,
            body.len(),
            body
        )
        .expect("write");
        let mut buf = vec![];
        s.read_to_end(&mut buf).expect("read");
        let text = String::from_utf8_lossy(&buf);
        let (head, rest) = text.split_once("\r\n\r\n").expect("http response");
        let body = if head
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            let mut out = String::new();
            let mut r = rest;
            loop {
                let (len, tail) = r.split_once("\r\n").expect("chunk");
                let n = usize::from_str_radix(len.trim(), 16).expect("chunk len");
                if n == 0 {
                    break;
                }
                out.push_str(&tail[..n]);
                r = &tail[n + 2..];
            }
            out
        } else {
            rest.to_string()
        };
        let v: Value = serde_json::from_str(&body).unwrap_or_else(|e| panic!("{e}: {body}"));
        if let Some(e) = v.get("error") {
            panic!("{method}: {e}");
        }
        v["result"].clone()
    }

    fn blockhash(&mut self) -> Hash {
        let r = self.call("getLatestBlockhash", json!([{"commitment": "processed"}]));
        r["value"]["blockhash"]
            .as_str()
            .expect("hash")
            .parse()
            .expect("base58")
    }

    fn balance(&mut self, k: &Address) -> u64 {
        self.call(
            "getBalance",
            json!([k.to_string(), {"commitment": "confirmed"}]),
        )["value"]
            .as_u64()
            .expect("u64")
    }

    /// Data length of an account (None if absent).
    fn data_len(&mut self, k: &Address) -> Option<usize> {
        let r = self.call(
            "getAccountInfo",
            json!([k.to_string(), {"encoding": "base64", "commitment": "confirmed"}]),
        );
        let v = &r["value"];
        if v.is_null() {
            return None;
        }
        let d = v["data"][0].as_str().expect("base64");
        Some(
            base64::engine::general_purpose::STANDARD
                .decode(d)
                .expect("b64")
                .len(),
        )
    }

    /// Sends without preflight and waits for confirmation: the error (None on success) and the fee.
    fn send(&mut self, t: &Transaction) -> (Option<Value>, u64) {
        let wire = bincode::serialize(t).expect("wire");
        let b64 = base64::engine::general_purpose::STANDARD.encode(wire);
        let sig = self.call(
            "sendTransaction",
            json!([b64, {"encoding": "base64", "skipPreflight": true, "maxRetries": 5}]),
        );
        let sig = sig.as_str().expect("signature").to_string();
        let start = Instant::now();
        loop {
            let st = self.call(
                "getSignatureStatuses",
                json!([[sig], {"searchTransactionHistory": true}]),
            );
            let s = &st["value"][0];
            if !s.is_null()
                && s["confirmationStatus"]
                    .as_str()
                    .is_some_and(|c| c != "processed")
            {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(60),
                "{sig} not confirmed"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
        let tx = self.call(
            "getTransaction",
            json!([sig, {"encoding": "json", "commitment": "confirmed", "maxSupportedTransactionVersion": 0}]),
        );
        let meta = &tx["meta"];
        let err = if meta["err"].is_null() {
            None
        } else {
            Some(meta["err"].clone())
        };
        (err, meta["fee"].as_u64().unwrap_or(0))
    }
}

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("{k} unset: run drill/validator-drill.sh"))
}

fn read_keypair(path: &str) -> Keypair {
    let v: Vec<u8> =
        serde_json::from_str(&std::fs::read_to_string(path).expect("keypair file")).expect("json");
    Keypair::try_from(v.as_slice()).expect("64-byte keypair")
}

fn loaded_limit_ix(bytes: u32) -> Instruction {
    fclient::tx::set_loaded_accounts_data_size_limit(bytes)
}

/// One composition: the probe's no-op (tag 2) listing `extra` read-only accounts.
fn probe_tx(
    program: Address,
    extra: &[Address],
    limit: u32,
    payer: &Keypair,
    bh: Hash,
) -> Transaction {
    let ix = Instruction {
        program_id: program,
        accounts: extra
            .iter()
            .map(|k| AccountMeta::new_readonly(*k, false))
            .collect(),
        data: vec![2, 0, 0],
    };
    let msg =
        Message::new_with_blockhash(&[loaded_limit_ix(limit), ix], Some(&payer.pubkey()), &bh);
    let mut t = Transaction::new_unsigned(msg);
    t.sign(&[payer], bh);
    t
}

fn is_loaded_exceeded(e: &Option<Value>) -> bool {
    e.as_ref()
        .is_some_and(|v| v.to_string().contains("MaxLoadedAccountsDataSizeExceeded"))
}

/// The smallest limit this composition loads under, found from `guess`:
/// returns (limit, the fee a failed load was charged, payer delta).
fn min_limit(
    rpc: &mut Rpc,
    program: Address,
    extra: &[Address],
    payer: &Keypair,
    guess: u32,
) -> u32 {
    let ok = |rpc: &mut Rpc, l: u32| -> bool {
        let bh = rpc.blockhash();
        let (err, _) = rpc.send(&probe_tx(program, extra, l, payer, bh));
        match &err {
            None => true,
            e if is_loaded_exceeded(e) => false,
            Some(e) => panic!("unexpected failure at limit {l}: {e}"),
        }
    };
    // Fast path: the prediction is exact.
    if ok(rpc, guess) && !ok(rpc, guess - 1) {
        return guess;
    }
    let (mut lo, mut hi) = (guess.saturating_sub(1 << 20).max(1), guess + (1 << 20));
    assert!(!ok(rpc, lo) && ok(rpc, hi), "bracket [{lo}, {hi}]");
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if ok(rpc, mid) {
            hi = mid
        } else {
            lo = mid
        }
    }
    hi
}

#[test]
#[ignore = "needs the drill validator: drill/validator-drill.sh"]
fn drill_validator_loaded_data() {
    let mut rpc = Rpc::new(&env("PSF_DRILL_RPC"));
    let program: Address = env("PSF_DRILL_PROGRAM").parse().expect("base58");
    let small: Address = env("PSF_DRILL_PROGRAM_SMALL").parse().expect("base58");
    let payer = read_keypair(&env("PSF_DRILL_KEYPAIR"));

    let v = rpc.call("getVersion", json!([]));
    println!("drill: validator {}", v);

    // Accounts for the compositions.
    let absent =
        Address::new_from_array(permutation_frontier_svm_tests::sha256(&[b"drill absent"]));
    assert!(rpc.data_len(&absent).is_none() && rpc.balance(&absent) == 0);
    let prefunded = Keypair::new_from_array(permutation_frontier_svm_tests::sha256(&[
        b"drill prefunded",
    ]));
    let data_acc = Keypair::new_from_array(permutation_frontier_svm_tests::sha256(&[
        b"drill data 10000",
    ]));
    let bh = rpc.blockhash();
    if rpc.balance(&prefunded.pubkey()) == 0 {
        let t = Transaction::new_signed_with_payer(
            &[fclient::tx::transfer(
                payer.pubkey(),
                prefunded.pubkey(),
                1_000_000,
            )],
            Some(&payer.pubkey()),
            &[&payer],
            bh,
        );
        assert_eq!(rpc.send(&t).0, None);
    }
    if rpc.data_len(&data_acc.pubkey()).is_none() {
        // System CreateAccount (tag 0) of 10,000 B owned by the System program.
        let space = 10_000u64;
        let lamports = rpc
            .call("getMinimumBalanceForRentExemption", json!([space]))
            .as_u64()
            .expect("rent");
        let mut d = vec![0u8; 52];
        d[4..12].copy_from_slice(&lamports.to_le_bytes());
        d[12..20].copy_from_slice(&space.to_le_bytes());
        let ix = Instruction {
            program_id: Address::default(),
            accounts: vec![
                AccountMeta::new(payer.pubkey(), true),
                AccountMeta::new(data_acc.pubkey(), true),
            ],
            data: d,
        };
        let bh = rpc.blockhash();
        let t = Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &data_acc],
            bh,
        );
        assert_eq!(rpc.send(&t).0, None);
    }
    assert_eq!(rpc.data_len(&data_acc.pubkey()), Some(10_000));
    let ix_sysvar = fclient::addr::instructions_sysvar();
    let system = Address::default();
    let cbp = fclient::addr::compute_budget_program();

    // The prediction for the base composition from the accounts' real sizes.
    let pd = fclient::addr::programdata(&program);
    let pd_small = fclient::addr::programdata(&small);
    let pd_len = rpc.data_len(&pd).expect("programdata");
    let pd_small_len = rpc.data_len(&pd_small).expect("programdata (small)");
    let prog_len = rpc.data_len(&program).expect("program account");
    let cbp_len = rpc.data_len(&cbp).unwrap_or(0);
    let sys_len = rpc.data_len(&system).unwrap_or(0);
    let payer_len = rpc.data_len(&payer.pubkey()).unwrap_or(0);
    println!(
        "drill: programdata {pd_len} B (small {pd_small_len} B), program account {prog_len} B, ComputeBudget account {cbp_len} B, System account {sys_len} B, payer {payer_len} B"
    );
    let base_pred = (64 + payer_len) + (64 + prog_len) + (64 + pd_len) + (64 + cbp_len);

    let base = min_limit(&mut rpc, program, &[], &payer, base_pred as u32);
    println!("drill: base need {base} B (predicted {base_pred} B)");
    let with = |rpc: &mut Rpc, extra: &[Address], add: usize| {
        min_limit(rpc, program, extra, &payer, (base_pred + add) as u32)
    };
    let absent_need = with(&mut rpc, &[absent], 0);
    let pre_need = with(&mut rpc, &[prefunded.pubkey()], 64);
    let data_need = with(&mut rpc, &[data_acc.pubkey()], 64 + 10_000);
    let ixs_need = with(&mut rpc, &[ix_sysvar], 0);
    let sys_need = with(&mut rpc, &[system], 64 + sys_len);
    let small_need = min_limit(
        &mut rpc,
        small,
        &[],
        &payer,
        (base_pred - pd_len + pd_small_len) as u32,
    );

    println!(
        "drill: absent account adds {} B",
        absent_need as i64 - base as i64
    );
    println!(
        "drill: pre-funded account (no data) adds {} B",
        pre_need as i64 - base as i64
    );
    println!(
        "drill: 10,000-B account adds {} B",
        data_need as i64 - base as i64
    );
    println!(
        "drill: instructions sysvar adds {} B",
        ixs_need as i64 - base as i64
    );
    println!(
        "drill: listed System program adds {} B",
        sys_need as i64 - base as i64
    );
    println!(
        "drill: programdata {} B less → need {} B less",
        pd_len - pd_small_len,
        base as i64 - small_need as i64
    );

    assert_eq!(
        base as usize, base_pred,
        "base = Σ(64 + data) incl. the unlisted ProgramData"
    );
    assert_eq!(absent_need, base, "an absent account counts 0");
    assert_eq!(
        pre_need,
        base + 64,
        "a pre-funded, data-less account counts 64"
    );
    assert_eq!(
        data_need,
        base + 64 + 10_000,
        "a data account counts 64 + data"
    );
    assert_eq!(ixs_need, base, "the instructions sysvar counts 0");
    assert_eq!(
        sys_need as usize,
        base as usize + 64 + sys_len,
        "a listed builtin counts 64 + its data"
    );
    assert_eq!(
        base as usize - small_need as usize,
        pd_len - pd_small_len,
        "the ProgramData counts toward the limit"
    );

    // A load failure lands in a block and is charged.
    let before = rpc.balance(&payer.pubkey());
    let bh = rpc.blockhash();
    let (err, fee) = rpc.send(&probe_tx(program, &[], base - 1, &payer, bh));
    let after = rpc.balance(&payer.pubkey());
    println!("drill: one byte below the need: err {err:?}, fee {fee}, payer {before} → {after}");
    assert!(
        is_loaded_exceeded(&err),
        "MaxLoadedAccountsDataSizeExceeded"
    );
    assert!(fee >= 5_000, "the fee is charged on a load failure");
    assert_eq!(before - after, fee, "the payer paid exactly the fee");
    let page_below = ((base as u64).div_ceil(32_768) * 32_768 - 32_768) as u32;
    let bh = rpc.blockhash();
    let (err, fee2) = rpc.send(&probe_tx(program, &[], page_below, &payer, bh));
    println!("drill: one page below round_up(need): err {err:?}, fee {fee2}");
    assert!(is_loaded_exceeded(&err) && fee2 >= 5_000);
    println!(
        "DRILL RESULT: need {base} B for a {pd_len}-B ProgramData; absent +{}, pre-funded +{}, ix sysvar +{}, System +{}; failed load charged {fee} lamports",
        absent_need - base,
        pre_need - base,
        ixs_need - base,
        sys_need - base
    );
}
