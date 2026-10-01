//! `permutation-frontier`: the Wylls M1 program (M1 contract
//! §0–§6; new program id, SBPF v2, no money, no MagicBlock).
//!
//! Two layers, split by the `program` feature (on by default):
//!
//! | layer | modules | built |
//! |---|---|---|
//! | pure | [`error`], [`ix`], [`addr`], [`clock`], [`events`], [`evidence`], [`heap`], [`layout`], [`crypto`], [`markers`] | always (`--no-default-features` too) |
//! | program | `init`, `prologue`, `proc`, [`dispatch`] and the entrypoint | feature `program` |
//!
//! The pure layer never touches an `AccountInfo`, a sysvar or a syscall
//! other than the hashing and BLS12-381 ones (which are stubs on the host),
//! so the LiteSVM suite, `frontier-node` and host tests can link it. Account
//! layouts, tags, instruction data, error codes and PS2 log records come
//! from `frontier-abi` (I-03, I-55); the rules come from
//! `permutation-rules::frontier`.
//!
//! Code rules (§3.3): no `unwrap`/`expect`/`panic!` outside tests (the
//! crate denies the clippy lints), checked arithmetic (overflow checks are
//! on in the release profile, `Overflow` 19 where a code is wanted), no
//! borsh on hot paths, exact account lists checked first, never
//! `CreateAccount*`, never a caller-supplied bump, every keyed address a
//! reader trusts recomputed.
//!
//! Wave 2 (W2-A) implements the lifecycle and beacon instructions of §5.7
//! and §5.8 listed in `docs/frontier/m1/W2-A-NOTES.md`; every other tag is
//! dispatched to its area file in [`proc`](crate::proc), where it returns
//! `NotImplemented` (99) until its wave-3/4 unit hands it in.

#![allow(unexpected_cfgs)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

extern crate alloc;

pub mod addr;
pub mod clock;
pub mod crypto;
pub mod error;
pub mod events;
pub mod evidence;
pub mod heap;
pub mod ix;
pub mod layout;
pub mod markers;

#[cfg(feature = "program")]
pub mod init;
#[cfg(feature = "program")]
pub mod proc;
#[cfg(feature = "program")]
pub mod prologue;

pub use error::{Error, FrontierError, R};

/// The ruleset hash this binary enforces (§3.2): every instruction that
/// reads a Created or later Season compares it with `Season.ruleset_hash`.
pub const RULESET_HASH: [u8; 32] = frontier_abi::presets::RULESET_HASH;

/// `sha256` of the beacon public key this binary verifies against: the
/// quicknet group key, or the local test key in a `test-beacon` build
/// (I-53). CreateSeason refuses parameters naming another key.
pub const QUICKNET_PK_HASH: [u8; 32] = crypto::quick::PK_HASH;

/// Program version written by CreateSeason's parameters and logged; bumped
/// with every deployable change of the instruction set.
pub const PROGRAM_VERSION: u16 = 1;

