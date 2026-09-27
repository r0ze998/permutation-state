// Producer of seal-vectors-v1.json (unit W1-C). NOT compiled by any
// workspace: files under `vectors/` are data. To regenerate, put this file
// at `src/main.rs` of a throwaway crate (toolchain 1.95.0, tlock is
// edition 2024) with the manifest below, then run
//   cargo run --release -- <S-TLOCK results dir> <SP-V2 beacons dir> \
//       permutation-rules/vectors/seal-vectors-v1.json
// (the M1 lab copies live in scratchpad/frontier/m1/lab/w1c-seal-vectors and,
// with the integ-W1 cases, scratchpad/frontier/m1/integ-w1r/seal-vectors,
// whose Cargo.lock is S-TLOCK rs-interop's). Output is deterministic.
//
// [package]
// name = "w1c-seal-vectors"
// edition = "2021"
// [workspace]
// [dependencies]
// tlock = "=0.0.10"
// ark-bls12-381 = "=0.6.0"
// ark-ec = "=0.6.0"
// ark-ff = "=0.6.0"
// ark-serialize = "=0.6.0"
// sha2 = "=0.10.9"
// hex = "=0.4.3"
// serde_json = "=1.0.151"
// permutation-rules = { path = "<repo>/permutation-rules", features = ["std"] }
//
//! Producer of `permutation-rules/vectors/seal-vectors-v1.json` (unit W1-C).
//!
//! Deterministic compact-16 march seals to real quicknet rounds whose
//! signatures were recorded (S-TLOCK q3/q4, SP-V2 beacon fixtures):
//!
//! - the IBE block (U, V, W) is tlock 0.0.10's `ibe::encrypt` with a fixed
//!   `sigma` instead of a random one (same hashes, same serialisation);
//! - the body is the 37-byte plaintext XOR `sha256("PS-KS" ‖ k ‖ [c])`;
//! - every seal is opened again with the **stock** `tlock::decrypt` and
//!   the recorded signature, and the body/commitment are recomputed here
//!   with `sha2` independently of the rules kernel, then compared with it.
//!
//! `k` and `sigma` are derived from the S-TLOCK q3/q4 commitments
//! ("seeded from q3/q4"): `k_i = sha256("PSF-SEALVEC-k" ‖ q3.commit ‖
//! q4.commit ‖ i)[..16]` (last byte forced non-zero: stock `decrypt`
//! strips trailing zeros), `sigma_i = sha256("PSF-SEALVEC-sigma" ‖ … ‖
//! i)[..16]`.
//!
//! Usage: `cargo run --release -- <S-TLOCK results dir> <SP-V2 beacons dir> <out.json>`
//! Producer of `permutation-rules/vectors/seal-vectors-v1.json` (unit W1-C).
//!
//! Deterministic compact-16 march seals to real quicknet rounds whose
//! signatures were recorded (S-TLOCK q3/q4, SP-V2 beacon fixtures):
//!
//! - the IBE block (U, V, W) is tlock 0.0.10's `ibe::encrypt` with a fixed
//!   `sigma` instead of a random one (same hashes, same serialisation);
//! - the body is the 37-byte plaintext XOR `sha256("PS-KS" ‖ k ‖ [c])`;
//! - every seal is opened again with the **stock** `tlock::decrypt` and
//!   the recorded signature, and the body/commitment are recomputed here
//!   with `sha2` independently of the rules kernel, then compared with it.
//!
//! `k` and `sigma` are derived from the S-TLOCK q3/q4 commitments
//! ("seeded from q3/q4"): `k_i = sha256("PSF-SEALVEC-k" ‖ q3.commit ‖
//! q4.commit ‖ i)[..16]` (last byte forced non-zero: stock `decrypt`
//! strips trailing zeros), `sigma_i = sha256("PSF-SEALVEC-sigma" ‖ … ‖
//! i)[..16]`.
//!
//! Usage: `cargo run --release -- <S-TLOCK results dir> <SP-V2 beacons dir> <out.json>`

