//! Keeper payer pools (M1 contract §8.2, I-49; DESIGN §6.4).
//!
//! `payer_i = ed25519(sha256("PS-FRONTIER-PAYER-v1" ‖ master_seed ‖ pool_id ‖ le32(i)))`.
//! Two pools per keeper fleet: `reveal` (class W, N ≥ 150) and `delay`
//! (classes D and N, N ≥ 32); both refuse smaller sizes unless `dev`. Each
//! transaction version draws a payer **uniformly at random** from the pool's
//! payers above the floor (OS CSPRNG, no round-robin). The payer pays rent
//! and gets it back when the account closes, so a pool refills itself;
//! **≥ 4 funders** top payers up at rest and sweep the excess, never inside
//! a critical transaction.

use rand::Rng;
use sha2::{Digest, Sha256};
use solana_address::Address;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_signer::Signer;

use crate::abi::{rent, size};
use crate::fees;

pub const DOMAIN: &[u8] = b"PS-FRONTIER-PAYER-v1";
pub const REVEAL_POOL: &str = "reveal";
pub const DELAY_POOL: &str = "delay";
pub const RELAY_POOL: &str = "relay";
pub const FUNDER_POOL: &str = "funder";
pub const MIN_REVEAL: usize = 150;
pub const MIN_DELAY: usize = 32;
pub const MIN_FUNDERS: usize = 4;
/// c4 v3 p99 reveals per bell for one fleet (CL-26 default until W1-D's run).
pub const R99_DEFAULT: u64 = 4_000;
/// Delay-pool floor default (§8.2).
pub const DELAY_FLOOR_DEFAULT: u64 = 500_000_000;
/// P_def = 2.0 in milli.
pub const P_DEF_MILLI: u64 = 2_000;
/// P_delay = 0.5 in milli.
pub const P_DELAY_MILLI: u64 = 500;

/// Derives payer `i` of `pool_id`.
pub fn derive(master_seed: &[u8; 32], pool_id: &str, i: u32) -> Keypair {
    let sk: [u8; 32] = Sha256::new()
        .chain_update(DOMAIN)
        .chain_update(master_seed)
        .chain_update(pool_id.as_bytes())
        .chain_update(i.to_le_bytes())
        .finalize()
        .into();
    Keypair::new_from_array(sk)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PoolError {
    TooSmall { pool: String, n: usize, min: usize },
    FloorAboveCeiling,
}

/// The reveal floor `F_r = 3 × ⌈R99 / N⌉ × (rent(slot) + rent(day) + fee(P_def, reveal))`
/// (≈ 0.215 SOL at R99 = 4,000, N = 150).
pub fn reveal_floor(r99: u64, n: usize, reveal_cost: u64) -> u64 {
    let per = rent(size::ARRIVAL_SLOT)
        + rent(size::ARRIVAL_DAY)
        + fees::fee_for(P_DEF_MILLI, reveal_cost);
    3 * r99.div_ceil(n.max(1) as u64) * per
}

/// A pool of derived payers with its balance band.
pub struct Pool {
    pub id: String,
    pub keys: Vec<Keypair>,
    pub floor: u64,
    pub ceiling: u64,
}

impl Pool {
    /// `n` payers; refuses fewer than the pool's minimum unless `dev`.
    pub fn new(
        master_seed: &[u8; 32],
        id: &str,
        n: usize,
        floor: u64,
        ceiling: u64,
        dev: bool,
    ) -> Result<Pool, PoolError> {
        let min = match id {
            REVEAL_POOL | RELAY_POOL => MIN_REVEAL,
            DELAY_POOL => MIN_DELAY,
            FUNDER_POOL => MIN_FUNDERS,
            _ => 1,
        };
        if n < min && !dev {
            return Err(PoolError::TooSmall {
                pool: id.into(),
                n,
                min,
            });
        }
        if floor > ceiling {
            return Err(PoolError::FloorAboveCeiling);
        }
        let keys = (0..n as u32).map(|i| derive(master_seed, id, i)).collect();
        Ok(Pool {
            id: id.into(),
            keys,
            floor,
            ceiling,
        })
    }

    pub fn addresses(&self) -> Vec<Address> {
        self.keys.iter().map(|k| k.pubkey()).collect()
    }

    /// Payers above the floor.
    pub fn eligible(&self, balance: &dyn Fn(&Address) -> u64) -> Vec<usize> {
        (0..self.keys.len())
            .filter(|&i| balance(&self.keys[i].pubkey()) >= self.floor)
            .collect()
    }

    /// Effective N (payers above the floor), reported per bell (E5 criterion 4).
    pub fn effective_n(&self, balance: &dyn Fn(&Address) -> u64) -> usize {
        self.eligible(balance).len()
    }

    /// One uniform draw among the payers above the floor. When none is
    /// above it the draw is over all payers: the keeper alerts but never
    /// stops sending W writes (I-49).
    pub fn draw<R: Rng + ?Sized>(
        &self,
        balance: &dyn Fn(&Address) -> u64,
        rng: &mut R,
    ) -> &Keypair {
        let el = self.eligible(balance);
        if el.is_empty() {
            &self.keys[rng.gen_range(0..self.keys.len())]
        } else {
            &self.keys[el[rng.gen_range(0..el.len())]]
        }
    }

    /// Draw with the OS CSPRNG.
    pub fn draw_os(&self, balance: &dyn Fn(&Address) -> u64) -> &Keypair {
        self.draw(balance, &mut rand::rngs::OsRng)
    }
}

/// A transfer the payer-care loop wants to send (class N, at rest).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transfer {
    pub from: Address,
    pub to: Address,
    pub lamports: u64,
}

