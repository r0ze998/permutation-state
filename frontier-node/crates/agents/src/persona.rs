//! The thirteen adversarial personas of M1 contract §8.6, each with the
//! outcome the contract expects (checked by the bot runner where a bot can
//! see it, and by the stack report and the verifier where only the chain
//! can).

/// Adversarial personas (each ≤ 1% of bots in the exit run).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

    /// The §8.6 name.
    pub fn name(self) -> &'static str {
        match self {
            Persona::MinTip => "min_tip",
            Persona::GarbageSeal => "garbage_seal",
            Persona::BadPlaintext => "bad_plaintext",
            Persona::SettleRacer => "settle_racer",
            Persona::Prefunder => "prefunder",
            Persona::Squatter => "squatter",
            Persona::LateRevealer => "late_revealer",
            Persona::Forger => "forger",
            Persona::Spammer => "spammer",
            Persona::DoubleArrival => "double_arrival",
            Persona::ZeroTip => "zero_tip",
            Persona::SelfTip => "self_tip",
            Persona::TicketHolder => "ticket_holder",
        }
    }

    pub fn parse(s: &str) -> Option<Persona> {
        Persona::ALL.into_iter().find(|p| p.name() == s)
    }

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

    /// Whether the persona sends transactions of its own (paid by its own
    /// funded key, not through the relay): the relay never sponsors a
    /// Reveal, a transfer or a zero tip, so these personas need the local
    /// chain's RPC (`--rpc`). Without it they record `skipped: no direct
    /// port` for that part and keep the relay part.
    pub fn needs_direct(self) -> bool {
        matches!(
            self,
            Persona::Prefunder
                | Persona::LateRevealer
                | Persona::Forger
                | Persona::ZeroTip
                | Persona::SelfTip
                | Persona::TicketHolder
        )
    }

    /// Whether the bot itself can see the expected outcome (a refusal it
    /// receives); the others need the chain's final state (stack report,
    /// verifier).
    pub fn locally_checkable(self) -> bool {
        matches!(
            self,
            Persona::SettleRacer
                | Persona::LateRevealer
                | Persona::Forger
                | Persona::Spammer
                | Persona::ZeroTip
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thirteen_personas_with_unique_names() {
        assert_eq!(Persona::ALL.len(), 13);
        let mut names: Vec<&str> = Persona::ALL.iter().map(|p| p.name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 13);
        for p in Persona::ALL {
            assert_eq!(Persona::parse(p.name()), Some(p));
            assert!(!p.expected().is_empty());
        }
    }
}
