//! The keeper's payers (M1 contract §8.2, I-49; DESIGN §6.4).
//!
//! Two pools per keeper fleet, both derived from one master seed
//! (`fclient::payers::derive`): **reveal** (class W, N ≥ 150) and **delay**
//! (classes D and N, N ≥ 32), plus **≥ 4 funders**. Every transaction
//! version draws its fee payer uniformly at random (OS CSPRNG) among the
//! pool's payers above the floor — no round-robin, so no payer is
//! predictable for a critical write. The fee payer also pays any rent the
//! write creates and is the account's `rent_to`, so the pool refills as
//! anchors, caches, slots and inputs close (I-49). Balances are a cache,
//! refreshed at rest and debited optimistically by each version's fee.
//! Below an effective N of 150 the keeper alerts and tops up; it never
//! stops sending W writes (the exclusion C4 guards against).

use std::collections::HashMap;

use rand::Rng;
use solana_address::Address;
use solana_keypair::Keypair;
use solana_signer::Signer;

use fclient::abi::Class;
use fclient::budgets::Budgets;
use fclient::payers::{self, Funders, Pool, PoolError, Transfer, DELAY_POOL, REVEAL_POOL};
use fclient::ports::{ChainPort, PortResult};

use crate::config::KeeperConfig;

pub struct Payers {
    pub reveal: Pool,
    pub delay: Pool,
    pub funders: Funders,
    balances: HashMap<Address, u64>,
    pub refreshed_slot: Option<u64>,
    /// Keys that sign as fixed payers besides the pools (the claim key of
    /// ClaimDefence: the beneficiary, W4-C).
    pub extra: Vec<Keypair>,
}

/// Write locks of the Reveal the floor is priced at: the first reveal of a
/// province-bell writes the ArrivalSlot, the ArrivalDay and the payer (the
/// larger cost; every in-play Reveal of the M1 runs so far was one).
pub const REVEAL_FLOOR_WRITE_LOCKS: u8 = 3;

impl Payers {
    /// Derives the pools of `cfg` from `master` with the canonical budgets
    /// table (`frontier-abi/vectors/budgets.json`). The reveal floor is
    /// `F_r` from R99 (§8.2) unless configured; ceilings are 2 × floor.
    pub fn new(master: &[u8; 32], cfg: &KeeperConfig) -> Result<Payers, PoolError> {
        Payers::with_budgets(master, cfg, &Budgets::canonical())
    }

    /// [`Payers::new`] with the budgets table the keeper sends with: the
    /// reveal floor's `fee(P_def, reveal)` is priced at the Reveal the
    /// keeper requests — its CU limit and `L(reveal)` from `budgets`
    /// (26,500 and 1,146,880 B on the canonical table) with
    /// [`REVEAL_FLOOR_WRITE_LOCKS`] — not at the §5.5 gate and the 1-MiB
    /// placeholder (integ-W6 review, W6-E F1: that floor ran ≈ 4,900
    /// lamports per payer low at R99 ≤ 150).
    pub fn with_budgets(
        master: &[u8; 32],
        cfg: &KeeperConfig,
        budgets: &Budgets,
    ) -> Result<Payers, PoolError> {
        let reveal = budgets.get(fclient::abi::tag::REVEAL);
        let reveal_cost = fclient::fees::cost(
            reveal.cu_limit,
            1,
            REVEAL_FLOOR_WRITE_LOCKS,
            reveal.loaded_limit,
        );
        let fr = cfg.reveal_floor.unwrap_or_else(|| {
            payers::reveal_floor(cfg.r99_reveals, cfg.reveal_pool.max(1), reveal_cost)
        });
        Ok(Payers {
            reveal: Pool::new(
                master,
                REVEAL_POOL,
                cfg.reveal_pool,
                fr,
                fr.saturating_mul(2),
                cfg.dev,
            )?,
            delay: Pool::new(
                master,
                DELAY_POOL,
                cfg.delay_pool,
                cfg.delay_floor,
                cfg.delay_floor.saturating_mul(2),
                cfg.dev,
            )?,
            funders: Funders::new(master, cfg.funders, cfg.dev)?,
            balances: HashMap::new(),
            refreshed_slot: None,
            extra: vec![],
        })
    }

