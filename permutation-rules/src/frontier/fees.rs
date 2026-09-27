//! Fee, priority and loaded-data formulas (contract §10.1; closeout
//! CL-22; conflicts I-08, I-45).
//!
//! ```text
//! cost      = cu_limit + 720·n_sig + 300·n_write_locks + 8·ceil(L / 32,768)
//! L(kind)   = round_up(programdata_len + 45 + Σ (data_len + 64), 32,768)
//! priority  = (priority_fee + 2,500) / cost          (lamports per cost unit)
//! fee(p)    = max(0, p·cost − 2,500);  cu_price_µl = ceil(fee·10⁶ / cu_limit)
//! tip_min   = ceil(p_min × cost(reveal_cu_limit, 1 sig, 2 writes, L_reveal)) + 2,500
//! ```
//!
//! Priorities are carried in **milli** (lamports per 1,000 cost units):
//! 433 = 0.433. Every function is integer-only and saturating, so no input
//! panics. `fclient::fees` and the JS `fees.mjs` share vectors with these
//! functions (`frontier-abi` writes them).

/// Kernel version of this module (part of the ruleset hash).
pub const FEES_VERSION: u16 = 1;

/// Cost units per signature.
pub const SIG_COST: u64 = 720;
/// Cost units per write lock.
pub const WRITE_LOCK_COST: u64 = 300;
/// Cost units per started 32 KiB of loaded account data.
pub const LOADED_COST_PER_UNIT: u64 = 8;
/// The loaded-data accounting unit (32 KiB).
pub const LOADED_UNIT: u32 = 32_768;
/// Per-account overhead SIMD-0186 adds to each loaded account's data.
pub const LOADED_PER_ACCOUNT: u32 = 64;
/// Overhead of the LoaderV3 programdata counted with it.
pub const LOADED_PROGRAMDATA_EXTRA: u32 = 45;
/// The largest loaded-data limit a transaction may request (64 MiB).
pub const LOADED_MAX: u32 = 64 * 1024 * 1024;
/// Working default `L(reveal)` until the release `.so` is measured (I-45).
pub const LOADED_DEFAULT: u32 = 1024 * 1024;
/// The scheduler's constant: half the 5,000-lamport signature fee.
pub const PRIORITY_BASE_LAMPORTS: u64 = 2_500;
/// A Reveal transaction: one signature (the fee payer), two write locks
/// (the fee payer and the ArrivalSlot).
pub const REVEAL_SIGS: u8 = 1;
pub const REVEAL_WRITES: u8 = 2;
/// Season defaults (contract §5.3).
pub const MIN_REVEAL_PRIORITY_MILLI_DEFAULT: u32 = 433;
pub const REVEAL_CU_LIMIT_DEFAULT: u32 = 26_000;
pub const DEFENCE_CAP_MILLI_DEFAULT: u32 = 2_000;

/// Number of started 32-KiB units in `loaded` bytes.
pub const fn loaded_units(loaded: u32) -> u64 {
    (loaded as u64).div_ceil(LOADED_UNIT as u64)
}

/// `cost = limit + 720·sigs + 300·writes + 8·ceil(loaded / 32,768)`.
pub const fn cost(limit: u32, sigs: u8, writes: u8, loaded: u32) -> u64 {
    limit as u64
        + SIG_COST * sigs as u64
        + WRITE_LOCK_COST * writes as u64
        + LOADED_COST_PER_UNIT * loaded_units(loaded)
}

/// `priority = (fee + 2,500) / cost` in milli (floor); 0 when `cost` is 0.
/// `fee` is the priority fee in lamports (not the signature fee).
pub const fn priority_milli(fee: u64, cost: u64) -> u64 {
    if cost == 0 {
        return 0;
    }
    let num = (fee as u128 + PRIORITY_BASE_LAMPORTS as u128) * 1_000;
    let q = num / cost as u128;
    if q > u64::MAX as u128 {
        u64::MAX
    } else {
        q as u64
    }
}

/// The priority fee that reaches priority `p_milli` at `cost`:
/// `max(0, ceil(p·cost) − 2,500)`.
pub const fn fee_for_priority(p_milli: u64, cost: u64) -> u64 {
    let need = (p_milli as u128 * cost as u128).div_ceil(1_000);
    let fee = need.saturating_sub(PRIORITY_BASE_LAMPORTS as u128);
    if fee > u64::MAX as u128 {
        u64::MAX
    } else {
        fee as u64
    }
}

/// Compute-unit price (µlamports per CU) that pays `fee` at `cu_limit`:
/// `ceil(fee·10⁶ / cu_limit)`; 0 for a zero limit.
pub const fn cu_price_micro(fee: u64, cu_limit: u32) -> u64 {
    if cu_limit == 0 {
        return 0;
    }
    let q = (fee as u128 * 1_000_000).div_ceil(cu_limit as u128);
    if q > u64::MAX as u128 {
        u64::MAX
    } else {
        q as u64
    }
}

/// The priority fee a compute-unit price pays: `price·limit / 10⁶`
/// (floor, as the runtime charges).
pub const fn fee_of_price(price_micro: u64, cu_limit: u32) -> u64 {
    let q = price_micro as u128 * cu_limit as u128 / 1_000_000;
    if q > u64::MAX as u128 {
        u64::MAX
    } else {
        q as u64
    }
}

