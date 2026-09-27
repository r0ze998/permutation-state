//! `keeper-core` (M1 contract §8.2).
//!
//! **W1 skeleton.** W2-F builds the beacon duties, payer pools in use, the
//! escalation engine, the journal and the loopback API; W3/W4 add land,
//! reveal, gather, resolve and settle duties. What exists now is the bid
//! schedule of the spec (§8.2), which everything else composes with.

use fclient::abi::Class;

/// P_def = 2.0, P_delay = 0.5 (milli-units, §8.2).
pub const P_DEF_MILLI: u64 = 2_000;
pub const P_DELAY_MILLI: u64 = 500;
/// The fixed low bid of class N writes.
pub const P_LOW_MILLI: u64 = 100;

/// The priority (milli) of the version sent `slots` slots after the first:
/// W writes start at `p_start` (= `p_tip` by default) and double every slot
/// up to `min(P_def, season cap)`; D writes double up to P_delay; N writes
/// stay at the low bid. Every level is a new signature from a newly drawn
/// payer (`fclient::payers::Pool::draw_os`).
pub fn bid_milli(class: Class, p_start: u64, slots: u32, season_cap_milli: u64) -> u64 {
    let doubled = |cap: u64| {
        p_start
            .max(1)
            .saturating_mul(1u64 << slots.min(20))
            .min(cap)
    };
    match class {
        Class::W => doubled(P_DEF_MILLI.min(season_cap_milli)),
        Class::D => doubled(P_DELAY_MILLI),
        _ => P_LOW_MILLI,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalation_by_class() {
        assert_eq!(bid_milli(Class::W, 433, 0, 2_000), 433);
        assert_eq!(bid_milli(Class::W, 433, 1, 2_000), 866);
        assert_eq!(bid_milli(Class::W, 433, 3, 2_000), 2_000);
        assert_eq!(bid_milli(Class::W, 433, 9, 1_500), 1_500, "season cap");
        assert_eq!(bid_milli(Class::D, 433, 2, 2_000), 500);
        assert_eq!(bid_milli(Class::N, 433, 9, 2_000), P_LOW_MILLI);
    }
}
