//! SHA-256 over a list of byte slices. Every hash in the rules goes through
//! here: on Solana it is the `sol_sha256` syscall, elsewhere the `sha2`
//! crate. Both produce the same digest for the same concatenated bytes.

pub type Digest32 = [u8; 32];

#[cfg(target_os = "solana")]
#[allow(unsafe_code)]
pub fn sha256(parts: &[&[u8]]) -> Digest32 {
    solana_define_syscall::define_syscall!(fn sol_sha256(vals: *const u8, val_len: u64, hash_result: *mut u8) -> u64);
    let mut out = [0u8; 32];
    // SAFETY: `parts` is a slice of `&[u8]` (pointer, length) pairs, the ABI
    // the syscall expects; `out` has room for the 32-byte digest.
    unsafe {
        sol_sha256(
            parts.as_ptr() as *const u8,
            parts.len() as u64,
            out.as_mut_ptr(),
        );
    }
    out
}

#[cfg(not(target_os = "solana"))]
pub fn sha256(parts: &[&[u8]]) -> Digest32 {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}
