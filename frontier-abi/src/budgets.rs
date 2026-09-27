//! Budgets (M1 contract §5.5, §10.1, §10.2, I-45, I-50).
//!
//! Per instruction kind: the CU budget the gate asserts, the CU limit to
//! request, the loaded-data limit `L(kind)`, the tx byte ceiling, the heap
//! ceiling and the lock count. **Wave-1 values are placeholders**: the CU
//! limit equals the budget until W5-A regenerates it from the G1
//! measurements (measured max + 5%), and `L(kind)` is computed from the
//! kind's worst account set (from [`crate::prologue::accounts_of`]) with a
//! placeholder programdata length until the release `.so` exists; the
//! value requested is never below the 1-MiB working default (I-45).
//!
//! `L(kind) = round_up(programdata_len + 45 + Σ_accounts (data_len + 64), 32,768)`
//! over every account the transaction loads (SIMD-0186 counts the LoaderV3
//! programdata), with `programdata_len` = the deployed `max_len` =
//! `round_up(1.25 × .so, 4,096)`.

use crate::ix::data_len_range;
use crate::prologue::{accounts_of, count_bounds, Acc, Wr};
use crate::tags::Ix;

/// Most regions per PostAnchorMulti in a legacy transaction ≤ 1,232 B
/// (`tests::multi_anchor_fits`: 7 fits, 8 does not). W2-A confirms it with
/// its tx-size test.
pub const MULTI_MAX_REGIONS: usize = 7;
/// Heap ceiling at every gated fill (28 KiB).
pub const HEAP_GATE: u32 = 28_672;
/// The bump allocator's ceiling under a heap frame (I-50).
pub const HEAP_FRAME: u32 = 262_144;
/// Largest CU limit of the keeper's retry ladder (I-50).
pub const CU_LADDER_MAX: u32 = 1_400_000;
/// Account locks per transaction.
pub const LOCKS_MAX: u32 = 64;
/// Legacy transaction size limit.
pub const TX_MAX: u32 = 1_232;
/// `L` working default until the release `.so` is measured (I-45).
pub const LOADED_LIMIT_WORKING_DEFAULT: u32 = 1_048_576;
/// SIMD-0186 per-account overhead.
pub const ACCOUNT_OVERHEAD: u32 = 64;
/// LoaderV3 ProgramData metadata before the ELF.
pub const PROGRAMDATA_META: u32 = 45;
/// LoaderV3 Program account data (state tag + programdata address).
pub const PROGRAM_ACCOUNT_LEN: u32 = 36;
/// Data counted for a builtin program account (System, ComputeBudget,
/// the incinerator) [estimate, conservative].
pub const BUILTIN_DATA_LEN: u32 = 64;
/// Placeholder programdata length: SP-V2's `program-kprobe` release `.so`
/// (540,608 B, sbpfv2 [measured]) deployed at `round_up(1.25 × .so, 4 KiB)`.
/// Replaced by the M1 release `.so` once W2-A builds it.
pub const PLACEHOLDER_SO_LEN: u32 = 540_608;
pub const PLACEHOLDER_PROGRAMDATA_LEN: u32 = max_len_for(PLACEHOLDER_SO_LEN);

/// `--max-len = round_up(1.25 × so_len, 4,096)` (I-45).
pub const fn max_len_for(so_len: u32) -> u32 {
    let x = (so_len as u64 * 5).div_ceil(4);
    (x.div_ceil(4_096) * 4_096) as u32
}

/// One row of the budgets table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub ix: Ix,
    /// Gate ceiling (0 = recorded, not gated: ResolveClash).
    pub cu_budget: u32,
    /// Extra CU per unit of work (SkipQuiet: per recomputed bell).
    pub cu_per_unit: u32,
    /// CU limit to request [placeholder: the budget until W5-A].
    pub cu_limit: u32,
    /// Tx byte ceiling as §5.5 lists it. Many rows are below the smallest
    /// real transaction (64-B signatures, blockhash, the three ComputeBudget
    /// instructions); [`tx_ceiling`] is the ceiling to gate on.
    pub tx_contract: u32,
}

