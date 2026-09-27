//! Fee and priority formulas (M1 contract §10.1, I-08, I-45), shared with
//! the `fees` kernel and `fees.mjs` through vectors.
//!
//! ```text
//! cost      = cu_limit + 720·n_sig + 300·n_write_locks + 8·ceil(L / 32,768)
//! L(kind)   = round_up(programdata_len + 45 + Σ_accounts (data_len + 64), 32,768)
//! priority  = (priority_fee + 2,500) / cost
//! fee(p)    = max(0, p·cost − 2,500);  cu_price_µl = ceil(fee(p)·10⁶ / cu_limit)
//! p_tip     = (tip − 2,500) / cost_reveal
//! tip_min   = ceil(p_min × (reveal_cu_limit + 1,320 + 8·ceil(L_reveal / 32,768))) + 2,500
//! ```
//!
//! Priorities are carried in milli-units (lamports per 1,000 cost units) as
//! integers, as `Season.min_reveal_priority_milli` and the keeper caps are.
//!
//! **Integration note.** `permutation_rules::frontier::fees` (W1-C) is the
//! kernel home of these formulas; this module is its off-chain twin with the
//! same integer rounding, and W2-F points it at the kernel once merged. The
//! pinned figures (14,441 and 10,111 lamports) are tested below.

pub const COST_PER_SIG: u64 = 720;
pub const COST_PER_WRITE: u64 = 300;
pub const COST_PER_32K: u64 = 8;
pub const PAGE: u32 = 32_768;
/// The constant term of the priority formula.
pub const PRIORITY_BASE: u64 = 2_500;
/// Default `L` until the release `.so` is measured (I-45).
pub const DEFAULT_LOADED_LIMIT: u32 = 1_048_576;
/// Runtime default when a transaction sets no loaded-data limit (64 MiB).
pub const RUNTIME_MAX_LOADED: u32 = 64 * 1024 * 1024;
/// ProgramData metadata of LoaderV3 (4 + 8 + 1 + 32).
pub const PROGRAMDATA_META: u32 = 45;
/// SIMD-0186 per-account base.
pub const ACCOUNT_BASE: u32 = 64;
/// Lamports per signature (base fee).
pub const LAMPORTS_PER_SIGNATURE: u64 = 5_000;

/// `ceil(x / 32,768)`.
pub const fn pages(loaded: u32) -> u64 {
    (loaded as u64).div_ceil(PAGE as u64)
}

/// `round_up(x, 32,768)`.
pub const fn round_up_page(x: u64) -> u64 {
    x.div_ceil(PAGE as u64) * PAGE as u64
}

/// Cost units of a transaction.
pub const fn cost(limit: u32, sigs: u8, writes: u8, loaded: u32) -> u64 {
    limit as u64
        + COST_PER_SIG * sigs as u64
        + COST_PER_WRITE * writes as u64
        + COST_PER_32K * pages(loaded)
}

/// `(fee + 2,500) × 1,000 / cost`, rounded down.
pub const fn priority_milli(fee: u64, cost: u64) -> u64 {
    if cost == 0 {
        return 0;
    }
    (fee + PRIORITY_BASE) * 1_000 / cost
}

/// The smallest priority fee (lamports) that reaches `p_milli` at `cost`.
pub const fn fee_for(p_milli: u64, cost: u64) -> u64 {
    let need = (p_milli * cost).div_ceil(1_000);
    need.saturating_sub(PRIORITY_BASE)
}

/// `ceil(fee × 10⁶ / cu_limit)` µlamports per CU.
pub const fn cu_price_micro(fee: u64, cu_limit: u32) -> u64 {
    if cu_limit == 0 {
        return 0;
    }
    (fee as u128 * 1_000_000).div_ceil(cu_limit as u128) as u64
}

/// The priority fee the runtime charges: `ceil(price × limit / 10⁶)`.
pub const fn priority_fee(cu_price_micro: u64, cu_limit: u32) -> u64 {
    (cu_price_micro as u128 * cu_limit as u128).div_ceil(1_000_000) as u64
}

/// `p_tip = (tip − 2,500) / cost_reveal` in milli, rounded down.
pub const fn p_tip_milli(tip: u64, cost_reveal: u64) -> u64 {
    if cost_reveal == 0 {
        return 0;
    }
    tip.saturating_sub(PRIORITY_BASE) * 1_000 / cost_reveal
}

/// `L = round_up(programdata_len + 45 + account_bytes + 64 × n, 32 KiB)`.
/// `account_bytes` = Σ data lengths of every other loaded account (the
/// program account included); `n` counts every loaded account including the
/// ProgramData (I-45; the SIMD-0186 accounting `localnet` enforces).
pub const fn loaded_limit(programdata_len: u32, account_bytes: u32, n_accounts: u8) -> u32 {
    round_up_page(
        programdata_len as u64
            + PROGRAMDATA_META as u64
            + account_bytes as u64
            + ACCOUNT_BASE as u64 * n_accounts as u64,
    ) as u32
}

