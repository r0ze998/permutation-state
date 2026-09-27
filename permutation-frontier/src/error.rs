//! Errors (M1 contract §5.4): the stable codes live in
//! `frontier_abi::FrontierError`; this module carries them through the
//! program as [`Error`], together with the runtime's own errors (a failed
//! CPI, a borrow conflict), and turns them into `ProgramError::Custom(code)`
//! at the entrypoint.
//!
//! Two codes carry a sub-code in the log (§5.4): `Crypto` (9) and `Kernel`
//! (15). [`crypto`] and [`kernel`] log `sol_log_64(SUB_TAG, code, sub, 0,
//! 0)` before returning the code, so the keeper and the verifier can tell a
//! bad hint from a failed pairing without a new error code.

use solana_program::program_error::ProgramError;

pub use frontier_abi::FrontierError;

/// First argument of the sub-code log line (`"PSFE"`).
pub const SUB_TAG: u64 = 0x5053_4645;

/// Crypto sub-codes (§5.4 code 9).
pub mod crypto_sub {
    /// A hash-to-curve hint does not check (SSWU branch, inverse, sign).
    pub const BAD_HINT: u64 = 1;
    /// A point does not decompress or is not in the subgroup.
    pub const BAD_POINT: u64 = 2;
    /// The pairing equation does not hold (wrong signature or round).
    pub const PAIRING_FAILED: u64 = 3;
    /// A BLS12-381 syscall refused its input.
    pub const SYSCALL: u64 = 4;
    /// Malformed signature bytes (flags).
    pub const BAD_DATA: u64 = 5;
}

/// A program failure: a Frontier code or a runtime error passed through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Frontier(FrontierError),
    Program(ProgramError),
}

/// Result of every handler and helper.
pub type R<T> = Result<T, Error>;

impl From<FrontierError> for Error {
    fn from(e: FrontierError) -> Self {
        Error::Frontier(e)
    }
}

impl From<ProgramError> for Error {
    fn from(e: ProgramError) -> Self {
        Error::Program(e)
    }
}

impl From<Error> for ProgramError {
    fn from(e: Error) -> Self {
        match e {
            Error::Frontier(f) => ProgramError::Custom(f.code()),
            Error::Program(p) => p,
        }
    }
}

impl Error {
    /// The Frontier code, if this is one.
    pub fn code(&self) -> Option<u32> {
        match self {
            Error::Frontier(f) => Some(f.code()),
            Error::Program(_) => None,
        }
    }
}

/// `Crypto` (9) with a logged sub-code.
pub fn crypto(sub: u64) -> Error {
    log_sub(FrontierError::Crypto, sub);
    Error::Frontier(FrontierError::Crypto)
}

/// `Kernel` (15) with a logged sub-code.
pub fn kernel(sub: u64) -> Error {
    log_sub(FrontierError::Kernel, sub);
    Error::Frontier(FrontierError::Kernel)
}

fn log_sub(code: FrontierError, sub: u64) {
    #[cfg(target_os = "solana")]
    solana_program::log::sol_log_64(SUB_TAG, code.code() as u64, sub, 0, 0);
    #[cfg(not(target_os = "solana"))]
    let _ = (code, sub);
}

/// `Overflow` (19): the checked-arithmetic refusal.
pub const OVERFLOW: Error = Error::Frontier(FrontierError::Overflow);

/// `BadAccount` (2) shorthand for layout reads that cannot fail once the
/// account's size has been checked.
pub const BAD_ACCOUNT: Error = Error::Frontier(FrontierError::BadAccount);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontier_codes_become_custom_errors() {
        for e in FrontierError::ALL {
            let p: ProgramError = Error::from(e).into();
            assert_eq!(p, ProgramError::Custom(e.code()));
        }
        let p: ProgramError = Error::from(ProgramError::AccountBorrowFailed).into();
        assert_eq!(p, ProgramError::AccountBorrowFailed);
        assert_eq!(crypto(crypto_sub::BAD_HINT).code(), Some(9));
        assert_eq!(kernel(3).code(), Some(15));
    }
}
