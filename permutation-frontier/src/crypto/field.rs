//! Byte conversions for ark prime fields (big-endian, canonical) and the
//! out-of-line multiplication the SBF stack needs. Ported from SP-V2
//! unchanged.

use ark_ff::{BigInt, PrimeField};

/// `bytes` (big-endian, length a multiple of 16) reduced mod p, by Horner
/// over 128-bit chunks (every chunk is below p for BLS12-381's Fq).
pub fn from_be_mod<F: PrimeField>(bytes: &[u8]) -> F {
    let two128 = F::from(u128::MAX) + F::one();
    let mut acc = F::zero();
    for c in bytes.chunks(16) {
        let mut b = [0u8; 16];
        let n = c.len().min(16);
        b[16 - n..].copy_from_slice(&c[..n]);
        acc = acc * two128 + F::from(u128::from_be_bytes(b));
    }
    acc
}

/// Canonical big-endian decoding; `None` if the value is not below p or
/// the length is not `8 N`.
pub fn from_be<F: PrimeField<BigInt = BigInt<N>>, const N: usize>(bytes: &[u8]) -> Option<F> {
    if bytes.len() != 8 * N {
        return None;
    }
    let mut limbs = [0u64; N];
    for (i, limb) in limbs.iter_mut().enumerate() {
        let mut w = [0u8; 8];
        w.copy_from_slice(bytes.get((N - 1 - i) * 8..(N - i) * 8)?);
        *limb = u64::from_be_bytes(w);
    }
    F::from_bigint(BigInt(limbs))
}

/// Canonical big-endian encoding into `out` (`8 N` bytes; shorter output
/// is left untouched).
pub fn to_be<F: PrimeField<BigInt = BigInt<N>>, const N: usize>(f: &F, out: &mut [u8]) {
    if out.len() < 8 * N {
        return;
    }
    let limbs = f.into_bigint().0;
    for (i, limb) in limbs.iter().enumerate() {
        out[(N - 1 - i) * 8..(N - i) * 8].copy_from_slice(&limb.to_be_bytes());
    }
}

/// RFC 9380 `sgn0` for a prime field: the parity of the canonical integer.
pub fn sgn0<F: PrimeField<BigInt = BigInt<N>>, const N: usize>(f: &F) -> u64 {
    f.into_bigint().0[0] & 1
}

/// Out-of-line multiplication (see the module note of [`super`]).
#[inline(never)]
pub fn mul<F: ark_ff::Field>(a: F, b: F) -> F {
    a * b
}

/// Out-of-line squaring.
#[inline(never)]
pub fn sqr<F: ark_ff::Field>(a: F) -> F {
    a.square()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_bls12_381::Fq;
    use ark_ff::{One, Zero};

    #[test]
    fn be_codec_round_trips_and_refuses_non_canonical() {
        let x: Fq = from_be_mod(&[0xAB; 64]);
        let mut b = [0u8; 48];
        to_be::<Fq, 6>(&x, &mut b);
        assert_eq!(from_be::<Fq, 6>(&b), Some(x));
        assert_eq!(from_be::<Fq, 6>(&[0xFF; 48]), None, "above p");
        assert_eq!(from_be::<Fq, 6>(&[0; 47]), None, "short");
        let mut one = [0u8; 48];
        one[47] = 1;
        assert_eq!(from_be::<Fq, 6>(&one), Some(Fq::one()));
        assert_eq!(sgn0::<Fq, 6>(&Fq::one()), 1);
        assert_eq!(sgn0::<Fq, 6>(&Fq::zero()), 0);
        assert_eq!(mul(x, Fq::one()), x);
        assert_eq!(sqr(x), x * x);
    }
}
