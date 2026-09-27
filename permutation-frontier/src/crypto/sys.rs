//! Raw SIMD-0388 BLS12-381 syscalls (signatures as in Agave 4.x; declared
//! here because the pinned `solana-define-syscall` predates the 5-argument
//! `sol_curve_pairing_map`). Ported from SP-V2 unchanged. On the host every
//! call fails (returns non-zero), so host code can never "verify" anything.

#[cfg(target_os = "solana")]
extern "C" {
    fn sol_curve_group_op(
        curve: u64,
        op: u64,
        left: *const u8,
        right: *const u8,
        result: *mut u8,
    ) -> u64;
    fn sol_curve_decompress(curve: u64, point: *const u8, result: *mut u8) -> u64;
    fn sol_curve_pairing_map(
        curve: u64,
        n: u64,
        g1: *const u8,
        g2: *const u8,
        result: *mut u8,
    ) -> u64;
}

pub const BLS12_381_BE: u64 = 4 | 0x80;
pub const BLS12_381_G1_BE: u64 = 5 | 0x80;
pub const BLS12_381_G2_BE: u64 = 6 | 0x80;
pub const GROUP_OP_ADD: u64 = 0;
pub const GROUP_OP_MUL: u64 = 2;

#[cfg(not(target_os = "solana"))]
unsafe fn sol_curve_group_op(_: u64, _: u64, _: *const u8, _: *const u8, _: *mut u8) -> u64 {
    1
}
#[cfg(not(target_os = "solana"))]
unsafe fn sol_curve_decompress(_: u64, _: *const u8, _: *mut u8) -> u64 {
    1
}
#[cfg(not(target_os = "solana"))]
unsafe fn sol_curve_pairing_map(_: u64, _: u64, _: *const u8, _: *const u8, _: *mut u8) -> u64 {
    1
}

/// Why a syscall refused its input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysError {
    /// A group operation refused its operands.
    Syscall,
    /// Not a point (or not in the subgroup where the syscall checks it).
    BadPoint,
}

/// G1 addition of two uncompressed big-endian (Zcash) points: on-curve
/// checked, subgroup unchecked (SIMD-0388).
pub fn bls_g1_add(a: &[u8; 96], b: &[u8; 96]) -> Result<[u8; 96], SysError> {
    let mut out = [0u8; 96];
    // SAFETY: both inputs are 96-B buffers and `out` has room for 96 B, the
    // sizes the syscall reads and writes for a G1 group operation.
    let rc = unsafe {
        sol_curve_group_op(
            BLS12_381_G1_BE,
            GROUP_OP_ADD,
            a.as_ptr(),
            b.as_ptr(),
            out.as_mut_ptr(),
        )
    };
    if rc != 0 {
        return Err(SysError::Syscall);
    }
    Ok(out)
}

/// G1 decompression (48-B Zcash big-endian), subgroup checked.
pub fn bls_g1_decompress(c: &[u8; 48]) -> Result<[u8; 96], SysError> {
    let mut out = [0u8; 96];
    // SAFETY: 48-B input, 96-B output, as the syscall expects for G1.
    let rc = unsafe { sol_curve_decompress(BLS12_381_G1_BE, c.as_ptr(), out.as_mut_ptr()) };
    if rc != 0 {
        return Err(SysError::BadPoint);
    }
    Ok(out)
}

/// G2 decompression (96-B Zcash big-endian), subgroup checked; `None` if
/// the bytes are not a G2 point.
pub fn bls_g2_decompress(c: &[u8; 96]) -> Option<[u8; 192]> {
    let mut out = [0u8; 192];
    // SAFETY: 96-B input, 192-B output, as the syscall expects for G2.
    let rc = unsafe { sol_curve_decompress(BLS12_381_G2_BE, c.as_ptr(), out.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    Some(out)
}

/// `scalar` (32-B big-endian, below r) times the G2 point `p`.
pub fn bls_g2_mul(scalar: &[u8; 32], p: &[u8; 192]) -> Option<[u8; 192]> {
    let mut out = [0u8; 192];
    // SAFETY: 32-B scalar, 192-B point, 192-B output (G2 multiplication).
    let rc = unsafe {
        sol_curve_group_op(
            BLS12_381_G2_BE,
            GROUP_OP_MUL,
            scalar.as_ptr(),
            p.as_ptr(),
            out.as_mut_ptr(),
        )
    };
    if rc != 0 {
        return None;
    }
    Some(out)
}

/// Product of `n` pairings; the points are fully validated by the syscall
/// (on curve, prime-order subgroup). `g1` holds `n` × 96 B, `g2` `n` ×
/// 192 B; the result is the 576-B big-endian Gt element.
pub fn bls_pairing(g1: &[u8], g2: &[u8], n: u64, out: &mut [u8; 576]) -> Result<(), SysError> {
    let n_us = n as usize;
    if g1.len() < 96 * n_us || g2.len() < 192 * n_us {
        return Err(SysError::BadPoint);
    }
    // SAFETY: the lengths were checked above for `n` pairs; `out` is 576 B.
    let rc = unsafe {
        sol_curve_pairing_map(BLS12_381_BE, n, g1.as_ptr(), g2.as_ptr(), out.as_mut_ptr())
    };
    if rc != 0 {
        return Err(SysError::BadPoint);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_stubs_fail_closed() {
        assert!(bls_g1_decompress(&[0; 48]).is_err());
        assert!(bls_g2_decompress(&[0; 96]).is_none());
        assert!(bls_g1_add(&[0; 96], &[0; 96]).is_err());
        assert!(bls_g2_mul(&[0; 32], &[0; 192]).is_none());
        let mut o = [0u8; 576];
        assert!(bls_pairing(&[0; 96], &[0; 192], 1, &mut o).is_err());
        assert!(bls_pairing(&[0; 95], &[0; 192], 1, &mut o).is_err());
    }
}
