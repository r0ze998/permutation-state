//! `frontier-bots` against the recorded herald fixtures
//! (`crates/agents/fixtures/herald`) and a mock relay that enforces the
//! relay's sponsored shapes (§8.3): the bots join, file tickets, build,
//! seal and send marches (the seal opens with the test key), journal
//! before they send, reveal in the arrival bell, and the personas meet the
//! refusals §8.6 expects. The last test runs 1,000 bots in one process.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use fclient::abi::tag;
use fclient::beacon::TestKey;
use fclient::ports::{PortError, SimResult};
use fclient::seal::{self as fs, Plain};
use fclient::{Address, Hash, Keypair, Signer, Transaction};
use frontier_agents::fixture::{self, FINAL, JOINED, NOW, PROVISIONAL, SEED, UNJOINED};
use frontier_agents::obs::{b64, b64_encode};
use frontier_agents::policy::{MarchMemo, SealKind};
use frontier_agents::profile::{roster, AgentSpec, Arch, Mix};
use frontier_agents::Persona;
use frontier_bots::bot::{Bot, ClockSource, Config, Shared};
use frontier_bots::fleet::Fleet;
use frontier_bots::journal::Journal;
use frontier_bots::ports::{Answer, DirHerald, DirectPort, NoDirect, RelayPort};
use frontier_bots::report::Verdict;
use serde_json::{json, Value};

/// What the mock relay saw.
#[derive(Default)]
struct RelayLog {
    /// (path, tag, signer keys other than the fee payer, tip for a Depart).
    posts: Vec<(String, u8, Vec<Address>, Option<u64>)>,
    departs: Vec<Transaction>,
    /// Departs refused `HostInTransit` (the settle racer's redepart).
    in_transit: Vec<Transaction>,
    reveals: Vec<Value>,
    settles: Vec<Value>,
    /// Sponsored transactions per authority this game day.
    per_authority: BTreeMap<Address, u32>,
    /// Whether the journal already held the march when its Depart came.
    journalled_first: Vec<bool>,
}

struct MockRelay {
    fee_payer: Keypair,
    presets: [u64; 3],
    quota: u32,
    /// Reveals refused as after the window (410 WindowClosed).
    window_closed: bool,
    /// Reveals refused as the keeper's §5.11 step-6 check does (409
    /// `{"error":"Shielded"}`, passed through; W6T-3).
    shielded: bool,
    journal: Option<std::path::PathBuf>,
    log: Mutex<RelayLog>,
}

impl MockRelay {
    fn new(presets: [u64; 3]) -> MockRelay {
        MockRelay {
            fee_payer: Keypair::new_from_array([0xFE; 32]),
            presets,
            quota: 40,
            window_closed: false,
            shielded: false,
            journal: None,
            log: Mutex::new(RelayLog::default()),
        }
    }

    fn refuse(status: u16, code: &str) -> Answer {
        Answer::new(status, json!({"error": code, "code": code}))
    }

