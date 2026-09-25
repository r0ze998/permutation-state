//! Compute-unit probes for measurement builds: with the `cu-trace` feature on
//! Solana, `probe` logs its label and the compute units left; otherwise it
//! does nothing and compiles away.

#[cfg(all(feature = "cu-trace", target_os = "solana"))]
#[allow(unsafe_code)]
pub fn probe(label: &str) {
    use solana_define_syscall::define_syscall;
    define_syscall!(fn sol_log_(message: *const u8, len: u64));
    define_syscall!(fn sol_log_compute_units_());
    // SAFETY: `label` is a valid UTF-8 slice; both syscalls only read it.
    unsafe {
        sol_log_(label.as_ptr(), label.len() as u64);
        sol_log_compute_units_();
    }
}

#[cfg(not(all(feature = "cu-trace", target_os = "solana")))]
#[inline(always)]
pub fn probe(_label: &str) {}