macro_rules! budgets {
    ($( $ix:ident: $cu:expr, $per:expr, $tx:expr; )*) => {
        /// §5.5 budget table.
        pub const TABLE: &[Budget] = &[ $( Budget {
            ix: Ix::$ix,
            cu_budget: $cu,
            cu_per_unit: $per,
            cu_limit: limit_of($cu, $per),
            tx_contract: $tx,
        }, )* ];
    };
}

/// Placeholder CU limit: the budget (SkipQuiet: 24 recomputed bells;
/// ResolveClash: the ladder maximum), capped at 1.4M.
const fn limit_of(cu: u32, per: u32) -> u32 {
    let l = if cu == 0 {
        CU_LADDER_MAX
    } else {
        cu + 24 * per
    };
    if l > CU_LADDER_MAX {
        CU_LADDER_MAX
    } else {
        l
    }
}

budgets! {
    AnnounceSeason: 25_000, 0, 480;
    CreateSeason: 70_000, 0, 1_100;
    InitBeaconLogs: 80_000, 0, 900;
    InitShards: 45_000, 0, 600;
    ConsumeGenesisSeed: 345_000, 0, 760;
    EndSeason: 10_000, 0, 300;
    CloseSeason: 60_000, 0, 1_232;
    AbortSeason: 20_000, 0, 300;
    SetWindowSchedule: 5_000, 0, 200;
    PostAnchor: 345_000, 0, 800;
    PostAnchorMulti: 400_000, 0, 1_232;
    PostSeed: 345_000, 0, 800;
    PostBeacon: 340_000, 0, 760;
    ArchiveAnchors: 60_000, 0, 1_232;
    CloseSeedCache: 6_000, 0, 300;
    OpenRing: 30_000, 0, 600;
    ConsumeRingSeed: 345_000, 0, 760;
    OpenProvince: 220_000, 0, 400;
    FoldOccupancy: 30_000, 0, 1_200;
    CloseProvince: 10_000, 0, 300;
    Join: 25_000, 0, 700;
    SetSession: 6_000, 0, 300;
    SetVigil: 6_000, 0, 250;
    FileTicket: 17_000, 0, 560;
    SettleTicket: 40_000, 0, 900;
    ReleaseDormant: 25_000, 0, 480;
    CloseHolding: 15_000, 0, 330;
    CloseCitizen: 10_000, 0, 300;
    Harvest: 12_000, 0, 320;
    Build: 22_000, 0, 360;
    Train: 15_000, 0, 330;
    Muster: 25_000, 0, 380;
    Dissolve: 25_000, 0, 380;
    Garrison: 25_000, 0, 380;
    Explore: 18_000, 0, 380;
    SettleExplore: 15_000, 0, 400;
    DisbandStranded: 12_000, 0, 300;
    Depart: 15_000, 0, 800;
    Reveal: 26_000, 0, 1_100;
    SettleDeparture: 15_000, 0, 400;
    SettleTransit: 85_000, 0, 1_100;
    SweepPoolOwed: 8_000, 0, 300;
    GatherClash: 40_000, 0, 1_232;
    ResolveFromInputs: 340_000, 0, 460;
    ResolveClash: 0, 0, 1_232;
    SkipQuiet: 60_000, 30_000, 1_232;
    CloseClashInputs: 8_000, 0, 300;
    CloseArrivalDay: 8_000, 0, 300;
    CloseArrivalSlot: 8_000, 0, 300;
    ClaimDefence: 25_000, 0, 1_000;
}

/// The row of `ix`.
pub fn budget(ix: Ix) -> Budget {
    TABLE
        .iter()
        .copied()
        .find(|b| b.ix == ix)
        .unwrap_or(Budget {
            ix,
            cu_budget: 0,
            cu_per_unit: 0,
            cu_limit: CU_LADDER_MAX,
            tx_contract: TX_MAX,
        })
}

/// Worst-case transaction size estimate of `ix`: its account list at the
/// group maxima, its longest data, every signer position signing, and the
/// three ComputeBudget instructions.
pub fn tx_worst_estimate(ix: Ix) -> u32 {
    let (_, hi) = count_bounds(ix);
    let (_, dhi) = data_len_range(ix);
    tx_size_estimate(signers(ix).max(1), hi as u32, dhi as u32)
}

