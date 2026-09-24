use solana_program::program_error::ProgramError;

/// Program errors, surfaced as `Custom(code)`. Clients map codes to text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ChainError {
    InvalidInstruction = 1,
    MissingSignature,
    WrongPda,
    AlreadyInitialized,
    NotInitialized,
    WrongStatus,
    Unauthorized,
    SeasonFull,
    InvalidName,
    WrongMint,
    WrongTokenAccount,
    WorldTooSmall,
    Rules,
    WrongTick,
    OverBudget,
    TooEarly,
    MissingOrders,
    WrongDelegationProgram,
    WrongMagicProgram,
    AlreadyClaimed,
    NothingToClaim,
    SeasonNotOver,
    TooManyCivs,
    InvalidParams,
    WrongWorld,
}

impl From<ChainError> for ProgramError {
    fn from(e: ChainError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
