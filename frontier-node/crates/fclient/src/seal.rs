//! March seals (DESIGN rev 3.1 §6.2, M1 contract §7 `seal`, I-06, I-27,
//! I-28, I-44).
//!
//! A seal is 165 bytes: tlock's standard 128-byte IBE block (U 96, V 16,
//! W 16; quicknet, the stock `tlock =0.0.10` crate) on a random 16-byte key
//! `k`, then the 37-byte plaintext XOR a SHA-256 keystream of `k`
//! (`sha256("PS-KS" ‖ k ‖ counter)`). `salt = sha256("PS-SALT" ‖ k)`,
//! `commit = sha256("PS-FRONTIER-MARCH-v1" ‖ plain37 ‖ salt)`,
//! `seal_root = sha256(commit ‖ sha256(seal))`.
//!
//! Bots encrypt with [`seal`]; keepers and the verifier open with
//! [`open`] and judge with [`judge`], which returns SettleTransit's seal
//! code (0 valid, 1–5 bad) as the stock opener sees it.
//!
//! **One rule set (integ-W1, D2 resolved).** `Plain`, `PlainError`,
//! `pack`/`unpack`/`validate`, `salt_of`, `seal_root`, the domains and
//! `RETREAT_MAX_BPS` are the kernel's (`permutation_rules::frontier::seal`,
//! W1-C), re-exported: the keeper, verifier and bots judge a plaintext
//! exactly as the program does (the first twin had no `dest_tile < 61`
//! check). `commit`, `ct_hash` and `body_xor` stay here because they take
//! slices of any length (the S-TLOCK q4 vector's 69-byte plaintext);
//! `twins_equal_the_kernel` pins them to the kernel's on 37-byte input and
//! `kernel_seal_vectors_judge_as_recorded` runs the kernel's
//! `seal-vectors-v1.json` through [`judge`].

use sha2::{Digest, Sha256};

use crate::abi::seal_code;

pub use permutation_rules::frontier::seal::{
    pack, salt_of, seal_root, unpack, validate, Plain, PlainError, DOMAIN_KS, DOMAIN_MARCH,
    DOMAIN_POSTURE, DOMAIN_SALT, PLAIN_LEN, RETREAT_MAX_BPS,
};
pub const IBE_LEN: usize = 128;
pub const SEAL_LEN: usize = IBE_LEN + PLAIN_LEN;
pub const KEY_LEN: usize = 16;
pub const MAX_PATH: u8 = 32;
/// Stances: Hold, Assault, Flank, Brace (kernel `stance::Stance` order).
pub const STANCES: u8 = permutation_rules::frontier::seal::STANCE_MAX + 1;

/// Direction of step `i` (3 bits, little-endian across the 12 bytes).
pub fn step(path: &[u8; 12], i: u8) -> u8 {
    permutation_rules::frontier::seal::path_step(path, i as usize)
}

/// Packs directions into the 12-byte path (each masked to 3 bits, so a
/// test can build an invalid direction 6 or 7).
pub fn path_of(dirs: &[u8]) -> [u8; 12] {
    assert!(dirs.len() <= MAX_PATH as usize);
    let mut bits = 0u128;
    for (i, &d) in dirs.iter().enumerate() {
        bits |= (d as u128 & 7) << (3 * i);
    }
    bits.to_le_bytes()[..12].try_into().expect("12")
}

/// `sha256("PS-FRONTIER-MARCH-v1" ‖ plain ‖ salt)` (any plaintext length:
/// the S-TLOCK q4 vector uses the older 69-byte form).
pub fn commit(plain: &[u8], salt: &[u8; 32]) -> [u8; 32] {
    Sha256::new()
        .chain_update(DOMAIN_MARCH)
        .chain_update(plain)
        .chain_update(salt)
        .finalize()
        .into()
}

/// `sha256(seal)`.
pub fn ct_hash(seal: &[u8]) -> [u8; 32] {
    Sha256::digest(seal).into()
}

/// XOR with `sha256("PS-KS" ‖ k ‖ [c])` blocks (any length).
pub fn body_xor(k: &[u8; KEY_LEN], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for (c, chunk) in data.chunks(32).enumerate() {
        let blk = Sha256::new()
            .chain_update(DOMAIN_KS)
            .chain_update(k)
            .chain_update([c as u8])
            .finalize();
        out.extend(chunk.iter().zip(blk.iter()).map(|(a, b)| a ^ b));
    }
    out
}

