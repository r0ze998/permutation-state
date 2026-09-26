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
    /// Sealed orders: the commitments are closed (the reveal window is open),
    /// or not yet closed for a reveal.
    WrongPhase,
    /// A reveal that does not hash to the office's commitment.
    CommitMismatch,
    /// `SubmitOrders` is retired: orders are sealed (`CommitOrders`,
    /// `RevealOrders`).
    Retired,
    /// The operator's AI roster is not revealed yet, and its grace period
    /// has not passed (V5 §18.2).
    RosterPending,
    /// A roster reveal that does not match the member's tag or the
    /// commitment.
    RosterMismatch,
    /// CloseCommits, LogTickInput, ResolveTick or UndelegatePart shares its
    /// transaction with an instruction other than compute-budget ones, or
    /// runs as a CPI.
    NotAlone,
    /// UndelegatePart targets are not the next ones of
    /// `state::undelegation_order`.
    UndelegationOrder,
    /// Delegate of a target already delegated this season
    /// (`Season::delegated`).
    AlreadyDelegated,
    /// StartSeason with the operator's bond below `state::bond_floor` (top
    /// it up with `PostBond`).
    BondTooSmall,
    /// LogTickInput or ResolveTick before the tick's randomness was drawn.
    RandomnessPending,
    /// The VRF program, oracle queue, program identity or callback signer
    /// is not the expected one.
    WrongOracle,
    /// What the vault owes would exceed what it holds (FinishSeason), or a
    /// payment would exceed what FinishSeason set aside (`outstanding`).
    Insolvent,
    /// Delegate of world chunk 0 before every other account of the season.
    DelegationOrder,
    /// FinishSeason after a world chunk was rolled back
    /// (`RollbackUndelegation`): the season can only be aborted.
    WorldRolledBack,
    /// Delegate to a validator other than the season's
    /// (`Season::validator`).
    WrongValidator,
    /// This build runs other rules or settlement logic than the season was
    /// created under (`rules::require_rules`).
    RulesMismatch,
}

impl ChainError {
    /// Every error, in code order (`ALL[i]` has code `i + 1`). Clients (the JS
    /// codec) are checked against this list through the codec vectors.
    pub const ALL: [ChainError; 44] = [
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
        ChainError::WrongPhase,
        ChainError::CommitMismatch,
        ChainError::Retired,
        ChainError::RosterPending,
        ChainError::RosterMismatch,
        ChainError::NotAlone,
        ChainError::UndelegationOrder,
        ChainError::AlreadyDelegated,
        ChainError::BondTooSmall,
        ChainError::RandomnessPending,
        ChainError::WrongOracle,
        ChainError::Insolvent,
        ChainError::DelegationOrder,
        ChainError::WorldRolledBack,
        ChainError::WrongValidator,
        ChainError::RulesMismatch,
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
            ChainError::WrongPhase => "WrongPhase",
            ChainError::CommitMismatch => "CommitMismatch",
            ChainError::Retired => "Retired",
            ChainError::RosterPending => "RosterPending",
            ChainError::RosterMismatch => "RosterMismatch",
            ChainError::NotAlone => "NotAlone",
            ChainError::UndelegationOrder => "UndelegationOrder",
            ChainError::AlreadyDelegated => "AlreadyDelegated",
            ChainError::BondTooSmall => "BondTooSmall",
            ChainError::RandomnessPending => "RandomnessPending",
            ChainError::WrongOracle => "WrongOracle",
            ChainError::Insolvent => "Insolvent",
            ChainError::DelegationOrder => "DelegationOrder",
            ChainError::WorldRolledBack => "WorldRolledBack",
            ChainError::WrongValidator => "WrongValidator",
            ChainError::RulesMismatch => "RulesMismatch",
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
            ChainError::RulesMismatch as u32,
            "the last variant closes the list"
        );
    }

    /// Clients hardcode these codes (§1.2 of the v9 contract): append-only.
    #[test]
    fn codes_are_pinned() {
        let pinned = [
            (ChainError::RosterMismatch, 33),
            (ChainError::NotAlone, 34),
            (ChainError::UndelegationOrder, 35),
            (ChainError::AlreadyDelegated, 36),
            (ChainError::BondTooSmall, 37),
            (ChainError::RandomnessPending, 38),
            (ChainError::WrongOracle, 39),
            (ChainError::Insolvent, 40),
            (ChainError::DelegationOrder, 41),
            (ChainError::WorldRolledBack, 42),
            (ChainError::WrongValidator, 43),
            (ChainError::RulesMismatch, 44),
        ];
        for (e, code) in pinned {
            assert_eq!(e as u32, code, "{}", e.name());
        }
        assert_eq!(ChainError::ALL.len(), 44);
    }
}