/// `L(kind) = round_up(programdata_len + 45 + account_bytes + 64·n,
/// 32,768)`, capped at 64 MiB (I-45). `account_bytes` is Σ data_len of the
/// kind's worst account set (programdata excluded), `n` its account count.
pub const fn loaded_limit(programdata_len: u32, account_bytes: u32, n_accounts: u8) -> u32 {
    let sum = programdata_len as u64
        + LOADED_PROGRAMDATA_EXTRA as u64
        + account_bytes as u64
        + LOADED_PER_ACCOUNT as u64 * n_accounts as u64;
    let rounded = sum.div_ceil(LOADED_UNIT as u64) * LOADED_UNIT as u64;
    if rounded > LOADED_MAX as u64 {
        LOADED_MAX
    } else {
        rounded as u32
    }
}

/// The deploy `--max-len`: `round_up(1.25 × so_len, 4,096)`.
pub const fn deploy_max_len(so_len: u32) -> u32 {
    let x = (so_len as u64 * 5).div_ceil(4);
    let r = x.div_ceil(4_096) * 4_096;
    if r > u32::MAX as u64 {
        u32::MAX
    } else {
        r as u32
    }
}

/// The Reveal's cost at a CU limit and loaded-data limit.
pub const fn reveal_cost(limit: u32, loaded: u32) -> u64 {
    cost(limit, REVEAL_SIGS, REVEAL_WRITES, loaded)
}

/// `tip_min = ceil(p × (limit + 1,320 + 8·⌈loaded/32,768⌉)) + 2,500`
/// lamports, `p = p_milli / 1,000` (I-08): the smallest tip whose whole
/// spend on a Reveal reaches priority `p` (`p_tip = (tip − 2,500) / cost`).
pub const fn min_tip_lamports(p_milli: u32, limit: u32, loaded: u32) -> u64 {
    let c = reveal_cost(limit, loaded) as u128;
    let x = (p_milli as u128 * c).div_ceil(1_000) + PRIORITY_BASE_LAMPORTS as u128;
    if x > u64::MAX as u128 {
        u64::MAX
    } else {
        x as u64
    }
}

/// The priority (milli) a tip buys when a keeper spends all of it on the
/// Reveal: `(tip − 2,500) / cost` (floor; 0 below 2,500).
pub const fn tip_priority_milli(tip: u64, limit: u32, loaded: u32) -> u64 {
    let c = reveal_cost(limit, loaded);
    if c == 0 {
        return 0;
    }
    (tip.saturating_sub(PRIORITY_BASE_LAMPORTS) as u128 * 1_000 / c as u128) as u64
}

/// Evidence a Reveal recorded in its ArrivalSlot (§5.3 `ev_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Evidence {
    /// Compute-unit price paid, µlamports per CU.
    pub price_micro: u64,
    /// CU limit requested.
    pub limit: u32,
    /// Loaded-data limit requested.
    pub loaded: u32,
    /// The Reveal also created the ArrivalDay (one more write lock).
    pub created_day: bool,
}

/// Season parameters of a defence refund.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefenceParams {
    /// `Season.defence_cap_milli` (priority cap, 2,000 = 2.0).
    pub defence_cap_milli: u32,
    /// The season's `tip_min` (lamports).
    pub tip_min: u64,
}

/// ClaimDefence's refund for one Reveal (contract §5.12):
/// `min(price·limit / 10⁶, cap·cost − 2,500) − (tip_min − 2,500)` if
/// positive, `cost = limit + 720 + 300·(2 + created_day) + 8·⌈loaded /
/// 32,768⌉`. `cap·cost` is floored.
pub const fn defence_refund(ev: &Evidence, s: &DefenceParams) -> u64 {
    let c = cost(ev.limit, 1, 2 + ev.created_day as u8, ev.loaded);
    let paid = fee_of_price(ev.price_micro, ev.limit);
    let capped = (s.defence_cap_milli as u128 * c as u128 / 1_000)
        .saturating_sub(PRIORITY_BASE_LAMPORTS as u128);
    let capped = if capped > u64::MAX as u128 {
        u64::MAX
    } else {
        capped as u64
    };
    let spent = if paid < capped { paid } else { capped };
    spent.saturating_sub(s.tip_min.saturating_sub(PRIORITY_BASE_LAMPORTS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_values() {
        // §10.1: 14,441 at 26k with L = 1 MiB; 10,111 at 16k.
        assert_eq!(min_tip_lamports(433, 26_000, LOADED_DEFAULT), 14_441);
        assert_eq!(min_tip_lamports(433, 16_000, LOADED_DEFAULT), 10_111);
        // v1.0's 64-KiB figure, integer ceil: 10,007 (not 10,006).
        assert_eq!(min_tip_lamports(433, 16_000, 65_536), 10_007);
        assert_eq!(reveal_cost(26_000, LOADED_DEFAULT), 27_576);
        assert_eq!(loaded_limit(0, 0, 0), 32_768);
        assert_eq!(loaded_limit(32_768 - 45, 0, 0), 32_768);
        assert_eq!(loaded_limit(32_768 - 44, 0, 0), 65_536);
        assert_eq!(loaded_limit(u32::MAX, u32::MAX, 255), LOADED_MAX);
        assert_eq!(deploy_max_len(540_608), 675_840);
    }

    #[test]
    fn zero_inputs_do_not_divide_by_zero() {
        assert_eq!(priority_milli(1, 0), 0);
        assert_eq!(cu_price_micro(1, 0), 0);
        assert_eq!(tip_priority_milli(0, 0, 0), 0);
        assert_eq!(fee_for_priority(u64::MAX, u64::MAX), u64::MAX);
    }
}