    /// The relay's checks that the bots must satisfy (shapes.mjs).
    fn check(&self, path: &str, b: &Value) -> Answer {
        let Some(wire) = b
            .get("tx")
            .and_then(|x| x.as_str())
            .and_then(|s| b64(s).ok())
        else {
            return Self::refuse(400, "BadBody");
        };
        let t: Transaction = match fclient::tx::from_wire(&wire) {
            Ok(t) => t,
            Err(_) => return Self::refuse(400, "InvalidTransaction"),
        };
        let msg = &t.message;
        if msg.instructions.len() != 4 || msg.account_keys[0] != self.fee_payer.pubkey() {
            return Self::refuse(400, "RelayRejected");
        }
        let pb = fclient::tx::parse_budget(msg);
        let ix = &msg.instructions[3];
        let tg = ix.data[0];
        let want = frontier_bots::txb::budgets().get(tg);
        if pb.cu_price != Some(0)
            || pb.effective_cu_limit() != want.cu_limit
            || pb.effective_loaded_limit() != want.loaded_limit
        {
            return Self::refuse(400, "RelayRejected");
        }
        if tg == tag::REVEAL {
            return Self::refuse(400, "UseRevealRoute");
        }
        // Every signer except the fee payer has signed.
        let data = t.message_data();
        let n = msg.header.num_required_signatures as usize;
        let mut signers = vec![];
        for i in 1..n {
            if !t.signatures[i].verify(msg.account_keys[i].as_ref(), &data) {
                return Self::refuse(400, "BadSignature");
            }
            signers.push(msg.account_keys[i]);
        }
        let settle = tg == tag::SETTLE_TRANSIT || tg == tag::SETTLE_EXPLORE;
        if settle != signers.is_empty() {
            return Self::refuse(400, "RelayRejected");
        }
        if settle {
            let req: Address = b["requester"].as_str().unwrap().parse().unwrap();
            let sig = fclient::ports::Signature::from(
                <[u8; 64]>::try_from(b64(b["requesterSig"].as_str().unwrap()).unwrap()).unwrap(),
            );
            if !sig.verify(req.as_ref(), &data) {
                return Self::refuse(400, "BadSignature");
            }
        }
        if (path == "/f/join") != (tg == tag::JOIN) {
            return Self::refuse(400, "RelayRejected");
        }
        let mut tip = None;
        if tg == tag::DEPART {
            // The program refuses a Depart of the fixture's host in transit.
            let host = u64::from_le_bytes(ix.data[1..9].try_into().unwrap());
            let (hp, hq) = fixture::world().home;
            if host
                == fclient::addr::host_id(hp as i32, hq as i32, 0, 1, fixture::IN_TRANSIT_SEQ)
                    .unwrap()
            {
                self.log.lock().unwrap().in_transit.push(t.clone());
                return Self::refuse(400, "HostInTransit");
            }
            let t8: [u8; 8] = ix.data[210..218].try_into().unwrap();
            let x = u64::from_le_bytes(t8);
            tip = Some(x);
            if !self.presets.contains(&x) {
                return Self::refuse(400, "TipNotPreset");
            }
        }
        let mut log = self.log.lock().unwrap();
        if let Some(&a) = signers.first() {
            let c = log.per_authority.entry(a).or_default();
            if *c >= self.quota {
                return Answer::new(429, json!({"code": "QuotaExceeded", "retryAt": 0}));
            }
            *c += 1;
        }
        if tg == tag::DEPART {
            let host = u64::from_le_bytes(ix.data[1..9].try_into().unwrap());
            let journalled = self.journal.as_ref().is_some_and(|p| {
                std::fs::read_to_string(p)
                    .is_ok_and(|s| s.contains(&format!("\"host\":\"{host}\"")))
            });
            log.journalled_first.push(journalled);
            log.departs.push(t.clone());
        }
        if settle {
            log.settles.push(b.clone());
        }
        log.posts.push((path.to_string(), tg, signers, tip));
        Answer::new(
            200,
            json!({"ok": true, "signature": t.signatures[1.min(n - 1)].to_string()}),
        )
    }
}

impl RelayPort for MockRelay {
    async fn get(&self, path: &str) -> Result<Answer, PortError> {
        assert!(path.starts_with("/f/relay"), "{path}");
        Ok(Answer::new(
            200,
            json!({
                "feePayer": self.fee_payer.pubkey().to_string(),
                "blockhash": Hash::new_from_array([7; 32]).to_string(),
                "lastValidBlockHeight": 1_000,
                "programId": fixture::program().to_string(),
            }),
        ))
    }
    async fn post(&self, path: &str, b: &Value) -> Result<Answer, PortError> {
        match path {
            "/f/reveal" => {
                self.log.lock().unwrap().reveals.push(b.clone());
                Ok(if self.window_closed {
                    Self::refuse(410, "WindowClosed")
                } else if self.shielded {
                    Answer::new(409, json!({"error": "Shielded"}))
                } else {
                    Answer::new(202, json!({"accepted": true, "track": "t1"}))
                })
            }
            "/f/relay" | "/f/join" => Ok(self.check(path, b)),
            _ => Ok(Self::refuse(404, "NotFound")),
        }
    }
}

/// A local chain that refuses what the program would.
#[derive(Default)]
struct MockDirect {
    log: Mutex<Vec<(u8, usize)>>,
    holds: Mutex<Vec<(Vec<Address>, u32, u64)>>,
}

