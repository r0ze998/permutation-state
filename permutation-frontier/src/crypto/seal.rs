//! The on-chain tlock opener (S-TLOCK-V, SP-V2 `seal.rs`): opens a compact-16
//! quicknet seal with the round-T(arrive) signature THE anchor (or, once
//! archived, the archive entry) stores, runs the Fujisaki–Okamoto check and
//! judges the result with the contract's seal codes (§5.3 SealVerdict note,
//! I-44). SettleTransit (W4-B) calls [`judge`]; it is here, with the rest of
//! the crypto port, so the wave-2 build already carries and measures it.
//!
//! Seal (165 B): `U (96, compressed G2) ‖ V (16) ‖ W (16) ‖ body (37)`.
//! With `s` = the round signature (G1): `sigma = V ⊕ H2(e(s, U))`, `k = W ⊕
//! H4(sigma)`, FO check `U == H3(sigma, k) · G2`, then the body opens with
//! `k` (`seal::body_xor`) and must hash to the logged commitment under
//! `salt_of(k)` (I-06). The hash domains and the FO scalar derivation are
//! tlock 0.0.10's, unchanged from SP-V2 (measured against stock tlock
//! there and in `seal-vectors-v1.json`).
//!
//! **Codes:** 0 valid, 1 FO check failed, 2 bad point / not in the
//! subgroup, 3 wrong round, 4 commitment mismatch after opening, 5
//! plaintext invalid (`seal::validate`, I-28). A seal sealed to another
//! round cannot be told apart from a tampered one: the pairing with the
//! wrong signature yields a random `sigma` and the FO check fails, so it is
//! judged 1 (the `wrong_round` vector expects `fo_fail`); code 3 is kept in
//! the table for a future opener that can prove the round.

use permutation_rules::frontier::seal as kseal;

use super::{sys, xmd::sha256};
use frontier_abi::log::seal_code;

/// Compact-16 seal length.
pub const SEAL_LEN: usize = kseal::SEAL_LEN;
const N: usize = kseal::K_LEN;
const U: usize = kseal::SEAL_U;
const V: usize = kseal::SEAL_V;
const W: usize = kseal::SEAL_W;

// BLS12-381 G2 generator, uncompressed Zcash BE (x.c1, x.c0, y.c1, y.c0).
const G2_GEN: [u8; 192] = hex192(
    b"13e02b6052719f607dacd3a088274f65596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e\
024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb8\
0606c4a02ea734cc32acd2b02bc28b99cb3e287e85a763af267492ab572e99ab3f370d275cec1da1aaa9075ff05f79be\
0ce5d527727d6e118cc9cdc6da2e351aadfd9baa8cbdd3a76d429a695160d12c923ac9cc3baca289e193548608b82801",
);
// r = BLS12-381 scalar field order, big-endian.
const R_ORDER: [u8; 32] =
    super::quick::hex32(b"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001");

const fn nib(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => 0,
    }
}

const fn hex192(s: &[u8]) -> [u8; 192] {
    let mut o = [0u8; 192];
    let mut i = 0;
    while i < 192 {
        o[i] = (nib(s[2 * i]) << 4) | nib(s[2 * i + 1]);
        i += 1;
    }
    o
}

