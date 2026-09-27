//! March seals for Depart, Reveal and SettleTransit tests (W3-B, W4-B use
//! them; G10 compares the program's seal code with the stock opener's).
//!
//! - [`Vector`]: the S-TLOCK q3/q4 vectors W1-F shipped
//!   (`fclient/fixtures/tlock`), sealed by tlock-js to real quicknet rounds
//!   with the recorded signatures.
//! - [`SealKit`]: 165-B seals built with the stock `tlock =0.0.10` crate to
//!   any round of a [`Beacons`] source, one per seal code 0–5 (valid, FO
//!   failure, bad point, commitment mismatch, bad plaintext; code 3 "wrong
//!   round" opens as an FO failure off chain, fclient `judge`), with the
//!   judgement the stock opener gives.

use fclient::seal::{self as fs, Plain, Sealed, PLAIN_LEN, SEAL_LEN};
use serde_json::Value;

use crate::fixtures::Beacons;

/// One S-TLOCK vector: a real quicknet round, its signature, a tlock-js
/// ciphertext on a 32-byte key (`ct`, 197 B: the S-TLOCK form) and its
/// commitment, the previous round's signature (wrong-round case) and, for
/// q4, the M1 compact form on a 16-byte key (`ct16`: the 128-B IBE block
/// and a 37-B body, 165 B like a Depart seal).
#[derive(Clone, Debug)]
pub struct Vector {
    pub name: &'static str,
    pub round: u64,
    pub sig48: [u8; 48],
    pub ct: Vec<u8>,
    pub commit: [u8; 32],
    pub ct16: Option<Vec<u8>>,
    pub prev_round_sig: Option<[u8; 48]>,
}

fn hex_arr<const N: usize>(v: &Value, k: &str) -> Option<[u8; N]> {
    hex::decode(v.get(k)?.as_str()?).ok()?.try_into().ok()
}

impl Vector {
    /// `q3` and `q4` from fclient's fixture directory.
    pub fn all() -> Vec<Vector> {
        ["q3", "q4"]
            .iter()
            .map(|n| {
                let p = fs::fixture_dir().join(format!("{n}-vector.json"));
                let v: Value = serde_json::from_str(
                    &std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())),
                )
                .expect("json");
                Vector {
                    name: if *n == "q3" { "q3" } else { "q4" },
                    round: v["round"].as_u64().expect("round"),
                    sig48: hex_arr(&v, "sig").expect("sig"),
                    ct: hex::decode(v["ct"].as_str().expect("ct")).expect("hex"),
                    commit: hex_arr(&v, "commit").expect("commit"),
                    ct16: v
                        .get("ct16")
                        .and_then(|x| x.as_str())
                        .map(|h| hex::decode(h).expect("hex")),
                    prev_round_sig: hex_arr(&v, "prev_round_sig"),
                }
            })
            .collect()
    }
}

/// What a seal is meant to be judged as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SealCase {
    /// Code 0.
    Valid,
    /// Code 1: sealed to another round (FO check fails with T(arrive)'s signature).
    WrongRound,
    /// Code 1: a byte of V or W flipped (FO check fails).
    TamperedVW,
    /// Code 2: U is not a valid compressed G2 point.
    Garbage,
    /// Code 4: the logged commitment does not match the opened plaintext.
    CommitMismatch,
    /// Code 5: a valid seal of a plaintext `Plain::validate` refuses.
    BadPlaintext,
}

impl SealCase {
    pub const ALL: [SealCase; 6] = [
        SealCase::Valid,
        SealCase::WrongRound,
        SealCase::TamperedVW,
        SealCase::Garbage,
        SealCase::CommitMismatch,
        SealCase::BadPlaintext,
    ];
    /// The code the stock opener gives (fclient `judge`).
    pub const fn code(self) -> u8 {
        match self {
            SealCase::Valid => 0,
            SealCase::WrongRound | SealCase::TamperedVW => 1,
            SealCase::Garbage => 2,
            SealCase::CommitMismatch => 4,
            SealCase::BadPlaintext => 5,
        }
    }
}

/// A seal with the commitment the Depart logs and the judgement.
#[derive(Clone, Debug)]
pub struct MadeSeal {
    pub case: SealCase,
    pub plain: [u8; PLAIN_LEN],
    pub seal: [u8; SEAL_LEN],
    /// The commitment Depart carries (for CommitMismatch: another plaintext's).
    pub commit: [u8; 32],
    pub salt: [u8; 32],
    pub ct_hash: [u8; 32],
    pub seal_root: [u8; 32],
}