    /// The pool a class draws from: W → reveal; D, N (and O in tests) → delay.
    pub fn pool(&self, class: Class) -> &Pool {
        match class {
            Class::W => &self.reveal,
            _ => &self.delay,
        }
    }

    pub fn balance(&self, a: &Address) -> u64 {
        self.balances.get(a).copied().unwrap_or(0)
    }

    /// Every payer and funder address.
    pub fn all_addresses(&self) -> Vec<Address> {
        let mut v = self.reveal.addresses();
        v.extend(self.delay.addresses());
        v.extend(self.funders.keys.iter().map(|k| k.pubkey()));
        v
    }

    /// Re-reads every balance.
    pub async fn refresh<P: ChainPort>(&mut self, port: &P, slot: u64) -> PortResult<()> {
        let keys = self.all_addresses();
        let got = port.accounts(&keys, 0).await?;
        for (k, a) in keys.iter().zip(got) {
            self.balances.insert(*k, a.map(|a| a.lamports).unwrap_or(0));
        }
        self.refreshed_slot = Some(slot);
        Ok(())
    }

    /// One uniform draw for a version of a `class` write (OS CSPRNG).
    pub fn draw(&self, class: Class) -> &Keypair {
        self.draw_with(class, &mut rand::rngs::OsRng)
    }

    pub fn draw_with<R: Rng + ?Sized>(&self, class: Class, rng: &mut R) -> &Keypair {
        let bal = |a: &Address| self.balance(a);
        self.pool(class).draw(&bal, rng)
    }

    /// Payers above the floor (reported per bell; E5 criterion 4 for the reveal pool).
    pub fn effective_n(&self, class: Class) -> usize {
        let bal = |a: &Address| self.balance(a);
        self.pool(class).effective_n(&bal)
    }

    /// Debits a version's fee (and any rent it may create) until the next refresh.
    pub fn note_spend(&mut self, payer: &Address, lamports: u64) {
        let b = self.balances.entry(*payer).or_default();
        *b = b.saturating_sub(lamports);
    }

    /// The keypair of a payer or funder.
    pub fn keypair(&self, a: &Address) -> Option<&Keypair> {
        self.reveal
            .keys
            .iter()
            .chain(self.delay.keys.iter())
            .chain(self.funders.keys.iter())
            .chain(self.extra.iter())
            .find(|k| k.pubkey() == *a)
    }

