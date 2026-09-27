//! Account lists and the common checks, as pure functions over byte slices
//! (M1 contract §5.6–§5.12, I-55).
//!
//! The program's `prologue.rs` is a thin wrapper: it turns each
//! `AccountInfo` into an [`AccountView`] and calls these functions, so the
//! off-chain builders (`fclient::ix`), the relay's shape allowlist, the
//! budgets table and the program share one definition of every account
//! list and of the checks every instruction starts with (DESIGN §8.6 rule
//! 6: exact count, owners, magics, signers, writability, within ≈ 1.5k CU).
//!
//! Nothing here reads a clock or a sysvar: the caller passes `now`
//! (Clock `unix_timestamp`) and the stack height.

use crate::addr::{citizen_tag15, AddrCtx};
use crate::bytes::{rd_arr, rd_i16, rd_i64, rd_u16, rd_u32, rd_u64, rd_u8, wr_u32, wr_u8};
use crate::error::FrontierError;
use crate::layout::player::{citizen as C, holding as H, transit as T};
use crate::layout::province::{cohort, province as P};
use crate::layout::world::season as S;
use crate::layout::AccountKind;
use crate::tags::Ix;

/// Well-known ids (32 raw bytes).
pub mod ids {
    /// `11111111111111111111111111111111`.
    pub const SYSTEM_PROGRAM: [u8; 32] = [0; 32];
    /// `Sysvar1nstructions1111111111111111111111111`.
    pub const INSTRUCTIONS_SYSVAR: [u8; 32] = [
        0x06, 0xa7, 0xd5, 0x17, 0x18, 0x7b, 0xd1, 0x66, 0x35, 0xda, 0xd4, 0x04, 0x55, 0xfd, 0xc2,
        0xc0, 0xc1, 0x24, 0xc6, 0x8f, 0x21, 0x56, 0x75, 0xa5, 0xdb, 0xba, 0xcb, 0x5f, 0x08, 0x00,
        0x00, 0x00,
    ];
    /// `ComputeBudget111111111111111111111111111111`.
    pub const COMPUTE_BUDGET_PROGRAM: [u8; 32] = [
        0x03, 0x06, 0x46, 0x6f, 0xe5, 0x21, 0x17, 0x32, 0xff, 0xec, 0xad, 0xba, 0x72, 0xc3, 0x9b,
        0xe7, 0xbc, 0x8c, 0xe5, 0xbb, 0xc5, 0xf7, 0x12, 0x6b, 0x2c, 0x43, 0x9b, 0x3a, 0x40, 0x00,
        0x00, 0x00,
    ];
    /// `BPFLoaderUpgradeab1e11111111111111111111111` (LoaderV3).
    pub const LOADER_V3: [u8; 32] = [
        0x02, 0xa8, 0xf6, 0x91, 0x4e, 0x88, 0xa1, 0xb0, 0xe2, 0x10, 0x15, 0x3e, 0xf7, 0x63, 0xae,
        0x2b, 0x00, 0xc2, 0xb9, 0x3d, 0x16, 0xc1, 0x24, 0xd2, 0xc0, 0x53, 0x7a, 0x10, 0x04, 0x80,
        0x00, 0x00,
    ];
    /// `1nc1nerator11111111111111111111111111111111`.
    pub const INCINERATOR: [u8; 32] = [
        0x00, 0x33, 0x90, 0x72, 0x8d, 0x34, 0x11, 0x60, 0x79, 0xbd, 0xc9, 0x11, 0xbf, 0xff, 0x00,
        0xdb, 0xd4, 0x4d, 0x2e, 0xcd, 0xcc, 0xf7, 0x9c, 0xa6, 0xe1, 0x00, 0x38, 0xe1, 0x00, 0x00,
        0x00, 0x00,
    ];
}

// ------------------------------------------------------------ account tables

/// What occupies an account position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acc {
    /// A program account of this kind (or its canonical absent address).
    Kind(AccountKind),
    /// One of two kinds (anchor or archive; seed cache or archive).
    Either(AccountKind, AccountKind),
    /// A wallet or system account holding no data (signers, recipients).
    Wallet,
    /// The System program.
    System,
    /// The instructions sysvar.
    IxSysvar,
    /// The Frontier program account (AnnounceSeason reads its ProgramData).
    ProgramAccount,
    /// The Frontier program's ProgramData (LoaderV3).
    ProgramData,
    /// The incinerator (bond burn).
    Incinerator,
    /// Test-only (ResolveClash): any account.
    Any,
}

/// Writability of a position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wr {
    R,
    W,
    /// Read or write, decided by the instruction (Reveal's ArrivalDay and
    /// target slot).
    Either,
}

/// One account position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spec {
    pub name: &'static str,
    pub acc: Acc,
    pub signer: bool,
    pub wr: Wr,
}

/// A run of positions repeated `min..=max` times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Group {
    pub specs: &'static [Spec],
    pub min: u8,
    pub max: u8,
}

macro_rules! s {
    ($name:expr, $acc:expr, $signer:expr, $wr:expr $(,)?) => {
        Spec {
            name: $name,
            acc: $acc,
            signer: $signer,
            wr: $wr,
        }
    };
}
macro_rules! one {
    ($specs:expr $(,)?) => {
        Group {
            specs: $specs,
            min: 1,
            max: 1,
        }
    };
}
macro_rules! rep {
    ($specs:expr, $min:expr, $max:expr $(,)?) => {
        Group {
            specs: $specs,
            min: $min,
            max: $max,
        }
    };
}

use Acc::{Either, IxSysvar, Kind, System, Wallet};
use AccountKind as K;
use Wr::{R, W};

const PLAYER: Spec = s!("actor", Wallet, true, R);
const PAYER_SW: Spec = s!("payer", Wallet, true, W);
const SEASON_R: Spec = s!("season", Kind(K::Season), false, R);
const SEASON_W: Spec = s!("season", Kind(K::Season), false, W);
const CITIZEN_W: Spec = s!("citizen", Kind(K::Citizen), false, W);
const HOLDING_W: Spec = s!("holding", Kind(K::Holding), false, W);
const PROVINCE_W: Spec = s!("province", Kind(K::Province), false, W);
const SYSTEM: Spec = s!("system", System, false, R);
const IX_SYSVAR: Spec = s!("ix_sysvar", IxSysvar, false, R);
const FEE_PAYER: Spec = s!("fee_payer", Wallet, true, W);
const ANY_S: Spec = s!("any", Wallet, true, R);
const AUTH_SW: Spec = s!("authority", Wallet, true, W);
const ANCHOR_OR_ARCHIVE: Spec = s!(
    "anchor_or_archive",
    Either(K::BellAnchor, K::AnchorArchive),
    false,
    R
);
const CACHE_OR_ARCHIVE: Spec = s!(
    "seedcache_or_archive",
    Either(K::SeedCache, K::AnchorArchive),
    false,
    R
);

/// Player prologue `[actor s] [payer s,w] [season r] [citizen w]` (§5.6).
pub const PLAYER_PROLOGUE: &[Spec] = &[PLAYER, PAYER_SW, SEASON_R, CITIZEN_W];
/// Keeper-write prologue `[fee_payer s,w] [season r]` (§5.6).
pub const KEEPER_PROLOGUE: &[Spec] = &[FEE_PAYER, SEASON_R];
/// Number of player prologue accounts.
pub const PLAYER_PROLOGUE_LEN: usize = 4;

