//! Office term limit (owner decision D23; closeout CL-31).
//!
//! **At most one office term per wallet per season** (Warden or Minister).
//! A by-election term counts, and so does a term cut short by recall; the
//! caretaker first term (H2) does not [design proposal, flagged in
//! CL-31]. When no eligible candidate stands, the seat stays **vacant**
//! for the term (no pay, no Assembly weight) — the rule that keeps small
//! seasons (the 50–200-person playtest) from stalling.
//!
//! M1 only validates the parameter (CreateSeason: `office_terms_per_wallet
//! == 1`) and reserves `Citizen.office_terms_used` (zero); enforcement is
//! M3's. The simulator's `office_term_limit` default is 1 (W1-D).

/// Kernel version of this module (part of the ruleset hash).
pub const OFFICE_VERSION: u16 = 1;

/// D23: one term per wallet per season.
pub const OFFICE_TERMS_PER_WALLET_DEFAULT: u8 = 1;

/// Governance parameters of a season.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GovernanceParams {
    pub office_terms_per_wallet: u8,
}

impl Default for GovernanceParams {
    fn default() -> Self {
        GovernanceParams {
            office_terms_per_wallet: OFFICE_TERMS_PER_WALLET_DEFAULT,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GovernanceError {
    /// D23 is decided: the limit is exactly one term.
    TermLimit,
}

impl GovernanceParams {
    /// D23 is decided, so a season accepts only the limit 1.
    pub const fn validate(&self) -> Result<(), GovernanceError> {
        if self.office_terms_per_wallet != OFFICE_TERMS_PER_WALLET_DEFAULT {
            return Err(GovernanceError::TermLimit);
        }
        Ok(())
    }
}

/// Whether a wallet that has used `terms_used` counted terms may stand.
pub const fn may_stand(terms_used: u8, p: &GovernanceParams) -> bool {
    terms_used < p.office_terms_per_wallet
}

/// How a term was held, for the count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TermKind {
    Elected,
    ByElection,
    /// Cut short by recall.
    Recalled,
    /// The caretaker first term (H2).
    Caretaker,
}

/// Whether a term counts toward the limit (every kind but the caretaker
/// term).
pub const fn counts_toward_limit(k: TermKind) -> bool {
    !matches!(k, TermKind::Caretaker)
}

/// A seat with no eligible candidate stays vacant for the term.
pub const fn seat_vacant(eligible_candidates: u32) -> bool {
    eligible_candidates == 0
}
