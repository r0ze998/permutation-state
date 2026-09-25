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
    MissingNation,
    WrongDelegationProgram,
    WrongMagicProgram,
    AlreadyClaimed,
    NothingToClaim,
    SeasonNotOver,
    WrongOffice,
    InvalidParams,
    WrongWorld,
    InboxFull,
    /// The open tick's input is being published; submissions wait for the next tick.
    TickFrozen,
    /// The open tick's input has not been published in full (`LogTickInput`).
    InputNotPublished,
}

impl ChainError {
    /// Every error, in code order (`ALL[i]` has code `i + 1`). Clients (the JS
    /// codec) are checked against this list through the codec vectors.
    pub const ALL: [ChainError; 28] = [
        ChainError::InvalidInstruction,
        ChainError::MissingSignature,
        ChainError::WrongPda,
        ChainError::AlreadyInitialized,
        ChainError::NotInitialized,
        ChainError::WrongStatus,
        ChainError::Unauthorized,
        ChainError::SeasonFull,
        ChainError::InvalidName,
        ChainError::WrongMint,
        ChainError::WrongTokenAccount,
        ChainError::WorldTooSmall,
        ChainError::Rules,
        ChainError::WrongTick,
        ChainError::OverBudget,
        ChainError::TooEarly,
        ChainError::MissingNation,
        ChainError::WrongDelegationProgram,
        ChainError::WrongMagicProgram,
        ChainError::AlreadyClaimed,
        ChainError::NothingToClaim,
        ChainError::SeasonNotOver,
        ChainError::WrongOffice,
        ChainError::InvalidParams,
        ChainError::WrongWorld,
        ChainError::InboxFull,
        ChainError::TickFrozen,
        ChainError::InputNotPublished,
    ];

    /// The error's name, as clients show it. Exhaustive, so a new variant
    /// does not compile until it is named here (and added to `ALL`).
    pub const fn name(self) -> &'static str {
        match self {
            ChainError::InvalidInstruction => "InvalidInstruction",
            ChainError::MissingSignature => "MissingSignature",
            ChainError::WrongPda => "WrongPda",
            ChainError::AlreadyInitialized => "AlreadyInitialized",
            ChainError::NotInitialized => "NotInitialized",
            ChainError::WrongStatus => "WrongStatus",
            ChainError::Unauthorized => "Unauthorized",
            ChainError::SeasonFull => "SeasonFull",
            ChainError::InvalidName => "InvalidName",
            ChainError::WrongMint => "WrongMint",
            ChainError::WrongTokenAccount => "WrongTokenAccount",
            ChainError::WorldTooSmall => "WorldTooSmall",
            ChainError::Rules => "Rules",
            ChainError::WrongTick => "WrongTick",
            ChainError::OverBudget => "OverBudget",
            ChainError::TooEarly => "TooEarly",
            ChainError::MissingNation => "MissingNation",
            ChainError::WrongDelegationProgram => "WrongDelegationProgram",
            ChainError::WrongMagicProgram => "WrongMagicProgram",
            ChainError::AlreadyClaimed => "AlreadyClaimed",
            ChainError::NothingToClaim => "NothingToClaim",
            ChainError::SeasonNotOver => "SeasonNotOver",
            ChainError::WrongOffice => "WrongOffice",
            ChainError::InvalidParams => "InvalidParams",
            ChainError::WrongWorld => "WrongWorld",
            ChainError::InboxFull => "InboxFull",
            ChainError::TickFrozen => "TickFrozen",
            ChainError::InputNotPublished => "InputNotPublished",
        }
    }
}

impl From<ChainError> for ProgramError {
    fn from(e: ChainError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::ChainError;

    #[test]
    fn all_is_in_code_order_and_complete() {
        for (i, e) in ChainError::ALL.iter().enumerate() {
            assert_eq!(*e as u32, i as u32 + 1, "{}", e.name());
            assert_eq!(e.name(), format!("{e:?}"));
        }
        assert_eq!(
            ChainError::ALL.len() as u32,
            ChainError::InputNotPublished as u32,
            "the last variant closes the list"
        );
    }
}