/// The account list of every instruction (§5.7–§5.12), as groups.
pub fn accounts_of(ix: Ix) -> &'static [Group] {
    match ix {
        Ix::AnnounceSeason => {
            const {
                &[one!(&[
                    AUTH_SW,
                    s!("season", Kind(K::Season), false, W),
                    s!("program", Acc::ProgramAccount, false, R),
                    s!("programdata", Acc::ProgramData, false, R),
                    SYSTEM,
                ])]
            }
        }
        Ix::CreateSeason => {
            const {
                &[
                    one!(&[
                        AUTH_SW,
                        SEASON_W,
                        s!("frontier", Kind(K::Frontier), false, W)
                    ]),
                    rep!(&[s!("pfund", Kind(K::ProvinceFund), false, W)], 6, 6),
                    one!(&[s!("dpool", Kind(K::DefencePool), false, W), SYSTEM]),
                ]
            }
        }
        Ix::InitBeaconLogs => {
            const {
                &[
                    one!(&[AUTH_SW, SEASON_R]),
                    rep!(&[s!("beaconlog", Kind(K::BeaconLog), false, W)], 16, 16),
                    one!(&[SYSTEM]),
                ]
            }
        }
        Ix::InitShards => {
            const {
                &[
                    one!(&[AUTH_SW, SEASON_R]),
                    rep!(&[s!("joinshard", Kind(K::JoinShard), false, W)], 8, 8),
                    one!(&[SYSTEM]),
                ]
            }
        }
        Ix::ConsumeGenesisSeed => const { &[one!(&[FEE_PAYER, SEASON_W])] },
        Ix::EndSeason => const { &[one!(&[ANY_S, SEASON_W])] },
        Ix::CloseSeason => {
            const {
                &[
                    one!(&[
                        AUTH_SW,
                        SEASON_W,
                        s!("frontier", Kind(K::Frontier), false, W)
                    ]),
                    rep!(&[s!("pfund", Kind(K::ProvinceFund), false, W)], 6, 6),
                    one!(&[s!("dpool", Kind(K::DefencePool), false, W)]),
                    rep!(&[s!("joinshard", Kind(K::JoinShard), false, W)], 0, 48),
                    rep!(&[s!("beaconlog", Kind(K::BeaconLog), false, W)], 0, 16),
                ]
            }
        }
        Ix::AbortSeason => {
            const {
                &[one!(&[
                    ANY_S,
                    SEASON_W,
                    s!("authority", Wallet, false, W),
                    s!("incinerator", Acc::Incinerator, false, W),
                ])]
            }
        }
        Ix::SetWindowSchedule => const { &[one!(&[s!("authority", Wallet, true, R), SEASON_W])] },
        Ix::PostAnchor => {
            const {
                &[one!(&[
                    FEE_PAYER,
                    SEASON_R,
                    s!("anchor", Kind(K::BellAnchor), false, W),
                    s!("archive", Kind(K::AnchorArchive), false, R),
                    IX_SYSVAR,
                    SYSTEM,
                ])]
            }
        }
        Ix::PostAnchorMulti => {
            const {
                &[
                    one!(&[FEE_PAYER, SEASON_R]),
                    rep!(
                        &[s!("anchor", Kind(K::BellAnchor), false, W)],
                        1,
                        crate::budgets::MULTI_MAX_REGIONS as u8
                    ),
                    rep!(
                        &[s!("archive", Kind(K::AnchorArchive), false, R)],
                        1,
                        crate::budgets::MULTI_MAX_REGIONS as u8
                    ),
                    one!(&[IX_SYSVAR, SYSTEM]),
                ]
            }
        }
        Ix::PostSeed => {
            const {
                &[one!(&[
                    FEE_PAYER,
                    SEASON_R,
                    s!("anchor", Kind(K::BellAnchor), false, R),
                    s!("cache", Kind(K::SeedCache), false, W),
                    IX_SYSVAR,
                    SYSTEM,
                ])]
            }
        }
        Ix::PostBeacon => {
            const {
                &[one!(&[
                    FEE_PAYER,
                    SEASON_R,
                    s!("beaconlog", Kind(K::BeaconLog), false, W)
                ])]
            }
        }
        Ix::ArchiveAnchors => {
            const {
                &[
                    one!(&[
                        PAYER_SW,
                        SEASON_R,
                        s!("archive", Kind(K::AnchorArchive), false, W),
                        SYSTEM
                    ]),
                    rep!(
                        &[
                            s!("anchor", Kind(K::BellAnchor), false, W),
                            s!("cache", Kind(K::SeedCache), false, R),
                            s!("anchor_rent_to", Wallet, false, W),
                        ],
                        1,
                        crate::ix::MAX_ARCHIVE_BELLS as u8,
                    ),
                ]
            }
        }
        Ix::CloseSeedCache => {
            const {
                &[one!(&[
                    ANY_S,
                    SEASON_R,
                    s!("cache", Kind(K::SeedCache), false, W),
                    s!("archive", Kind(K::AnchorArchive), false, R),
                    s!("rent_to", Wallet, false, W),
                ])]
            }
        }
        Ix::OpenRing => {
            const {
                &[
                    one!(&[
                        PAYER_SW,
                        SEASON_R,
                        s!("frontier", Kind(K::Frontier), false, W),
                        s!("ringseed", Kind(K::RingSeed), false, W),
                    ]),
                    rep!(&[s!("pfund", Kind(K::ProvinceFund), false, R)], 6, 6),
                    one!(&[SYSTEM]),
                ]
            }
        }
        Ix::ConsumeRingSeed => {
            const {
                &[one!(&[
                    FEE_PAYER,
                    SEASON_R,
                    s!("ringseed", Kind(K::RingSeed), false, W)
                ])]
            }
        }
        Ix::OpenProvince => {
            const {
                &[one!(&[
                    PAYER_SW,
                    SEASON_R,
                    s!("ringseed", Kind(K::RingSeed), false, W),
                    s!("pfund", Kind(K::ProvinceFund), false, W),
                    PROVINCE_W,
                    SYSTEM,
                ])]
            }
        }
        Ix::FoldOccupancy => {
            const {
                &[
                    one!(&[
                        s!("payer", Wallet, true, R),
                        SEASON_R,
                        s!("frontier", Kind(K::Frontier), false, W)
                    ]),
                    // parts 0, 1: 24 shards and no fund; part 2: 6 funds
                    // (v1.2, `ix::FoldOccupancy::{shards_in, funds_in}`)
                    rep!(&[s!("joinshard", Kind(K::JoinShard), false, R)], 0, 24),
                    rep!(&[s!("pfund", Kind(K::ProvinceFund), false, R)], 0, 6),
                ]
            }
        }
        Ix::CloseProvince => {
            const {
                &[one!(&[
                    ANY_S,
                    SEASON_R,
                    PROVINCE_W,
                    s!("pfund", Kind(K::ProvinceFund), false, W)
                ])]
            }
        }
        Ix::Join => {
            const {
                &[
                    one!(&[
                        s!("wallet", Wallet, true, R),
                        PAYER_SW,
                        SEASON_R,
                        s!("frontier", Kind(K::Frontier), false, R),
                        CITIZEN_W,
                        s!("joinshard", Kind(K::JoinShard), false, W),
                        SYSTEM,
                    ]),
                    rep!(&[s!("join_gate", Wallet, true, R)], 0, 1),
                ]
            }
        }
        Ix::SetSession | Ix::SetVigil => const { &[one!(PLAYER_PROLOGUE)] },
        Ix::Harvest | Ix::Train => const { &[one!(PLAYER_PROLOGUE), one!(&[HOLDING_W])] },
        Ix::FileTicket => {
            const {
                &[
                    one!(PLAYER_PROLOGUE),
                    one!(&[s!("frontier", Kind(K::Frontier), false, R)]),
                    rep!(&[PROVINCE_W], 1, 3),
                    one!(&[SYSTEM]),
                ]
            }
        }
        Ix::SettleTicket => {
            const {
                &[
                    one!(&[
                        PAYER_SW,
                        SEASON_R,
                        CITIZEN_W,
                        HOLDING_W,
                        PROVINCE_W,
                        s!("joinshard", Kind(K::JoinShard), false, W),
                        s!("seedcache", Kind(K::SeedCache), false, R),
                        ANCHOR_OR_ARCHIVE,
                    ]),
                    rep!(&[s!("other_province", Kind(K::Province), false, W)], 0, 2),
                    rep!(
                        &[
                            s!("displaced_rent_payer", Wallet, false, W),
                            s!("displaced_citizen", Kind(K::Citizen), false, W),
                            s!("displaced_joinshard", Kind(K::JoinShard), false, W),
                        ],
                        0,
                        1,
                    ),
                    one!(&[SYSTEM]),
                ]
            }
        }
        Ix::ReleaseDormant => {
            const {
                &[one!(&[
                    ANY_S,
                    SEASON_R,
                    HOLDING_W,
                    PROVINCE_W,
                    CITIZEN_W,
                    s!("joinshard", Kind(K::JoinShard), false, W),
                    s!("rent_payer", Wallet, false, W),
                    s!("dpool", Kind(K::DefencePool), false, W),
                ])]
            }
        }
        Ix::CloseHolding => {
            const {
                &[one!(&[
                    ANY_S,
                    SEASON_R,
                    HOLDING_W,
                    s!("rent_payer", Wallet, false, W),
                    s!("dpool", Kind(K::DefencePool), false, W),
                ])]
            }
        }
        Ix::CloseCitizen => {
            const {
                &[
                    one!(&[
                        ANY_S,
                        SEASON_R,
                        CITIZEN_W,
                        s!("rent_payer", Wallet, false, W)
                    ]),
                    rep!(&[s!("ticket_funder", Wallet, false, W)], 0, 1),
                ]
            }
        }
        Ix::Build => {
            const {
                &[
                    one!(PLAYER_PROLOGUE),
                    one!(&[HOLDING_W]),
                    rep!(&[PROVINCE_W], 0, 1),
                ]
            }
        }
        Ix::Muster | Ix::Dissolve | Ix::Garrison | Ix::Explore => {
            const { &[one!(PLAYER_PROLOGUE), one!(&[HOLDING_W, PROVINCE_W])] }
        }
        Ix::SettleExplore => {
            const {
                &[one!(&[
                    s!("payer", Wallet, true, R),
                    SEASON_R,
                    HOLDING_W,
                    CITIZEN_W,
                    CACHE_OR_ARCHIVE,
                    ANCHOR_OR_ARCHIVE,
                ])]
            }
        }
        Ix::DisbandStranded => {
            const {
                &[one!(&[
                    ANY_S,
                    SEASON_R,
                    PROVINCE_W,
                    s!("holding", Kind(K::Holding), false, R),
                ])]
            }
        }
        Ix::Depart => {
            const {
                &[
                    one!(PLAYER_PROLOGUE),
                    one!(&[HOLDING_W, PROVINCE_W, SYSTEM]),
                ]
            }
        }
        Ix::Reveal => {
            const {
                &[
                    one!(&[
                        FEE_PAYER,
                        SEASON_R,
                        s!("holding", Kind(K::Holding), false, R),
                        s!("anchor", Kind(K::BellAnchor), false, R),
                        s!("archive", Kind(K::AnchorArchive), false, R),
                        s!("beaconlog", Kind(K::BeaconLog), false, R),
                        s!("inputs", Kind(K::ClashInputs), false, R),
                        s!("dest_province", Kind(K::Province), false, R),
                        s!("arrivalday", Kind(K::ArrivalDay), false, Wr::Either),
                    ]),
                    rep!(&[s!("slot", Kind(K::ArrivalSlot), false, Wr::Either)], 4, 4),
                    rep!(&[s!("path_province", Kind(K::Province), false, R)], 0, 3),
                    one!(&[IX_SYSVAR, SYSTEM]),
                ]
            }
        }
        Ix::SettleDeparture => {
            const {
                &[one!(&[
                    s!("payer", Wallet, true, R),
                    SEASON_R,
                    s!("origin_province", Kind(K::Province), false, W),
                    HOLDING_W,
                ])]
            }
        }
        Ix::SettleTransit => {
            const {
                &[one!(&[
                    PAYER_SW,
                    SEASON_R,
                    HOLDING_W,
                    s!("dest_province", Kind(K::Province), false, W),
                    s!("inputs", Kind(K::ClashInputs), false, W),
                    s!("slot", Kind(K::ArrivalSlot), false, W),
                    s!("home_province", Kind(K::Province), false, W),
                    ANCHOR_OR_ARCHIVE,
                    s!("slot_beneficiary", Wallet, false, W),
                    s!("resolver", Wallet, false, W),
                    s!("holding_rent_payer", Wallet, false, W),
                    s!("settle_beneficiary", Wallet, false, W),
                    SYSTEM,
                ])]
            }
        }
        Ix::SweepPoolOwed => {
            const {
                &[one!(&[
                    ANY_S,
                    SEASON_R,
                    HOLDING_W,
                    s!("dpool", Kind(K::DefencePool), false, W)
                ])]
            }
        }
        Ix::GatherClash => {
            const {
                &[
                    one!(&[
                        FEE_PAYER,
                        SEASON_R,
                        s!("province", Kind(K::Province), false, R),
                        ANCHOR_OR_ARCHIVE,
                        s!("arrivalday", Kind(K::ArrivalDay), false, R),
                        s!("inputs", Kind(K::ClashInputs), false, W),
                        IX_SYSVAR,
                        SYSTEM,
                    ]),
                    rep!(&[s!("slot", Kind(K::ArrivalSlot), false, R)], 0, 24),
                    rep!(&[s!("holding", Kind(K::Holding), false, R)], 0, 10),
                ]
            }
        }
        Ix::ResolveFromInputs => {
            const {
                &[one!(&[
                    FEE_PAYER,
                    SEASON_R,
                    PROVINCE_W,
                    s!("inputs", Kind(K::ClashInputs), false, W),
                    CACHE_OR_ARCHIVE,
                    ANCHOR_OR_ARCHIVE,
                    IX_SYSVAR,
                ])]
            }
        }
        Ix::ResolveClash => {
            const {
                &[
                    one!(&[FEE_PAYER, SEASON_R, PROVINCE_W]),
                    rep!(&[s!("any", Acc::Any, false, Wr::Either)], 0, 60),
                ]
            }
        }
        Ix::SkipQuiet => {
            const {
                &[
                    one!(&[
                        s!("payer", Wallet, true, R),
                        SEASON_R,
                        PROVINCE_W,
                        s!("arrivalday_0", Kind(K::ArrivalDay), false, R),
                        s!("arrivalday_1", Kind(K::ArrivalDay), false, R),
                    ]),
                    rep!(&[ANCHOR_OR_ARCHIVE], 1, 24),
                ]
            }
        }
        Ix::CloseClashInputs => {
            const {
                &[one!(&[
                    ANY_S,
                    SEASON_R,
                    s!("province", Kind(K::Province), false, R),
                    s!("inputs", Kind(K::ClashInputs), false, W),
                    s!("rent_to", Wallet, false, W),
                ])]
            }
        }
        Ix::CloseArrivalDay => {
            const {
                &[one!(&[
                    ANY_S,
                    SEASON_R,
                    s!("province", Kind(K::Province), false, R),
                    s!("day", Kind(K::ArrivalDay), false, W),
                    s!("rent_to", Wallet, false, W),
                ])]
            }
        }
        Ix::CloseArrivalSlot => {
            const {
                &[
                    one!(&[
                        ANY_S,
                        SEASON_R,
                        s!("slot", Kind(K::ArrivalSlot), false, W),
                        s!("rent_to", Wallet, false, W)
                    ]),
                    rep!(&[s!("anchor", Kind(K::BellAnchor), false, R)], 0, 1),
                ]
            }
        }
        Ix::ClaimDefence => {
            const {
                &[
                    one!(&[
                        s!("keeper", Wallet, true, W),
                        SEASON_R,
                        s!("dpool", Kind(K::DefencePool), false, W),
                        s!("claim", Kind(K::DefenceClaim), false, W),
                        SYSTEM,
                    ]),
                    rep!(
                        &[
                            s!("slot", Kind(K::ArrivalSlot), false, W),
                            s!("anchor", Kind(K::BellAnchor), false, R)
                        ],
                        1,
                        crate::layout::beacon::defence_claim::MAX_SLOTS as u8,
                    ),
                ]
            }
        }
    }
}