/// Opens `seal` with the round signature `s_aff` (uncompressed G1, 96 B)
/// and checks the body against `commit`. `Ok(plain)` when the seal opens
/// and the opened body hashes to `commit`; otherwise the seal code (1, 2
/// or 4).
#[inline(never)]
pub fn open(s_aff: &[u8; 96], seal: &[u8; SEAL_LEN], commit: &[u8; 32]) -> Result<[u8; 37], u8> {
    let mut u = [0u8; 96];
    u.copy_from_slice(&seal[U..U + 96]);
    let Some(u_aff) = sys::bls_g2_decompress(&u) else {
        return Err(seal_code::BAD_POINT);
    };
    crate::heap::trace_checkpoint(31);
    let mut gt = [0u8; 576];
    if sys::bls_pairing(s_aff, &u_aff, 1, &mut gt).is_err() {
        return Err(seal_code::BAD_POINT);
    }
    crate::heap::trace_checkpoint(32);
    let h2 = sha256(&[b"IBE-H2", &gt]);
    let mut sigma = [0u8; N];
    for i in 0..N {
        sigma[i] = seal[V + i] ^ h2[i];
    }
    let h4 = sha256(&[b"IBE-H4", &sigma]);
    let mut k = [0u8; N];
    for i in 0..N {
        k[i] = seal[W + i] ^ h4[i];
    }
    // FO check: U == H3(sigma, k) · G2.
    let h3 = sha256(&[b"IBE-H3", &sigma, &k]);
    let mut r = [0u8; 32];
    let mut ok = false;
    for i in 1u16..u16::MAX {
        r = sha256(&[&i.to_le_bytes(), &h3]);
        r[0] >>= 1;
        if r < R_ORDER {
            ok = true;
            break;
        }
    }
    if !ok {
        return Err(seal_code::FO_FAILED);
    }
    let Some(rp) = sys::bls_g2_mul(&r, &G2_GEN) else {
        return Err(seal_code::FO_FAILED);
    };
    if rp != u_aff {
        return Err(seal_code::FO_FAILED);
    }
    crate::heap::trace_checkpoint(33);
    kseal::open_body(&k, seal, commit).ok_or(seal_code::COMMIT_MISMATCH)
}

/// The verdict SettleTransit logs: the seal code and, for code 0, the
/// march's plaintext. `host_id` and `arrive_bell` are the transit's.
pub fn judge(
    s_aff: &[u8; 96],
    seal: &[u8; SEAL_LEN],
    commit: &[u8; 32],
    host_id: u64,
    arrive_bell: u32,
) -> (u8, Option<kseal::Plain>) {
    match open(s_aff, seal, commit) {
        Err(code) => (code, None),
        Ok(pt) => {
            let plain = kseal::unpack(&pt);
            match kseal::validate(&plain, host_id, arrive_bell) {
                Ok(()) => (seal_code::VALID, Some(plain)),
                Err(_) => (seal_code::BAD_PLAINTEXT, None),
            }
        }
    }
}

