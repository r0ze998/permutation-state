//! Program error codes (M1 contract §5.4).
//!
//! `ProgramError::Custom(code)`. Codes are **stable forever**: a code is
//! never renumbered or reused, a removed one stays reserved (14, 61 is a
//! relay-only code reserved so the web maps one table). The web maps every
//! code to JA/EN text from `vectors/errors.json` (§9.6).

/// Every error the Frontier program returns, with its stable code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u32)]
pub enum FrontierError {
    BadData = 1,
    /// Owner, magic, season or stored key wrong.
    BadAccount = 2,
    /// Not the canonical address.
    BadAddress = 3,
    /// Missing signature or wrong signer (`NotSigner` / `Auth`).
    Auth = 4,
    /// Season status does not allow the instruction.
    WrongStatus = 5,
    RulesetMismatch = 6,
    WrongRound = 7,
    NoAnchor = 8,
    /// BadHint / BadPoint / PairingFailed (sub-code in the log).
    Crypto = 9,
    Capacity = 10,
    SiteTaken = 11,
    WindowClosed = 12,
    TooEarly = 13,
    /// Reserved (was SealValid; ProveBadSeal removed in v1.1, I-44).
    Reserved14 = 14,
    /// Kernel refusal (sub-code in the log).
    Kernel = 15,
    /// Tombstoned bell.
    Archived = 16,
    Bucket = 17,
    NotTopLevel = 18,
    Overflow = 19,
    NotOwner = 20,
    /// Resources, reserve or lamports.
    Insufficient = 21,
    QueueFull = 22,
    NoTicket = 23,
    /// Holding provisional (I-29).
    NotFinal = 24,
    TooManyAccounts = 25,
    NotResident = 26,
    ProvinceFull = 27,
    HostBusy = 28,
    /// Cooldown / NoStamina.
    Cooldown = 29,
    TransitState = 30,
    /// Kernel `TravelError`.
    ArrivalBell = 31,
    /// Not adjacent, impassable, too long, > 4 provinces, wrong end.
    Path = 32,
    CommitMismatch = 33,
    /// Kernel `SlotRefusal`.
    QuotaRefused = 34,
    /// Keeper: re-read the slots and retry.
    SlotMoved = 35,
    NeedArrivalDay = 36,
    Shielded = 37,
    DepartureUnsettled = 38,
    NotGathered = 39,
    /// `bell != resolved_next`.
    OutOfOrder = 40,
    NotQuiet = 41,
    InputsOpen = 42,
    /// NotEligible / AlreadyClaimed (defence).
    NotEligible = 43,
    FoldStale = 44,
    TicketState = 45,
    /// NotDormant / HasTransits.
    NotDormant = 46,
    Explored = 47,
    SessionExpired = 48,
    WrongRegion = 49,
    Aborted = 50,
    /// I-08.
    TipTooLow = 51,
    /// Idempotent repeat; the keeper treats it as success.
    AlreadyDone = 52,
    /// ClashInputs exists or the bell is resolved (I-07).
    LatchClosed = 53,
    SeedNotReady = 54,
    /// I-28.
    BadPlaintext = 55,
    /// Lead < 24 h, params hash mismatch, id used.
    Announce = 56,
    /// Rings 0–1.
    ReservedSite = 57,
    /// I-44.
    HostInTransit = 58,
    /// I-51.
    JoinGate = 59,
    /// I-47.
    CohortFull = 60,
    /// Relay only; never returned by the program (reserved so the web maps
    /// one table).
    TipNotPreset = 61,
    /// Dev stubs only; `RELEASE_CHECK=1` fails if any path returns it.
    NotImplemented = 99,
}

impl FrontierError {
    /// Every code, in code order.
    pub const ALL: [FrontierError; 62] = {
        use FrontierError::*;
        [
            BadData,
            BadAccount,
            BadAddress,
            Auth,
            WrongStatus,
            RulesetMismatch,
            WrongRound,
            NoAnchor,
            Crypto,
            Capacity,
            SiteTaken,
            WindowClosed,
            TooEarly,
            Reserved14,
            Kernel,
            Archived,
            Bucket,
            NotTopLevel,
            Overflow,
            NotOwner,
            Insufficient,
            QueueFull,
            NoTicket,
            NotFinal,
            TooManyAccounts,
            NotResident,
            ProvinceFull,
            HostBusy,
            Cooldown,
            TransitState,
            ArrivalBell,
            Path,
            CommitMismatch,
            QuotaRefused,
            SlotMoved,
            NeedArrivalDay,
            Shielded,
            DepartureUnsettled,
            NotGathered,
            OutOfOrder,
            NotQuiet,
            InputsOpen,
            NotEligible,
            FoldStale,
            TicketState,
            NotDormant,
            Explored,
            SessionExpired,
            WrongRegion,
            Aborted,
            TipTooLow,
            AlreadyDone,
            LatchClosed,
            SeedNotReady,
            BadPlaintext,
            Announce,
            ReservedSite,
            HostInTransit,
            JoinGate,
            CohortFull,
            TipNotPreset,
            NotImplemented,
        ]
    };