impl DirectPort for MockDirect {
    async fn blockhash(&self) -> Result<Hash, PortError> {
        Ok(Hash::new_from_array([8; 32]))
    }
    async fn simulate(&self, wire: &[u8]) -> Result<SimResult, PortError> {
        let t = fclient::tx::from_wire(wire).unwrap();
        let ix = t.message.instructions.last().unwrap();
        let prog = t.message.account_keys[ix.program_id_index as usize];
        let tg = if prog == fixture::program() {
            ix.data[0]
        } else {
            0
        };
        self.log
            .lock()
            .unwrap()
            .push((tg, t.message.instructions.len()));
        let code = match tg {
            tag::DEPART if u64::from_le_bytes(ix.data[210..218].try_into().unwrap()) == 0 => {
                Some(51)
            }
            // A reveal two bells after its arrival bell: WindowClosed.
            tag::REVEAL
                if u32::from_le_bytes(ix.data[12..16].try_into().unwrap()) + 2 <= fixture::BELL =>
            {
                Some(12)
            }
            // A forged anchor (0xF1…) is not THE anchor.
            tag::REVEAL
                if t.message
                    .account_keys
                    .contains(&Address::new_from_array([0xF1; 32])) =>
            {
                Some(2)
            }
            _ => None,
        };
        Ok(SimResult {
            err: code.map(|c| format!("custom program error: {c:#x}")),
            code,
            ..SimResult::default()
        })
    }
    async fn send(&self, _: &[u8]) -> Result<String, PortError> {
        Ok("sig".into())
    }
    async fn airdrop(&self, _: &Address, _: u64) -> Result<(), PortError> {
        Ok(())
    }
    async fn balance(&self, _: &Address) -> Result<u64, PortError> {
        Ok(0)
    }
    async fn hold(&self, keys: &[Address], p: u32, slots: u64) -> Result<(), PortError> {
        self.holds.lock().unwrap().push((keys.to_vec(), p, slots));
        Ok(())
    }
}

fn herald() -> DirHerald {
    DirHerald::new(fixture::dir())
}

fn presets() -> [u64; 3] {
    let v: Value =
        serde_json::from_slice(&std::fs::read(fixture::dir().join("h/season.json")).unwrap())
            .unwrap();
    let s = frontier_agents::obs::SeasonView::from_json(&v).unwrap();
    s.tip_presets(frontier_bots::txb::budgets().get(tag::REVEAL).loaded_limit)
}

fn spec(i: u32, arch: Arch, persona: Option<Persona>) -> AgentSpec {
    AgentSpec {
        index: i,
        arch,
        faction: 0,
        join_day: 0,
        join_bell: 5,
        persona,
    }
}

fn shared<D: DirectPort>(relay: MockRelay, direct: Option<D>) -> Shared<DirHerald, MockRelay, D> {
    shared_seed(SEED, relay, direct)
}