use ark_bls12_381::{g1, Bls12_381, Fr, G1Projective, G2Affine};
use ark_ec::{
    hashing::{curve_maps::wb::WBMap, map_to_curve_hasher::MapToCurveBasedHasher, HashToCurve},
    models::short_weierstrass,
    pairing::Pairing,
    AffineRepr, CurveGroup,
};
use ark_ff::{field_hashers::DefaultFieldHasher, PrimeField};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress};
use permutation_rules::frontier::{addr, beacon, seal};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::ops::Mul;

const QUICKNET_PK: &str = "83cf0f2896adee7eb8b5f01fcad3912212c437e0073e911fb90022d3e760183c8c4b450b6a0a6c3ac6a5776a2d1064510d1fec758c921cc22b0e17e63aaf4bcb5ed66304de9cf809bd274ca73bab4af5a6e9c76a4bc09e76eae8991ef5ece45a";
const G1_DOMAIN: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_";

fn sha(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// tlock's ExpandMsgDrand: H(i ‖ h3) masked, first value < r.
fn h3_scalar(sigma: &[u8; 16], msg: &[u8; 16]) -> Fr {
    let h3 = sha(&[b"IBE-H3", sigma, msg]);
    for i in 1u16..u16::MAX {
        let mut h = sha(&[&i.to_le_bytes(), &h3]).to_vec();
        h[0] >>= 1;
        h.reverse();
        if Fr::deserialize_compressed(h.as_slice()).is_ok() {
            return Fr::from_le_bytes_mod_order(&h);
        }
    }
    panic!("no scalar");
}

/// tlock 0.0.10 `ibe::encrypt` (G2 public key, G1 signatures, RFC 9380)
/// with a caller-supplied sigma. Returns U (96, compressed) ‖ V ‖ W.
fn ibe_encrypt(pk: &G2Affine, round: u64, sigma: &[u8; 16], msg: &[u8; 16]) -> [u8; 128] {
    let id = sha(&[&round.to_be_bytes()]);
    let mapper = MapToCurveBasedHasher::<
        short_weierstrass::Projective<g1::Config>,
        DefaultFieldHasher<Sha256, 128>,
        WBMap<g1::Config>,
    >::new(G1_DOMAIN)
    .unwrap();
    let qid = G1Projective::from(mapper.hash(&id).unwrap()).into_affine();
    let gid = Bls12_381::pairing(qid, pk);
    let r = h3_scalar(sigma, msg);
    let u = G2Affine::generator().mul(r).into_affine();
    let mut rgid = vec![];
    gid.mul(r)
        .serialize_with_mode(&mut rgid, Compress::Yes)
        .unwrap();
    rgid.reverse();
    let h2 = sha(&[b"IBE-H2", &rgid]);
    let h4 = sha(&[b"IBE-H4", sigma]);
    let mut out = [0u8; 128];
    u.serialize_with_mode(&mut out[..96], Compress::Yes).unwrap();
    for i in 0..16 {
        out[96 + i] = sigma[i] ^ h2[i];
        out[112 + i] = msg[i] ^ h4[i];
    }
    out
}

/// The stock opener: `Some(k)` iff tlock 0.0.10 decrypts (it panics on a
/// failed FO check, so catch that).
fn stock_open(ibe: &[u8], sig: &[u8]) -> Option<[u8; 16]> {
    let ibe = ibe.to_vec();
    let sig = sig.to_vec();
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(move || {
        let mut k = vec![];
        tlock::decrypt(&mut k, ibe.as_slice(), &sig).ok().map(|_| k)
    })
    .ok()
    .flatten();
    std::panic::set_hook(hook);
    let r = r?;
    let mut out = [0u8; 16];
    if r.len() > 16 {
        return None;
    }
    out[..r.len()].copy_from_slice(&r); // stock strips trailing zeros
    Some(out)
}

/// Independent body keystream (not the kernel's).
fn body_ref(k: &[u8; 16], pt: &[u8; 37]) -> [u8; 37] {
    let mut o = *pt;
    for (c, chunk) in o.chunks_mut(32).enumerate() {
        let blk = sha(&[b"PS-KS", k, &[c as u8]]);
        for (x, b) in chunk.iter_mut().zip(blk) {
            *x ^= b;
        }
    }
    o
}

fn hx(b: &[u8]) -> String {
    hex::encode(b)
}

fn seedhash(tag: &[u8], q3c: &[u8], q4c: &[u8], i: u8) -> [u8; 16] {
    let h = sha(&[tag, q3c, q4c, &[i]]);
    let mut o = [0u8; 16];
    o.copy_from_slice(&h[..16]);
    o
}

struct Case {
    name: &'static str,
    expect: &'static str,
    round: u64,
    sig: Vec<u8>,
    k: [u8; 16],
    sigma: [u8; 16],
    plain: seal::Plain,
    arrive_bell: u32,
    /// Mutations after sealing.
    tamper: Tamper,
    /// Open with this signature instead (wrong round).
    open_sig: Option<Vec<u8>>,
    /// The order the seal is judged against (host, arrive bell); the
    /// plaintext's own values unless a mismatch case overrides them.
    order: Option<(u64, u32)>,
}

#[derive(Clone, Copy)]
enum Tamper {
    None,
    V,
    W,
    U,
    Body,
    ForgedCommit,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (stl, spv2, out) = (&args[1], &args[2], &args[3]);
    let rd = |p: String| -> Value { serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap() };
    let q3 = rd(format!("{stl}/q3-vector.json"));
    let q4 = rd(format!("{stl}/q4-vector.json"));
    let b1 = rd(format!("{spv2}/quicknet/32551361.json"));
    let b2 = rd(format!("{spv2}/quicknet/32553016.json"));
    let hv = |v: &Value, k: &str| hex::decode(v[k].as_str().unwrap()).unwrap();
    let (q3c, q4c) = (hv(&q3, "commit"), hv(&q4, "commit"));
    let pk = G2Affine::deserialize_compressed(hex::decode(QUICKNET_PK).unwrap().as_slice()).unwrap();

    let key = |i: u8| {
        let mut k = seedhash(b"PSF-SEALVEC-k", &q3c, &q4c, i);
        if k[15] == 0 {
            k[15] = 1;
        }
        k
    };
    let sigma = |i: u8| seedhash(b"PSF-SEALVEC-sigma", &q3c, &q4c, i);

    let path = |dirs: &[u8]| seal::encode_path(dirs).unwrap();
    let base = |host: u64, arrive: u32, dirs: &[u8]| {
        let (n, p) = path(dirs);
        seal::Plain {
            version: 1,
            host_id: host,
            arrive_bell: arrive,
            dest_p: 3,
            dest_q: -2,
            dest_tile: 17,
            stance: 1,
            retreat_bps: 5_000,
            path_len: n,
            path: p,
            reserved: [0; 3],
        }
    };
    let host_a = addr::host_id(2, -1, 4, 0, 7);
    let host_b = addr::host_id(-128, 64, 11, 255, u32::MAX);
    let host_c = addr::host_id(0, 5, 0, 1, 0);
    let dirs32: Vec<u8> = (0..32).map(|i| (i * 5 % 6) as u8).collect();

    let (r3, s3) = (q3["round"].as_u64().unwrap(), hv(&q3, "sig"));
    let (r4, s4) = (q4["round"].as_u64().unwrap(), hv(&q4, "sig"));
    let (rb1, sb1) = (b1["round"].as_u64().unwrap(), hv(&b1, "signature"));
    let (rb2, sb2) = (b2["round"].as_u64().unwrap(), hv(&b2, "signature"));

    let mut cases: Vec<Case> = vec![];
    let mut add = |name, expect, round, sig: &Vec<u8>, i: u8, plain: seal::Plain, tamper, open_sig: Option<&Vec<u8>>| {
        cases.push(Case {
            name,
            expect,
            round,
            sig: sig.clone(),
            k: key(i),
            sigma: sigma(i),
            arrive_bell: plain.arrive_bell,
            plain,
            tamper,
            open_sig: open_sig.cloned(),
            order: None,
        })
    };
    add("valid_q3", "valid", r3, &s3, 0, base(host_a, 144, &[0, 0, 1, 2, 5]), Tamper::None, None);
    add("valid_q4_min", "valid", r4, &s4, 1, {
        let mut p = base(host_c, 2, &[]);
        p.stance = 0;
        p.retreat_bps = 0;
        p.dest_tile = 0;
        p
    }, Tamper::None, None);
    add("valid_q4_max", "valid", r4, &s4, 2, {
        let mut p = base(host_b, 1_007, &dirs32);
        p.stance = 3;
        p.retreat_bps = 60_000;
        p.dest_p = i16::MIN;
        p.dest_q = i16::MAX;
        p.dest_tile = 60;
        p
    }, Tamper::None, None);
    add("valid_spv2_32551361", "valid", rb1, &sb1, 3, base(host_a, 300, &[3, 3, 4]), Tamper::None, None);
    add("valid_spv2_32553016", "valid", rb2, &sb2, 4, base(host_c, 72, &[5]), Tamper::None, None);
    add("bad_plaintext_retreat", "bad_plaintext", r3, &s3, 5, {
        let mut p = base(host_a, 144, &[0]);
        p.retreat_bps = 60_001;
        p
    }, Tamper::None, None);
    add("bad_plaintext_version", "bad_plaintext", r3, &s3, 6, {
        let mut p = base(host_a, 144, &[0]);
        p.version = 2;
        p
    }, Tamper::None, None);
    add("bad_plaintext_reserved", "bad_plaintext", r4, &s4, 7, {
        let mut p = base(host_a, 144, &[0]);
        p.reserved = [0, 0, 1];
        p
    }, Tamper::None, None);
    add("bad_plaintext_path_bits", "bad_plaintext", r4, &s4, 8, {
        let mut p = base(host_a, 144, &[1, 1]);
        p.path_len = 1;
        p
    }, Tamper::None, None);
    add("bad_plaintext_stance", "bad_plaintext", r4, &s4, 9, {
        let mut p = base(host_a, 144, &[1]);
        p.stance = 4;
        p
    }, Tamper::None, None);
    // integ-W1 review: the checks the first set missed (kernel I-28 plus
    // Tile and Direction).
    add("bad_plaintext_tile_61", "bad_plaintext", r3, &s3, 16, {
        let mut p = base(host_a, 144, &[0]);
        p.dest_tile = 61;
        p
    }, Tamper::None, None);
    add("bad_plaintext_tile_255", "bad_plaintext", r4, &s4, 17, {
        let mut p = base(host_c, 144, &[4]);
        p.dest_tile = 255;
        p
    }, Tamper::None, None);
    add("bad_plaintext_direction_6", "bad_plaintext", r3, &s3, 18, {
        let mut p = base(host_a, 144, &[]);
        p.path_len = 1;
        p.path[0] = 6;
        p
    }, Tamper::None, None);
    add("bad_plaintext_direction_7", "bad_plaintext", r4, &s4, 19, {
        let mut p = base(host_a, 144, &[0, 0]);
        p.path_len = 3;
        p.path[0] |= 7 << 6; // step 2 spans bytes 0-1: bits 6..9
        p.path[1] |= 1;
        p
    }, Tamper::None, None);
    add("bad_plaintext_path_33", "bad_plaintext", r3, &s3, 20, {
        let mut p = base(host_a, 144, &[]);
        p.path_len = 33;
        p
    }, Tamper::None, None);
    add("bad_plaintext_host_mismatch", "bad_plaintext", r4, &s4, 21, base(host_b, 144, &[1]), Tamper::None, None);
    add("bad_plaintext_arrive_mismatch", "bad_plaintext", r3, &s3, 22, base(host_a, 145, &[1]), Tamper::None, None);
    add("wrong_round", "fo_fail", r3, &s3, 10, base(host_a, 144, &[2]), Tamper::None, Some(&s4));
    add("tampered_u", "bad_point_or_fo_fail", r3, &s3, 11, base(host_a, 144, &[2]), Tamper::U, None);
    add("tampered_v", "fo_fail", r3, &s3, 12, base(host_a, 144, &[2]), Tamper::V, None);
    add("tampered_w", "fo_fail", r4, &s4, 13, base(host_a, 144, &[2]), Tamper::W, None);
    add("tampered_body", "commit_mismatch", r4, &s4, 14, base(host_a, 144, &[2]), Tamper::Body, None);
    add("forged_commit", "commit_mismatch", r4, &s4, 15, base(host_a, 144, &[2]), Tamper::ForgedCommit, None);
    for c in cases.iter_mut() {
        if c.name == "bad_plaintext_host_mismatch" || c.name == "bad_plaintext_arrive_mismatch" {
            c.order = Some((host_a, 144));
        }
    }

    let mut vs = vec![];
    for c in &cases {
        let pt = seal::pack(&c.plain);
        let salt = seal::salt_of(&c.k);
        assert_eq!(salt, sha(&[b"PS-SALT", &c.k]));
        let body = body_ref(&c.k, &pt);
        assert_eq!(body, seal::body_xor(&c.k, &pt), "kernel body_xor");
        let mut commit = sha(&[b"PS-FRONTIER-MARCH-v1", &pt, &salt]);
        assert_eq!(commit, seal::commit(&pt, &salt), "kernel commit");
        let ibe = ibe_encrypt(&pk, c.round, &c.sigma, &c.k);
        // A freshly sealed pair opens with the stock tlock crate.
        assert_eq!(stock_open(&ibe, &c.sig), Some(c.k), "{}: stock open of the untampered seal", c.name);
        let mut s = [0u8; 165];
        s[..128].copy_from_slice(&ibe);
        s[128..].copy_from_slice(&body);
        match c.tamper {
            Tamper::None => {}
            Tamper::U => s[40] ^= 0x01,
            Tamper::V => s[100] ^= 0x01,
            Tamper::W => s[120] ^= 0x01,
            Tamper::Body => s[150] ^= 0x01,
            Tamper::ForgedCommit => commit = sha(&[b"PS-FRONTIER-MARCH-v1", &[0u8; 37], &salt]),
        }
        let open_sig = c.open_sig.clone().unwrap_or_else(|| c.sig.clone());
        let opened = stock_open(&s[..128], &open_sig);
        let opens_to_commit = opened
            .map(|k| sha(&[b"PS-FRONTIER-MARCH-v1", &body_ref(&k, s[128..].try_into().unwrap()), &sha(&[b"PS-SALT", &k])]) == commit)
            .unwrap_or(false);
        let (order_host, order_arrive) = c.order.unwrap_or((c.plain.host_id, c.arrive_bell));
        let validate = match seal::validate(&c.plain, order_host, order_arrive) {
            Ok(()) => "ok".to_string(),
            Err(e) => format!("{e:?}"),
        };
        // Consistency of the class with what stock tlock says.
        match c.expect {
            "valid" => assert!(opens_to_commit && validate == "ok", "{}", c.name),
            "bad_plaintext" => assert!(opens_to_commit && validate != "ok", "{}", c.name),
            "commit_mismatch" => assert!(opened.is_some() && !opens_to_commit, "{}", c.name),
            _ => assert!(opened.is_none(), "{}", c.name),
        }
        let ct_hash = sha(&[&s]);
        let root = sha(&[&commit, &ct_hash]);
        // A season in which T(arrive) is exactly this round: bell_end(arrive) = round_time(round).
        let rt = beacon::round_time(beacon::QUICKNET.genesis, 3, c.round);
        let genesis_ts = rt - 600 * (order_arrive as i64 + 1);
        assert_eq!(beacon::tlock_round(&beacon::QUICKNET, genesis_ts, order_arrive), c.round);
        vs.push(json!({
            "name": c.name,
            "expect": c.expect,
            "round": c.round,
            "sig": hx(&c.sig),
            "open_sig": hx(&open_sig),
            "genesis_ts": genesis_ts,
            "arrive_bell": order_arrive,
            "host_id": order_host.to_string(),
            "k": hx(&c.k),
            "sigma": hx(&c.sigma),
            "plain": hx(&pt),
            "salt": hx(&salt),
            "commit": hx(&commit),
            "seal": hx(&s),
            "ct_hash": hx(&ct_hash),
            "seal_root": hx(&root),
            "validate": validate,
            "stock_tlock_opens": opened.is_some(),
            "stock_k": opened.map(|k| hx(&k)),
        }));
    }

    // The recorded tlock-js compact-16 seal of S-TLOCK q4 (random k, the
    // pre-3.1 69-byte plaintext layout): it opens, and its first 37 bytes
    // are not a valid 3.1 plaintext.
    {
        let ct16 = hv(&q4, "ct16");
        let commit16 = hv(&q4, "commit16");
        let pt16 = hv(&q4, "pt16");
        let k = stock_open(&ct16[..128], &s4).expect("q4 ct16 opens");
        let body = body_ref(&k, ct16[128..].try_into().unwrap());
        assert_eq!(&body[..], &pt16[..37]);
        assert_eq!(&sha(&[b"PS-SALT", &k])[..], &pt16[37..]);
        let salt = sha(&[b"PS-SALT", &k]);
        assert_eq!(sha(&[b"PS-FRONTIER-MARCH-v1", &body, &salt]).to_vec(), commit16);
        let plain = seal::unpack(&body);
        let validate = match seal::validate(&plain, plain.host_id, plain.arrive_bell) {
            Ok(()) => "ok".to_string(),
            Err(e) => format!("{e:?}"),
        };
        let s: [u8; 165] = ct16.as_slice().try_into().unwrap();
        let ct_hash = sha(&[&s]);
        vs.push(json!({
            "name": "recorded_q4_tlockjs",
            "expect": "bad_plaintext",
            "round": r4,
            "sig": hx(&s4),
            "open_sig": hx(&s4),
            "genesis_ts": null,
            "arrive_bell": plain.arrive_bell,
            "host_id": plain.host_id.to_string(),
            "k": hx(&k),
            "sigma": null,
            "plain": hx(&body),
            "salt": hx(&salt),
            "commit": hx(&commit16),
            "seal": hx(&s),
            "ct_hash": hx(&ct_hash),
            "seal_root": hx(&sha(&[&commit16, &ct_hash])),
            "validate": validate,
            "stock_tlock_opens": true,
            "stock_k": hx(&k),
        }));
    }

    let doc = json!({
        "version": 1,
        "producer": "scratchpad/frontier/m1/lab/w1c-seal-vectors (tlock =0.0.10, ark 0.6; source copied to permutation-rules/vectors/seal-vectors-gen.rs)",
        "network": "quicknet",
        "public_key": QUICKNET_PK,
        "scheme": "compact-16: U 96 (G2 compressed) | V 16 | W 16 | body 37 = plain XOR sha256('PS-KS'||k||c); salt = sha256('PS-SALT'||k); commit = sha256('PS-FRONTIER-MARCH-v1'||plain||salt); ct_hash = sha256(seal); seal_root = sha256(commit||ct_hash)",
        "expect_classes": {
            "valid": "opens, commitment matches, Plain::validate ok (seal code 0)",
            "bad_plaintext": "opens, commitment matches, Plain::validate fails (seal code 5)",
            "commit_mismatch": "IBE opens, the opened body does not hash to the commitment (seal code 4)",
            "fo_fail": "the stock opener refuses (FO check; the program's code 1 or 3)",
            "bad_point_or_fo_fail": "U altered: not a valid point or FO failure (code 2 or 1)"
        },
        "cases": vs,
    });
    std::fs::write(out, serde_json::to_string_pretty(&doc).unwrap() + "\n").unwrap();
    println!("wrote {} cases to {out}", cases.len() + 1);
}
