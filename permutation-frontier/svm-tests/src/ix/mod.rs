//! Instruction builders by area. Every builder is `fclient::ix`'s (the
//! off-chain spine's account order and data, §5; the relay and the keeper
//! build the same bytes), so a test sends exactly what a client sends; the
//! area files add what tests need on top (params hashing, argument
//! bundles, deliberate forgeries through `chain::{with_account,
//! with_writable, without_signer}`).
//!
//! | file | instructions | owner |
//! |---|---|---|
//! | `season` | AnnounceSeason, CreateSeason, InitBeaconLogs, InitShards, ConsumeGenesisSeed, SetWindowSchedule (W2-A); EndSeason, AbortSeason, CloseSeason | W2-B, then W4-B |
//! | `beacon` | PostAnchor, PostAnchorMulti, PostSeed, PostBeacon (W2-A); ArchiveAnchors, CloseSeedCache | W2-B, then W4-B |
//! | `map`, `citizen` | rings, provinces, joins, tickets | W3-A (stub until then) |
//! | `holding`, `host`, `reveal` | holding actions, marches, Reveal | W3-B (stub) |
//! | `clash` | gathers, resolves, skips, closes | W4-A (stub) |
//! | `transit`, `defence` | SettleTransit, sweeps, claims | W4-B (stub) |

pub use fclient::ix::*;

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
