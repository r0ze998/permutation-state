//! The Frontier's clock and beacon rounds (contract §5.1; closeout CL-19,
//! CL-20; conflict I-18).
//!
//! Every rule time is a function of the chain's Clock (`unix_timestamp`),
//! never the wall clock. Integers only: bells are `u32`, seconds `i64`,
//! drand rounds `u64`.
//!
//! **One rounding rule everywhere:** a beacon draw scheduled "at time x" is
//! *the first quicknet round scheduled at or after x*
//! ([`first_round_from`]). DESIGN's older `round_at(x)` rounded down, which
//! could put a seed before the moment it must follow; nothing here rounds
//! down.
//!
//! | Quantity | Definition | fn |
//! |---|---|---|
//! | bell of t | `⌊(t − genesis_ts) / 600⌋`, t ≥ genesis_ts | [`bell_at`] |
//! | bell bounds | `bell_start(b) = genesis_ts + 600 b`, `bell_end(b) = bell_start(b + 1)` | [`bell_start`], [`bell_end`] |
//! | tlock round | `T(b) = first_round_from(bell_end(b))` | [`tlock_round`] |
//! | window | `window_next` from `window_from_bell` on, else `reveal_window` | [`window`] |
//! | reveal close | `A + W` | [`reveal_close`] |
//! | seed round | `S = first_round_from(close + Δ)` | [`seed_round`] |
//! | ring seed round | `first_round_from(t_open + 600 + Δ)` | [`ring_seed_round`] |
//! | genesis round | `first_round_from(t_create_min + 600 + Δ)` | [`genesis_seed_round`] |
//!
//! The roster freeze and the posture close are wall-clock bell boundaries
//! (`bell_start(b)`), never derived from `T(b)`, so the drand phase cannot
//! move them (CL-20).
//!
//! The drand network clock is [`clash::BeaconClock`] (not moved, CL-19);
//! the free functions below take its fields so that callers holding only a
//! Season's stored `drand_genesis`/`drand_period` need no struct.

pub use super::clash::{BeaconClock, QUICKNET};
use super::travel::{BELLS_PER_DAY, BELL_SECS};

/// Kernel version of this module (part of the ruleset hash).
pub const BEACON_VERSION: u16 = 1;

/// Lead of a ring or genesis seed draw over the event that fixes it:
/// `first_round_from(t + SEED_LEAD_SECS + Δ)` (contract §5.1).
pub const SEED_LEAD_SECS: i64 = 600;
/// Smallest seed margin Δ a season may use (M0 review: Δ ≥ 60 s).
pub const SEED_MARGIN_MIN: u32 = 60;
/// Reveal window bounds (SeasonParams, SetWindowSchedule).
pub const WINDOW_MIN: u32 = 600;
pub const WINDOW_MAX: u32 = 1_800;
/// A window change needs this many bells' notice.
pub const WINDOW_NOTICE_BELLS: u32 = 144;
/// `window_from_bell` value meaning "no scheduled change".
pub const NO_WINDOW_CHANGE: u32 = u32::MAX;
/// `genesis_ts = round_time(genesis_round) + GENESIS_LEAD_SECS`.
pub const GENESIS_LEAD_SECS: i64 = 600;

fn clamp_i64(x: i128) -> i64 {
    if x > i64::MAX as i128 {
        i64::MAX
    } else if x < i64::MIN as i128 {
        i64::MIN
    } else {
        x as i64
    }
}

/// Scheduled time of round `r` of a drand network (`r ≥ 1`; round 0 is
/// treated as round 1, as `BeaconClock::round_time` does). A period of 0
/// is treated as 1 (no division by zero; seasons validate `period ≥ 1`).
pub fn round_time(genesis: i64, period: u32, r: u64) -> i64 {
    let p = period.max(1) as i128;
    clamp_i64(genesis as i128 + (r.saturating_sub(1) as i128) * p)
}

/// The first round scheduled at or after `t` (1 if `t` is at or before
/// genesis): the smallest `r ≥ 1` with `round_time(r) ≥ t`.
pub fn first_round_from(genesis: i64, period: u32, t: i64) -> u64 {
    if t <= genesis {
        return 1;
    }
    let p = period.max(1) as i128;
    let d = t as i128 - genesis as i128;
    let k = (d + p - 1) / p; // ceil(d / p) ≥ 1
    let r = k + 1;
    if r > u64::MAX as i128 {
        u64::MAX
    } else {
        r as u64
    }
}