/// `seal_root = sha256(commit ‖ sha256(seal))` (Depart stores it;
/// SettleTransit checks the logged pair against it).
pub fn seal_root_of(commit: &[u8; 32], seal: &[u8; SEAL_LEN]) -> [u8; 32] {
    kseal::seal_root(commit, &kseal::ct_hash(seal))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g2_generator_constant_is_arks() {
        use super::super::field::from_be;
        use ark_bls12_381::{Fq, Fq2, G2Affine};
        use ark_ec::AffineRepr;
        let f = |o: usize| from_be::<Fq, 6>(&G2_GEN[o..o + 48]).unwrap();
        let g = G2Affine::new_unchecked(Fq2::new(f(48), f(0)), Fq2::new(f(144), f(96)));
        assert_eq!(g, G2Affine::generator());
    }

    #[test]
    fn host_opening_is_a_bad_point() {
        // No G2 decompression syscall on the host: every seal is code 2
        // there, never "valid".
        assert_eq!(
            judge(&[0; 96], &[0; SEAL_LEN], &[0; 32], 1, 2),
            (seal_code::BAD_POINT, None)
        );
    }

    #[test]
    fn seal_root_is_the_kernels() {
        let seal = [5u8; SEAL_LEN];
        let c = [6u8; 32];
        assert_eq!(seal_root_of(&c, &seal), sha256(&[&c, &sha256(&[&seal])]));
    }

    fn hex(s: &str) -> alloc::vec::Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// The cases of `permutation-rules/vectors/seal-vectors-v1.json` as
    /// (key, value) lists (the file is pretty-printed one field per line).
    fn cases() -> alloc::vec::Vec<alloc::vec::Vec<(alloc::string::String, alloc::string::String)>> {
        let text = include_str!("../../../permutation-rules/vectors/seal-vectors-v1.json");
        let mut out = alloc::vec::Vec::new();
        let mut cur: Option<alloc::vec::Vec<_>> = None;
        for line in text.lines() {
            let t = line.trim();
            if t == "{" && text.contains("\"cases\"") {
                cur = Some(alloc::vec::Vec::new());
                continue;
            }
            if t.starts_with('}') {
                if let Some(c) = cur.take() {
                    if !c.is_empty() {
                        out.push(c);
                    }
                }
                continue;
            }
            if let (Some(c), Some((k, v))) = (cur.as_mut(), t.split_once(": ")) {
                let k = k.trim_matches('"').into();
                let v = v.trim_end_matches(',').trim_matches('"').into();
                c.push((k, v));
            }
        }
        out
    }

    fn get<'a>(c: &'a [(alloc::string::String, alloc::string::String)], k: &str) -> &'a str {
        c.iter()
            .find(|(kk, _)| kk == k)
            .map(|(_, v)| v.as_str())
            .unwrap_or("null")
    }

    /// Everything of the opener except the pairing (the host has no
    /// syscall) against the W1-C seal vectors: `k = W ⊕ H4(sigma)`, the FO
    /// scalar `r = H3(sigma, k)` gives `r · G2 = U`, and the body opens to
    /// the vector's plaintext under the commitment (or does not, for the
    /// commitment-mismatch cases), with the vector's validation verdict.
    #[test]
    fn opener_steps_match_the_seal_vectors() {
        use ark_bls12_381::{Fr, G2Affine};
        use ark_ec::{AffineRepr, CurveGroup};
        use ark_ff::PrimeField;
        let mut checked = 0;
        for c in cases() {
            let sigma = get(&c, "sigma");
            if sigma == "null" || get(&c, "stock_tlock_opens") != "true" {
                continue;
            }
            let seal: [u8; SEAL_LEN] = hex(get(&c, "seal")).try_into().unwrap();
            let sigma: [u8; N] = hex(sigma).try_into().unwrap();
            let h4 = sha256(&[b"IBE-H4", &sigma]);
            let mut k = [0u8; N];
            for i in 0..N {
                k[i] = seal[W + i] ^ h4[i];
            }
            assert_eq!(k.to_vec(), hex(get(&c, "k")), "{}", get(&c, "name"));
            let h3 = sha256(&[b"IBE-H3", &sigma, &k]);
            let mut r = [0u8; 32];
            for i in 1u16..u16::MAX {
                r = sha256(&[&i.to_le_bytes(), &h3]);
                r[0] >>= 1;
                if r < R_ORDER {
                    break;
                }
            }
            let p = (G2Affine::generator() * Fr::from_be_bytes_mod_order(&r)).into_affine();
            let mut x = [0u8; 96];
            super::super::field::to_be::<ark_bls12_381::Fq, 6>(&p.x.c1, &mut x[..48]);
            super::super::field::to_be::<ark_bls12_381::Fq, 6>(&p.x.c0, &mut x[48..]);
            let mut ux = [0u8; 96];
            ux.copy_from_slice(&seal[..96]);
            ux[0] &= 0x1f;
            assert_eq!(x, ux, "{}: r·G2 = U", get(&c, "name"));
            let commit: [u8; 32] = hex(get(&c, "commit")).try_into().unwrap();
            let opened = kseal::open_body(&k, &seal, &commit);
            match get(&c, "expect") {
                "commit_mismatch" => assert_eq!(opened, None, "{}", get(&c, "name")),
                exp => {
                    let pt = opened.unwrap();
                    assert_eq!(pt.to_vec(), hex(get(&c, "plain")));
                    let host: u64 = get(&c, "host_id").parse().unwrap();
                    let arrive: u32 = get(&c, "arrive_bell").parse().unwrap();
                    let v = kseal::validate(&kseal::unpack(&pt), host, arrive);
                    assert_eq!(v.is_ok(), exp == "valid", "{}", get(&c, "name"));
                }
            }
            checked += 1;
        }
        assert_eq!(
            checked, 19,
            "every vector that stock tlock opens with a recorded sigma"
        );
    }
}
