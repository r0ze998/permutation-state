//! In-process harness: `localnet` on virtual time, the program (the
//! test-beacon `.so` when `PSF_FRONTIER_SO` names one, else the native
//! model of `tests/model`), a season announced and created by rule, and a
//! test-key drand gated by the chain's Clock.

#![allow(dead_code)]

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use fclient::addr::Addresses;
use fclient::beacon::TestKey;
use fclient::decode::Season;
use fclient::ports::{Beacon, ChainInfo, ChainPort, DrandPort, PortError};
use fclient::{ix, tx, Address, Instruction, Keypair, Signer};
use localnet::{Config, InProcess};
use solana_program_runtime::solana_sbpf::program::BuiltinFunctionDefinition;

use crate::model;

pub const SEASON_ID: u64 = 7;
/// Publication delay of a test-key round after its time (quicknet ≈ 0.8–2 s).
pub const DRAND_DELAY: i64 = 1;

/// Which program the run uses.
pub enum Program {
    Model,
    So(String),
}

impl Program {
    pub fn from_env() -> Program {
        match std::env::var("PSF_FRONTIER_SO") {
            Ok(p) if !p.is_empty() => Program::So(p),
            _ => Program::Model,
        }
    }
    pub fn label(&self) -> String {
        match self {
            Program::Model => "native model (tests/model)".into(),
            Program::So(p) => format!("test-beacon .so {p}"),
        }
    }
}

/// drand's test key, released only once `round_time + delay ≤` the chain's
/// Clock (drand-replay's rule, §8.7 v1.2).
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
    pub which: Program,
}

impl World {
    pub fn now(&self) -> i64 {
        self.ip.lock().unix_timestamp()
    }
    pub fn slot(&self) -> u64 {
        self.ip.lock().slot()
    }
    /// Produces one block and moves the drand gate to the new Clock.
    pub fn step(&self) {
        self.ip.step(1);
        self.drand.now.store(self.now(), Ordering::SeqCst);
    }
    pub fn season(&self) -> Season {
        let a = self.ip.lock().account(&self.addrs.season).expect("season");
        Season::decode(&a.data).expect("season decodes")
    }
    pub fn airdrop(&self, k: &Address, lamports: u64) {
        self.ip.lock().airdrop(k, lamports).unwrap();
    }

    /// Sends one operator transaction and lands it in the next block.
    pub async fn send(&self, ixs: &[Instruction], signers: &[&Keypair]) {
        let (bh, _) = self.ip.blockhash().await.unwrap();
        let budget = tx::TxBudget {
            cu_limit: 1_400_000,
            cu_price: 0,
            loaded_limit: 4 * 1024 * 1024,
            heap: None,
        };
        let t = tx::build(ixs, &budget, signers, &bh).unwrap();
        let sig = self.ip.send(&tx::wire(&t)).await.unwrap();
        self.step();
        let st = self.ip.statuses(&[sig]).await.unwrap()[0].clone();
        let st = st.expect("landed");
        if st.err.is_some() {
            let logs = self
                .ip
                .lock()
                .transaction(&sig)
                .map(|l| l.logs.clone())
                .unwrap_or_default();
            panic!("operator tx failed: {:?}\n{}", st.err, logs.join("\n"));
        }
    }
}

/// A chain with the program, a funded authority, and a season announced,
/// created (at scale 2,000 through the 24-h lead) and with its beacon logs;
/// the Clock then runs at 20× (8 game s per slot).
pub async fn world() -> World {
    let which = Program::from_env();
    let program = Address::new_from_array([0x5F; 32]);
    let authority = Keypair::new_from_array([0xA1; 32]);
    let ip = InProcess::new(
        Config {
            scale: 20.0,
            ..Config::default()
        },
        Some(program),
    );
    {
        let mut c = ip.lock();
        match &which {
            Program::Model => c.svm.add_builtin(program, model::ModelProgram::register),
            Program::So(p) => {
                let so = std::fs::read(p).expect("PSF_FRONTIER_SO");
                let max_len = fclient::fees::deploy_max_len(so.len() as u64) as usize;
                c.deploy(program, &so, max_len, Some(authority.pubkey()))
                    .unwrap();
            }
        }
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
        which,
    };
    w.step();
    // AnnounceSeason: t_create_min 24 h + 1 min ahead.
    let mut p = frontier_abi::presets::M1_LOCAL_7D;
    p.quicknet_pk_hash = fclient::beacon::pk_hash(&key.pk96);
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
    // The pre-season at scale 2,000 (§8.7: the 24-h lead in ≈ 43 s of slots).
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
    assert_eq!(w.season().status, fclient::abi::status::CREATED);
    w
}

/// The keeper's configuration for the runs: every region, the operator
/// roles, the contract's pool minimums (150 / 32 / 4).
pub fn keeper_config(w: &World) -> keeper_core::config::KeeperConfig {
    let mut c = keeper_core::config::KeeperConfig::new(
        w.program,
        SEASON_ID,
        Address::new_from_array([0xBE; 32]),
    );
    c.roles = vec!["beacon".into(), "rings".into(), "archive".into()];
    c
}

/// Funds the delay pool (2 SOL each) and the funders (40 SOL each); the
/// reveal pool starts empty and payer care fills it from the funders.
pub fn fund(w: &World, k: &keeper_core::pools::Payers) {
    for a in k.delay.addresses() {
        w.airdrop(&a, 2_000_000_000);
    }
    for f in &k.funders.keys {
        w.airdrop(&f.pubkey(), 40_000_000_000);
    }
}
