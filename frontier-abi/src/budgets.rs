//! Budgets (M1 contract §5.5, §10.1, §10.2, I-45, I-50).
//!
//! Per instruction kind: the CU budget the gate asserts, the CU limit to
//! request, the loaded-data limit `L(kind)`, the tx byte ceiling, the heap
//! ceiling and the lock count. **W5-A regenerated the table:** the CU
//! limit is the G1 measured maximum + 5 % ([`MEASURED`]), and `L(kind)` is
//! computed from the kind's worst account set (from
//! [`crate::prologue::accounts_of`]) at the release `.so`'s programdata
//! length ([`PLACEHOLDER_SO_LEN`]); the value requested is never below the
//! 1-MiB working default (I-45).
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
/// The `.so` length `L(kind)` is computed for, deployed at
/// `round_up(1.25 × .so, 4 KiB)` (I-45). Waves 1–3 used SP-V2's
/// `program-kprobe` release `.so` (540,608 B); integ-W4 a 1-MiB
/// placeholder over the merged wave-4 `.so` (1,021,160 B; v1.7:
/// 1,032,184 B). **W5-A (regenerated from the release `.so`):** the
/// kernel's shared sort brought the release `.so` to **869,536 B**
/// [measured, `scripts/build-frontier.sh`, max_len 1,089,536]; the value is
/// that length rounded up to 16 KiB (≈ 15 KB of growth headroom). The svm
/// guard `g01_loaded_limit_table_covers_the_release_so` fails when the
/// release `.so` outgrows it.
pub const PLACEHOLDER_SO_LEN: u32 = 884_736;
pub const PLACEHOLDER_PROGRAMDATA_LEN: u32 = max_len_for(PLACEHOLDER_SO_LEN);

/// `--max-len = round_up(1.25 × so_len, 4,096)` (I-45).
pub const fn max_len_for(so_len: u32) -> u32 {
    let x = (so_len as u64 * 5).div_ceil(4);
    (x.div_ceil(4_096) * 4_096) as u32
}

/// Most `[target w] [recipient w]` pairs in one CloseSeason float part
/// (v1.8 wave-5 review, G1 at the maximal account list, §13.1): parts 8
/// (RingSeeds) and 10 (DefenceClaims) take at most 10 — part 10 with ten
/// distinct beneficiaries is 1,177 B and an eleventh pair passes 1,232 B,
/// and at 10 pairs every float part stays within CloseSeason's measured
/// maximum (part 10, 35,169 CU); at 24 pairs parts 9 and 10 passed the
/// 60k gate. Part 9 (AnchorArchives, 6,144 B each) takes at most
/// [`CLOSE_ARCHIVE_PAIRS_MAX`], the most whose loaded data fits
/// `L(CloseSeason)` at any programdata length (the worst set's 48
/// JoinShards and 16 BeaconLogs load 18,432 B beyond the fixed accounts; an
/// archive pair loads 6,272 B).
pub const CLOSE_FLOAT_PAIRS_MAX: usize = 10;
/// Most AnchorArchive pairs in one CloseSeason part 9 (see
/// [`CLOSE_FLOAT_PAIRS_MAX`]).
pub const CLOSE_ARCHIVE_PAIRS_MAX: usize = 2;

/// Most pairs CloseSeason float part `part` accepts (0 for parts 0–7,
/// which carry no pairs).
pub const fn close_float_pairs_max(part: u8) -> usize {
    match part {
        8 | 10 => CLOSE_FLOAT_PAIRS_MAX,
        9 => CLOSE_ARCHIVE_PAIRS_MAX,
        _ => 0,
    }
}

/// One row of the budgets table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub ix: Ix,
    /// Gate ceiling (0 = recorded, not gated: ResolveClash).
    pub cu_budget: u32,
    /// Extra CU per unit of work (SkipQuiet: per recomputed bell).
    pub cu_per_unit: u32,
    /// CU limit to request (W5-A: the G1 maximum + 5 %, [`MEASURED`]).
    pub cu_limit: u32,
    /// Tx byte ceiling as §5.5 lists it. Many rows are below the smallest
    /// real transaction (64-B signatures, blockhash, the three ComputeBudget
    /// instructions); [`tx_ceiling`] is the ceiling to gate on.
    pub tx_contract: u32,
}

macro_rules! budgets {
    ($( $ix:ident: $cu:expr, $per:expr, $tx:expr, $measured:expr; )*) => {
        /// §5.5 budget table.
        pub const TABLE: &[Budget] = &[ $( Budget {
            ix: Ix::$ix,
            cu_budget: $cu,
            cu_per_unit: $per,
            cu_limit: limit_of($cu, $per, $measured),
            tx_contract: $tx,
        }, )* ];
        /// The G1 maxima the CU limits come from (W5-A), per kind in table
        /// order: `(kind, measured max CU)`; 0 = no fixed measure (SkipQuiet
        /// scales with its bells, ResolveClash is ungated and test-only).
        pub const MEASURED: &[(Ix, u32)] = &[ $( (Ix::$ix, $measured), )* ];
    };
}

