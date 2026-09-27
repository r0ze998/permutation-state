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
//! **Integration note.** `Plain`, `pack`/`unpack`/`validate`, `salt_of`,
//! `commit`, `seal_root` and `body_xor` are the kernel's
//! (`permutation_rules::frontier::seal`, W1-C); this is their twin and W2-F
//! points it at the kernel once merged. The layout is DESIGN §6.2's.

use sha2::{Digest, Sha256};

use crate::abi::seal_code;

pub const DOMAIN_MARCH: &[u8] = b"PS-FRONTIER-MARCH-v1";
pub const DOMAIN_POSTURE: &[u8] = b"PS-FRONTIER-POSTURE-v1";
pub const DOMAIN_SALT: &[u8] = b"PS-SALT";
pub const DOMAIN_KS: &[u8] = b"PS-KS";
/// `clash::RETREAT_MAX_BPS` (defined by W1-A in `clash.rs`, I-27).
pub const RETREAT_MAX_BPS: u16 = 60_000;
pub const PLAIN_LEN: usize = 37;
pub const IBE_LEN: usize = 128;
pub const SEAL_LEN: usize = IBE_LEN + PLAIN_LEN;
pub const KEY_LEN: usize = 16;
pub const MAX_PATH: u8 = 32;
/// Stances: Hold, Assault, Flank, Brace (kernel `stance::Stance` order).
pub const STANCES: u8 = 4;

/// The 37-byte march plaintext (DESIGN §6.2, pinned):
/// `version u8 = 1 | host_id u64 | arrive_bell u32 | dest P i16 | dest Q i16 |
/// dest tile u8 | stance u8 | retreat_bps u16 | path_len u8 | path 12 B | reserved 3 B`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Plain {
    pub version: u8,
    pub host_id: u64,
    pub arrive_bell: u32,
    pub dest_p: i16,
    pub dest_q: i16,
    pub dest_tile: u8,
    pub stance: u8,
    /// 0 = never retreat; 1..=60,000 the ratio in bps (I-27).
    pub retreat_bps: u16,
    pub path_len: u8,
    /// ≤ 32 steps × 3-bit hex directions, little-endian bit order
    /// (0 = E, then counter-clockwise, as `hex`).
    pub path: [u8; 12],
    pub reserved: [u8; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlainError {
    Version,
    Reserved,
    PathLen,
    PathBits,
    Direction,
    Stance,
    Retreat,
    Host,
    Arrive,
}

pub fn pack(p: &Plain) -> [u8; PLAIN_LEN] {
    let mut b = [0u8; PLAIN_LEN];
    b[0] = p.version;
    b[1..9].copy_from_slice(&p.host_id.to_le_bytes());
    b[9..13].copy_from_slice(&p.arrive_bell.to_le_bytes());
    b[13..15].copy_from_slice(&p.dest_p.to_le_bytes());
    b[15..17].copy_from_slice(&p.dest_q.to_le_bytes());
    b[17] = p.dest_tile;
    b[18] = p.stance;
    b[19..21].copy_from_slice(&p.retreat_bps.to_le_bytes());
    b[21] = p.path_len;
    b[22..34].copy_from_slice(&p.path);
    b[34..37].copy_from_slice(&p.reserved);
    b
}

pub fn unpack(b: &[u8; PLAIN_LEN]) -> Plain {
    Plain {
        version: b[0],
        host_id: u64::from_le_bytes(b[1..9].try_into().expect("8")),
        arrive_bell: u32::from_le_bytes(b[9..13].try_into().expect("4")),
        dest_p: i16::from_le_bytes(b[13..15].try_into().expect("2")),
        dest_q: i16::from_le_bytes(b[15..17].try_into().expect("2")),
        dest_tile: b[17],
        stance: b[18],
        retreat_bps: u16::from_le_bytes(b[19..21].try_into().expect("2")),
        path_len: b[21],
        path: b[22..34].try_into().expect("12"),
        reserved: b[34..37].try_into().expect("3"),
    }
}

/// Direction of step `i` (3 bits, little-endian across the 12 bytes).
pub fn step(path: &[u8; 12], i: u8) -> u8 {
    let bits = u128::from_le_bytes({
        let mut w = [0u8; 16];
        w[..12].copy_from_slice(path);
        w
    });
    ((bits >> (3 * i as u32)) & 7) as u8
}

/// Packs directions (each < 6) into the 12-byte path.
pub fn path_of(dirs: &[u8]) -> [u8; 12] {
    assert!(dirs.len() <= MAX_PATH as usize);
    let mut bits = 0u128;
    for (i, &d) in dirs.iter().enumerate() {
        bits |= (d as u128 & 7) << (3 * i);
    }
    bits.to_le_bytes()[..12].try_into().expect("12")
}

/// `Plain::validate` (I-28): version 1, reserved zero, `path_len ≤ 32`,
/// unused path bits zero, every step a hex direction (< 6), stance one of
/// the four, retreat ≤ 60,000, host and arrival bell equal to the transit's.
pub fn validate(p: &Plain, host_id: u64, arrive_bell: u32) -> Result<(), PlainError> {
    if p.version != 1 {
        return Err(PlainError::Version);
    }
    if p.reserved != [0; 3] {
        return Err(PlainError::Reserved);
    }
    if p.path_len > MAX_PATH {
        return Err(PlainError::PathLen);
    }
    let bits = u128::from_le_bytes({
        let mut w = [0u8; 16];
        w[..12].copy_from_slice(&p.path);
        w
    });
    if p.path_len < MAX_PATH && bits >> (3 * p.path_len as u32) != 0 {
        return Err(PlainError::PathBits);
    }
    if (0..p.path_len).any(|i| step(&p.path, i) >= 6) {
        return Err(PlainError::Direction);
    }
    if p.stance >= STANCES {
        return Err(PlainError::Stance);
    }
    if p.retreat_bps > RETREAT_MAX_BPS {
        return Err(PlainError::Retreat);
    }
    if p.host_id != host_id {
        return Err(PlainError::Host);
    }
    if p.arrive_bell != arrive_bell {
        return Err(PlainError::Arrive);
    }
    Ok(())
}

/// `sha256("PS-SALT" ‖ k)`.
pub fn salt_of(k: &[u8; KEY_LEN]) -> [u8; 32] {
    Sha256::new()
        .chain_update(DOMAIN_SALT)
        .chain_update(k)
        .finalize()
        .into()
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

/// `sha256(commit ‖ ct_hash)`.
pub fn seal_root(commit: &[u8; 32], ct_hash: &[u8; 32]) -> [u8; 32] {
    Sha256::new()
        .chain_update(commit)
        .chain_update(ct_hash)
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
        assert_eq!(validate(&p, 8, 44), Err(PlainError::Host));
        assert_eq!(validate(&p, 9, 45), Err(PlainError::Arrive));
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
}