/// Fewest and most accounts of an instruction.
pub fn count_bounds(ix: Ix) -> (usize, usize) {
    accounts_of(ix).iter().fold((0, 0), |(lo, hi), g| {
        (
            lo + g.specs.len() * g.min as usize,
            hi + g.specs.len() * g.max as usize,
        )
    })
}

/// The position list for given repeat counts (one count per group; fixed
/// groups take their `min`). `None` if a count is out of range or the
/// list is longer than `out`.
pub fn resolve(ix: Ix, counts: &[u8], out: &mut [Spec]) -> Option<usize> {
    let groups = accounts_of(ix);
    if counts.len() != groups.len() {
        return None;
    }
    let mut n = 0;
    for (g, &c) in groups.iter().zip(counts) {
        if c < g.min || c > g.max {
            return None;
        }
        for _ in 0..c {
            for sp in g.specs {
                *out.get_mut(n)? = *sp;
                n += 1;
            }
        }
    }
    Some(n)
}

/// Repeat counts from the total account count when at most one group
/// varies (the common case); `None` when two groups vary or the count does
/// not fit.
pub fn counts_from_len(ix: Ix, n_accounts: usize, out: &mut [u8]) -> Option<usize> {
    let groups = accounts_of(ix);
    if out.len() < groups.len() {
        return None;
    }
    let fixed: usize = groups.iter().map(|g| g.specs.len() * g.min as usize).sum();
    let varying: usize = groups.iter().filter(|g| g.min != g.max).count();
    for (i, g) in groups.iter().enumerate() {
        out[i] = g.min;
    }
    if varying == 0 {
        return (n_accounts == fixed).then_some(groups.len());
    }
    if varying > 1 {
        return None;
    }
    let (i, g) = groups.iter().enumerate().find(|(_, g)| g.min != g.max)?;
    let extra = n_accounts.checked_sub(fixed)?;
    let w = g.specs.len();
    if extra % w != 0 {
        return None;
    }
    let c = g.min as usize + extra / w;
    if c > g.max as usize {
        return None;
    }
    out[i] = c as u8;
    Some(groups.len())
}