/// Routes one instruction by its tag (§5.5). Unknown and reserved tags are
/// `BadData`; ResolveClash exists only in the `oracle` build.
#[cfg(feature = "program")]
pub fn dispatch(
    program_id: &solana_program::pubkey::Pubkey,
    accounts: &[solana_program::account_info::AccountInfo],
    data: &[u8],
) -> R<()> {
    use frontier_abi::tags::Ix;
    use proc::*;
    markers::touch();
    heap::trace_checkpoint(0);
    let ix = frontier_abi::ix::tag_of(data)?;
    let p = program_id;
    let a = accounts;
    let r = match ix {
        // §5.7 lifecycle
        Ix::AnnounceSeason => season::announce_season(p, a, data),
        Ix::CreateSeason => season::create_season(p, a, data),
        Ix::InitBeaconLogs => season::init_beacon_logs(p, a, data),
        Ix::InitShards => season::init_shards(p, a, data),
        Ix::ConsumeGenesisSeed => season::consume_genesis_seed(p, a, data),
        Ix::EndSeason => season::end_season(p, a, data),
        Ix::CloseSeason => season::close_season(p, a, data),
        Ix::AbortSeason => season::abort_season(p, a, data),
        Ix::SetWindowSchedule => season::set_window_schedule(p, a, data),
        // §5.8 beacons and archives
        Ix::PostAnchor => beacon::post_anchor(p, a, data),
        Ix::PostAnchorMulti => beacon::post_anchor_multi(p, a, data),
        Ix::PostSeed => beacon::post_seed(p, a, data),
        Ix::PostBeacon => beacon::post_beacon(p, a, data),
        Ix::ArchiveAnchors => beacon::archive_anchors(p, a, data),
        Ix::CloseSeedCache => beacon::close_seed_cache(p, a, data),
        // §5.9 rings and provinces
        Ix::OpenRing => map::open_ring(p, a, data),
        Ix::ConsumeRingSeed => map::consume_ring_seed(p, a, data),
        Ix::OpenProvince => map::open_province(p, a, data),
        Ix::FoldOccupancy => map::fold_occupancy(p, a, data),
        Ix::CloseProvince => map::close_province(p, a, data),
        // §5.9 citizens and land
        Ix::Join => citizen::join(p, a, data),
        Ix::SetSession => citizen::set_session(p, a, data),
        Ix::SetVigil => citizen::set_vigil(p, a, data),
        Ix::FileTicket => citizen::file_ticket(p, a, data),
        Ix::SettleTicket => citizen::settle_ticket(p, a, data),
        Ix::ReleaseDormant => citizen::release_dormant(p, a, data),
        Ix::CloseHolding => citizen::close_holding(p, a, data),
        Ix::CloseCitizen => citizen::close_citizen(p, a, data),
        // §5.10 holdings and resident actions
        Ix::Harvest => holding::harvest(p, a, data),
        Ix::Build => holding::build(p, a, data),
        Ix::Train => holding::train(p, a, data),
        Ix::Explore => holding::explore(p, a, data),
        Ix::SettleExplore => holding::settle_explore(p, a, data),
        Ix::Muster => host::muster(p, a, data),
        Ix::Dissolve => host::dissolve(p, a, data),
        Ix::Garrison => host::garrison(p, a, data),
        Ix::DisbandStranded => host::disband_stranded(p, a, data),
        // §5.11 marches, reveals, transits
        Ix::Depart => host::depart(p, a, data),
        Ix::SettleDeparture => host::settle_departure(p, a, data),
        Ix::Reveal => reveal::reveal(p, a, data),
        Ix::SettleTransit => transit::settle_transit(p, a, data),
        Ix::SweepPoolOwed => transit::sweep_pool_owed(p, a, data),
        // §5.11 clashes
        Ix::GatherClash => clash::gather_clash(p, a, data),
        Ix::ResolveFromInputs => clash::resolve_from_inputs(p, a, data),
        Ix::ResolveClash => resolve_clash(p, a, data),
        Ix::SkipQuiet => clash::skip_quiet(p, a, data),
        Ix::CloseClashInputs => clash::close_clash_inputs(p, a, data),
        Ix::CloseArrivalDay => clash::close_arrival_day(p, a, data),
        Ix::CloseArrivalSlot => clash::close_arrival_slot(p, a, data),
        // §5.12 defence pool
        Ix::ClaimDefence => defence::claim_defence(p, a, data),
    };
    heap::trace_checkpoint(0xffff);
    r
}

/// ResolveClash (0x62) exists only in the `oracle` build (tests); in every
/// other build the tag is unknown data.
#[cfg(feature = "program")]
fn resolve_clash(
    p: &solana_program::pubkey::Pubkey,
    a: &[solana_program::account_info::AccountInfo],
    d: &[u8],
) -> R<()> {
    #[cfg(feature = "oracle")]
    {
        proc::clash::resolve_clash(p, a, d)
    }
    #[cfg(not(feature = "oracle"))]
    {
        let _ = (p, a, d);
        Err(FrontierError::BadData.into())
    }
}

#[cfg(all(
    feature = "program",
    target_os = "solana",
    not(feature = "no-entrypoint")
))]
mod entrypoint {
    use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, pubkey::Pubkey};

    solana_program::entrypoint!(process_instruction);

    fn process_instruction(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        data: &[u8],
    ) -> ProgramResult {
        crate::dispatch(program_id, accounts, data).map_err(Into::into)
    }
}