/// A sealed march: what Depart carries and what the owner keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sealed {
    pub seal: [u8; SEAL_LEN],
    pub commit: [u8; 32],
    pub salt: [u8; 32],
    pub ct_hash: [u8; 32],
    pub seal_root: [u8; 32],
}

/// Seals `plain` to `round` under the drand key `pk96` with key `k`
/// (random in production: [`random_key`]). The IBE block is the stock
/// `tlock::encrypt` (its FO randomness σ is drawn inside the crate).
pub fn seal_with_key(
    plain: &[u8; PLAIN_LEN],
    pk96: &[u8; 96],
    round: u64,
    k: &[u8; KEY_LEN],
) -> Result<Sealed, String> {
    let mut ibe = Vec::with_capacity(IBE_LEN);
    tlock::encrypt(&mut ibe, &k[..], pk96, round).map_err(|e| e.to_string())?;
    if ibe.len() != IBE_LEN {
        return Err(format!("tlock produced {} B, want {IBE_LEN}", ibe.len()));
    }
    let mut seal = [0u8; SEAL_LEN];
    seal[..IBE_LEN].copy_from_slice(&ibe);
    seal[IBE_LEN..].copy_from_slice(&body_xor(k, plain));
    let salt = salt_of(k);
    let c = commit(plain, &salt);
    let h = ct_hash(&seal);
    Ok(Sealed {
        seal,
        commit: c,
        salt,
        ct_hash: h,
        seal_root: seal_root(&c, &h),
    })
}

pub fn random_key() -> [u8; KEY_LEN] {
    use rand::RngCore;
    let mut k = [0u8; KEY_LEN];
    rand::rngs::OsRng.fill_bytes(&mut k);
    k
}

/// Seals with a fresh random key.
pub fn seal(plain: &[u8; PLAIN_LEN], pk96: &[u8; 96], round: u64) -> Result<Sealed, String> {
    seal_with_key(plain, pk96, round, &random_key())
}

/// Why the IBE block does not open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenError {
    /// U is not a valid compressed G2 point in the subgroup (code 2).
    BadPoint,
    /// The Fujisaki–Okamoto check failed: wrong round or a forged block (code 1).
    FoCheck,
}

/// Opens the 128-byte IBE block with the round's signature (stock
/// `tlock::decrypt`). The crate asserts the FO check with a panic; it is
/// caught here and reported as [`OpenError::FoCheck`].
pub fn open_ibe(ibe: &[u8], sig48: &[u8; 48]) -> Result<[u8; KEY_LEN], OpenError> {
    if ibe.len() < IBE_LEN {
        return Err(OpenError::BadPoint);
    }
    let block = ibe[..IBE_LEN].to_vec();
    let sig = *sig48;
    let r = std::panic::catch_unwind(move || {
        let mut out = Vec::with_capacity(KEY_LEN);
        tlock::decrypt(&mut out, block.as_slice(), &sig).map(|_| out)
    });
    match r {
        Ok(Ok(mut k)) => {
            // tlock strips trailing zero bytes of the 16-byte message.
            if k.len() > KEY_LEN {
                return Err(OpenError::FoCheck);
            }
            k.resize(KEY_LEN, 0);
            Ok(k.try_into().expect("16"))
        }
        Ok(Err(_)) => Err(OpenError::BadPoint),
        Err(_) => Err(OpenError::FoCheck),
    }
}

/// Opens a 165-byte seal: `(k, plain)`.
pub fn open(
    seal: &[u8; SEAL_LEN],
    sig48: &[u8; 48],
) -> Result<([u8; KEY_LEN], [u8; PLAIN_LEN]), OpenError> {
    let k = open_ibe(&seal[..IBE_LEN], sig48)?;
    let pt: [u8; PLAIN_LEN] = body_xor(&k, &seal[IBE_LEN..]).try_into().expect("37");
    Ok((k, pt))
}

/// SettleTransit's seal code as the stock opener gives it (I-44):
/// 0 valid; 1 FO check failed (incl. a wrong round's signature); 2 bad
/// point; 4 commitment mismatch after opening; 5 plaintext invalid. Code 3
/// (the program's "wrong round" label) is not separable off chain from 1:
/// the verifier's rule is "code > 0 ⇔ the stock opener fails", which holds.
pub fn judge(
    seal: &[u8; SEAL_LEN],
    commit_logged: &[u8; 32],
    sig48: &[u8; 48],
    host_id: u64,
    arrive_bell: u32,
) -> (u8, Option<Plain>) {
    let (k, pt) = match open(seal, sig48) {
        Ok(x) => x,
        Err(OpenError::FoCheck) => return (seal_code::FO_FAILED, None),
        Err(OpenError::BadPoint) => return (seal_code::BAD_POINT, None),
    };
    if &commit(&pt, &salt_of(&k)) != commit_logged {
        return (seal_code::COMMIT_MISMATCH, None);
    }
    let p = unpack(&pt);
    if validate(&p, host_id, arrive_bell).is_err() {
        return (seal_code::PLAINTEXT_INVALID, Some(p));
    }
    (seal_code::VALID, Some(p))
}