    pub const fn code(self) -> u32 {
        self as u32
    }

    pub fn from_code(code: u32) -> Option<FrontierError> {
        Self::ALL.iter().copied().find(|e| e.code() == code)
    }

    /// The name used in logs, vectors and the web error table.
    pub const fn name(self) -> &'static str {
        use FrontierError::*;
        match self {
            BadData => "BadData",
            BadAccount => "BadAccount",
            BadAddress => "BadAddress",
            Auth => "Auth",
            WrongStatus => "WrongStatus",
            RulesetMismatch => "RulesetMismatch",
            WrongRound => "WrongRound",
            NoAnchor => "NoAnchor",
            Crypto => "Crypto",
            Capacity => "Capacity",
            SiteTaken => "SiteTaken",
            WindowClosed => "WindowClosed",
            TooEarly => "TooEarly",
            Reserved14 => "Reserved14",
            Kernel => "Kernel",
            Archived => "Archived",
            Bucket => "Bucket",
            NotTopLevel => "NotTopLevel",
            Overflow => "Overflow",
            NotOwner => "NotOwner",
            Insufficient => "Insufficient",
            QueueFull => "QueueFull",
            NoTicket => "NoTicket",
            NotFinal => "NotFinal",
            TooManyAccounts => "TooManyAccounts",
            NotResident => "NotResident",
            ProvinceFull => "ProvinceFull",
            HostBusy => "HostBusy",
            Cooldown => "Cooldown",
            TransitState => "TransitState",
            ArrivalBell => "ArrivalBell",
            Path => "Path",
            CommitMismatch => "CommitMismatch",
            QuotaRefused => "QuotaRefused",
            SlotMoved => "SlotMoved",
            NeedArrivalDay => "NeedArrivalDay",
            Shielded => "Shielded",
            DepartureUnsettled => "DepartureUnsettled",
            NotGathered => "NotGathered",
            OutOfOrder => "OutOfOrder",
            NotQuiet => "NotQuiet",
            InputsOpen => "InputsOpen",
            NotEligible => "NotEligible",
            FoldStale => "FoldStale",
            TicketState => "TicketState",
            NotDormant => "NotDormant",
            Explored => "Explored",
            SessionExpired => "SessionExpired",
            WrongRegion => "WrongRegion",
            Aborted => "Aborted",
            TipTooLow => "TipTooLow",
            AlreadyDone => "AlreadyDone",
            LatchClosed => "LatchClosed",
            SeedNotReady => "SeedNotReady",
            BadPlaintext => "BadPlaintext",
            Announce => "Announce",
            ReservedSite => "ReservedSite",
            HostInTransit => "HostInTransit",
            JoinGate => "JoinGate",
            CohortFull => "CohortFull",
            TipNotPreset => "TipNotPreset",
            NotImplemented => "NotImplemented",
        }
    }

    /// Whether the program may return this code (61 is relay-only, 14 is
    /// reserved, 99 only from dev stubs).
    pub const fn program_code(self) -> bool {
        !matches!(
            self,
            FrontierError::Reserved14 | FrontierError::TipNotPreset | FrontierError::NotImplemented
        )
    }
}

/// Keeper mapping (offchain P8, §5.4): what an off-chain duty does on a code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeeperAction {
    /// `AlreadyDone`: treat as success.
    Success,
    /// `SlotMoved`: re-read the slots and retry (≤ 4 times).
    RetrySlots,
    /// `WindowClosed` / `Archived` / `LatchClosed`: stop.
    Stop,
    /// `NoAnchor`, `SeedNotReady`, `NotGathered`, `TooEarly`: wait for the
    /// dependency and retry.
    Wait,
    /// Anything else: a refusal to journal.
    Refused,
}

pub const fn keeper_action(e: FrontierError) -> KeeperAction {
    use FrontierError::*;
    match e {
        AlreadyDone => KeeperAction::Success,
        SlotMoved => KeeperAction::RetrySlots,
        WindowClosed | Archived | LatchClosed => KeeperAction::Stop,
        NoAnchor | SeedNotReady | NotGathered | TooEarly | DepartureUnsettled => KeeperAction::Wait,
        _ => KeeperAction::Refused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_the_contract_table() {
        // 1..=61 contiguous, then 99.
        let mut expect = 1u32;
        for e in FrontierError::ALL.iter().take(61) {
            assert_eq!(e.code(), expect, "{}", e.name());
            expect += 1;
        }
        assert_eq!(FrontierError::ALL[61].code(), 99);
        for c in 1..=61 {
            assert_eq!(FrontierError::from_code(c).map(|e| e.code()), Some(c));
        }
        assert_eq!(FrontierError::from_code(0), None);
        assert_eq!(FrontierError::from_code(62), None);
        assert_eq!(
            FrontierError::from_code(99),
            Some(FrontierError::NotImplemented)
        );
        assert_eq!(FrontierError::TipTooLow.code(), 51);
        assert_eq!(FrontierError::HostInTransit.code(), 58);
        assert_eq!(FrontierError::CohortFull.code(), 60);
    }
}
