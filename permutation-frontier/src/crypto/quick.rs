//! drand quicknet (`bls-unchained-g1-rfc9380`) signature verification with
//! hash-to-curve hints (S-BEACON, SP-V2 `quick.rs`).
//!
//! RFC 9380 BLS12381G1_XMD:SHA-256_SSWU_RO_: `u0, u1` = the two 64-byte
//! halves of `expand_message_xmd(sha256(be64(round)), DST, 128)` reduced
//! mod p; `Q_i = iso11(SSWU(u_i))`; `H = h_eff · (Q0 + Q1)` with `h_eff =
//! 0xd201000000010001`. Accept iff `e(sig, −G2) · e(H, pk) = 1` (the pairing
//! syscall checks both G1 points are in the prime-order subgroup). The
//! prover supplies, per `u_i`, the SSWU branch, `1/tv1`, the square root
//! `y'` and `1/(x_den · y_den)`; the program checks each hint with a few
//! multiplications instead of computing square roots and inversions.
//!
//! **Keys (I-53).** A normal build verifies against the quicknet group key
//! (`QUICKNET_PK_HASH` = `frontier_abi::presets::QUICKNET_PK_HASH`). A
//! build with feature `test-beacon` verifies against the deterministic
//! local key of `drand-replay --test-key` (`sk = hash_to_field(
//! "PSF-TEST-BEACON-v1")`, fclient's `TestKey`) and pins that key's own
//! hash; it carries the marker `PSF_TEST_BEACON_BUILD` and
//! `build-frontier.sh` never ships it.

use ark_bls12_381::{g1::Config as G1Config, Fq};
use ark_ec::hashing::curve_maps::wb::WBConfig;
use ark_ff::{AdditiveGroup, Field, MontFp, Zero};

use super::field::{from_be, from_be_mod, mul, sgn0, sqr, to_be};
use super::{sys, xmd};
use crate::error::crypto_sub;

pub const DST: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_";
/// drand network id of quicknet (the seed domain carries it).
pub const NET_QUICKNET: u8 = 2;
/// Domain of the bell seeds: `sha256("PSF-SEED-v1" ‖ net ‖ be64(round) ‖
/// sig96)` (SP-V2 `seed_of`).
pub const SEED_DOMAIN: &[u8] = frontier_abi::layout::beacon::seed_cache::SEED_DOMAIN;

/// The quicknet group public key, uncompressed big-endian (Zcash) G2,
/// decompressed from `https://api.drand.sh/v2/beacons/quicknet/info`.
#[cfg(not(feature = "test-beacon"))]
pub const PK: &[u8; 192] = include_bytes!("consts/quicknet_pk_g2_be.bin");
/// The published 96-B compressed key (its hash is `PK_HASH`).
#[cfg(not(feature = "test-beacon"))]
pub const PK96: [u8; 96] = frontier_abi::presets::QUICKNET_PUBLIC_KEY;
/// `sha256(PK96)`: CreateSeason refuses a season naming another key.
#[cfg(not(feature = "test-beacon"))]
pub const PK_HASH: [u8; 32] = frontier_abi::presets::QUICKNET_PK_HASH;

/// The local test key (I-53), uncompressed big-endian G2 (fclient
/// `TestKey::new().pk96`, decompressed with blstrs).
#[cfg(feature = "test-beacon")]
pub const PK: &[u8; 192] = include_bytes!("consts/test_beacon_pk_g2_be.bin");
/// The test key, 96-B compressed (`permutation-gateway/test/frontier-vectors.json`
/// `beacon.test_key.info.public_key`).
#[cfg(feature = "test-beacon")]
pub const PK96: [u8; 96] = TEST_PK96;
/// `sha256(TEST_PK96)` (fclient `beacon::pk_hash`).
#[cfg(feature = "test-beacon")]
pub const PK_HASH: [u8; 32] = TEST_PK_HASH;

/// The test key, compressed (both builds know it, so host tests can check
/// the constants; only a `test-beacon` build verifies against it).
pub const TEST_PK96: [u8; 96] = hex96(
    b"97d982017d4a0e479d996abe5672544640704d96eb9f846006e41bbedb3ea41ad1089c266769526f85ae306171f0db85\
0ab461df81d7dd0c805169f5990203966e1e95646a54ca50a05e733f1c71242779917f5fc8a0e7c1f1d25914310c98b9",
);
/// `sha256(TEST_PK96)`.
pub const TEST_PK_HASH: [u8; 32] =
    hex32(b"faa6e379244f54e13045e9810e27ff540156e9cc7dc2f15747f95df12e41b9ef");