/// Unix time bell `b` starts: `genesis_ts + 600 b`.
pub fn bell_start(genesis_ts: i64, b: u32) -> i64 {
    clamp_i64(genesis_ts as i128 + b as i128 * BELL_SECS as i128)
}

/// Unix time bell `b` ends (= `bell_start(b + 1)`, exclusive).
pub fn bell_end(genesis_ts: i64, b: u32) -> i64 {
    clamp_i64(genesis_ts as i128 + (b as i128 + 1) * BELL_SECS as i128)
}

/// The bell containing `t`, or `None` before genesis or past bell
/// `u32::MAX`. Agrees with `travel::bell_at` wherever it is `Some`.
pub fn bell_at(genesis_ts: i64, t: i64) -> Option<u32> {
    if t < genesis_ts {
        return None;
    }
    let b = (t as i128 - genesis_ts as i128) / BELL_SECS as i128;
    u32::try_from(b).ok()
}

/// Game day of a bell: `b / 144`.
pub const fn day(b: u32) -> u32 {
    b / BELLS_PER_DAY
}

/// First bell of game day `d` (saturating).
pub const fn day_start_bell(d: u32) -> u32 {
    d.saturating_mul(BELLS_PER_DAY)
}

/// `T(b)`: the quicknet round a march arriving at bell `b` is sealed to,
/// the first round scheduled at or after the end of bell `b`.
pub fn tlock_round(c: &BeaconClock, genesis_ts: i64, b: u32) -> u64 {
    first_round_from(c.genesis, period_of(c), bell_end(genesis_ts, b))
}

/// Reveal close of a bell whose THE anchor landed at `a`: `A + W`.
pub fn reveal_close(a: i64, w: u32) -> i64 {
    clamp_i64(a as i128 + w as i128)
}

/// `S(b, r)`: the first round scheduled at or after `close + Δ`.
pub fn seed_round(c: &BeaconClock, close: i64, margin: u32) -> u64 {
    first_round_from(
        c.genesis,
        period_of(c),
        clamp_i64(close as i128 + margin as i128),
    )
}

/// The seed round of a ring opened at `t_open`:
/// `first_round_from(t_open + 600 + Δ)`.
pub fn ring_seed_round(c: &BeaconClock, t_open: i64, margin: u32) -> u64 {
    first_round_from(
        c.genesis,
        period_of(c),
        clamp_i64(t_open as i128 + SEED_LEAD_SECS as i128 + margin as i128),
    )
}

/// The genesis seed round of a season whose earliest creation time
/// (fixed by AnnounceSeason, CL-24) is `t_create_min`:
/// `first_round_from(t_create_min + 600 + Δ)`.
pub fn genesis_seed_round(c: &BeaconClock, t_create_min: i64, margin: u32) -> u64 {
    first_round_from(
        c.genesis,
        period_of(c),
        clamp_i64(t_create_min as i128 + SEED_LEAD_SECS as i128 + margin as i128),
    )
}

/// `genesis_ts = round_time(genesis_round) + 600` (set by CreateSeason).
pub fn genesis_ts(c: &BeaconClock, genesis_round: u64) -> i64 {
    clamp_i64(
        round_time(c.genesis, period_of(c), genesis_round) as i128 + GENESIS_LEAD_SECS as i128,
    )
}

/// A season's reveal-window schedule (Season `reveal_window`,
/// `window_next`, `window_from_bell`; SetWindowSchedule).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowSchedule {
    pub reveal_window: u32,
    pub window_next: u32,
    /// First bell that uses `window_next`; [`NO_WINDOW_CHANGE`] for none.
    pub window_from_bell: u32,
}

impl WindowSchedule {
    /// A schedule with no pending change.
    pub const fn fixed(w: u32) -> WindowSchedule {
        WindowSchedule {
            reveal_window: w,
            window_next: w,
            window_from_bell: NO_WINDOW_CHANGE,
        }
    }
}

/// `W(b)`: `window_next` if a change is scheduled and `b ≥
/// window_from_bell`, else `reveal_window`.
pub fn window(s: &WindowSchedule, b: u32) -> u32 {
    if s.window_from_bell != NO_WINDOW_CHANGE && b >= s.window_from_bell {
        s.window_next
    } else {
        s.reveal_window
    }
}

/// Whether a window value is allowed (600–1,800 s).
pub const fn window_valid(w: u32) -> bool {
    w >= WINDOW_MIN && w <= WINDOW_MAX
}

