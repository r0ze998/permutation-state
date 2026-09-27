//! `frontier-agents` (M1 contract §8.6).
//!
//! **W1 skeleton.** Wave 3 copies `Arch`/`Profile` from `frontier-sim`
//! (with the field-equality test, I-36) and writes the policies. What exists
//! now is the adversarial persona list of §8.6 with each expected outcome,
//! which the stack report asserts.

/// Adversarial personas (each ≤ 1% of bots in the exit run).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Persona {
    MinTip,
    GarbageSeal,
    BadPlaintext,
    SettleRacer,
    Prefunder,
    Squatter,
    LateRevealer,
    Forger,
    Spammer,
    DoubleArrival,
    ZeroTip,
    SelfTip,
    TicketHolder,
}

impl Persona {
    pub const ALL: [Persona; 13] = [
        Persona::MinTip,
        Persona::GarbageSeal,
        Persona::BadPlaintext,
        Persona::SettleRacer,
        Persona::Prefunder,
        Persona::Squatter,
        Persona::LateRevealer,
        Persona::Forger,
        Persona::Spammer,
        Persona::DoubleArrival,
        Persona::ZeroTip,
        Persona::SelfTip,
        Persona::TicketHolder,
    ];

    /// The outcome §8.6 expects.
    pub fn expected(self) -> &'static str {
        match self {
            Persona::MinTip => "revealed by keepers; PASS",
            Persona::GarbageSeal => "destroyed at SettleTransit; verifier agrees",
            Persona::BadPlaintext => "destroyed at SettleTransit, seal code 5",
            Persona::SettleRacer => "Depart refused HostInTransit; destroyed",
            Persona::Prefunder => "nothing blocked",
            Persona::Squatter => "displaced",
            Persona::LateRevealer => "refused",
            Persona::Forger => "refused",
            Persona::Spammer => "429 / Bucket",
            Persona::DoubleArrival => "second bounced without loss",
            Persona::ZeroTip => "refused TipTooLow",
            Persona::SelfTip => "collects <= 2 x tip_min; reported",
            Persona::TicketHolder => {
                "higher score still displaces it; finality waits for the cohort"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn thirteen_personas() {
        assert_eq!(super::Persona::ALL.len(), 13);
    }
}