/// The test key, uncompressed (for the host consistency test).
pub const TEST_PK: &[u8; 192] = include_bytes!("consts/test_beacon_pk_g2_be.bin");
/// The quicknet key, uncompressed (for the host consistency test).
pub const QUICKNET_PK: &[u8; 192] = include_bytes!("consts/quicknet_pk_g2_be.bin");

/// −G2 generator, uncompressed big-endian.
pub const NEG_G2: &[u8; 192] = include_bytes!("consts/bls_neg_g2_be.bin");
/// The identity of Gt as the pairing syscall serialises it (big-endian).
pub const GT_ONE: &[u8; 576] = include_bytes!("consts/bls_gt_one_be.bin");

// E': y^2 = x^3 + A'x + B', Z = 11 (RFC 9380 §8.8.1).
const A: Fq = MontFp!("12190336318893619529228877361869031420615612348429846051986726275283378313155663745811710833465465981901188123677");
const B: Fq = MontFp!("2906670324641927570491258158026293881577086121416628140204402091718288198173574630967936031029026176254968826637280");
const Z: Fq = MontFp!("11");
/// −B'/A'.
const NEG_B_OVER_A: Fq = MontFp!("1165829013300031051498189320085913366300917435352691079268722832469236884678791583237077507172460989948189414825084");
/// h_eff for G1 = 1 − x = 0xd201000000010001.
const H_EFF: u64 = 0xd201_0000_0001_0001;

/// One hint: the SSWU branch (1: x1, 2: x2 = Z u² x1), 1/tv1, the chosen
/// y' on E', and 1/(x_den(x') · y_den(x')) for the isogeny.
pub struct Hint {
    pub branch: u8,
    pub inv_tv1: Fq,
    pub y: Fq,
    pub inv_den: Fq,
}

/// Hint length (SP-V2 `quick::HINT_LEN`, `frontier_abi::ix::HINT_LEN`).
pub const HINT_LEN: usize = 1 + 3 * 48;
const _: () = assert!(HINT_LEN == frontier_abi::ix::HINT_LEN);

/// Verification failures, mapped to the `Crypto` sub-codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyError {
    BadHint,
    BadPoint,
    PairingFailed,
    Syscall,
}

impl VerifyError {
    pub const fn sub_code(self) -> u64 {
        match self {
            VerifyError::BadHint => crypto_sub::BAD_HINT,
            VerifyError::BadPoint => crypto_sub::BAD_POINT,
            VerifyError::PairingFailed => crypto_sub::PAIRING_FAILED,
            VerifyError::Syscall => crypto_sub::SYSCALL,
        }
    }
}

impl From<sys::SysError> for VerifyError {
    fn from(e: sys::SysError) -> Self {
        match e {
            sys::SysError::Syscall => VerifyError::Syscall,
            sys::SysError::BadPoint => VerifyError::BadPoint,
        }
    }
}

impl From<VerifyError> for crate::Error {
    fn from(e: VerifyError) -> Self {
        crate::error::crypto(e.sub_code())
    }
}

fn fe(b: &[u8]) -> Result<Fq, VerifyError> {
    from_be::<Fq, 6>(b).ok_or(VerifyError::BadHint)
}

/// Parses one 145-B hint.
pub fn parse_hint(d: &[u8]) -> Result<Hint, VerifyError> {
    if d.len() < HINT_LEN {
        return Err(VerifyError::BadHint);
    }
    let branch = d[0];
    if branch != 1 && branch != 2 {
        return Err(VerifyError::BadHint);
    }
    Ok(Hint {
        branch,
        inv_tv1: fe(&d[1..49])?,
        y: fe(&d[49..97])?,
        inv_den: fe(&d[97..145])?,
    })
}

fn gp(x: Fq) -> Fq {
    mul(sqr(x) + A, x) + B
}

fn horner(c: &[Fq], x: Fq) -> Fq {
    let mut acc = Fq::ZERO;
    for k in c.iter().rev() {
        acc = mul(acc, x) + k;
    }
    acc
}