/// Instructions whose account list at every group maximum cannot fit one
/// legacy transaction: their builders split the work (GatherClash ≤ 22
/// slots + holdings per part, CloseSeason in parts, ResolveClash is
/// test-only, FoldOccupancy lists either 24 shards or 6 funds per part,
/// v1.2: the table's union of both is never one transaction).
pub const fn builder_limited(ix: Ix) -> bool {
    matches!(
        ix,
        Ix::GatherClash | Ix::CloseSeason | Ix::ResolveClash | Ix::FoldOccupancy
    )
}

/// The tx byte ceiling to gate on: the §5.5 value, raised to the worst-case
/// estimate (rounded up to 8 B) where the table is below it, never above
/// 1,232 B.
pub fn tx_ceiling(ix: Ix) -> u32 {
    let b = budget(ix);
    let est = tx_worst_estimate(ix).div_ceil(8) * 8;
    let c = if est > b.tx_contract {
        est
    } else {
        b.tx_contract
    };
    if c > TX_MAX {
        TX_MAX
    } else {
        c
    }
}

/// Worst-case gate CU of `ix` doing `units` of work (SkipQuiet bells).
pub fn cu_gate(ix: Ix, units: u32) -> u32 {
    let b = budget(ix);
    b.cu_budget
        .saturating_add(b.cu_per_unit.saturating_mul(units))
}

/// ComputeBudget instructions every transaction carries: data lengths of
/// SetComputeUnitLimit (5), SetComputeUnitPrice (9),
/// SetLoadedAccountsDataSizeLimit (5).
pub const COMPUTE_BUDGET_IX_DATA: [u32; 3] = [5, 9, 5];

/// Data size of the instructions sysvar for a transaction whose
/// instructions have these `(accounts, data)` sizes: `u16` count, `u16`
/// offsets, per instruction `u16` accounts, `33 × accounts` (flags + key),
/// program id, `u16` data length, data; then the `u16` current index.
pub fn ix_sysvar_len(ixs: &[(u32, u32)]) -> u32 {
    2 + 2 * ixs.len() as u32
        + ixs
            .iter()
            .map(|(a, d)| 2 + 33 * a + 32 + 2 + d)
            .sum::<u32>()
        + 2
}

/// Worst account set of `ix`: its positions at the group maxima.
fn worst_positions(ix: Ix) -> impl Iterator<Item = crate::prologue::Spec> {
    accounts_of(ix).iter().flat_map(|g| {
        core::iter::repeat_n(g.specs, g.max as usize)
            .flatten()
            .copied()
    })
}

/// Data bytes SIMD-0186 counts for one position (without the overhead).
fn position_data(acc: Acc, ix_sysvar: u32) -> u32 {
    match acc {
        Acc::Kind(k) => k.size() as u32,
        Acc::Either(a, b) => {
            let (x, y) = (a.size() as u32, b.size() as u32);
            if x > y {
                x
            } else {
                y
            }
        }
        Acc::Wallet | Acc::Any => 0,
        Acc::System | Acc::Incinerator => BUILTIN_DATA_LEN,
        Acc::IxSysvar => ix_sysvar,
        Acc::ProgramAccount => PROGRAM_ACCOUNT_LEN,
        // The executing program's programdata is counted once, below.
        Acc::ProgramData => 0,
    }
}

/// Loaded-data need of `ix`'s worst account set for a programdata length.
pub fn loaded_need(ix: Ix, programdata_len: u32) -> u64 {
    let (bytes, n) = loaded_accounts(ix);
    programdata_len as u64
        + PROGRAMDATA_META as u64
        + bytes as u64
        + ACCOUNT_OVERHEAD as u64 * n as u64
}

/// The loaded accounts of `ix`'s worst set besides the ELF: Σ data
/// lengths (the instruction's accounts, the Frontier program account, the
/// ComputeBudget builtin) and their count, **plus the ProgramData account**
/// (counted with its 64-B overhead; its data is the programdata length the
/// caller adds). The inputs of `fees::loaded_limit` (I-45).
pub fn loaded_accounts(ix: Ix) -> (u32, u8) {
    let (_, hi) = count_bounds(ix);
    let (_, data_max) = data_len_range(ix);
    let mut ixs = [(0u32, 0u32); 4];
    for (i, d) in COMPUTE_BUDGET_IX_DATA.iter().enumerate() {
        ixs[i] = (0, *d);
    }
    ixs[3] = (hi as u32, data_max as u32);
    let sysvar = ix_sysvar_len(&ixs);
    let (mut bytes, mut n) = (0u32, 0u32);
    for sp in worst_positions(ix) {
        bytes += position_data(sp.acc, sysvar);
        n += 1;
    }
    // The Frontier program account, the ComputeBudget program, the
    // ProgramData account.
    bytes += PROGRAM_ACCOUNT_LEN + BUILTIN_DATA_LEN;
    n += 3;
    (bytes, n.min(u8::MAX as u32) as u8)
}

