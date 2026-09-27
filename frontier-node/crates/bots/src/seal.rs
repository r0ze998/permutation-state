//! Sealing a march (DESIGN §6.2, M1 contract §8.1 `seal`, §8.6): the stock
//! Rust `tlock =0.0.10` IBE through `fclient::seal`, to `tlock_round(arrive)`
//! under the drand key the herald publishes (`/h/season` → `drand.
//! publicKey`), so the test key works exactly as quicknet does (I-53).
//!
//! The adversarial seals:
//! - **garbage** (garbage_seal, settle_racer): 165 random bytes whose byte
//!   0 is a compressed-G2 flag (bit 7 set, bit 6 clear) so Depart's syntax
//!   check passes, with a *valid* commitment to a valid plaintext and a
//!   random salt; the owner can reveal it (the commitment matches), and
//!   SettleTransit's opener fails (code 1 or 2) and destroys the host;
//! - **bad plaintext** (bad_plaintext): an honest seal over a plaintext
//!   with stance 9 (seal code 5; Reveal refuses `BadPlaintext`).
//!
//! Seals are made on a shared blocking pool (one `tlock::encrypt` is
//! ≈ 1 ms [measured, S-TLOCK]; 1,000 bots share `available_parallelism`
//! workers).

use std::sync::Arc;

use fclient::seal::{self as fs, Plain, PLAIN_LEN, SEAL_LEN};
use frontier_agents::policy::SealKind;
use rand::RngCore;
use tokio::sync::Semaphore;

/// What the bot journals and later reveals or settles with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedMarch {
    /// The plaintext as sealed (invalid for `BadPlaintext`).
    pub plain: [u8; PLAIN_LEN],
    pub salt: [u8; 32],
    pub commit: [u8; 32],
    pub seal: [u8; SEAL_LEN],
    pub ct_hash: [u8; 32],
    pub seal_root: [u8; 32],
    pub round: u64,
}

/// The stance byte a bad plaintext carries (valid stances are 0–3).
pub const BAD_STANCE: u8 = 9;

/// Seals `plain` to `round` (blocking: call it on the pool).
pub fn make(
    kind: SealKind,
    plain: &Plain,
    pk96: &[u8; 96],
    round: u64,
) -> Result<SealedMarch, String> {
    match kind {
        SealKind::Honest | SealKind::BadPlaintext => {
            let mut p = *plain;
            if kind == SealKind::BadPlaintext {
                p.stance = BAD_STANCE;
            }
            let packed = fs::pack(&p);
            let s = fs::seal(&packed, pk96, round)?;
            Ok(SealedMarch {
                plain: packed,
                salt: s.salt,
                commit: s.commit,
                seal: s.seal,
                ct_hash: s.ct_hash,
                seal_root: s.seal_root,
                round,
            })
        }
        SealKind::Garbage => {
            let mut seal = [0u8; SEAL_LEN];
            let mut salt = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut seal);
            rand::rngs::OsRng.fill_bytes(&mut salt);
            seal[0] = 0x80 | (seal[0] & 0x3F);
            let packed = fs::pack(plain);
            let commit = fs::commit(&packed, &salt);
            let ct_hash = fs::ct_hash(&seal);
            Ok(SealedMarch {
                plain: packed,
                salt,
                commit,
                seal,
                ct_hash,
                seal_root: fs::seal_root(&commit, &ct_hash),
                round,
            })
        }
    }
}

/// The shared sealing pool.
#[derive(Clone)]
pub struct SealPool {
    permits: Arc<Semaphore>,
}

impl SealPool {
    pub fn new(workers: usize) -> SealPool {
        SealPool {
            permits: Arc::new(Semaphore::new(workers.max(1))),
        }
    }

    /// One worker per available core.
    pub fn default_size() -> SealPool {
        SealPool::new(std::thread::available_parallelism().map_or(2, |n| n.get()))
    }

    pub async fn seal(
        &self,
        kind: SealKind,
        plain: Plain,
        pk96: [u8; 96],
        round: u64,
    ) -> Result<SealedMarch, String> {
        let _p = self
            .permits
            .acquire()
            .await
            .map_err(|_| "seal pool closed".to_string())?;
        tokio::task::spawn_blocking(move || make(kind, &plain, &pk96, round))
            .await
            .map_err(|e| e.to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fclient::abi::seal_code;
    use fclient::beacon::TestKey;

    fn plain() -> Plain {
        Plain {
            version: 1,
            host_id: 77,
            arrive_bell: 12,
            dest_p: 2,
            dest_q: 0,
            dest_tile: 30,
            stance: 1,
            retreat_bps: 0,
            path_len: 2,
            path: fs::path_of(&[0, 1]),
            reserved: [0; 3],
        }
    }

    #[tokio::test]
    async fn seals_judge_as_the_personas_expect() {
        let key = TestKey::new();
        let round = 9_000;
        let sig = key.sign(round);
        let pool = SealPool::new(2);
        // Honest: opens with the stock opener, valid, salt = salt_of(k).
        let h = pool
            .seal(SealKind::Honest, plain(), key.pk96, round)
            .await
            .unwrap();
        let (code, p) = fs::judge(&h.seal, &h.commit, &sig, 77, 12);
        assert_eq!(code, seal_code::VALID);
        assert_eq!(p, Some(plain()));
        assert_eq!(fs::commit(&h.plain, &h.salt), h.commit);
        assert_eq!(fs::seal_root(&h.commit, &fs::ct_hash(&h.seal)), h.seal_root);
        assert_eq!(h.seal[0] & 0xC0, 0x80, "compressed G2 flag");
        // Garbage: a consistent commitment the owner could reveal, a seal
        // the opener rejects.
        let g = pool
            .seal(SealKind::Garbage, plain(), key.pk96, round)
            .await
            .unwrap();
        assert_eq!(fs::commit(&g.plain, &g.salt), g.commit);
        assert_eq!(g.seal[0] & 0xC0, 0x80, "passes Depart's syntax check");
        let (gc, _) = fs::judge(&g.seal, &g.commit, &sig, 77, 12);
        assert!(
            gc == seal_code::FO_FAILED || gc == seal_code::BAD_POINT,
            "code {gc}"
        );
        // Bad plaintext: seal code 5.
        let b = pool
            .seal(SealKind::BadPlaintext, plain(), key.pk96, round)
            .await
            .unwrap();
        assert_eq!(
            fs::judge(&b.seal, &b.commit, &sig, 77, 12).0,
            seal_code::PLAINTEXT_INVALID
        );
        assert_eq!(fs::unpack(&b.plain).stance, BAD_STANCE);
        // The wrong round's signature does not open an honest seal.
        assert_eq!(
            fs::judge(&h.seal, &h.commit, &key.sign(round + 1), 77, 12).0,
            seal_code::FO_FAILED
        );
    }
}
