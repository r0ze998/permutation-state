//! Fixed-point conventions (§0.1). No floating point anywhere in the crate.

/// Resources in milli-units (1 unit = 1000). Signed so intermediate deficits
/// can be represented before they are resolved (§6.1).
pub type Milli = i64;

/// Troop counts in milli-troops (1 troop = 1000).
pub type MilliTroops = u32;

/// Multipliers and percentages in basis points (10000 = 100%).
pub type Bps = u32;

pub const MILLI: i64 = 1000;
pub const BPS_ONE: Bps = 10_000;

/// Whole units → milli-units.
#[inline]
pub const fn milli(units: i64) -> Milli {
    units * MILLI
}

/// `x × bps / 10000`, truncating toward zero (§0.1 rounding rule).
#[inline]
pub const fn apply_bps(x: i64, bps: Bps) -> i64 {
    x * bps as i64 / BPS_ONE as i64
}

/// `x × bps / 10000` for unsigned values.
#[inline]
pub const fn apply_bps_u64(x: u64, bps: Bps) -> u64 {
    x * bps as u64 / BPS_ONE as u64
}

/// Integer square root (floor), used by the growth threshold (§5.3).
pub const fn isqrt(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    // Newton iteration from an upper bound; converges monotonically downward.
    let mut x = n;
    let mut y = (x >> 1) + (x & 1); // ceil(n / 2) without overflow
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isqrt_matches_floor_sqrt() {
        for n in 0u64..10_000 {
            let r = isqrt(n);
            assert!(r * r <= n && (r + 1) * (r + 1) > n, "n={n} r={r}");
        }
        assert_eq!(isqrt(u64::MAX), 4_294_967_295);
    }

    #[test]
    fn bps_truncates_toward_zero() {
        assert_eq!(apply_bps(10_000, 8_500), 8_500);
        assert_eq!(apply_bps(3, 5_000), 1);
        assert_eq!(apply_bps(-3, 5_000), -1);
    }
}