/// `L(kind)` for a programdata length: the kernel's
/// `fees::loaded_limit` (one formula, integ-W1 review) over
/// [`loaded_accounts`].
pub fn loaded_limit_for(ix: Ix, programdata_len: u32) -> u32 {
    let (bytes, n) = loaded_accounts(ix);
    permutation_rules::frontier::fees::loaded_limit(programdata_len, bytes, n)
}

/// `L(kind)` to request now: the need at the placeholder programdata
/// length, never below the 1-MiB working default (I-45).
pub fn loaded_limit(ix: Ix) -> u32 {
    let l = loaded_limit_for(ix, PLACEHOLDER_PROGRAMDATA_LEN);
    if l < LOADED_LIMIT_WORKING_DEFAULT {
        LOADED_LIMIT_WORKING_DEFAULT
    } else {
        l
    }
}

/// Writable accounts of the worst set (lock count; signers counted once).
pub fn write_locks(ix: Ix) -> u32 {
    worst_positions(ix)
        .filter(|sp| !matches!(sp.wr, Wr::R))
        .count() as u32
}

/// Size of a legacy transaction: `n_sigs` signatures, `n_keys` distinct
/// account keys (program ids included), and instructions of
/// `(accounts, data)` sizes.
pub fn legacy_tx_size(n_sigs: u32, n_keys: u32, ixs: &[(u32, u32)]) -> u32 {
    fn cu16(x: u32) -> u32 {
        if x < 0x80 {
            1
        } else if x < 0x4000 {
            2
        } else {
            3
        }
    }
    cu16(n_sigs)
        + 64 * n_sigs
        + 3
        + cu16(n_keys)
        + 32 * n_keys
        + 32
        + cu16(ixs.len() as u32)
        + ixs
            .iter()
            .map(|(a, d)| 1 + cu16(*a) + a + cu16(*d) + d)
            .sum::<u32>()
}

/// Estimated size of `ix`'s transaction with `n_accounts` accounts (all
/// distinct) and `data` bytes, the three ComputeBudget instructions, and
/// `n_sigs` signers.
pub fn tx_size_estimate(n_sigs: u32, n_accounts: u32, data: u32) -> u32 {
    let ixs = [
        (0, COMPUTE_BUDGET_IX_DATA[0]),
        (0, COMPUTE_BUDGET_IX_DATA[1]),
        (0, COMPUTE_BUDGET_IX_DATA[2]),
        (n_accounts, data),
    ];
    // + the Frontier program id and the ComputeBudget program id
    legacy_tx_size(n_sigs, n_accounts + 2, &ixs)
}