fn shared_seed<D: DirectPort>(
    seed: u64,
    relay: MockRelay,
    direct: Option<D>,
) -> Shared<DirHerald, MockRelay, D> {
    Shared::new(
        herald(),
        relay,
        direct,
        Config::new(seed),
        ClockSource::fixed(NOW),
    )
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("w3e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[tokio::test]
async fn fixture_wallets_act_in_the_relays_shapes() {
    let dir = tmp("shapes");
    let jp = dir.join("marchbook.jsonl");
    let mut relay = MockRelay::new(presets());
    relay.journal = Some(jp.clone());
    let sh = shared::<NoDirect>(relay, None).with_journal(Journal::open(&jp).unwrap());
    let r = vec![
        spec(UNJOINED, Arch::Daily, None),
        spec(JOINED, Arch::Daily, None),
        spec(PROVISIONAL, Arch::Daily, None),
        spec(FINAL, Arch::VerySkilled, Some(Persona::MinTip)),
    ];
    let mut fleet = Fleet::new(sh, &r);
    let sent = fleet.step_all(true, 8).await;
    assert!(sent >= 3, "{sent}");
    let log = fleet.shared.relay.log.lock().unwrap();
    let by = |who: u32| -> Vec<u8> {
        let w = frontier_agents::keys::wallet(SEED, who).pubkey();
        let s = frontier_agents::keys::session(SEED, who).pubkey();
        log.posts
            .iter()
            .filter(|p| p.2.contains(&w) || p.2.contains(&s))
            .map(|p| p.1)
            .collect()
    };
    // Not joined: a Join signed by the wallet through /f/join.
    assert_eq!(by(UNJOINED), vec![tag::JOIN]);
    assert!(log.posts.iter().any(|p| p.0 == "/f/join"));
    // Joined without a holding: a ticket.
    assert_eq!(by(JOINED), vec![tag::FILE_TICKET]);
    // Provisional: waits.
    assert!(by(PROVISIONAL).is_empty());
    // Final (min_tip): economy and a march at tip_min.
    let f = by(FINAL);
    assert!(f.contains(&tag::DEPART), "{f:?}");
    assert!(
        f.contains(&tag::BUILD) || f.contains(&tag::TRAIN) || f.contains(&tag::EXPLORE),
        "{f:?}"
    );
    let d = log.posts.iter().find(|p| p.1 == tag::DEPART).unwrap();
    assert_eq!(d.3, Some(presets()[0]), "min_tip pays tip_min");
    // The journal line was on disk before the Depart reached the relay.
    assert_eq!(log.journalled_first, vec![true]);
    // The seal in the Depart opens with the test key at tlock_round(arrive)
    // and matches its commitment (the march is honest).
    let t = &log.departs[0];
    let ix = &t.message.instructions[3];
    let host = u64::from_le_bytes(ix.data[1..9].try_into().unwrap());
    let commit: [u8; 32] = ix.data[9..41].try_into().unwrap();
    let seal: [u8; 165] = ix.data[41..206].try_into().unwrap();
    let arrive = u32::from_le_bytes(ix.data[206..210].try_into().unwrap());
    let key = TestKey::new();
    let round = fclient::clock::Drand {
        genesis: key.info().genesis_time,
        period: key.info().period,
    }
    .tlock_round(fixture::GENESIS_TS, arrive);
    let (code, p) = fs::judge(&seal, &commit, &key.sign(round), host, arrive);
    assert_eq!(code, 0, "valid seal");
    let p: Plain = p.unwrap();
    assert_eq!(p.arrive_bell, arrive);
    drop(log);
    // The bot remembers the march, and the journal restores it.
    let b = fleet.bots.iter().find(|b| b.spec.index == FINAL).unwrap();
    assert_eq!(b.mem.marches.len(), 1);
    assert!(b.mem.marches[0].sent);
    let restored = Journal::load(&jp).unwrap();
    assert_eq!(restored[&FINAL][0].commit, commit);
    assert!(restored[&FINAL][0].sent);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A march of the `FINAL` wallet: its host in transit in the fixture
/// (sequence 5, slot 3) unless `landed_host` (sequence 1, no transit).
fn memo_for(arrive: u32, kind: SealKind, landed_host: bool) -> MarchMemo {
    let w = fixture::world();
    let (hp, hq) = w.home;
    let seq = if landed_host {
        1
    } else {
        fixture::IN_TRANSIT_SEQ
    };
    let host = fclient::addr::host_id(hp as i32, hq as i32, 0, 1, seq).unwrap();
    let plain = fs::pack(&Plain {
        version: 1,
        host_id: host,
        arrive_bell: arrive,
        dest_p: w.enemy_home.0,
        dest_q: w.enemy_home.1,
        dest_tile: 0,
        stance: 0,
        retreat_bps: 0,
        path_len: 1,
        path: fs::path_of(&[0]),
        reserved: [0; 3],
    });
    let salt = [2u8; 32];
    let seal = vec![0x80u8; 165];
    MarchMemo {
        key: (host, 30),
        h: fclient::ix::HoldingRef {
            p: hp,
            q: hq,
            site: 0,
        },
        transit_slot: if landed_host {
            0
        } else {
            fixture::IN_TRANSIT_SLOT
        },
        arrive_bell: arrive,
        dest: (w.enemy_home.0 as i32, w.enemy_home.1 as i32),
        path_others: vec![],
        plain,
        salt,
        commit: fs::commit(&plain, &salt),
        ct_hash: fs::ct_hash(&seal),
        seal,
        tip: presets()[0],
        kind,
        sent: true,
        reveal_tries: 0,
        revealed: false,
        accepted: false,
        last_code: None,
        late_done: false,
        settled: false,
        redeparted: false,
    }
}

#[tokio::test]
async fn owners_reveal_through_the_keeper_link() {
    let sh = shared::<NoDirect>(MockRelay::new(presets()), None);
    let mut bot = Bot::new(spec(FINAL, Arch::Daily, None), SEED);
    bot.mem
        .marches
        .push(memo_for(fixture::BELL, SealKind::Honest, false));
    bot.step(&sh, false).await;
    let log = sh.relay.log.lock().unwrap();
    assert_eq!(log.reveals.len(), 1);
    let r = &log.reveals[0];
    let m = &bot.mem.marches[0];
    assert_eq!(
        r["holding"],
        json!(m.h.address(&fixture::addresses()).to_string())
    );
    assert_eq!(r["transit_slot"], json!(fixture::IN_TRANSIT_SLOT));
    assert_eq!(
        b64(r["plain_b64"].as_str().unwrap()).unwrap(),
        m.plain.to_vec()
    );
    assert_eq!(
        b64(r["salt_b64"].as_str().unwrap()).unwrap(),
        m.salt.to_vec()
    );
    assert_eq!(
        b64(r["ct_hash_b64"].as_str().unwrap()).unwrap(),
        m.ct_hash.to_vec()
    );
    // W6T-3: a 202 queues the material at the keepers; only an observed
    // REVEAL marks the march revealed (`revealed_only_on_observed_reveal`).
    assert!(m.accepted && !m.revealed, "202 is accepted, not revealed");
    // Nothing else during a duty wake-up.
    assert!(log.posts.is_empty());
}

#[tokio::test]
async fn personas_meet_the_refusals_they_expect() {
    let direct = MockDirect::default();
    let mut relay = MockRelay::new(presets());
    relay.window_closed = true;
    let sh = shared(relay, Some(direct));
    // zero_tip: the relay refuses a non-preset tip; the program TipTooLow.
    let mut z = Bot::new(spec(FINAL, Arch::Bot, Some(Persona::ZeroTip)), SEED);
    z.step(&sh, true).await;
    // late_revealer: a reveal two bells after its arrival bell.
    let mut l = Bot::new(spec(FINAL, Arch::Bot, Some(Persona::LateRevealer)), SEED);
    l.mem
        .marches
        .push(memo_for(fixture::BELL - 2, SealKind::Honest, false));
    l.step(&sh, false).await;
    // forger: a forged direct reveal in the bell (and the honest one).
    let mut f = Bot::new(spec(FINAL, Arch::Bot, Some(Persona::Forger)), SEED);
    f.mem
        .marches
        .push(memo_for(fixture::BELL, SealKind::Honest, false));
    f.step(&sh, false).await;
    // spammer: a burst until the quota says no.
    let mut s = Bot::new(spec(FINAL, Arch::Bot, Some(Persona::Spammer)), SEED);
    s.step(&sh, true).await;
    // ticket_holder: holds its provisional holding's Province at 0.5.
    let mut t = Bot::new(
        spec(PROVISIONAL, Arch::Bot, Some(Persona::TicketHolder)),
        SEED,
    );
    t.step(&sh, false).await;
    let rep = sh.report.lock().unwrap().clone();
    assert_eq!(
        rep.verdict(Persona::ZeroTip),
        Verdict::Observed,
        "{:#}",
        rep.to_json()
    );
    assert_eq!(rep.verdict(Persona::LateRevealer), Verdict::Observed);
    assert_eq!(rep.verdict(Persona::Forger), Verdict::Observed);
    assert_eq!(rep.verdict(Persona::Spammer), Verdict::Observed);
    let zs: Vec<_> = rep
        .persona_outcomes
        .iter()
        .filter(|o| o.persona == Some(Persona::ZeroTip) && o.tag == Some("zero_tip"))
        .map(|o| (o.route, o.code.clone()))
        .collect();
    assert!(
        zs.contains(&("relay", Some("TipNotPreset".into()))),
        "{zs:?}"
    );
    assert!(zs.contains(&("direct", Some("TipTooLow".into()))), "{zs:?}");
    assert_eq!(z.mem.marches.len(), 0, "a refused march is not remembered");
    let holds = sh.direct.as_ref().unwrap().holds.lock().unwrap().clone();
    assert_eq!(holds.len(), 1);
    let (hp, hq) = fixture::world().home;
    assert_eq!(
        holds[0].0,
        vec![fixture::addresses().province(hp as i32, hq as i32)]
    );
    assert_eq!(holds[0].1, 500);
    // 3 bells of 600 s at 8 game seconds a slot (scale 20).
    assert_eq!(holds[0].2, 225);
    // The spammer's burst stopped at the quota (40 a session key in the
    // mock, shared by the test's bots of one wallet): its last try a 429.
    let spam: Vec<_> = rep
        .persona_outcomes
        .iter()
        .filter(|o| o.tag == Some("spam"))
        .collect();
    assert!((2..=41).contains(&spam.len()), "{}", spam.len());
    assert_eq!(spam.last().unwrap().status, 429);
    let j = rep.to_json();
    assert_eq!(j["personas"].as_array().unwrap().len(), 13);
}

#[tokio::test]
async fn a_thousand_bots_step_in_one_process() {
    // A fleet seed of its own: no fixture wallet among them (all unjoined).
    let m = Mix::for_season_days(7);
    let r = roster(1_000, 99, &m);
    let sh = shared_seed::<NoDirect>(99, MockRelay::new(presets()), None);
    let mut fleet = Fleet::new(sh, &r);
    let t0 = Instant::now();
    let sent = fleet.step_all(true, 1_000).await;
    let dt = t0.elapsed();
    let rep = fleet.shared.report.lock().unwrap().clone();
    // Every bot decided once; the ones whose join bell has come (≤ 40 at
    // the fixture's bell) joined.
    assert_eq!(rep.steps, 1_000);
    let due = r.iter().filter(|s| s.join_bell <= fixture::BELL).count();
    let joins = fleet
        .shared
        .relay
        .log
        .lock()
        .unwrap()
        .posts
        .iter()
        .filter(|p| p.1 == tag::JOIN)
        .count();
    assert_eq!(joins, due);
    assert_eq!(sent, due);
    eprintln!("W3-E measurement: 1,000 bots, one step each over the fixture herald (dir) and the mock relay: {dt:?}, {joins} joins");
    assert!(dt.as_secs() < 60, "{dt:?}");
    // Sealing throughput on the shared pool.
    let pool = frontier_bots::seal::SealPool::default_size();
    let key = TestKey::new();
    let p = Plain {
        version: 1,
        host_id: 1,
        arrive_bell: 50,
        dest_p: 2,
        dest_q: 0,
        dest_tile: 3,
        stance: 0,
        retreat_bps: 0,
        path_len: 1,
        path: fs::path_of(&[0]),
        reserved: [0; 3],
    };
    let t1 = Instant::now();
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..200 {
        let pool = pool.clone();
        let pk = key.pk96;
        set.spawn(async move { pool.seal(SealKind::Honest, p, pk, 1_000).await.unwrap() });
    }
    let mut n = 0;
    while let Some(x) = set.join_next().await {
        let s = x.unwrap();
        assert_eq!(s.seal.len(), 165);
        n += 1;
    }
    eprintln!(
        "W3-E measurement: 200 tlock seals on the shared pool: {:?}",
        t1.elapsed()
    );
    assert_eq!(n, 200);
    let _ = b64_encode;
}

/// The HTTP transports against a loopback server on an ephemeral port
/// (§3.5: tests bind 127.0.0.1:0).
#[tokio::test]
async fn http_ports_round_trip_on_loopback() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(vec![]));
    let seen2 = seen.clone();
    let server = tokio::spawn(async move {
        loop {
            let (mut s, _) = l.accept().await.unwrap();
            // Read the head, then the body by Content-Length.
            let mut buf = vec![];
            let mut chunk = [0u8; 4_096];
            loop {
                let n = s.read(&mut chunk).await.unwrap();
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if buf.len() >= end + 4 + len {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let req = String::from_utf8_lossy(&buf).to_string();
            let line = req.lines().next().unwrap_or("").to_string();
            seen2.lock().unwrap().push(req.clone());
            let (status, body) = if line.starts_with("GET /h/season") {
                (
                    200,
                    std::fs::read(fixture::dir().join("h/season.json")).unwrap(),
                )
            } else if line.starts_with("GET /h/me/") {
                (404, b"{\"error\":\"unknown\"}".to_vec())
            } else if line.starts_with("POST /f/reveal") {
                (410, b"{\"code\":\"WindowClosed\"}".to_vec())
            } else {
                (200, b"{\"feePayer\":\"x\"}".to_vec())
            };
            let head = format!(
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            s.write_all(head.as_bytes()).await.unwrap();
            s.write_all(&body).await.unwrap();
        }
    });
    use frontier_bots::ports::{HeraldPort, HttpHerald, HttpRelay};
    let base = format!("http://127.0.0.1:{port}");
    let h = HttpHerald::new(&base);
    assert!(h.get("/h/season").await.unwrap().is_some());
    assert!(
        h.get("/h/me/abc").await.unwrap().is_none(),
        "404 = not joined"
    );
    let r = HttpRelay::new(&base);
    let a = r.post("/f/reveal", &json!({"x": 1})).await.unwrap();
    assert_eq!((a.status, a.code().as_deref()), (410, Some("WindowClosed")));
    assert!(!a.ok());
    let g = r.get("/f/relay").await.unwrap();
    assert!(g.ok());
    let reqs = seen.lock().unwrap().clone();
    assert!(reqs
        .iter()
        .any(|q| q.starts_with("POST /f/reveal") && q.contains("{\"x\":1}")));
    server.abort();
}

#[tokio::test]
async fn a_march_settled_by_someone_else_is_reconciled() {
    let sh = shared::<NoDirect>(MockRelay::new(presets()), None);
    let mut bot = Bot::new(spec(FINAL, Arch::Daily, None), SEED);
    // Arrived at bell 30; the fixture holding has no transit for it.
    bot.mem.marches.push(memo_for(30, SealKind::Honest, true));
    // An old settled march is forgotten.
    let mut old = memo_for(30, SealKind::Honest, true);
    old.key.1 = 1;
    old.arrive_bell = 0;
    old.settled = true;
    bot.mem.marches.push(old);
    // Still in transit in the fixture (arrived at 38, not settled).
    bot.mem.marches.push(memo_for(
        fixture::IN_TRANSIT_ARRIVE,
        SealKind::Honest,
        false,
    ));
    bot.step(&sh, false).await;
    assert_eq!(bot.mem.marches.len(), 3, "arrive 0 + 288 > 40: kept");
    let settled: Vec<bool> = bot.mem.marches.iter().map(|m| m.settled).collect();
    assert_eq!(settled, vec![true, true, false]);
}

/// integ-W6t review: a settle racer polls between its arrival and the
/// settlement (its window is a few slots, mid-bell); other bots and a racer
/// that already re-departed do not.
#[test]
fn the_settle_racer_polls_through_its_race() {
    use frontier_bots::fleet::settle_racer_polls;
    let a = fixture::IN_TRANSIT_ARRIVE;
    let mut bot = Bot::new(spec(FINAL, Arch::Bot, Some(Persona::SettleRacer)), SEED);
    bot.mem.marches.push(memo_for(a, SealKind::Garbage, false));
    assert!(!settle_racer_polls(&bot, a - 1), "before its arrival");
    assert!(
        !settle_racer_polls(&bot, a),
        "the resolve comes after the close"
    );
    assert!(settle_racer_polls(&bot, a + 1));
    assert!(settle_racer_polls(&bot, a + 4));
    assert!(!settle_racer_polls(&bot, a + 5), "the race is long over");
    bot.mem.marches[0].redeparted = true;
    assert!(!settle_racer_polls(&bot, a + 1));
    let mut other = Bot::new(spec(FINAL, Arch::Bot, Some(Persona::LateRevealer)), SEED);
    other.mem.marches.push(memo_for(a, SealKind::Honest, false));
    assert!(!settle_racer_polls(&other, a + 1));
}

#[tokio::test]
async fn the_settle_racer_redeparts_then_settles_at_the_first_instant() {
    let sh = shared::<NoDirect>(MockRelay::new(presets()), None);
    // One bell later: close(38) + 600 has passed, the destination resolved.
    sh.clock.set(NOW + 700);
    let mut bot = Bot::new(spec(FINAL, Arch::Bot, Some(Persona::SettleRacer)), SEED);
    bot.mem.marches.push(memo_for(
        fixture::IN_TRANSIT_ARRIVE,
        SealKind::Garbage,
        false,
    ));
    bot.step(&sh, false).await;
    let rep = sh.report.lock().unwrap().clone();
    assert_eq!(
        rep.verdict(Persona::SettleRacer),
        Verdict::Observed,
        "{:#}",
        rep.to_json()
    );
    let log = sh.relay.log.lock().unwrap();
    // The settle shape carried the requester's signature and citizen, and
    // the logged commit and seal.
    assert_eq!(log.settles.len(), 1);
    let b = &log.settles[0];
    let a = fixture::addresses();
    assert_eq!(
        b["citizen"],
        json!(a
            .citizen(&frontier_agents::keys::wallet(SEED, FINAL).pubkey())
            .to_string())
    );
    assert_eq!(
        b["requester"],
        json!(frontier_agents::keys::session(SEED, FINAL)
            .pubkey()
            .to_string())
    );
    let t = fclient::tx::from_wire(&b64(b["tx"].as_str().unwrap()).unwrap()).unwrap();
    let ix = &t.message.instructions[3];
    assert_eq!(ix.data[0], tag::SETTLE_TRANSIT);
    let m = &bot.mem.marches[0];
    assert_eq!(&ix.data[2..34], &m.commit);
    assert_eq!(&ix.data[34..199], m.seal.as_slice());
    assert!(m.settled && m.redeparted);
    // The redepart names the province the host stands in after its
    // arrival — the destination — not its origin (W4-F).
    let rd = log
        .in_transit
        .first()
        .expect("the redepart reached the relay");
    let rix = &rd.message.instructions[3];
    let w0 = fixture::world();
    assert_eq!(
        rd.message.account_keys[rix.accounts[5] as usize],
        a.province(w0.enemy_home.0 as i32, w0.enemy_home.1 as i32)
    );
    // Wave-3 review (W3-E): the slot, its beneficiary and the resolver are
    // the arrival bell's (per-bell envelope), not the latest bell's.
    let w = fixture::world();
    let key = |i: usize| t.message.account_keys[ix.accounts[i] as usize];
    assert_eq!(
        key(5),
        a.arrival_slot(
            w.enemy_home.0 as i32,
            w.enemy_home.1 as i32,
            fixture::IN_TRANSIT_ARRIVE,
            0,
            fixture::IN_TRANSIT_SLOT_I
        ),
        "the arrival's slot"
    );
    assert_eq!(
        key(8),
        fclient::Address::new_from_array(fixture::SLOT_BENEFICIARY)
    );
    assert_eq!(key(9), fclient::Address::new_from_array(fixture::RESOLVER));
}

/// W6T-3 (w6-s7 R4): the marchbook said `revealed` for 27 marches whose
/// every Reveal the program refused (`Shielded`), because the relay's 202
/// was taken for a landing. Now a 2xx journals `accepted`; `revealed` only
/// when the herald shows the march's REVEAL (its ArrivalSlot or ClashInputs
/// record); a march settled without one is in `report.json`'s
/// `unrevealed` with its route and last refusal code.
#[tokio::test]
async fn revealed_only_on_observed_reveal() {
    let dir = tmp("revealed-observed");
    let jp = dir.join("marchbook.jsonl");
    let mut sh = shared::<NoDirect>(MockRelay::new(presets()), None);
    sh.journal = Some(Journal::open(&jp).unwrap());
    let states = |jp: &std::path::Path| -> Vec<(String, u32)> {
        std::fs::read_to_string(jp)
            .unwrap()
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .map(|v| {
                (
                    v["ev"].as_str().unwrap().to_string(),
                    v["depart_bell"].as_u64().unwrap() as u32,
                )
            })
            .collect()
    };
    // (a) In its arrival bell: the owner's POST is answered 202.
    let mut bot = Bot::new(spec(FINAL, Arch::Daily, None), SEED);
    let mut a = memo_for(fixture::BELL, SealKind::Honest, false);
    a.key.1 = 31;
    bot.mem.marches.push(a);
    bot.step(&sh, false).await;
    let m = bot.mem.march((bot.mem.marches[0].key.0, 31)).unwrap();
    assert!(m.accepted && !m.revealed);
    let st = states(&jp);
    assert!(st.contains(&("accepted".into(), 31)), "{st:?}");
    assert!(!st.contains(&("revealed".into(), 31)), "{st:?}");
    // (b) The herald shows the REVEAL of the march arriving at 38 (its
    // ArrivalSlot in the bell-38 envelope): now it is revealed.
    let mut bot = Bot::new(spec(FINAL, Arch::Daily, None), SEED);
    let mut b = memo_for(fixture::IN_TRANSIT_ARRIVE, SealKind::Honest, false);
    b.key.1 = 32;
    b.accepted = true;
    bot.mem.marches.push(b);
    bot.step(&sh, false).await;
    assert!(bot.mem.marches[0].revealed);
    assert!(states(&jp).contains(&("revealed".into(), 32)));
    // (c) Refused (the keeper's 409 passed through by the relay) and
    // settled by someone else with no REVEAL seen: listed unrevealed.
    let mut relay = MockRelay::new(presets());
    relay.shielded = true;
    let mut sh2 = shared::<NoDirect>(relay, None);
    sh2.journal = Some(Journal::open(&jp).unwrap());
    let mut bot = Bot::new(spec(FINAL, Arch::Daily, None), SEED);
    let mut c = memo_for(fixture::BELL, SealKind::Honest, false);
    c.key.1 = 33;
    bot.mem.marches.push(c);
    bot.step(&sh2, false).await;
    let m = &bot.mem.marches[0];
    assert!(!m.accepted && !m.revealed);
    assert_eq!(m.last_code.as_deref(), Some("Shielded"));
    // Journalled with its code (the fold is `journal::tests`').
    let refused = std::fs::read_to_string(&jp)
        .unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["ev"] == "reveal_refused" && v["depart_bell"] == 33)
        .expect("reveal_refused line");
    assert_eq!(refused["code"], "Shielded");
    assert_eq!(refused["route"], "keeper");
    assert!(!states(&jp).contains(&("revealed".into(), 33)));
    // Settled at bell 30 with no transit left and no REVEAL seen.
    let mut bot = Bot::new(spec(FINAL, Arch::Daily, None), SEED);
    let mut d = memo_for(30, SealKind::Honest, true);
    d.reveal_tries = 2;
    d.last_code = Some("Shielded".into());
    bot.mem.marches.push(d);
    bot.step(&sh2, false).await;
    assert!(bot.mem.marches[0].settled);
    let rep = sh2.report.lock().unwrap().to_json();
    let u = rep["unrevealed"].as_array().unwrap();
    assert_eq!(u.len(), 1, "{rep}");
    assert_eq!(u[0]["arrive"], 30);
    assert_eq!(u[0]["route"], "keeper");
    assert_eq!(u[0]["last_code"], "Shielded");
    assert_eq!(u[0]["seal"], "honest");
    assert_eq!(rep["unrevealed_honest"], 1);
    let _ = std::fs::remove_dir_all(&dir);
}
