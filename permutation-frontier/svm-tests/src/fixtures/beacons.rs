//! Beacon rounds for the program's BLS checks.
//!
//! - **Quicknet** (release build): the 32 real rounds SP-V2 recorded
//!   (`frontier-node/crates/fclient/fixtures/quicknet`, verified with
//!   blstrs on load). A test that needs a round the fixture lacks cannot run
//!   on the release binary; it runs on the test-beacon build, or places its
//!   season so the round it needs is a fixture round ([`Beacons::align`]).
//! - **Test key** (`test-beacon` build, I-53): any round, signed on demand by
//!   the deterministic local key `fclient::beacon::TestKey` (W1-F pinned its
//!   derivation; W2-A's feature pins its public key and `QUICKNET_PK_HASH`).
//!
//! Every round is handed out as the instruction argument
//! (`round ‖ sig48 ‖ 290-B hints`), the hints computed by fclient's port of
//! SP-V2's hash-to-curve.

use std::collections::BTreeMap;

use fclient::beacon::{self as fb, FixtureDrand, TestKey};
use fclient::ix::BeaconArg;
use fclient::ports::Beacon;
use permutation_rules::frontier::beacon as kb;
use permutation_rules::frontier::clash::QUICKNET;

use crate::chain::Build;

/// Where the rounds come from.
pub enum Beacons {
    Quicknet(BTreeMap<u64, Beacon>),
    TestKey(TestKey),
}

impl Beacons {
    /// The source the build verifies: the test key for `test-beacon`,
    /// real quicknet otherwise.
    pub fn for_build(b: Build) -> Beacons {
        if b.test_key() {
            Beacons::test_key()
        } else {
            Beacons::quicknet()
        }
    }

    /// The 32 SP-V2 rounds, verified against the quicknet key.
    pub fn quicknet() -> Beacons {
        let f = FixtureDrand::load(&fb::fixture_dir(), fb::quicknet_info())
            .expect("SP-V2 fixture rounds verify");
        Beacons::Quicknet(f.rounds)
    }

    pub fn test_key() -> Beacons {
        Beacons::TestKey(TestKey::new())
    }

    /// The group public key (96-B compressed G2).
    pub fn pk96(&self) -> [u8; 96] {
        match self {
            Beacons::Quicknet(_) => fb::quicknet_info().public_key,
            Beacons::TestKey(k) => k.pk96,
        }
    }

    /// `QUICKNET_PK_HASH = sha256(pk96)` as the build embeds it (W1-F D7:
    /// the test key's hash has the same form).
    pub fn pk_hash(&self) -> [u8; 32] {
        fb::pk_hash(&self.pk96())
    }

    /// The round's beacon, if this source has it.
    pub fn beacon(&self, round: u64) -> Option<Beacon> {
        match self {
            Beacons::Quicknet(m) => m.get(&round).cloned(),
            Beacons::TestKey(k) => Some(k.beacon(round)),
        }
    }

    /// The round as an instruction argument (with its hints).
    pub fn arg(&self, round: u64) -> Option<BeaconArg> {
        self.beacon(round).map(|b| fb::beacon_arg(&b))
    }

    /// Like [`Beacons::arg`], panicking with the fixture's limits.
    #[track_caller]
    pub fn must(&self, round: u64) -> BeaconArg {
        self.arg(round).unwrap_or_else(|| {
            panic!(
                "round {round} is not among the {} SP-V2 fixture rounds; run this test on the test-beacon build",
                self.rounds().len()
            )
        })
    }

    /// The rounds a fixture holds (empty for the test key: every round).
    pub fn rounds(&self) -> Vec<u64> {
        match self {
            Beacons::Quicknet(m) => m.keys().copied().collect(),
            Beacons::TestKey(_) => vec![],
        }
    }

    /// Whether any round is available.
    pub fn any_round(&self) -> bool {
        matches!(self, Beacons::TestKey(_))
    }

    /// The seed a round gives (`sha256("PSF-SEED-v1" ‖ net ‖ be64(round) ‖ sig96)`).
    pub fn seed(&self, round: u64) -> Option<[u8; 32]> {
        let b = self.beacon(round)?;
        let sig96 = fb::decompress_sig(&b.sig48)?;
        Some(fb::seed_of(round, &sig96))
    }

    /// A `t_create_min` whose genesis round (`first_round_from(t + 600 + Δ)`)
    /// is `round` exactly: `round_time(round) − 600 − Δ`.
    pub fn align(round: u64, margin: u32) -> i64 {
        kb::round_time(QUICKNET.genesis, QUICKNET.period as u32, round)
            - kb::SEED_LEAD_SECS
            - margin as i64
    }

    /// A round that is not signed by this source's key (the other key's
    /// signature for the same round), for `Crypto` refusals.
    pub fn forged(&self, round: u64) -> BeaconArg {
        let sig48 = match self {
            Beacons::Quicknet(_) => TestKey::new().sign(round),
            Beacons::TestKey(_) => {
                // A valid G1 point that is not this round's signature: the
                // test key's signature of the next round.
                TestKey::new().sign(round + 1)
            }
        };
        BeaconArg {
            round,
            sig48,
            hints: fb::hints_bytes(round),
        }
    }
}

/// quicknet's `round_time(r)`.
pub fn round_time(r: u64) -> i64 {
    kb::round_time(QUICKNET.genesis, QUICKNET.period as u32, r)
}

/// quicknet's `first_round_from(t)`.
pub fn first_round_from(t: i64) -> u64 {
    kb::first_round_from(QUICKNET.genesis, QUICKNET.period as u32, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quicknet_fixture_has_the_32_sp_v2_rounds() {
        let q = Beacons::quicknet();
        let r = q.rounds();
        assert_eq!(r.len(), 32);
        assert_eq!(r[0], 32_551_361);
        for x in r.iter().take(3) {
            let a = q.must(*x);
            assert_eq!(a.hints.len(), fclient::ix::HINTS_LEN);
            assert!(fb::verify(*x, &a.sig48, &q.pk96()));
        }
        assert!(q.arg(r[0] + 1).is_none());
        assert_eq!(q.pk_hash(), frontier_abi::presets::QUICKNET_PK_HASH);
    }

    #[test]
    fn test_key_signs_any_round_and_not_as_quicknet() {
        let t = Beacons::test_key();
        let a = t.must(123_456_789);
        assert!(fb::verify(123_456_789, &a.sig48, &t.pk96()));
        assert!(!fb::verify(
            123_456_789,
            &a.sig48,
            &Beacons::quicknet().pk96()
        ));
        assert_ne!(t.pk_hash(), frontier_abi::presets::QUICKNET_PK_HASH);
        let f = t.forged(5_000);
        assert!(!fb::verify(5_000, &f.sig48, &t.pk96()));
    }

    #[test]
    fn align_puts_the_genesis_round_on_a_fixture_round() {
        for r in Beacons::quicknet().rounds() {
            for margin in [60u32, 61, 62, 90] {
                let t = Beacons::align(r, margin);
                assert_eq!(kb::genesis_seed_round(&QUICKNET, t, margin), r);
            }
        }
    }
}
