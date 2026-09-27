//! The in-process world: `localnet` on virtual time with the test-beacon
//! program deployed, a season announced and created by rule (the 24-h lead
//! at scale 2,000, then 20×: 8 game seconds per 400-ms slot, I-54), and the
//! test-key drand gated by the chain's Clock (drand-replay's rule, §8.7:
//! a round is public once `round_time + delay ≤` Clock).
//!
//! The same set-up as the keeper tests' harness, owned here so the system
//! test does not depend on another crate's test files.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use fclient::addr::Addresses;
use fclient::beacon::TestKey;
use fclient::decode::Season;
use fclient::ports::{Beacon, ChainInfo, ChainPort, DrandPort, PortError};
use fclient::{ix, tx, Address, Instruction, Keypair, Signer};
use frontier_abi::presets::SeasonParams;
use localnet::{Config, InProcess};

use crate::program::So;

pub const SEASON_ID: u64 = 7;
/// Publication delay of a test-key round after its time (quicknet ≈ 0.8–2 s).
pub const DRAND_DELAY: i64 = 1;
/// The program id of the in-process runs.
pub const PROGRAM: [u8; 32] = [0x5F; 32];

/// drand's test key, released only once `round_time + delay ≤` the chain's
/// Clock.
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
    pub so: So,
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
        self.ip.lock().airdrop(k, lamports).expect("airdrop");
    }

    /// Sends one operator transaction and lands it in the next block.
    pub async fn send(&self, ixs: &[Instruction], signers: &[&Keypair]) {
        let (bh, _) = self.ip.blockhash().await.expect("blockhash");
        let budget = tx::TxBudget {
            cu_limit: 1_400_000,
            cu_price: 0,
            loaded_limit: 4 * 1024 * 1024,
            heap: None,
        };
        let t = tx::build(ixs, &budget, signers, &bh).expect("tx");
        let sig = self.ip.send(&tx::wire(&t)).await.expect("send");
        self.step();
        let st = self.ip.statuses(&[sig]).await.expect("statuses")[0]
            .clone()
            .expect("landed");
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
/// created and with its beacon logs; the Clock then runs at 20×. `f` edits
/// the season parameters before the announcement (the params hash covers
/// them).
pub async fn world(so: &So, f: impl FnOnce(&mut SeasonParams)) -> World {
    let program = Address::new_from_array(PROGRAM);
    let authority = Keypair::new_from_array([0xA1; 32]);
    let ip = InProcess::new(
        Config {
            scale: 20.0,
            ..Config::default()
        },
        Some(program),
    );
    {
        let bytes = std::fs::read(&so.path).expect("the test-beacon .so");
        let max_len = fclient::fees::deploy_max_len(bytes.len() as u64) as usize;
        let mut c = ip.lock();
        c.deploy(program, &bytes, max_len, Some(authority.pubkey()))
            .expect("deploy");
        c.airdrop(&authority.pubkey(), 500_000_000_000)
            .expect("airdrop");
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
        so: so.clone(),
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
    // The 48 JoinShards (operator, before genesis: Join and FoldOccupancy
    // read them).
    for f in 0..fclient::abi::FACTIONS {
        w.send(&[ix::init_shards(&w.addrs, auth, f)], &[&w.authority])
            .await;
    }
    assert_eq!(w.season().status, fclient::abi::status::CREATED);
    w
}