// ------------------------------------------------------------ views

/// What the program knows of one account.
#[derive(Clone, Copy, Debug)]
pub struct AccountView<'a> {
    pub key: &'a [u8; 32],
    pub owner: &'a [u8; 32],
    pub lamports: u64,
    pub data: &'a [u8],
    pub is_signer: bool,
    pub is_writable: bool,
}

/// Signer and writability flags of the resolved list (`TooManyAccounts`
/// for a wrong count, `Auth` for a missing signature, `BadAccount` for a
/// writability mismatch).
pub fn check_flags(specs: &[Spec], accounts: &[AccountView<'_>]) -> Result<(), FrontierError> {
    if specs.len() != accounts.len() {
        return Err(FrontierError::TooManyAccounts);
    }
    for (sp, a) in specs.iter().zip(accounts) {
        if sp.signer && !a.is_signer {
            return Err(FrontierError::Auth);
        }
        match sp.wr {
            Wr::W if !a.is_writable => return Err(FrontierError::BadAccount),
            Wr::R if a.is_writable && matches!(sp.acc, Acc::Kind(_) | Acc::Either(..)) => {
                // A program account listed read-only must not be writable
                // (keeps the lock set of §8.8 honest).
                return Err(FrontierError::BadAccount);
            }
            _ => {}
        }
        match sp.acc {
            Acc::System if *a.key != ids::SYSTEM_PROGRAM => return Err(FrontierError::BadAccount),
            Acc::IxSysvar if *a.key != ids::INSTRUCTIONS_SYSVAR => {
                return Err(FrontierError::BadAccount)
            }
            Acc::Incinerator if *a.key != ids::INCINERATOR => {
                return Err(FrontierError::BadAccount)
            }
            _ => {}
        }
    }
    Ok(())
}

/// Instructions marked top-level only refuse CPI (`NotTopLevel`).
pub fn check_top_level(ix: Ix, stack_height: usize) -> Result<(), FrontierError> {
    if ix.top_level_only() && stack_height > 1 {
        Err(FrontierError::NotTopLevel)
    } else {
        Ok(())
    }
}

/// Absent: the canonical address, owned by System, no data; lamports are
/// ignored (pre-funded = absent, §4.1).
pub fn is_absent(a: &AccountView<'_>) -> bool {
    *a.owner == ids::SYSTEM_PROGRAM && a.data.is_empty()
}

/// The account must be at `expected` (`BadAddress`).
pub fn check_key(a: &AccountView<'_>, expected: &[u8; 32]) -> Result<(), FrontierError> {
    if a.key == expected {
        Ok(())
    } else {
        Err(FrontierError::BadAddress)
    }
}

/// A present program account of `kind` for `season_id`: owner = program,
/// full size, magic, season (`BadAccount`).
pub fn check_present(
    a: &AccountView<'_>,
    program: &[u8; 32],
    kind: AccountKind,
    season_id: u64,
) -> Result<(), FrontierError> {
    if a.owner != program || a.data.len() < kind.size() {
        return Err(FrontierError::BadAccount);
    }
    match crate::layout::read_short_header(a.data) {
        Some((m, id)) if m == kind.magic() && id == season_id => Ok(()),
        _ => Err(FrontierError::BadAccount),
    }
}

/// Present (program-owned, checked) or absent; anything else is
/// `BadAccount`. The caller has checked the address.
pub fn presence(
    a: &AccountView<'_>,
    program: &[u8; 32],
    kind: AccountKind,
    season_id: u64,
) -> Result<bool, FrontierError> {
    if is_absent(a) {
        return Ok(false);
    }
    check_present(a, program, kind, season_id).map(|_| true)
}

// ------------------------------------------------------------ season

/// The Season fields every prologue needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeasonHdr {
    pub id: u64,
    pub bump: u8,
    pub stored_status: u8,
    pub status: u8,
    pub genesis_ts: i64,
    pub end_bell: u32,
    pub join_close_bell: u32,
    pub bucket_rate_per_h: u16,
    pub bucket_burst: u16,
}

