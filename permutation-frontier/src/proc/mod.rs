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
//!
//! **Hand-over rules for W3-A, W3-B, W4-A and W4-B (v1.3, integ-W2):**
//! - `init::close_to` does **not** log: every close path (CloseProvince,
//!   CloseHolding, CloseCitizen, ReleaseDormant, ArchiveAnchors' anchor
//!   closes, CloseSeedCache, CloseClashInputs, CloseArrivalDay,
//!   CloseArrivalSlot, the slot close of SettleTransit, CloseSeason) emits
//!   `CLOSE` (§6: kind, key, the final seq and head of a chained account,
//!   recipient, lamports) itself, **before** calling `close_to`, or the
//!   verifier's chain walk breaks (§4.2 as amended).
//! - The player prologue calls the same Season address check as
//!   `prologue::{season, keeper}` (`addr::season_pda`, §3.3).
//! - The seal opener never returns code 3: a seal to another round fails
//!   the FO check and is code 1 (§5.3 as amended; W1-C's `wrong_round`
//!   vector).
//! - AnchorArchives are per region and half day (`part = bell / 72`,
//!   6,144 B, v1.3): address every archive with `archive_part_of(bell)`.

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