/// Builds seals to the rounds of a beacon source.
pub struct SealKit<'a> {
    pub beacons: &'a Beacons,
}

impl<'a> SealKit<'a> {
    pub fn new(beacons: &'a Beacons) -> SealKit<'a> {
        SealKit { beacons }
    }

    /// A seal of `plain` to `round` shaped as `case`, with a fixed key per
    /// (case, round) so fixtures are reproducible.
    pub fn make(&self, case: SealCase, plain: &Plain, round: u64) -> MadeSeal {
        let pk = self.beacons.pk96();
        let k: [u8; 16] =
            crate::sha256(&[b"PSF-SVM-SEAL-KEY", &[case as u8], &round.to_le_bytes()])[..16]
                .try_into()
                .expect("16");
        let mut p = *plain;
        if case == SealCase::BadPlaintext {
            p.retreat_bps = fs::RETREAT_MAX_BPS + 1;
        }
        let pt = fs::pack(&p);
        let seal_round = if case == SealCase::WrongRound {
            round + 1
        } else {
            round
        };
        let s: Sealed = fs::seal_with_key(&pt, &pk, seal_round, &k).expect("tlock seal");
        let mut seal = s.seal;
        let mut commit = s.commit;
        match case {
            SealCase::TamperedVW => seal[100] ^= 0x01, // inside V
            SealCase::Garbage => {
                seal[..96].fill(0xAB);
            }
            SealCase::CommitMismatch => {
                let mut other = p;
                other.stance = (other.stance + 1) % fs::STANCES;
                commit = fs::commit(&fs::pack(&other), &s.salt);
            }
            _ => {}
        }
        let ct_hash = fs::ct_hash(&seal);
        MadeSeal {
            case,
            plain: pt,
            seal,
            commit,
            salt: s.salt,
            ct_hash,
            seal_root: fs::seal_root(&commit, &ct_hash),
        }
    }

    /// The stock opener's code for `m` opened with `round`'s signature.
    pub fn judge(&self, m: &MadeSeal, round: u64, host_id: u64, arrive_bell: u32) -> u8 {
        let b = self
            .beacons
            .beacon(round)
            .expect("the round is available from this source");
        fs::judge(&m.seal, &m.commit, &b.sig48, host_id, arrive_bell).0
    }
}

/// A plaintext that passes `Plain::validate` for `(host_id, arrive_bell)`.
pub fn sample_plain(host_id: u64, arrive_bell: u32) -> Plain {
    Plain {
        version: 1,
        host_id,
        arrive_bell,
        dest_p: 3,
        dest_q: -2,
        dest_tile: 30,
        stance: 1,
        retreat_bps: 0,
        path_len: 4,
        path: fs::path_of(&[0, 1, 2, 3]),
        reserved: [0; 3],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s_tlock_vectors_open_with_their_round_only() {
        let v = Vector::all();
        assert_eq!(v.len(), 2);
        assert!(
            v.iter().any(|x| x.ct16.is_some()),
            "q4 carries the compact form"
        );
        for x in &v {
            assert!(fclient::beacon::verify(
                x.round,
                &x.sig48,
                &fclient::beacon::quicknet_info().public_key
            ));
            let Some(ct16) = &x.ct16 else { continue };
            assert_eq!(ct16.len(), SEAL_LEN);
            assert!(fs::open_ibe(ct16, &x.sig48).is_ok(), "{} opens", x.name);
            if let Some(prev) = x.prev_round_sig {
                assert_eq!(
                    fs::open_ibe(ct16, &prev),
                    Err(fs::OpenError::FoCheck),
                    "{}: the previous round's signature fails the FO check",
                    x.name
                );
            }
        }
    }

    #[test]
    fn every_seal_case_is_judged_with_its_code() {
        let b = Beacons::test_key();
        let kit = SealKit::new(&b);
        let (host, arrive, round) = (0x0000_1234_0500_0007u64, 77u32, 40_000_000u64);
        let p = sample_plain(host, arrive);
        assert_eq!(fs::validate(&p, host, arrive), Ok(()));
        for case in SealCase::ALL {
            let m = kit.make(case, &p, round);
            assert_eq!(kit.judge(&m, round, host, arrive), case.code(), "{case:?}");
            assert_eq!(m.seal_root, fs::seal_root(&m.commit, &fs::ct_hash(&m.seal)));
        }
    }
}