impl SeasonHdr {
    /// Current bell (`None` before genesis).
    pub fn bell(&self, now: i64) -> Option<u32> {
        bell_at(self.genesis_ts, now)
    }
}

/// `b(t) = ⌊(t − genesis_ts) / 600⌋` for `t ≥ genesis_ts` (§5.1).
pub fn bell_at(genesis_ts: i64, t: i64) -> Option<u32> {
    let dt = t.checked_sub(genesis_ts)?;
    if dt < 0 {
        return None;
    }
    u32::try_from(dt / 600).ok()
}

/// The Season account, structurally: owner, magic, full size and the
/// fields every prologue needs (`BadAccount`). The status and the ruleset
/// are the caller's next steps, in that order (§5.6 step 1; a season that
/// is not Running reports `WrongStatus` whatever its ruleset, and an
/// Announced season has no ruleset yet). The caller verifies the PDA
/// address with the stored bump.
pub fn read_season(
    a: &AccountView<'_>,
    program: &[u8; 32],
    now: i64,
) -> Result<SeasonHdr, FrontierError> {
    let d = a.data;
    if a.owner != program || d.len() < S::SIZE || rd_arr::<8>(d, 0) != Some(S::MAGIC) {
        return Err(FrontierError::BadAccount);
    }
    let bad = FrontierError::BadAccount;
    let stored = rd_u8(d, S::STATUS).ok_or(bad)?;
    let genesis_ts = rd_i64(d, S::GENESIS_TS).ok_or(bad)?;
    Ok(SeasonHdr {
        id: rd_u64(d, S::SEASON_ID).ok_or(bad)?,
        bump: rd_u8(d, S::BUMP).ok_or(bad)?,
        stored_status: stored,
        status: S::effective_status(stored, genesis_ts, now),
        genesis_ts,
        end_bell: rd_u32(d, S::END_BELL).ok_or(bad)?,
        join_close_bell: rd_u32(d, S::JOIN_CLOSE_BELL).ok_or(bad)?,
        bucket_rate_per_h: rd_u16(d, S::BUCKET_RATE_PER_H).ok_or(bad)?,
        bucket_burst: rd_u16(d, S::BUCKET_BURST).ok_or(bad)?,
    })
}

/// `Season.ruleset_hash == RULESET_HASH`, else `RulesetMismatch` (§5.6
/// step 1, after the status).
pub fn check_ruleset(a: &AccountView<'_>, ruleset_hash: &[u8; 32]) -> Result<(), FrontierError> {
    if rd_arr::<32>(a.data, S::RULESET_HASH).as_ref() != Some(ruleset_hash) {
        return Err(FrontierError::RulesetMismatch);
    }
    Ok(())
}

/// [`read_season`], the effective status in `allowed` (`WrongStatus`),
/// then the ruleset when `ruleset_hash` is given (`RulesetMismatch`):
/// §5.6 step 1 in its pinned order. Status-agnostic lifecycle callers
/// (AnnounceSeason, CreateSeason, AbortSeason path b) pass `None`.
pub fn check_season(
    a: &AccountView<'_>,
    program: &[u8; 32],
    ruleset_hash: Option<&[u8; 32]>,
    allowed: &[u8],
    now: i64,
) -> Result<SeasonHdr, FrontierError> {
    let s = read_season(a, program, now)?;
    if !allowed.contains(&s.status) {
        return Err(FrontierError::WrongStatus);
    }
    if let Some(h) = ruleset_hash {
        check_ruleset(a, h)?;
    }
    Ok(s)
}

// ------------------------------------------------------------ player prologue

/// What the player prologue established.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerCtx {
    pub season: SeasonHdr,
    pub now_bell: u32,
    /// The actor signed as the session key (not the wallet).
    pub by_session: bool,
    pub citizen_tag: u64,
}

/// Steps 1 and 2 of the player prologue passed; [`PlayerStart::finish`]
/// runs steps 3 (the bucket, which writes the Citizen) and 4, so the
/// checks can only run in the pinned order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub struct PlayerStart {
    pub season: SeasonHdr,
    pub by_session: bool,
    pub citizen_tag: u64,
}

impl PlayerStart {
    /// Step 3 ([`debit_bucket`] on the Citizen's data, `Bucket`) then step
    /// 4 (`bell(now) < end_bell`, `WrongStatus`).
    pub fn finish(self, citizen: &mut [u8], now: i64) -> Result<PlayerCtx, FrontierError> {
        let s = self.season;
        debit_bucket(
            citizen,
            now,
            s.genesis_ts,
            s.bucket_rate_per_h,
            s.bucket_burst,
        )?;
        let now_bell = s.bell(now).ok_or(FrontierError::WrongStatus)?;
        if now_bell >= s.end_bell {
            return Err(FrontierError::WrongStatus);
        }
        Ok(PlayerCtx {
            season: s,
            now_bell,
            by_session: self.by_session,
            citizen_tag: self.citizen_tag,
        })
    }
}