/// Simplified SWU on E' (RFC 9380 §6.6.2) with hints, then the 11-isogeny
/// to E. If g(x1) is a square only branch 1 admits a y; if not, g(x2) = Z³
/// u⁶ g(x1) is a square and only branch 2 does, so the branch is forced.
pub fn map_hinted(u: Fq, h: &Hint) -> Result<(Fq, Fq), VerifyError> {
    let zu2 = mul(Z, sqr(u));
    let tv1 = sqr(zu2) + zu2;
    if mul(tv1, h.inv_tv1) != Fq::ONE {
        return Err(VerifyError::BadHint); // tv1 = 0 (exceptional case) not supported
    }
    let x1 = mul(NEG_B_OVER_A, Fq::ONE + h.inv_tv1);
    let xp = if h.branch == 1 {
        if sqr(h.y) != gp(x1) {
            return Err(VerifyError::BadHint);
        }
        x1
    } else {
        let x2 = mul(zu2, x1);
        let g2 = gp(x2);
        if g2.is_zero() || sqr(h.y) != g2 {
            return Err(VerifyError::BadHint);
        }
        x2
    };
    if sgn0::<Fq, 6>(&h.y) != sgn0::<Fq, 6>(&u) {
        return Err(VerifyError::BadHint);
    }
    let m = &G1Config::ISOGENY_MAP;
    let xn = horner(m.x_map_numerator, xp);
    let xd = horner(m.x_map_denominator, xp);
    let yn = horner(m.y_map_numerator, xp);
    let yd = horner(m.y_map_denominator, xp);
    if mul(mul(xd, yd), h.inv_den) != Fq::ONE {
        return Err(VerifyError::BadHint);
    }
    let x = mul(mul(xn, yd), h.inv_den);
    let y = mul(mul(mul(h.y, yn), xd), h.inv_den);
    Ok((x, y))
}

fn enc(x: Fq, y: Fq) -> [u8; 96] {
    let mut o = [0u8; 96];
    to_be::<Fq, 6>(&x, &mut o[..48]);
    to_be::<Fq, 6>(&y, &mut o[48..]);
    o
}

/// `h_eff · q` by double-and-add over the (subgroup-unchecked) G1 add
/// syscall; the multiplication syscall refuses points outside the subgroup.
pub fn clear_cofactor(q: &[u8; 96]) -> Result<[u8; 96], VerifyError> {
    let mut acc = *q;
    for bit in (0..63).rev() {
        acc = sys::bls_g1_add(&acc, &acc)?;
        if (H_EFF >> bit) & 1 == 1 {
            acc = sys::bls_g1_add(&acc, q)?;
        }
    }
    Ok(acc)
}

/// The two field elements `u0, u1` of round `round`.
pub fn round_us(round: u64) -> (Fq, Fq) {
    let msg = xmd::sha256(&[&round.to_be_bytes()]);
    let mut uu = [0u8; 128];
    xmd::xmd_sha256(&msg, DST, &mut uu);
    (from_be_mod(&uu[..64]), from_be_mod(&uu[64..]))
}

/// Verifies the compressed signature of `round` against [`PK`] with the
/// two hints. Returns the uncompressed signature (96 B), from which the
/// seed and the tlock opening are derived.
pub fn verify(
    round: u64,
    sig48: &[u8; 48],
    hints: &[u8; 2 * HINT_LEN],
) -> Result<[u8; 96], VerifyError> {
    crate::markers::touch_test_beacon();
    crate::heap::trace_checkpoint(20);
    let sig = sys::bls_g1_decompress(sig48)?;
    let h0 = parse_hint(&hints[..HINT_LEN])?;
    let h1 = parse_hint(&hints[HINT_LEN..])?;
    crate::heap::trace_checkpoint(21);
    let (u0, u1) = round_us(round);
    crate::heap::trace_checkpoint(22);
    let (x0, y0) = map_hinted(u0, &h0)?;
    let (x1, y1) = map_hinted(u1, &h1)?;
    crate::heap::trace_checkpoint(23);
    let q = sys::bls_g1_add(&enc(x0, y0), &enc(x1, y1))?;
    let h = clear_cofactor(&q)?;
    crate::heap::trace_checkpoint(24);
    let mut g1 = [0u8; 192];
    g1[..96].copy_from_slice(&sig);
    g1[96..].copy_from_slice(&h);
    let mut g2 = [0u8; 384];
    g2[..192].copy_from_slice(NEG_G2);
    g2[192..].copy_from_slice(PK);
    let mut gt = [0u8; 576];
    sys::bls_pairing(&g1, &g2, 2, &mut gt)?;
    crate::heap::trace_checkpoint(25);
    if &gt != GT_ONE {
        return Err(VerifyError::PairingFailed);
    }
    Ok(sig)
}