/// Signer positions of the worst set.
pub fn signers(ix: Ix) -> u32 {
    worst_positions(ix).filter(|sp| sp.signer).count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ix;

    #[test]
    fn table_covers_every_instruction_once() {
        for i in Ix::ALL {
            assert_eq!(
                TABLE.iter().filter(|b| b.ix == *i).count(),
                1,
                "{}",
                i.name()
            );
            let b = budget(*i);
            assert!(b.cu_limit <= CU_LADDER_MAX);
            assert!(b.tx_contract <= TX_MAX);
            assert!(tx_ceiling(*i) >= b.tx_contract.min(TX_MAX));
            if !builder_limited(*i) {
                assert!(tx_worst_estimate(*i) <= tx_ceiling(*i), "{}", i.name());
                assert!(tx_worst_estimate(*i) <= TX_MAX, "{}", i.name());
            }
        }
        assert_eq!(cu_gate(Ix::SkipQuiet, 2), 120_000);
        assert_eq!(budget(Ix::Reveal).cu_budget, 26_000);
        assert_eq!(budget(Ix::ResolveFromInputs).cu_budget, 340_000);
    }

    #[test]
    fn placeholder_max_len() {
        // round_up(1.25 × 540,608 = 675,760, 4,096) = 675,840
        assert_eq!(PLACEHOLDER_PROGRAMDATA_LEN, 675_840);
        assert_eq!(max_len_for(4_096), 8_192);
    }

    #[test]
    fn loaded_limits_are_32k_multiples_and_fit_the_default() {
        for i in Ix::ALL {
            let l = loaded_limit(*i);
            assert_eq!(l % 32_768, 0);
            assert!(l >= LOADED_LIMIT_WORKING_DEFAULT);
            // at the placeholder program every worst set fits in 1 MiB
            assert!(
                loaded_need(*i, PLACEHOLDER_PROGRAMDATA_LEN) <= LOADED_LIMIT_WORKING_DEFAULT as u64,
                "{}",
                i.name()
            );
        }
        // Reveal's worst set: 18 accounts, the largest a 12,192-B archive
        let r = loaded_need(Ix::Reveal, 0);
        assert!(r > 12_192 + 4 * 160 + 96 + 1_280 + 4 * 4_096);
    }

    #[test]
    fn transaction_sizes_fit_the_table() {
        // Depart: actor + relay payer sign; 7 accounts; 219 B.
        assert!(tx_size_estimate(2, 7, ix::Depart::LEN as u32) <= 800);
        // Reveal with three path provinces.
        assert!(tx_size_estimate(1, 18, ix::Reveal::LEN as u32) <= 1_100);
        // SettleTransit: 13 accounts, 231 B (≈ 990 B in §5.11).
        let st = tx_size_estimate(1, 13, ix::SettleTransit::LEN as u32);
        assert!(st <= 1_100, "{st}");
        assert!(tx_size_estimate(1, 6, ix::PostAnchor::LEN as u32) <= 800);
        assert!(tx_size_estimate(3, 8, ix::Join::LEN as u32) <= 700);
        // 465 B with the three ComputeBudget instructions: §5.5's 460 is 5 B short.
        assert_eq!(
            tx_size_estimate(1, 7, ix::ResolveFromInputs::LEN as u32),
            465
        );
        assert_eq!(tx_ceiling(Ix::ResolveFromInputs), 472);
        // FoldOccupancy: v1.1's part 1 (24 shards + 6 funds, 33 accounts)
        // did not fit; each v1.2 part does.
        assert!(tx_size_estimate(1, 3 + 24 + 6, ix::FoldOccupancy::LEN as u32) > TX_MAX);
        for part in 0..ix::FoldOccupancy::PARTS {
            let n = 3 + ix::FoldOccupancy::shards_in(part) + ix::FoldOccupancy::funds_in(part);
            let t = tx_size_estimate(1, n as u32, ix::FoldOccupancy::LEN as u32);
            assert!(t <= TX_MAX, "fold part {part}: {t} B");
        }
        assert_eq!(ix::FoldOccupancy::factions_of(2), None);
        // A GatherClash part fits 22 slots + holdings.
        assert!(tx_size_estimate(1, 8 + 22, ix::GatherClash::LEN as u32) <= TX_MAX);
        assert!(tx_size_estimate(1, 8 + 23, ix::GatherClash::LEN as u32) > TX_MAX);
        assert_eq!(tx_size_estimate(2, 5, ix::Harvest::LEN as u32), 427);
    }

    #[test]
    fn multi_anchor_fits() {
        let size = |k: u32| tx_size_estimate(1, 2 + 2 * k + 2, ix::PostAnchorMulti::LEN as u32);
        assert!(
            size(MULTI_MAX_REGIONS as u32) <= TX_MAX,
            "{}",
            size(MULTI_MAX_REGIONS as u32)
        );
        assert!(size(MULTI_MAX_REGIONS as u32 + 1) > TX_MAX);
    }

    #[test]
    fn locks_within_limit() {
        for i in Ix::ALL {
            if matches!(i, Ix::ResolveClash | Ix::CloseSeason) {
                continue;
            }
            let (_, hi) = count_bounds(*i);
            assert!(hi as u32 + 2 <= LOCKS_MAX, "{}", i.name());
            assert!(write_locks(*i) <= hi as u32);
        }
    }
}