impl Transfer {
    pub fn instruction(&self) -> Instruction {
        crate::tx::transfer(self.from, self.to, self.lamports)
    }
}

/// Funders (≥ 4) that top up and receive sweeps, drawn at random.
pub struct Funders {
    pub keys: Vec<Keypair>,
}

impl Funders {
    pub fn new(master_seed: &[u8; 32], n: usize, dev: bool) -> Result<Funders, PoolError> {
        if n < MIN_FUNDERS && !dev {
            return Err(PoolError::TooSmall {
                pool: FUNDER_POOL.into(),
                n,
                min: MIN_FUNDERS,
            });
        }
        Ok(Funders {
            keys: (0..n as u32)
                .map(|i| derive(master_seed, FUNDER_POOL, i))
                .collect(),
        })
    }
}

/// Payer care: every payer below the floor is topped up to the band's
/// middle from a random funder that can afford it; every payer above the
/// ceiling sweeps down to the middle into a random funder.
pub fn plan_care<R: Rng + ?Sized>(
    pool: &Pool,
    funders: &Funders,
    balance: &dyn Fn(&Address) -> u64,
    rng: &mut R,
) -> Vec<Transfer> {
    let mid = pool.floor / 2 + pool.ceiling / 2;
    let mut spent = vec![0u64; funders.keys.len()];
    let mut out = vec![];
    for k in &pool.keys {
        let a = k.pubkey();
        let b = balance(&a);
        if b < pool.floor {
            let need = mid - b;
            let start = rng.gen_range(0..funders.keys.len().max(1));
            for off in 0..funders.keys.len() {
                let fi = (start + off) % funders.keys.len();
                let f = funders.keys[fi].pubkey();
                if balance(&f).saturating_sub(spent[fi]) >= need + fees::LAMPORTS_PER_SIGNATURE {
                    spent[fi] += need + fees::LAMPORTS_PER_SIGNATURE;
                    out.push(Transfer {
                        from: f,
                        to: a,
                        lamports: need,
                    });
                    break;
                }
            }
        } else if b > pool.ceiling && !funders.keys.is_empty() {
            let f = funders.keys[rng.gen_range(0..funders.keys.len())].pubkey();
            out.push(Transfer {
                from: a,
                to: f,
                lamports: b - mid,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn derivation_is_stable_and_pool_sizes_enforced() {
        let m = [7u8; 32];
        assert_eq!(
            derive(&m, REVEAL_POOL, 3).pubkey(),
            derive(&m, REVEAL_POOL, 3).pubkey()
        );
        assert_ne!(
            derive(&m, REVEAL_POOL, 3).pubkey(),
            derive(&m, DELAY_POOL, 3).pubkey()
        );
        assert!(matches!(
            Pool::new(&m, REVEAL_POOL, 149, 1, 2, false),
            Err(PoolError::TooSmall { .. })
        ));
        assert!(Pool::new(&m, REVEAL_POOL, 10, 1, 2, true).is_ok());
        assert!(matches!(
            Pool::new(&m, DELAY_POOL, 31, 1, 2, false),
            Err(PoolError::TooSmall { .. })
        ));
        assert!(Funders::new(&m, 3, false).is_err());
    }

    #[test]
    fn reveal_floor_matches_the_contract() {
        // §8.2: F_r ≈ 0.215 SOL at R99 = 4,000, N = 150, Reveal at 26k and 1 MiB.
        let f = reveal_floor(
            R99_DEFAULT,
            150,
            fees::cost(26_000, 1, 2, fees::DEFAULT_LOADED_LIMIT),
        );
        assert_eq!(f, 214_942_572);
    }

    #[test]
    fn draws_are_uniform_over_eligible_payers() {
        let m = [1u8; 32];
        let pool = Pool::new(&m, REVEAL_POOL, 150, 100, 1_000, false).unwrap();
        let broke = pool.keys[0].pubkey();
        let bal = |a: &Address| if *a == broke { 0 } else { 500 };
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let mut hits = vec![0u32; 150];
        let n = 149 * 200;
        for _ in 0..n {
            let k = pool.draw(&bal, &mut rng).pubkey();
            let i = pool.keys.iter().position(|x| x.pubkey() == k).unwrap();
            hits[i] += 1;
        }
        assert_eq!(hits[0], 0, "a payer below the floor is never drawn");
        // χ² over 149 cells, 148 dof: p = 0.001 critical value ≈ 209.
        let e = n as f64 / 149.0;
        let chi2: f64 = hits[1..].iter().map(|&h| (h as f64 - e).powi(2) / e).sum();
        assert!(chi2 < 209.0, "χ² {chi2}");
        assert_eq!(pool.effective_n(&bal), 149);
    }

    #[test]
    fn care_tops_up_and_sweeps() {
        let m = [2u8; 32];
        let pool = Pool::new(&m, DELAY_POOL, 32, 100, 300, false).unwrap();
        let fund = Funders::new(&m, 4, false).unwrap();
        let low = pool.keys[0].pubkey();
        let high = pool.keys[1].pubkey();
        let funders: Vec<Address> = fund.keys.iter().map(|k| k.pubkey()).collect();
        let bal = |a: &Address| {
            if *a == low {
                10
            } else if *a == high {
                1_000
            } else if funders.contains(a) {
                1_000_000
            } else {
                200
            }
        };
        let plan = plan_care(
            &pool,
            &fund,
            &bal,
            &mut rand::rngs::StdRng::seed_from_u64(1),
        );
        assert_eq!(plan.len(), 2);
        assert!(plan
            .iter()
            .any(|t| t.to == low && t.lamports == 190 && funders.contains(&t.from)));
        assert!(plan
            .iter()
            .any(|t| t.from == high && t.lamports == 800 && funders.contains(&t.to)));
    }
}