/// The bell seed of a verified round: `sha256("PSF-SEED-v1" ‖ 2 ‖
/// be64(round) ‖ sig96)`.
pub fn seed_of(round: u64, sig96: &[u8; 96]) -> [u8; 32] {
    xmd::sha256(&[SEED_DOMAIN, &[NET_QUICKNET], &round.to_be_bytes(), sig96])
}

const fn nib(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => 0,
    }
}

const fn hex96(s: &[u8]) -> [u8; 96] {
    let mut o = [0u8; 96];
    let mut i = 0;
    while i < 96 {
        o[i] = (nib(s[2 * i]) << 4) | nib(s[2 * i + 1]);
        i += 1;
    }
    o
}

pub(crate) const fn hex32(s: &[u8]) -> [u8; 32] {
    let mut o = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        o[i] = (nib(s[2 * i]) << 4) | nib(s[2 * i + 1]);
        i += 1;
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_bls12_381::{g2::Config as G2Config, Fq2};
    use ark_ec::short_weierstrass::Affine;

    fn g2_of(u: &[u8; 192]) -> Affine<G2Config> {
        let f = |o: usize| from_be::<Fq, 6>(&u[o..o + 48]).unwrap();
        let x = Fq2::new(f(48), f(0));
        let y = Fq2::new(f(144), f(96));
        Affine::new_unchecked(x, y)
    }

    /// The uncompressed keys the program pairs with are the compressed
    /// keys whose hashes the Season pins: same x, the compressed sign bit
    /// matches y, on the curve, in the subgroup.
    #[test]
    fn embedded_keys_match_their_hashes() {
        for (u, c, h) in [
            (
                QUICKNET_PK,
                frontier_abi::presets::QUICKNET_PUBLIC_KEY,
                frontier_abi::presets::QUICKNET_PK_HASH,
            ),
            (TEST_PK, TEST_PK96, TEST_PK_HASH),
        ] {
            assert_eq!(xmd::sha256(&[&c]), h);
            assert_eq!(c[0] & 0x80, 0x80, "compressed flag");
            assert_eq!(c[0] & 0x40, 0, "not infinity");
            assert_eq!(c[0] & 0x1f, u[0], "x high byte");
            assert_eq!(&c[1..96], &u[1..96], "x");
            assert_eq!(u[0] & 0xe0, 0, "uncompressed carries no flags");
            let p = g2_of(u);
            assert!(p.is_on_curve());
            assert!(p.is_in_correct_subgroup_assuming_on_curve());
            // Zcash sign: y is the lexicographically larger root.
            let larger = p.y > -p.y;
            assert_eq!(c[0] & 0x20 != 0, larger, "sign bit");
        }
        assert_ne!(PK_HASH, [0; 32]);
        assert_eq!(
            &PK96[..],
            if cfg!(feature = "test-beacon") {
                &TEST_PK96[..]
            } else {
                &frontier_abi::presets::QUICKNET_PUBLIC_KEY[..]
            }
        );
    }

    #[test]
    fn neg_g2_is_minus_the_generator_and_gt_one_is_one() {
        use ark_ec::AffineRepr;
        let g = g2_of(NEG_G2);
        assert!(g.is_on_curve());
        assert_eq!(-g, ark_bls12_381::G2Affine::generator());
        // Gt identity: Fq12 one = 1 followed by zeros in the syscall's
        // big-endian coefficient order (one coefficient is 1).
        assert_eq!(GT_ONE.iter().filter(|b| **b != 0).count(), 1);
        assert_eq!(GT_ONE.iter().map(|b| *b as u32).sum::<u32>(), 1);
    }

    #[test]
    fn hints_are_parsed_strictly() {
        assert_eq!(parse_hint(&[1u8; 10]).err(), Some(VerifyError::BadHint));
        let mut h = [0u8; HINT_LEN];
        h[0] = 3;
        assert_eq!(parse_hint(&h).err(), Some(VerifyError::BadHint));
        h[0] = 2;
        assert!(parse_hint(&h).is_ok());
        h[1] = 0xff; // above p
        assert_eq!(parse_hint(&h).err(), Some(VerifyError::BadHint));
    }

    /// The host has no BLS syscalls: verification never succeeds there.
    #[test]
    fn host_verification_fails_closed() {
        assert!(verify(1, &[0; 48], &[0; 2 * HINT_LEN]).is_err());
    }

    #[test]
    fn seed_is_the_spv2_derivation() {
        let s = [3u8; 96];
        let want = xmd::sha256(&[b"PSF-SEED-v1", &[2], &7u64.to_be_bytes(), &s]);
        assert_eq!(seed_of(7, &s), want);
    }
}