/// The CU limit to request (§5.5, I-50): the G1 measured maximum + 5 %,
/// rounded up to 500 (W5-A, from the release and test-beacon `.so` over
/// the svm suite's worst fills, `PSF_CU_LOG`; regenerate with
/// `permutation-frontier/svm-tests/cu-table.py --write`); SkipQuiet: the
/// budget at 24 recomputed bells; ResolveClash: the ladder maximum; capped
/// at 1.4M. **v1.8 §10.2 (wave-5 review, DECISIONS N12):** the first
/// attempt keeps its 5 % headroom even where that passes the gate, by at
/// most 5 % of the gate (rounded to 500): the gate bounds what an
/// instruction *uses* (G1: the measured maximum never above it), the limit
/// is what a client *requests*.
const fn limit_of(cu: u32, per: u32, measured: u32) -> u32 {
    let l = if cu == 0 {
        CU_LADDER_MAX
    } else if measured == 0 {
        cu + 24 * per
    } else {
        ((measured as u64 * 105).div_ceil(100 * 500) * 500) as u32
    };
    if l > CU_LADDER_MAX {
        CU_LADDER_MAX
    } else {
        l
    }
}

budgets! {
    AnnounceSeason: 25_000, 0, 480, 18_608;
    CreateSeason: 70_000, 0, 1_100, 42_198;
    InitBeaconLogs: 80_000, 0, 900, 74_690;
    InitShards: 45_000, 0, 600, 39_314;
    ConsumeGenesisSeed: 345_000, 0, 760, 329_426;
    EndSeason: 10_000, 0, 300, 3_298;
    CloseSeason: 60_000, 0, 1_232, 37_456;
    AbortSeason: 20_000, 0, 300, 4_567;
    SetWindowSchedule: 5_000, 0, 200, 3_610;
    PostAnchor: 345_000, 0, 800, 339_142;
    PostAnchorMulti: 400_000, 0, 1_232, 380_130;
    PostSeed: 345_000, 0, 800, 339_178;
    PostBeacon: 340_000, 0, 760, 332_479;
    ArchiveAnchors: 60_000, 0, 1_232, 45_441;
    CloseSeedCache: 6_000, 0, 300, 5_642;
    OpenRing: 30_000, 0, 600, 15_210;
    ConsumeRingSeed: 345_000, 0, 760, 332_701;
    OpenProvince: 220_000, 0, 400, 148_459;
    FoldOccupancy: 30_000, 0, 1_200, 28_737;
    CloseProvince: 10_000, 0, 300, 5_502;
    Join: 25_000, 0, 700, 12_495;
    SetSession: 6_000, 0, 300, 5_259;
    SetVigil: 6_000, 0, 250, 5_204;
    FileTicket: 17_000, 0, 560, 16_096;
    SettleTicket: 40_000, 0, 900, 22_383;
    ReleaseDormant: 25_000, 0, 480, 11_864;
    CloseHolding: 15_000, 0, 330, 6_750;
    CloseCitizen: 10_000, 0, 300, 6_191;
    Harvest: 17_500, 0, 320, 16_409;
    Build: 22_000, 0, 360, 19_145;
    Train: 17_500, 0, 330, 16_641;
    Muster: 25_000, 0, 380, 20_669;
    Dissolve: 25_000, 0, 380, 18_602;
    Garrison: 25_000, 0, 380, 18_052;
    Explore: 20_000, 0, 380, 18_865;
    SettleExplore: 15_000, 0, 400, 8_281;
    DisbandStranded: 12_000, 0, 300, 5_439;
    Depart: 24_500, 0, 800, 23_167;
    Reveal: 26_000, 0, 1_100, 25_155;
    SettleDeparture: 48_000, 0, 400, 43_083;
    SettleTransit: 85_000, 0, 1_100, 63_281;
    SweepPoolOwed: 8_000, 0, 300, 5_066;
    GatherClash: 49_000, 0, 1_232, 46_551;
    ResolveFromInputs: 290_000, 0, 460, 271_673;
    ResolveClash: 0, 0, 1_232, 0;
    SkipQuiet: 90_000, 30_000, 1_232, 0;
    CloseClashInputs: 8_000, 0, 300, 7_187;
    CloseArrivalDay: 8_000, 0, 300, 5_637;
    CloseArrivalSlot: 8_000, 0, 300, 6_155;
    ClaimDefence: 25_500, 0, 1_000, 24_111;
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
        assert_eq!(cu_gate(Ix::SkipQuiet, 2), 150_000);
        assert_eq!(budget(Ix::Reveal).cu_budget, 26_000);
        // W6-B: Phase B (I-14, `CLASH_VERSION` 3) moves the RFI gate
        // 340,000 → 290,000 (§5.5); G1 maximum 271,673 on the Phase B .so.
        assert_eq!(budget(Ix::ResolveFromInputs).cu_budget, 290_000);
        // W5-A: limits are the G1 maxima + 5 %, rounded up to 500.
        assert_eq!(budget(Ix::Reveal).cu_limit, 26_500);
        assert_eq!(budget(Ix::ResolveFromInputs).cu_limit, 285_500);
        assert_eq!(budget(Ix::SettleDeparture).cu_limit, 45_500);
        assert_eq!(budget(Ix::SkipQuiet).cu_limit, 90_000 + 24 * 30_000);
        assert_eq!(budget(Ix::ResolveClash).cu_limit, CU_LADDER_MAX);
        for (ix, m) in MEASURED {
            let b = budget(*ix);
            assert!(
                b.cu_limit >= *m,
                "{}: the limit covers the maximum",
                ix.name()
            );
            if *m > 0 && b.cu_budget > 0 {
                assert!(
                    *m <= b.cu_budget,
                    "{}: the G1 maximum is within its gate",
                    ix.name()
                );
                assert!(
                    b.cu_limit <= *m + *m / 20 + 501,
                    "{}: +5 %, rounded to 500",
                    ix.name()
                );
                // v1.8 §10.2: never more than 5 % of the gate above it.
                let over = (b.cu_budget as u64 * 105).div_ceil(100 * 500) * 500;
                assert!(
                    b.cu_limit as u64 <= over,
                    "{}: limit {} > the gate + 5 % ({over})",
                    ix.name(),
                    b.cu_limit
                );
            }
        }
    }

    /// v1.8 wave-5 review: the CloseSeason float caps. At the cap every
    /// part's pairs load no more than the worst set's repeat groups (48
    /// JoinShards, 16 BeaconLogs) that `L(CloseSeason)` is computed from,
    /// and one more archive pair would; part 10 at the cap (distinct
    /// beneficiaries) fits a legacy transaction and one more pair does not.
    #[test]
    fn close_float_caps() {
        use crate::layout::beacon::{anchor_archive as AA, defence_claim as DC};
        use crate::layout::world::{beacon_log as BL, join_shard as JS, ring_seed as RS};
        let o = ACCOUNT_OVERHEAD as usize;
        let groups = 48 * (JS::SIZE + o) + 16 * (BL::SIZE + o);
        let pair = |size: usize| size + o + o; // target + a distinct recipient wallet
        assert!(CLOSE_ARCHIVE_PAIRS_MAX * pair(AA::SIZE) <= groups);
        assert!((CLOSE_ARCHIVE_PAIRS_MAX + 1) * pair(AA::SIZE) > groups);
        assert!(CLOSE_FLOAT_PAIRS_MAX * pair(RS::SIZE) <= groups);
        assert!(CLOSE_FLOAT_PAIRS_MAX * pair(DC::SIZE) <= groups);
        let data = ix::CloseSeason::LEN as u32;
        let tx = |pairs: u32| tx_size_estimate(1, 10 + 2 * pairs, data);
        assert!(tx(CLOSE_FLOAT_PAIRS_MAX as u32) <= TX_MAX);
        assert!(tx(CLOSE_FLOAT_PAIRS_MAX as u32 + 1) > TX_MAX);
        assert_eq!(close_float_pairs_max(7), 0);
        assert_eq!(close_float_pairs_max(11), 0);
    }

    #[test]
    fn placeholder_max_len() {
        // round_up(1.25 × 540,608 = 675,760, 4,096) = 675,840 (waves 1–3)
        assert_eq!(max_len_for(540_608), 675_840);
        // round_up(1.25 × 1,048,576 = 1,310,720, 4,096) (integ-W4)
        assert_eq!(max_len_for(1_048_576), 1_310_720);
        // W5-A: the release `.so` [measured] fits under the table's length
        assert_eq!(PLACEHOLDER_PROGRAMDATA_LEN, 1_105_920);
        assert!(max_len_for(869_536) <= PLACEHOLDER_PROGRAMDATA_LEN);
        assert_eq!(max_len_for(4_096), 8_192);
    }

    #[test]
    fn loaded_limits_are_32k_multiples_and_cover_the_need() {
        for i in Ix::ALL {
            let l = loaded_limit(*i);
            assert_eq!(l % 32_768, 0);
            assert!(l >= LOADED_LIMIT_WORKING_DEFAULT);
            // integ-W4: the placeholder program no longer fits 1 MiB (the
            // wave-4 `.so`); every worst set fits its own `L(kind)`
            assert!(
                loaded_need(*i, PLACEHOLDER_PROGRAMDATA_LEN) <= l as u64,
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
