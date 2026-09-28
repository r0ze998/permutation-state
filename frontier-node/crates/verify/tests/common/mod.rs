//! Shared test helpers: the recorded fixtures, and a recording harness
//! (`localnet` in process on virtual time, the test-beacon program, a
//! test-key drand gated by the chain's Clock) for `record_*` tests.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use fclient::addr::Addresses;
use fclient::beacon::TestKey;
use fclient::decode::Season;
use fclient::ports::{Beacon, ChainInfo, ChainPort, Cursor, DrandPort, PortError, TxRecord};
use fclient::{ix, tx, Address, Instruction, Keypair, Signer};
use localnet::{Config as NetConfig, InProcess};
use verify_core::input::{Config, Input};

pub const SEASON_ID: u64 = 7;
pub const DRAND_DELAY: i64 = 1;

pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/verify")
}

pub fn fixture(name: &str) -> Input {
    let p = fixtures_dir().join(name);
    Input::load(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// The recorded land season of the test-beacon program.
pub fn land() -> Input {
    fixture("land-program.json")
}

/// The recorded synthetic march season.
pub fn march() -> Input {
    fixture("march-synth.json")
}

/// The march season recorded from the merged program (`itest::inproc_day`
/// with `VERIFY_DUMP`; integ-W4 review, W4-D R5).
pub fn march_program() -> Input {
    fixture("march-program.json.gz")
}

/// A second, independent program recording (wave-5 review of W5-D): the
/// `inproc_day` W5-D recorded from the wave-base test-beacon `.so`
/// (`dc1281c3…`), kept when the fixtures were regenerated at the integ
/// head. The suite must build and detect every class on it too.
pub fn march_program_dc1281c3() -> Input {
    fixture("march-program-dc1281c3.json.gz")
}

#[derive(Clone)]
pub struct Gated {
    pub key: Arc<TestKey>,
    pub now: Arc<AtomicI64>,
}

impl DrandPort for Gated {
    fn round(
        &self,
        r: u64,
    ) -> impl std::future::Future<Output = Result<Option<Beacon>, PortError>> + Send {
        let info = self.key.info();
        let ok = info.round_time(r) + DRAND_DELAY <= self.now.load(Ordering::SeqCst);
        let b = ok.then(|| self.key.beacon(r));
        async move { Ok(b) }
    }
    fn info(&self) -> ChainInfo {
        self.key.info()
    }
}

pub struct World {
    pub ip: InProcess,
    pub program: Address,
    pub addrs: Addresses,
    pub authority: Keypair,
    pub drand: Gated,
    pub so: Vec<u8>,
}

impl World {
    pub fn now(&self) -> i64 {
        self.ip.lock().unix_timestamp()
    }
    pub fn slot(&self) -> u64 {
        self.ip.lock().slot()
    }
    pub fn step(&self) {
        self.ip.step(1);
        self.drand.now.store(self.now(), Ordering::SeqCst);
    }
    pub fn season(&self) -> Season {
        let a = self.ip.lock().account(&self.addrs.season).expect("season");
        Season::decode(&a.data).expect("season decodes")
    }
    pub fn bell(&self) -> u32 {
        let s = self.season();
        fclient::clock::bell_at(s.genesis_ts, self.now()).unwrap_or(0)
    }
    pub fn airdrop(&self, k: &Address, lamports: u64) {
        self.ip.lock().airdrop(k, lamports).unwrap();
    }
    pub async fn try_send(
        &self,
        ixs: &[Instruction],
        signers: &[&Keypair],
    ) -> Result<(), (Option<u32>, String)> {
        let (bh, _) = self.ip.blockhash().await.unwrap();
        let budget = tx::TxBudget {
            cu_limit: 1_400_000,
            cu_price: 0,
            loaded_limit: 4 * 1024 * 1024,
            heap: None,
        };
        let t = tx::build(ixs, &budget, signers, &bh).unwrap();
        let sig = self
            .ip
            .send(&tx::wire(&t))
            .await
            .map_err(|e| (None, e.to_string()))?;
        self.step();
        let st = self.ip.statuses(&[sig]).await.unwrap()[0]
            .clone()
            .expect("landed");
        match st.err {
            None => Ok(()),
            Some(e) => Err((st.code, e)),
        }
    }
    pub async fn send(&self, ixs: &[Instruction], signers: &[&Keypair]) {
        if let Err(e) = self.try_send(ixs, signers).await {
            panic!("tx failed: {e:?}");
        }
    }

    /// Every program transaction so far (feed order) and the final states
    /// of every account the program ever owned, as a fixture.
    pub async fn record(&self, provenance: &str, scenarios: &[&str]) -> Input {
        let mut txs: Vec<TxRecord> = vec![];
        let mut cur = Cursor(0);
        loop {
            let page = self.ip.feed(cur).await.unwrap();
            let Some(last) = page.last() else { break };
            cur = Cursor(last.seq);
            txs.extend(page);
        }
        // Keep the post-states of program accounts only (the fixture's
        // size); remember every key the program ever owned.
        let mut owned = std::collections::BTreeSet::new();
        for t in txs.iter_mut() {
            t.post
                .retain(|(_, a)| a.as_ref().is_some_and(|a| a.owner == self.program));
            for (k, _) in &t.post {
                owned.insert(*k);
            }
        }
        let c = self.ip.lock();
        let finals = owned
            .into_iter()
            .map(|k| {
                (
                    k.to_bytes(),
                    c.account(&k)
                        .filter(|a| a.lamports > 0 || !a.data.is_empty()),
                )
            })
            .collect();
        let so_hash: [u8; 32] = sha2_256(&self.so);
        Input {
            cfg: Config {
                program: self.program,
                season_id: SEASON_ID,
                quicknet_pk: self.drand.key.pk96,
                ruleset_hash: frontier_abi::presets::RULESET_HASH,
                program_hash: Some(so_hash),
            },
            txs,
            finals,
            final_slot: c.slot(),
            program_hashes: vec![(0, so_hash)],
            provenance: provenance.into(),
            scenarios: scenarios.iter().map(|s| s.to_string()).collect(),
        }
    }
}

pub fn sha2_256(b: &[u8]) -> [u8; 32] {
    permutation_rules::hash::sha256(&[b])
}

/// A chain with the test-beacon program deployed, a season announced and
/// created by rule (the 24-h lead at scale 2,000), its beacon logs and
/// shards; the Clock then runs at 20×.
pub async fn world(
    so_path: &str,
    f: impl FnOnce(&mut frontier_abi::presets::SeasonParams),
) -> World {
    let program = Address::new_from_array([0x5F; 32]);
    let authority = Keypair::new_from_array([0xA1; 32]);
    let so = std::fs::read(so_path).expect("PSF_FRONTIER_SO");
    let ip = InProcess::new(
        NetConfig {
            scale: 20.0,
            ..NetConfig::default()
        },
        Some(program),
    );
    {
        let mut c = ip.lock();
        let max_len = fclient::fees::deploy_max_len(so.len() as u64) as usize;
        c.deploy(program, &so, max_len, Some(authority.pubkey()))
            .unwrap();
        c.airdrop(&authority.pubkey(), 500_000_000_000).unwrap();
    }
    let key = Arc::new(TestKey::new());
    let w = World {
        drand: Gated {
            key: key.clone(),
            now: Arc::new(AtomicI64::new(0)),
        },
        addrs: Addresses::new(program, SEASON_ID),
        ip,
        program,
        authority,
        so,
    };
    w.step();
    let mut p = frontier_abi::presets::M1_LOCAL_7D;
    p.quicknet_pk_hash = fclient::beacon::pk_hash(&key.pk96);
    f(&mut p);
    let sp = p.to_bytes();
    let payout = permutation_rules::frontier::payout::PayoutParams::REV3.to_borsh();
    let ph = frontier_abi::presets::params_hash(&sp, &payout);
    let t_create_min = w.now() + 86_400 + 60;
    let auth = w.authority.pubkey();
    w.send(
        &[ix::announce_season(
            &w.addrs,
            auth,
            ph,
            t_create_min,
            1_000_000_000,
        )],
        &[&w.authority],
    )
    .await;
    w.ip.lock().set_scale(2_000.0);
    while w.now() < t_create_min {
        w.step();
    }
    w.ip.lock().set_scale(20.0);
    w.step();
    w.send(
        &[ix::create_season(&w.addrs, auth, &sp, &payout)],
        &[&w.authority],
    )
    .await;
    w.send(&[ix::init_beacon_logs(&w.addrs, auth)], &[&w.authority])
        .await;
    for fct in 0..6u8 {
        w.send(&[ix::init_shards(&w.addrs, auth, fct)], &[&w.authority])
            .await;
    }
    w
}

/// The findings of a report, one per line (assertion messages).
pub fn show(r: &verify_core::Report) -> String {
    r.findings
        .iter()
        .map(|f| {
            format!(
                "{:?} {} {} bell {} {}: {}",
                f.severity, f.check, f.code, f.bell, f.entity, f.detail
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