/// Whether SetWindowSchedule may set `from_bell` at `now_bell`: the
/// change needs ≥ 144 bells' notice.
pub const fn window_change_allowed(now_bell: u32, from_bell: u32) -> bool {
    from_bell as u64 >= now_bell as u64 + WINDOW_NOTICE_BELLS as u64
}

/// Reveal still open with THE anchor present (contract §5.1): before the
/// close by the chain's Clock **and** while no round at or after the seed
/// round has been posted (`latest_round`: the region's BeaconLog). Either
/// condition alone closes the window.
pub fn reveal_open(
    c: &BeaconClock,
    now: i64,
    anchor_ts: i64,
    w: u32,
    margin: u32,
    latest_round: u64,
) -> bool {
    let close = reveal_close(anchor_ts, w);
    now < close && latest_round < seed_round(c, close, margin)
}

fn period_of(c: &BeaconClock) -> u32 {
    if c.period <= 0 {
        1
    } else if c.period > u32::MAX as i64 {
        u32::MAX
    } else {
        c.period as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontier::travel;

    #[test]
    fn free_functions_match_the_clock_methods() {
        for g in [QUICKNET.genesis, QUICKNET.genesis + 1, QUICKNET.genesis + 2] {
            let c = BeaconClock {
                genesis: g,
                period: 3,
            };
            for t in g - 10..g + 1_000 {
                assert_eq!(first_round_from(g, 3, t), c.first_round_from(t));
            }
            for r in 0..500u64 {
                assert_eq!(round_time(g, 3, r), c.round_time(r));
            }
        }
    }

    #[test]
    fn bell_at_agrees_with_travel() {
        let g = 1_790_000_000;
        for t in g..g + 10_000 {
            assert_eq!(bell_at(g, t), Some(travel::bell_at(g, t)));
            let b = bell_at(g, t).unwrap();
            assert!(bell_start(g, b) <= t && t < bell_end(g, b));
            assert_eq!(bell_start(g, b), travel::bell_start(g, b));
        }
        assert_eq!(bell_at(g, g - 1), None);
        assert_eq!(bell_at(g, g + (u32::MAX as i64 + 1) * 600), None);
        assert_eq!(bell_at(g, g + u32::MAX as i64 * 600), Some(u32::MAX));
    }

    #[test]
    fn beacon_seed_round_matches_the_clash_kernel() {
        use crate::frontier::clash;
        for a in 1_790_000_000..1_790_000_000 + 3_000 {
            let close = reveal_close(a, 600);
            assert_eq!(close, clash::reveal_close(a));
            assert_eq!(
                seed_round(&QUICKNET, close, 60),
                clash::seed_round(&QUICKNET, a)
            );
            for latest in [
                0,
                clash::seed_round(&QUICKNET, a) - 1,
                clash::seed_round(&QUICKNET, a),
            ] {
                for now in [a, close - 1, close] {
                    assert_eq!(
                        reveal_open(&QUICKNET, now, a, 600, 60, latest),
                        clash::reveal_open(&QUICKNET, now, a, latest)
                    );
                }
            }
        }
    }

    #[test]
    fn window_schedule() {
        let s = WindowSchedule {
            reveal_window: 600,
            window_next: 900,
            window_from_bell: 200,
        };
        assert_eq!(window(&s, 199), 600);
        assert_eq!(window(&s, 200), 900);
        let f = WindowSchedule::fixed(1_200);
        assert_eq!(window(&f, u32::MAX), 1_200);
        assert!(window_valid(600) && window_valid(1_800));
        assert!(!window_valid(599) && !window_valid(1_801));
        assert!(window_change_allowed(10, 154));
        assert!(!window_change_allowed(10, 153));
        assert!(!window_change_allowed(u32::MAX, u32::MAX));
    }

    #[test]
    fn extremes_do_not_panic() {
        assert_eq!(first_round_from(0, 0, i64::MAX), i64::MAX as u64 + 1);
        assert_eq!(first_round_from(i64::MIN, 1, i64::MAX), u64::MAX);
        assert_eq!(round_time(i64::MAX, u32::MAX, u64::MAX), i64::MAX);
        assert_eq!(bell_end(i64::MAX, u32::MAX), i64::MAX);
        assert_eq!(bell_start(i64::MIN, 0), i64::MIN);
        assert_eq!(reveal_close(i64::MAX, 1_800), i64::MAX);
        let c = BeaconClock {
            genesis: 0,
            period: 0,
        };
        assert_eq!(tlock_round(&c, 0, 0), 601);
    }
}