/// `tip_min = ceil(p × (limit + 1,320 + 8·ceil(L/32,768))) + 2,500`.
pub const fn min_tip_lamports(p_milli: u32, limit: u32, loaded: u32) -> u64 {
    let c = limit as u64 + 1_320 + COST_PER_32K * pages(loaded);
    (p_milli as u64 * c).div_ceil(1_000) + PRIORITY_BASE
}

/// `--max-len` to deploy with: `round_up(1.25 × so_len, 4,096)`.
pub const fn deploy_max_len(so_len: u64) -> u64 {
    (so_len * 5).div_ceil(4).div_ceil(4_096) * 4_096
}

/// Landing evidence of a Reveal (§5.12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Evidence {
    /// µlamports per CU.
    pub ev_price: u64,
    pub ev_limit: u32,
    pub ev_loaded: u32,
    /// Whether the Reveal also created the ArrivalDay (one more write lock).
    pub created_day: bool,
}

/// Season parameters of the defence refund.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefenceParams {
    pub defence_cap_milli: u32,
    pub tip_min: u64,
}

/// `min(ev_price × ev_limit / 10⁶, defence_cap × cost − 2,500) − (tip_min − 2,500)`
/// if positive, with `cost = ev_limit + 720 + 300 × (2 + created_day) + 8 × ceil(ev_loaded / 32,768)`.
pub const fn defence_refund(ev: &Evidence, s: &DefenceParams) -> u64 {
    let writes = 2 + ev.created_day as u64;
    let c = ev.ev_limit as u64
        + COST_PER_SIG
        + COST_PER_WRITE * writes
        + COST_PER_32K * pages(ev.ev_loaded);
    let paid = priority_fee(ev.ev_price, ev.ev_limit);
    let cap = (s.defence_cap_milli as u64 * c / 1_000).saturating_sub(PRIORITY_BASE);
    let spent = if paid < cap { paid } else { cap };
    spent.saturating_sub(s.tip_min.saturating_sub(PRIORITY_BASE))
}

/// A bid: the compute-unit price that reaches `p_milli` for a transaction of
/// `cost` units at `cu_limit`.
pub const fn cu_price_for(p_milli: u64, cost: u64, cu_limit: u32) -> u64 {
    cu_price_micro(fee_for(p_milli, cost), cu_limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_tip_minimums() {
        // §10.1: 14,441 at 26k and 1 MiB; 10,111 at 16k and 1 MiB.
        assert_eq!(min_tip_lamports(433, 26_000, DEFAULT_LOADED_LIMIT), 14_441);
        assert_eq!(min_tip_lamports(433, 16_000, DEFAULT_LOADED_LIMIT), 10_111);
        // v1.0's 64-KiB figure (integer ceil gives 10,007 at 16k, §10.1 note).
        assert_eq!(min_tip_lamports(433, 16_000, 65_536), 10_007);
    }

    #[test]
    fn priority_round_trip() {
        let c = cost(26_000, 1, 2, DEFAULT_LOADED_LIMIT);
        assert_eq!(c, 27_576);
        for p in [100u64, 433, 500, 2_000] {
            let fee = fee_for(p, c);
            assert!(priority_milli(fee, c) >= p, "p {p}");
            let price = cu_price_micro(fee, 26_000);
            assert!(priority_fee(price, 26_000) >= fee);
        }
        assert_eq!(p_tip_milli(14_441, c), 433);
    }

    #[test]
    fn loaded_limit_rounds_to_pages() {
        // A 480,512-B SP-V2 program deployed at 1.25×: max_len 602,112.
        assert_eq!(deploy_max_len(480_512), 602_112);
        let l = loaded_limit(
            602_112,
            36 + 2_048 + 160 * 4 + 1_280 + 4_096 + 96 + 144 * 2 + 12_192,
            14,
        );
        assert_eq!(l % PAGE, 0);
        assert!(l as u64 >= 602_112 + 45 + 64 * 14);
        assert!(l <= DEFAULT_LOADED_LIMIT);
    }

    #[test]
    fn defence_refund_is_bounded() {
        let s = DefenceParams {
            defence_cap_milli: 2_000,
            tip_min: 14_441,
        };
        let cheap = Evidence {
            ev_price: 0,
            ev_limit: 26_000,
            ev_loaded: DEFAULT_LOADED_LIMIT,
            created_day: false,
        };
        assert_eq!(defence_refund(&cheap, &s), 0);
        let hot = Evidence {
            ev_price: 10_000_000,
            ev_limit: 26_000,
            ev_loaded: DEFAULT_LOADED_LIMIT,
            created_day: true,
        };
        let c = 26_000 + 720 + 900 + 256;
        assert_eq!(defence_refund(&hot, &s), 2 * c - 2_500 - (14_441 - 2_500));
    }
}