/// Player prologue steps 1 and 2 (§5.6) over `[actor, payer, season,
/// citizen]`, in order: signatures (`Auth`) and writability
/// (`BadAccount`, as `check_flags` reports it), then the season (status
/// Running, then the ruleset), then the citizen. Steps 3 and 4 follow in
/// [`PlayerStart::finish`]. `session_ok` lets SetSession insist on the
/// wallet (`false` = wallet only).
pub fn player_prologue(
    accounts: &[AccountView<'_>],
    program: &[u8; 32],
    ruleset_hash: &[u8; 32],
    now: i64,
    session_ok: bool,
) -> Result<PlayerStart, FrontierError> {
    let [actor, payer, season, citizen] = match accounts.get(..PLAYER_PROLOGUE_LEN) {
        Some([a, b, c, d]) => [a, b, c, d],
        _ => return Err(FrontierError::TooManyAccounts),
    };
    if !actor.is_signer || !payer.is_signer {
        return Err(FrontierError::Auth);
    }
    if !payer.is_writable || !citizen.is_writable {
        return Err(FrontierError::BadAccount);
    }
    // 1. season: status, then ruleset
    let s = check_season(
        season,
        program,
        Some(ruleset_hash),
        &[S::STATUS_RUNNING],
        now,
    )?;
    // 2. citizen
    check_present(citizen, program, AccountKind::Citizen, s.id)?;
    let d = citizen.data;
    let bad = FrontierError::BadAccount;
    let wallet: [u8; 32] = rd_arr(d, C::WALLET).ok_or(bad)?;
    let ctx = AddrCtx {
        season: *season.key,
        program: *program,
    };
    check_key(citizen, &ctx.citizen_by_tag15(&citizen_tag15(&wallet)))?;
    let by_session = if *actor.key == wallet {
        false
    } else {
        let session: [u8; 32] = rd_arr(d, C::SESSION).ok_or(bad)?;
        if !session_ok || session == [0u8; 32] || *actor.key != session {
            return Err(FrontierError::Auth);
        }
        if now >= rd_i64(d, C::SESSION_EXPIRY).ok_or(bad)? {
            return Err(FrontierError::SessionExpired);
        }
        true
    };
    Ok(PlayerStart {
        season: s,
        by_session,
        citizen_tag: crate::addr::citizen_tag(citizen.key),
    })
}

/// Step 3 (§5.6): refill `rate × Δt` milli-tokens (cap `burst × 1,000`),
/// debit 1,000, else `Bucket`. `bucket_t` counts seconds since genesis.
pub fn debit_bucket(
    citizen: &mut [u8],
    now: i64,
    genesis_ts: i64,
    rate_per_h: u16,
    burst: u16,
) -> Result<(), FrontierError> {
    let bad = FrontierError::BadAccount;
    let milli = rd_u32(citizen, C::BUCKET_MILLI).ok_or(bad)? as u64;
    let t0 = rd_u32(citizen, C::BUCKET_T).ok_or(bad)? as i64;
    let t = now
        .checked_sub(genesis_ts)
        .ok_or(FrontierError::Overflow)?
        .max(0);
    let dt = (t - t0).max(0) as u64;
    let cap = burst as u64 * 1_000;
    let refill = (rate_per_h as u64).saturating_mul(1_000).saturating_mul(dt) / 3_600;
    let have = milli.saturating_add(refill).min(cap);
    let left = have.checked_sub(1_000).ok_or(FrontierError::Bucket)?;
    let t32 = u32::try_from(t).map_err(|_| FrontierError::Overflow)?;
    if wr_u32(citizen, C::BUCKET_MILLI, left as u32) && wr_u32(citizen, C::BUCKET_T, t32) {
        Ok(())
    } else {
        Err(bad)
    }
}

// ------------------------------------------------------------ holdings

/// The Holding fields the resident checks need.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoldingHdr {
    pub p: i16,
    pub q: i16,
    pub site: u8,
    pub gen: u8,
    pub state: u8,
    pub ticket_bell: u32,
    pub final_ts: i64,
}

/// Step 5 (§5.6): owner, magic, season, canonical address from its stored
/// key, and `owner_citizen == citizen` (`NotOwner`).
pub fn check_holding(
    holding: &AccountView<'_>,
    ctx: &AddrCtx,
    season_id: u64,
    citizen: &[u8; 32],
) -> Result<HoldingHdr, FrontierError> {
    check_present(holding, &ctx.program, AccountKind::Holding, season_id)?;
    let d = holding.data;
    let bad = FrontierError::BadAccount;
    let h = HoldingHdr {
        p: rd_i16(d, H::P).ok_or(bad)?,
        q: rd_i16(d, H::Q).ok_or(bad)?,
        site: rd_u8(d, H::SITE).ok_or(bad)?,
        gen: rd_u8(d, H::GEN).ok_or(bad)?,
        state: rd_u8(d, H::STATE).ok_or(bad)?,
        ticket_bell: rd_u32(d, H::TICKET_BELL).ok_or(bad)?,
        final_ts: rd_i64(d, H::FINAL_TS).ok_or(bad)?,
    };
    check_key(holding, &ctx.holding(h.p as i32, h.q as i32, h.site))?;
    if rd_arr::<32>(d, H::OWNER_CITIZEN).as_ref() != Some(citizen) {
        return Err(FrontierError::NotOwner);
    }
    Ok(h)
}

/// Whether the Province's ticket cohort for `ticket_bell` is closed (I-47):
/// no open record of that bell (every ticket settled, or expired).
pub fn cohort_closed(province: &[u8], ticket_bell: u32, now_bell: u32) -> bool {
    if now_bell >= ticket_bell.saturating_add(P::COHORT_BELLS) {
        return true;
    }
    (0..P::COHORTS_N).all(|i| {
        let o = P::cohort(i);
        match (
            rd_u32(province, o + cohort::BELL),
            rd_u16(province, o + cohort::FILED),
            rd_u16(province, o + cohort::SETTLED),
        ) {
            (Some(b), Some(f), Some(s)) => b != ticket_bell || f == 0 || s >= f,
            _ => false,
        }
    })
}

/// The lazy provisional → final flip (§5.6 step 5, I-29, I-47): state 1,
/// `now ≥ final_ts` and the cohort closed.
pub fn finality_due(h: &HoldingHdr, province: &[u8], now: i64, now_bell: u32) -> bool {
    h.state == H::STATE_PROVISIONAL
        && now >= h.final_ts
        && cohort_closed(province, h.ticket_bell, now_bell)
}

/// Applies the flip: Holding state 2, Citizen flag 4 set, provisional flag
/// cleared. (JoinShard `final_holdings` is not written here, §5.6.)
pub fn apply_finality(holding: &mut [u8], citizen: &mut [u8]) -> Result<(), FrontierError> {
    let bad = FrontierError::BadAccount;
    let flags = rd_u8(citizen, C::FLAGS).ok_or(bad)?;
    let f = (flags | C::FLAG_FIRST_HOLDING_FINAL) & !C::FLAG_PROVISIONAL;
    if wr_u8(holding, H::STATE, H::STATE_FINAL) && wr_u8(citizen, C::FLAGS, f) {
        Ok(())
    } else {
        Err(bad)
    }
}

/// Step 6 (§5.6, I-44): the host appears in a transit record of the
/// Holding in state 1–3.
pub fn host_in_transit(holding: &[u8], host_id: u64) -> bool {
    (0..H::TRANSIT_N).any(|i| {
        let o = H::transit(i);
        matches!(rd_u8(holding, o + T::STATE), Some(s) if T::in_transit(s))
            && rd_u64(holding, o + T::HOST_ID) == Some(host_id)
    })
}

/// Resident actions at bell `b` need the province resolved through `b − 2`
/// (§5.1): `resolved_next + 1 ≥ b`.
pub const fn resident_ok(resolved_next: u32, b: u32) -> bool {
    resolved_next as u64 + 1 >= b as u64
}

// ------------------------------------------------------------ keeper prologue

