//! Instruction handlers, one file per area (M1 contract §3.1, §11). A file
//! belongs to the unit that owns it in the current wave; wave 2 (W2-A)
//! implements the lifecycle and beacon instructions and leaves every other
//! handler a stub that returns `NotImplemented` (99), handed over to:
//!
//! | file | instructions | owner |
//! |---|---|---|
//! | `season.rs` | AnnounceSeason, CreateSeason, InitBeaconLogs, InitShards, ConsumeGenesisSeed, SetWindowSchedule (W2-A); EndSeason, AbortSeason, CloseSeason | W4-B |
//! | `beacon.rs` | PostAnchor, PostAnchorMulti, PostSeed, PostBeacon (W2-A); ArchiveAnchors, CloseSeedCache | W4-B |
//! | `map.rs` | OpenRing, ConsumeRingSeed, OpenProvince, FoldOccupancy, CloseProvince | W3-A |
//! | `citizen.rs` | Join, SetSession, SetVigil, FileTicket, SettleTicket, ReleaseDormant, CloseHolding, CloseCitizen | W3-A |
//! | `holding.rs` | Harvest, Build, Train, Explore, SettleExplore | W3-B |
//! | `host.rs` | Muster, Dissolve, Garrison, DisbandStranded, Depart, SettleDeparture | W3-B |
//! | `reveal.rs` | Reveal | W3-B |
//! | `clash.rs` | GatherClash, ResolveFromInputs, ResolveClash (`oracle`), SkipQuiet, CloseClashInputs, CloseArrivalDay, CloseArrivalSlot | W4-A |
//! | `transit.rs` | SettleTransit, SweepPoolOwed | W4-B |
//! | `defence.rs` | ClaimDefence | W4-B |
//!
//! Every handler has the signature `fn(&Pubkey, &[AccountInfo], &[u8]) ->
//! R<()>`; `RELEASE_CHECK=1` (G13, wave 5) fails while any path still
//! returns `NotImplemented`.

pub mod beacon;
pub mod citizen;
pub mod clash;
pub mod defence;
pub mod holding;
pub mod host;
pub mod map;
pub mod reveal;
pub mod season;
pub mod transit;

/// Declares handlers that return `NotImplemented` (99) until their owning
/// unit implements them.
macro_rules! stubs {
    ($($name:ident),* $(,)?) => {$(
        #[doc = concat!("`", stringify!($name), "`: not implemented yet (`NotImplemented`, 99).")]
        pub fn $name(
            _program: &solana_program::pubkey::Pubkey,
            _accounts: &[solana_program::account_info::AccountInfo],
            _data: &[u8],
        ) -> crate::R<()> {
            Err(crate::FrontierError::NotImplemented.into())
        }
    )*};
}
pub(crate) use stubs;