/// The tlock fixture vectors shipped with this crate (S-TLOCK q3/q4).
pub fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tlock")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::{quicknet_info, TestKey};

    fn plain(host: u64, arrive: u32) -> Plain {
        Plain {
            version: 1,
            host_id: host,
            arrive_bell: arrive,
            dest_p: -3,
            dest_q: 7,
            dest_tile: 30,
            stance: 2,
            retreat_bps: 0,
            path_len: 5,
            path: path_of(&[0, 1, 5, 3, 2]),
            reserved: [0; 3],
        }
    }

    #[test]
    fn pack_unpack_validate() {
        let p = plain(9, 44);
        assert_eq!(unpack(&pack(&p)), p);
        assert_eq!(validate(&p, 9, 44), Ok(()));
        assert_eq!(step(&p.path, 2), 5);
        let mut q = p;
        q.path[2] |= 0x80; // a bit beyond step 5
        assert_eq!(validate(&q, 9, 44), Err(PlainError::PathBits));
        let mut q = p;
        q.retreat_bps = 60_001;
        assert_eq!(validate(&q, 9, 44), Err(PlainError::Retreat));
        let mut q = p;
        q.reserved[1] = 1;
        assert_eq!(validate(&q, 9, 44), Err(PlainError::Reserved));
        let mut q = p;
        q.path = path_of(&[0, 6, 0, 0, 0]);
        assert_eq!(validate(&q, 9, 44), Err(PlainError::Direction));
        assert_eq!(validate(&p, 8, 44), Err(PlainError::HostMismatch));
        assert_eq!(validate(&p, 9, 45), Err(PlainError::ArriveMismatch));
        // The kernel's tile bound (the first twin missed it: judge said 0,
        // the program says 5).
        let mut q = p;
        q.dest_tile = 61;
        assert_eq!(validate(&q, 9, 44), Err(PlainError::Tile));
        let mut full = p;
        full.path_len = 32;
        full.path = path_of(&[5u8; 32]);
        assert_eq!(validate(&full, 9, 44), Ok(()));
    }

    #[test]
    fn js_compact16_seal_opens_with_the_rust_crate() {
        // S-TLOCK q4: tlock-js ibe.encryptOnG2RFC9380 on a 16-byte key.
        let v: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(fixture_dir().join("q4-vector.json")).unwrap(),
        )
        .unwrap();
        let hx = |k: &str| hex::decode(v[k].as_str().unwrap()).unwrap();
        let (sig, ct16, commit16, pt16) = (hx("sig"), hx("ct16"), hx("commit16"), hx("pt16"));
        let sig48: [u8; 48] = sig.try_into().unwrap();
        assert!(crate::beacon::verify(
            v["round"].as_u64().unwrap(),
            &sig48,
            &quicknet_info().public_key
        ));
        let k = open_ibe(&ct16[..IBE_LEN], &sig48).unwrap();
        let pt = body_xor(&k, &ct16[IBE_LEN..]);
        // The compact body carries the plaintext without its trailing salt.
        assert_eq!(pt, pt16[..pt.len()]);
        assert_eq!(commit(&pt, &salt_of(&k)).to_vec(), commit16);
        // The previous round's signature does not open it (FO check).
        let prev: [u8; 48] = hx("prev_round_sig").try_into().unwrap();
        assert_eq!(open_ibe(&ct16[..IBE_LEN], &prev), Err(OpenError::FoCheck));
    }

    #[test]
    fn rust_seal_round_trip_and_judge_codes() {
        let key = TestKey::new();
        let round = 5_000;
        let sig = key.sign(round);
        let p = plain(77, 12);
        let s = seal(&pack(&p), &key.pk96, round).unwrap();
        assert_eq!(s.seal.len(), 165);
        let (k, pt) = open(&s.seal, &sig).unwrap();
        assert_eq!(pt, pack(&p));
        assert_eq!(salt_of(&k), s.salt);
        assert_eq!(
            judge(&s.seal, &s.commit, &sig, 77, 12),
            (seal_code::VALID, Some(p))
        );
        // Wrong round's signature → FO failure.
        assert_eq!(
            judge(&s.seal, &s.commit, &key.sign(round + 1), 77, 12).0,
            seal_code::FO_FAILED
        );
        // Garbage U → bad point.
        let mut g = s.seal;
        g[..96].fill(0xAB);
        assert_eq!(judge(&g, &s.commit, &sig, 77, 12).0, seal_code::BAD_POINT);
        // A body byte flipped → the commitment no longer matches.
        let mut b = s.seal;
        b[140] ^= 1;
        assert_eq!(
            judge(&b, &s.commit, &sig, 77, 12).0,
            seal_code::COMMIT_MISMATCH
        );
        // A valid seal over an invalid plaintext (the bad_plaintext persona).
        let mut badp = p;
        badp.stance = 9;
        let sb = seal(&pack(&badp), &key.pk96, round).unwrap();
        assert_eq!(
            judge(&sb.seal, &sb.commit, &sig, 77, 12).0,
            seal_code::PLAINTEXT_INVALID
        );
        // Root binds commit and ciphertext.
        assert_eq!(seal_root(&s.commit, &ct_hash(&s.seal)), s.seal_root);
    }

    /// The slice-based twins equal the kernel's on 37-byte input.
    #[test]
    fn twins_equal_the_kernel() {
        use permutation_rules::frontier::seal as k;
        let mut x = 0x1234_5678_9abc_def1u64;
        for _ in 0..2_000 {
            let mut pt = [0u8; PLAIN_LEN];
            let mut key = [0u8; KEY_LEN];
            for b in pt.iter_mut().chain(key.iter_mut()) {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = x as u8;
            }
            let salt = salt_of(&key);
            assert_eq!(commit(&pt, &salt), k::commit(&pt, &salt));
            assert_eq!(body_xor(&key, &pt), k::body_xor(&key, &pt).to_vec());
            let mut sl = [0u8; SEAL_LEN];
            sl[..PLAIN_LEN].copy_from_slice(&pt);
            assert_eq!(ct_hash(&sl), k::ct_hash(&sl));
        }
    }

    /// The kernel's seal vectors (W1-C, `permutation-rules/vectors/
    /// seal-vectors-v1.json`, 24 cases incl. every `PlainError`) judged with
    /// the stock opener: the code matches the recorded class and the
    /// plaintext verdict matches the kernel's.
    #[test]
    fn kernel_seal_vectors_judge_as_recorded() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../permutation-rules/vectors/seal-vectors-v1.json");
        let j: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("vectors")).expect("json");
        let cases = j["cases"].as_array().expect("cases");
        assert!(cases.len() >= 24);
        let hx = |c: &serde_json::Value, f: &str| hex::decode(c[f].as_str().unwrap()).unwrap();
        let mut tiles = 0;
        for c in cases {
            let name = c["name"].as_str().unwrap();
            let seal: [u8; SEAL_LEN] = hx(c, "seal").try_into().unwrap();
            let commit: [u8; 32] = hx(c, "commit").try_into().unwrap();
            let sig: [u8; 48] = hx(c, "open_sig").try_into().unwrap();
            let host: u64 = c["host_id"].as_str().unwrap().parse().unwrap();
            let arrive = c["arrive_bell"].as_u64().unwrap() as u32;
            let (code, p) = judge(&seal, &commit, &sig, host, arrive);
            let want: &[u8] = match c["expect"].as_str().unwrap() {
                "valid" => &[seal_code::VALID],
                "bad_plaintext" => &[seal_code::PLAINTEXT_INVALID],
                "commit_mismatch" => &[seal_code::COMMIT_MISMATCH],
                "fo_fail" => &[seal_code::FO_FAILED],
                "bad_point_or_fo_fail" => &[seal_code::FO_FAILED, seal_code::BAD_POINT],
                e => panic!("{name}: class {e}"),
            };
            assert!(want.contains(&code), "{name}: code {code}");
            if let Some(p) = p {
                let v = match validate(&p, host, arrive) {
                    Ok(()) => "ok".to_string(),
                    Err(e) => format!("{e:?}"),
                };
                assert_eq!(v, c["validate"].as_str().unwrap(), "{name}");
                tiles += (v == "Tile") as u32;
            }
        }
        assert_eq!(tiles, 2, "the tile cases reach the plaintext check");
    }
}