    /// Top-ups and sweeps for both pools (class N, at rest).
    pub fn care_plan(&self) -> Vec<Transfer> {
        let bal = |a: &Address| self.balance(a);
        let mut rng = rand::rngs::OsRng;
        let mut v = payers::plan_care(&self.reveal, &self.funders, &bal, &mut rng);
        v.extend(payers::plan_care(
            &self.delay,
            &self.funders,
            &bal,
            &mut rng,
        ));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn cfg(dev: bool, reveal: usize, delay: usize) -> KeeperConfig {
        let mut c = KeeperConfig::new(
            Address::new_from_array([1; 32]),
            1,
            Address::new_from_array([2; 32]),
        );
        c.reveal_pool = reveal;
        c.delay_pool = delay;
        c.dev = dev;
        c
    }

    #[test]
    fn pool_minimums_hold_unless_dev() {
        let m = [3u8; 32];
        assert!(Payers::new(&m, &cfg(false, 150, 32)).is_ok());
        assert!(Payers::new(&m, &cfg(false, 149, 32)).is_err());
        assert!(Payers::new(&m, &cfg(false, 150, 31)).is_err());
        let mut c = cfg(false, 150, 32);
        c.funders = 3;
        assert!(Payers::new(&m, &c).is_err(), "≥ 4 funders");
        assert!(Payers::new(&m, &cfg(true, 4, 2)).is_ok());
        let p = Payers::new(&m, &cfg(false, 150, 32)).unwrap();
        // F_r = 3 × ⌈4,000 / 150⌉ × (rent(slot) 1,463,040 + rent(day) 1,137,920
        // + fee(2.0, cost(26,500, 1 sig, 3 locks, 1,146,880 B) = 28,400) 54,300)
        // = 81 × 2,655,260 (DESIGN §22.4's per-reveal front).
        assert_eq!(p.reveal.floor, 215_076_060, "F_r at R99 = 4,000, N = 150");
        assert_eq!(p.reveal.ceiling, 2 * p.reveal.floor);
        assert_eq!(p.all_addresses().len(), 150 + 32 + 4);
    }

    /// The floor follows the budgets table the keeper sends with (W6-E F1):
    /// the canonical Reveal row (26,500 CU, `L` 1,146,880 B), not the §5.5
    /// gate at the 1-MiB placeholder, and a `budgets_file` moves it.
    #[test]
    fn reveal_floor_is_priced_at_the_requested_reveal() {
        use fclient::budgets::Budget;
        use fclient::fees;
        use fclient::payers::{reveal_floor, R99_DEFAULT};
        let m = [3u8; 32];
        let canonical = Budgets::canonical().get(fclient::abi::tag::REVEAL);
        assert_eq!(
            (canonical.cu_limit, canonical.loaded_limit),
            (26_500, 1_146_880)
        );
        let p = Payers::new(&m, &cfg(false, 150, 32)).unwrap();
        assert_eq!(
            p.reveal.floor,
            reveal_floor(R99_DEFAULT, 150, fees::cost(26_500, 1, 3, 1_146_880))
        );
        let placeholder = reveal_floor(
            R99_DEFAULT,
            150,
            fees::cost(26_000, 1, 2, fees::DEFAULT_LOADED_LIMIT),
        );
        assert_eq!(placeholder, 214_942_572, "the pre-fix floor");
        assert_eq!(p.reveal.floor - placeholder, 81 * (54_300 - 52_652));
        let mut b = Budgets::placeholder();
        b.set(
            fclient::abi::tag::REVEAL,
            Budget {
                cu_limit: 30_000,
                loaded_limit: 1_146_880,
            },
        );
        let q = Payers::with_budgets(&m, &cfg(false, 150, 32), &b).unwrap();
        assert_eq!(
            q.reveal.floor,
            reveal_floor(R99_DEFAULT, 150, fees::cost(30_000, 1, 3, 1_146_880))
        );
        assert!(q.reveal.floor > p.reveal.floor);
    }

    /// χ² over the delay pool: 32 payers, 32,000 draws, one below the floor.
    #[test]
    fn delay_pool_draws_are_uniform_and_skip_the_broke() {
        let m = [4u8; 32];
        let mut p = Payers::new(&m, &cfg(false, 150, 32)).unwrap();
        for k in p.all_addresses() {
            p.balances.insert(k, 1_000_000_000);
        }
        let broke = p.delay.keys[5].pubkey();
        p.balances.insert(broke, 0);
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let mut hits: HashMap<Address, u32> = HashMap::new();
        let n = 31 * 1_000;
        for _ in 0..n {
            *hits
                .entry(p.draw_with(Class::D, &mut rng).pubkey())
                .or_default() += 1;
        }
        assert!(!hits.contains_key(&broke));
        assert_eq!(hits.len(), 31);
        assert!(hits.keys().all(|k| p.delay.addresses().contains(k)));
        let e = n as f64 / 31.0;
        let chi2: f64 = hits.values().map(|&h| (h as f64 - e).powi(2) / e).sum();
        // 30 dof, p = 0.001 critical value 59.70.
        assert!(chi2 < 59.70, "χ² {chi2}");
        assert_eq!(p.effective_n(Class::D), 31);
        // W draws never come from the delay pool.
        for _ in 0..100 {
            let k = p.draw_with(Class::W, &mut rng).pubkey();
            assert!(p.reveal.addresses().contains(&k));
        }
    }
}
