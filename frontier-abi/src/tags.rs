//! Instruction tags, keeper classes and the top-level rule (M1 contract
//! §5.5). Byte 0 of instruction data is the tag. Tags are **never
//! reordered or reused**: removed ones stay reserved (0x53 ProveBadSeal),
//! and 0x80–0x8F (M2 money) and 0x90–0x91 (M3 postures) are reserved.

/// Keeper class (§5.5, I-21).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    /// Window-closing: escalate to P_def 2.0, pool-eligible (Reveal).
    W,
    /// Delay-only: escalate to P_delay 0.5 from the keeper's budget.
    D,
    /// Non-critical: fixed low bid.
    N,
    /// Player (priority 0 via the relay).
    P,
    /// Operator.
    O,
    /// Tests only (`oracle` feature).
    Test,
}

impl Class {
    pub const fn name(self) -> &'static str {
        match self {
            Class::W => "W",
            Class::D => "D",
            Class::N => "N",
            Class::P => "P",
            Class::O => "O",
            Class::Test => "tests",
        }
    }
}

macro_rules! tags {
    ($( $name:ident = $tag:literal, $class:ident, $top:literal; )*) => {
        /// Every M1 instruction (50: 49 plus the test-only ResolveClash).
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum Ix { $( $name = $tag, )* }

        impl Ix {
            pub const ALL: &'static [Ix] = &[ $( Ix::$name, )* ];

            pub const fn tag(self) -> u8 { self as u8 }

            pub const fn from_tag(t: u8) -> Option<Ix> {
                match t { $( $tag => Some(Ix::$name), )* _ => None }
            }

            pub const fn name(self) -> &'static str {
                match self { $( Ix::$name => stringify!($name), )* }
            }

            /// Keeper class.
            pub const fn class(self) -> Class {
                match self { $( Ix::$name => Class::$class, )* }
            }

            /// Refuses `get_stack_height() > 1` (`NotTopLevel`).
            pub const fn top_level_only(self) -> bool {
                match self { $( Ix::$name => $top, )* }
            }
        }
    };
}

tags! {
    // Lifecycle (§5.7)
    CreateSeason = 0x01, O, false;
    InitShards = 0x02, O, false;
    ConsumeGenesisSeed = 0x03, D, true;
    EndSeason = 0x04, N, false;
    CloseSeason = 0x05, O, false;
    AbortSeason = 0x06, N, false;
    SetWindowSchedule = 0x07, O, false;
    AnnounceSeason = 0x08, O, false;
    InitBeaconLogs = 0x09, O, false;
    // Beacons and archives (§5.8)
    PostAnchor = 0x10, D, true;
    PostAnchorMulti = 0x11, D, true;
    PostSeed = 0x12, D, true;
    PostBeacon = 0x13, N, true;
    ArchiveAnchors = 0x14, D, false;
    CloseSeedCache = 0x15, N, false;
    // Rings, provinces (§5.9)
    OpenRing = 0x20, D, false;
    ConsumeRingSeed = 0x21, D, true;
    OpenProvince = 0x22, D, true;
    FoldOccupancy = 0x23, D, false;
    CloseProvince = 0x24, N, false;
    // Citizens and land (§5.9)
    Join = 0x30, P, false;
    SetSession = 0x31, P, false;
    SetVigil = 0x32, P, false;
    FileTicket = 0x33, P, false;
    SettleTicket = 0x34, D, false;
    ReleaseDormant = 0x35, N, false;
    CloseHolding = 0x36, N, false;
    CloseCitizen = 0x37, N, false;
    // Holdings and resident actions (§5.10)
    Harvest = 0x40, P, false;
    Build = 0x41, P, false;
    Train = 0x42, P, false;
    Muster = 0x43, P, false;
    Dissolve = 0x44, P, false;
    Garrison = 0x45, P, false;
    Explore = 0x46, P, false;
    SettleExplore = 0x47, N, false;
    DisbandStranded = 0x48, N, false;
    // Marches (§5.11)
    Depart = 0x50, P, false;
    Reveal = 0x51, W, true;
    SettleDeparture = 0x52, D, false;
    SettleTransit = 0x54, D, false;
    SweepPoolOwed = 0x55, N, false;
    // Clashes (§5.11)
    GatherClash = 0x60, D, true;
    ResolveFromInputs = 0x61, D, true;
    ResolveClash = 0x62, Test, true;
    SkipQuiet = 0x63, D, true;
    CloseClashInputs = 0x64, N, false;
    CloseArrivalDay = 0x65, N, false;
    CloseArrivalSlot = 0x66, N, false;
    // Defence (§5.12)
    ClaimDefence = 0x70, D, false;
}

/// Reserved tags: never assigned (0x53 was ProveBadSeal, I-44).
pub const fn is_reserved(t: u8) -> bool {
    matches!(t, 0x53 | 0x80..=0x8F | 0x90 | 0x91)
}

/// Player instructions the relay may sponsor (§8.3 shape allowlist).
pub const fn relay_player_shape(ix: Ix) -> bool {
    matches!(ix.tag(), 0x30..=0x33 | 0x40..=0x46 | 0x50)
}

/// Settle shapes the relay may sponsor without an authority signature.
pub const fn relay_settle_shape(ix: Ix) -> bool {
    matches!(ix, Ix::SettleExplore | Ix::SettleTransit)
}

/// Instructions that exist only in the `oracle` build.
pub const fn oracle_only(ix: Ix) -> bool {
    matches!(ix, Ix::ResolveClash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifty_tags_unique_and_not_reserved() {
        assert_eq!(Ix::ALL.len(), 50);
        for (i, a) in Ix::ALL.iter().enumerate() {
            assert_eq!(Ix::from_tag(a.tag()), Some(*a));
            assert!(!is_reserved(a.tag()), "{}", a.name());
            for b in &Ix::ALL[i + 1..] {
                assert_ne!(a.tag(), b.tag());
            }
        }
        assert_eq!(Ix::from_tag(0x53), None);
        assert_eq!(Ix::from_tag(0x00), None);
        assert_eq!(Ix::Reveal.class(), Class::W);
        assert!(Ix::Reveal.top_level_only());
        let w: usize = Ix::ALL.iter().filter(|i| i.class() == Class::W).count();
        assert_eq!(w, 1, "only Reveal is class W in M1");
        // v1.1 D-class set (I-21, I-47, I-48, I-52)
        for ix in [
            Ix::SettleTicket,
            Ix::OpenRing,
            Ix::ConsumeRingSeed,
            Ix::OpenProvince,
            Ix::FoldOccupancy,
            Ix::ClaimDefence,
            Ix::SettleTransit,
            Ix::SkipQuiet,
        ] {
            assert_eq!(ix.class(), Class::D, "{}", ix.name());
        }
    }
}