/// Keeper-write prologue (§5.6): `[fee_payer s,w] [season]`; the season's
/// effective status must be in the instruction's `allowed` set
/// (`WrongStatus`), then the ruleset matches (`RulesetMismatch`). A
/// fee payer that did not sign is `Auth`; one not writable is
/// `BadAccount`.
pub fn keeper_prologue(
    accounts: &[AccountView<'_>],
    program: &[u8; 32],
    ruleset_hash: &[u8; 32],
    allowed: &[u8],
    now: i64,
) -> Result<SeasonHdr, FrontierError> {
    let (fee_payer, season) = match accounts.get(..2) {
        Some([a, b]) => (a, b),
        _ => return Err(FrontierError::TooManyAccounts),
    };
    if !fee_payer.is_signer {
        return Err(FrontierError::Auth);
    }
    if !fee_payer.is_writable {
        return Err(FrontierError::BadAccount);
    }
    check_season(season, program, Some(ruleset_hash), allowed, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::AddrCtx;
    use crate::layout::{world::season as SL, write_header};

    #[test]
    fn every_instruction_has_a_consistent_table() {
        for ix in Ix::ALL {
            let groups = accounts_of(*ix);
            assert!(!groups.is_empty(), "{}", ix.name());
            let (lo, hi) = count_bounds(*ix);
            assert!(
                lo >= 1 && lo <= hi && hi <= 128,
                "{}: {lo}..{hi}",
                ix.name()
            );
            let counts: std::vec::Vec<u8> = groups.iter().map(|g| g.max).collect();
            let mut out = [s!("", Wallet, false, R); 160];
            let n = resolve(*ix, &counts, &mut out).unwrap();
            assert_eq!(n, hi);
            // a program-side lock limit: 64 accounts per transaction (§5.5)
            if *ix != Ix::ResolveClash && *ix != Ix::CloseSeason {
                assert!(hi <= 64, "{} lists {hi} accounts", ix.name());
            }
        }
        // player instructions start with the player prologue
        for ix in [
            Ix::SetVigil,
            Ix::FileTicket,
            Ix::Harvest,
            Ix::Depart,
            Ix::Explore,
        ] {
            assert_eq!(accounts_of(ix)[0].specs, PLAYER_PROLOGUE, "{}", ix.name());
        }
        // Reveal: 18 accounts with three path provinces
        assert_eq!(count_bounds(Ix::Reveal), (15, 18));
    }

    #[test]
    fn counts_from_len_resolves_single_varying_group() {
        let mut c = [0u8; 8];
        assert_eq!(
            counts_from_len(Ix::FileTicket, 4 + 1 + 2 + 1, &mut c),
            Some(4)
        );
        assert_eq!(&c[..4], &[1, 1, 2, 1]);
        assert_eq!(counts_from_len(Ix::FileTicket, 4 + 1 + 4 + 1, &mut c), None);
        assert_eq!(counts_from_len(Ix::Depart, 7, &mut c), Some(2));
        assert_eq!(counts_from_len(Ix::Depart, 8, &mut c), None);
        assert_eq!(
            counts_from_len(Ix::GatherClash, 12, &mut c),
            None,
            "two varying groups"
        );
    }

    fn season_bytes(program_ruleset: [u8; 32], status: u8, genesis_ts: i64) -> std::vec::Vec<u8> {
        let mut d = std::vec![0u8; SL::SIZE];
        write_header(&mut d, AccountKind::Season, 7);
        d[SL::STATUS] = status;
        d[SL::RULESET_HASH..SL::RULESET_HASH + 32].copy_from_slice(&program_ruleset);
        d[SL::GENESIS_TS..SL::GENESIS_TS + 8].copy_from_slice(&genesis_ts.to_le_bytes());
        d[SL::END_BELL..SL::END_BELL + 4].copy_from_slice(&1_008u32.to_le_bytes());
        d[SL::BUCKET_RATE_PER_H..SL::BUCKET_RATE_PER_H + 2].copy_from_slice(&30u16.to_le_bytes());
        d[SL::BUCKET_BURST..SL::BUCKET_BURST + 2].copy_from_slice(&60u16.to_le_bytes());
        d
    }

    #[test]
    fn player_prologue_checks_in_order() {
        let program = [9u8; 32];
        let season_key = [5u8; 32];
        let ruleset = [1u8; 32];
        let wallet = [2u8; 32];
        let session = [3u8; 32];
        let ctx = AddrCtx {
            season: season_key,
            program,
        };
        let citizen_key = ctx.citizen(&wallet);
        let sd = season_bytes(ruleset, SL::STATUS_SEEDED, 1_000);
        let mut cd = std::vec![0u8; C::SIZE];
        write_header(&mut cd, AccountKind::Citizen, 7);
        cd[C::WALLET..C::WALLET + 32].copy_from_slice(&wallet);
        cd[C::SESSION..C::SESSION + 32].copy_from_slice(&session);
        cd[C::SESSION_EXPIRY..C::SESSION_EXPIRY + 8].copy_from_slice(&5_000i64.to_le_bytes());
        let sys = ids::SYSTEM_PROGRAM;
        let view = |key: &'static [u8; 32]| key;
        let _ = view;
        let actor_key = session;
        let accts_at = |now: i64,
                        actor: &[u8; 32],
                        sd: &[u8],
                        cd: &[u8],
                        ck: &[u8; 32]|
         -> Result<PlayerCtx, FrontierError> {
            let mut cm = cd.to_vec();
            let v = [
                AccountView {
                    key: actor,
                    owner: &sys,
                    lamports: 0,
                    data: &[],
                    is_signer: true,
                    is_writable: false,
                },
                AccountView {
                    key: &[8; 32],
                    owner: &sys,
                    lamports: 1,
                    data: &[],
                    is_signer: true,
                    is_writable: true,
                },
                AccountView {
                    key: &season_key,
                    owner: &program,
                    lamports: 1,
                    data: sd,
                    is_signer: false,
                    is_writable: false,
                },
                AccountView {
                    key: ck,
                    owner: &program,
                    lamports: 1,
                    data: cd,
                    is_signer: false,
                    is_writable: true,
                },
            ];
            let start = player_prologue(&v, &program, &ruleset, now, true)?;
            start.finish(&mut cm, now)
        };
        let accts = |actor: &[u8; 32], sd: &[u8], cd: &[u8], ck: &[u8; 32]| {
            accts_at(2_000, actor, sd, cd, ck)
        };
        cd[C::BUCKET_MILLI..C::BUCKET_MILLI + 4].copy_from_slice(&60_000u32.to_le_bytes());
        let ok = accts(&actor_key, &sd, &cd, &citizen_key).unwrap();
        assert!(ok.by_session);
        assert_eq!(ok.now_bell, 1);
        assert!(!accts(&wallet, &sd, &cd, &citizen_key).unwrap().by_session);
        assert_eq!(
            accts(&[4; 32], &sd, &cd, &citizen_key),
            Err(FrontierError::Auth)
        );
        assert_eq!(
            accts(&actor_key, &sd, &cd, &[0; 32]),
            Err(FrontierError::BadAddress)
        );
        let before = season_bytes(ruleset, SL::STATUS_SEEDED, 3_000);
        assert_eq!(
            accts(&actor_key, &before, &cd, &citizen_key),
            Err(FrontierError::WrongStatus)
        );
        let other_rules = season_bytes([0; 32], SL::STATUS_SEEDED, 1_000);
        assert_eq!(
            accts(&actor_key, &other_rules, &cd, &citizen_key),
            Err(FrontierError::RulesetMismatch)
        );
        let mut expired = cd.clone();
        expired[C::SESSION_EXPIRY..C::SESSION_EXPIRY + 8].copy_from_slice(&1_999i64.to_le_bytes());
        assert_eq!(
            accts(&actor_key, &sd, &expired, &citizen_key),
            Err(FrontierError::SessionExpired)
        );
        // Step 1's order (integ-W1 review): the status before the ruleset,
        // so a season that is not Running reports WrongStatus even with
        // another (or no) ruleset.
        let before_other = season_bytes([0; 32], SL::STATUS_SEEDED, 3_000);
        assert_eq!(
            accts(&actor_key, &before_other, &cd, &citizen_key),
            Err(FrontierError::WrongStatus)
        );
        let announced = season_bytes([0; 32], SL::STATUS_ANNOUNCED, 1_000);
        assert_eq!(
            accts(&actor_key, &announced, &cd, &citizen_key),
            Err(FrontierError::WrongStatus)
        );
        // Step 3 before step 4: an empty bucket after the end bell is
        // Bucket; with tokens it is WrongStatus.
        let after_end = 1_000 + 1_008 * 600 + 5;
        let mut empty = cd.clone();
        empty[C::BUCKET_MILLI..C::BUCKET_MILLI + 4].copy_from_slice(&0u32.to_le_bytes());
        empty[C::BUCKET_T..C::BUCKET_T + 4]
            .copy_from_slice(&((after_end - 1_000) as u32).to_le_bytes());
        empty[C::SESSION_EXPIRY..C::SESSION_EXPIRY + 8].copy_from_slice(&i64::MAX.to_le_bytes());
        assert_eq!(
            accts_at(after_end, &actor_key, &sd, &empty, &citizen_key),
            Err(FrontierError::Bucket)
        );
        let mut full = empty.clone();
        full[C::BUCKET_MILLI..C::BUCKET_MILLI + 4].copy_from_slice(&60_000u32.to_le_bytes());
        assert_eq!(
            accts_at(after_end, &actor_key, &sd, &full, &citizen_key),
            Err(FrontierError::WrongStatus)
        );
    }

    #[test]
    fn keeper_prologue_checks_status_then_ruleset() {
        let program = [9u8; 32];
        let ruleset = [1u8; 32];
        let sys = ids::SYSTEM_PROGRAM;
        let run = |signer: bool, writable: bool, sd: &[u8], allowed: &[u8]| {
            let v = [
                AccountView {
                    key: &[8; 32],
                    owner: &sys,
                    lamports: 1,
                    data: &[],
                    is_signer: signer,
                    is_writable: writable,
                },
                AccountView {
                    key: &[5; 32],
                    owner: &program,
                    lamports: 1,
                    data: sd,
                    is_signer: false,
                    is_writable: false,
                },
            ];
            keeper_prologue(&v, &program, &ruleset, allowed, 2_000).map(|s| s.status)
        };
        let running = season_bytes(ruleset, SL::STATUS_SEEDED, 1_000);
        let other = season_bytes([0; 32], SL::STATUS_SEEDED, 3_000);
        let ok = [SL::STATUS_RUNNING];
        assert_eq!(run(true, true, &running, &ok), Ok(SL::STATUS_RUNNING));
        assert_eq!(run(false, true, &running, &ok), Err(FrontierError::Auth));
        assert_eq!(
            run(true, false, &running, &ok),
            Err(FrontierError::BadAccount)
        );
        assert_eq!(
            run(true, true, &other, &ok),
            Err(FrontierError::WrongStatus)
        );
        assert_eq!(
            run(true, true, &other, &[SL::STATUS_SEEDED]),
            Err(FrontierError::RulesetMismatch)
        );
        // Status-agnostic lifecycle callers skip the ruleset.
        let v = AccountView {
            key: &[5; 32],
            owner: &program,
            lamports: 1,
            data: &other,
            is_signer: false,
            is_writable: false,
        };
        assert!(check_season(&v, &program, None, &[SL::STATUS_SEEDED], 2_000).is_ok());
    }

    #[test]
    fn bucket_refills_and_refuses() {
        let mut cd = std::vec![0u8; C::SIZE];
        cd[C::BUCKET_MILLI..C::BUCKET_MILLI + 4].copy_from_slice(&60_000u32.to_le_bytes());
        for _ in 0..60 {
            debit_bucket(&mut cd, 100, 100, 30, 60).unwrap();
        }
        assert_eq!(
            debit_bucket(&mut cd, 100, 100, 30, 60),
            Err(FrontierError::Bucket)
        );
        // 30 per hour: one token every 120 s
        debit_bucket(&mut cd, 220, 100, 30, 60).unwrap();
        assert_eq!(
            debit_bucket(&mut cd, 220, 100, 30, 60),
            Err(FrontierError::Bucket)
        );
    }

    #[test]
    fn cohorts_transit_and_finality() {
        let mut prov = std::vec![0u8; P::SIZE];
        let o = P::cohort(3);
        prov[o..o + 4].copy_from_slice(&50u32.to_le_bytes());
        prov[o + 4..o + 6].copy_from_slice(&2u16.to_le_bytes());
        prov[o + 6..o + 8].copy_from_slice(&1u16.to_le_bytes());
        assert!(!cohort_closed(&prov, 50, 60));
        assert!(cohort_closed(&prov, 50, 74), "expires after 24 bells");
        assert!(cohort_closed(&prov, 51, 60));
        let h = HoldingHdr {
            p: 0,
            q: 0,
            site: 0,
            gen: 0,
            state: 1,
            ticket_bell: 50,
            final_ts: 10,
        };
        assert!(!finality_due(&h, &prov, 10, 60));
        prov[o + 6..o + 8].copy_from_slice(&2u16.to_le_bytes());
        assert!(finality_due(&h, &prov, 10, 60));
        assert!(!finality_due(&h, &prov, 9, 60));

        let mut hd = std::vec![0u8; H::SIZE];
        let t = H::transit(2);
        hd[t + T::STATE] = T::STATE_SETTLED;
        hd[t + T::HOST_ID..t + T::HOST_ID + 8].copy_from_slice(&77u64.to_le_bytes());
        assert!(host_in_transit(&hd, 77));
        assert!(!host_in_transit(&hd, 78));
        hd[t + T::STATE] = T::STATE_FREE;
        assert!(!host_in_transit(&hd, 77));
        let mut cd = std::vec![0u8; C::SIZE];
        cd[C::FLAGS] = C::FLAG_JOINED | C::FLAG_PROVISIONAL;
        apply_finality(&mut hd, &mut cd).unwrap();
        assert_eq!(hd[H::STATE], H::STATE_FINAL);
        assert_eq!(cd[C::FLAGS], C::FLAG_JOINED | C::FLAG_FIRST_HOLDING_FINAL);
        assert!(resident_ok(8, 9) && !resident_ok(8, 10));
    }
}
