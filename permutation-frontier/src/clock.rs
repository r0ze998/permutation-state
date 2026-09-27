//! The bell model (M1 contract §5.1) over a Season's stored fields.
//!
//! Every rule time comes from the Clock sysvar's `unix_timestamp` (the
//! caller passes it); the arithmetic is the kernel's `frontier::beacon`
//! (CL-19/20: "the first quicknet round scheduled at or after x"
//! everywhere). This module only binds the kernel functions to the Season
//! fields that parameterise them, so no handler re-derives a round.

use permutation_rules::frontier::beacon::{self, BeaconClock, WindowSchedule};

use crate::layout::{season as S, Ro};
use crate::R;

/// The Season fields of the bell and beacon model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeasonClock {
    pub genesis_ts: i64,
    pub drand_genesis: i64,
    pub drand_period: u32,
    pub reveal_window: u32,
    pub window_next: u32,
    pub window_from_bell: u32,
    pub seed_margin: u32,
}

impl SeasonClock {
    /// Reads the clock fields of a Season account (length checked by the
    /// caller's presence check; a short account is `BadAccount`).
    pub fn read(season: &[u8]) -> R<SeasonClock> {
        let d = Ro(season);
        Ok(SeasonClock {
            genesis_ts: d.i64(S::GENESIS_TS)?,
            drand_genesis: d.i64(S::DRAND_GENESIS)?,
            drand_period: d.u32(S::DRAND_PERIOD)?,
            reveal_window: d.u32(S::REVEAL_WINDOW)?,
            window_next: d.u32(S::WINDOW_NEXT)?,
            window_from_bell: d.u32(S::WINDOW_FROM_BELL)?,
            seed_margin: d.u32(S::SEED_MARGIN)?,
        })
    }

    /// The drand network clock.
    pub fn beacon(&self) -> BeaconClock {
        BeaconClock {
            genesis: self.drand_genesis,
            period: self.drand_period as i64,
        }
    }

    /// The reveal-window schedule.
    pub fn schedule(&self) -> WindowSchedule {
        WindowSchedule {
            reveal_window: self.reveal_window,
            window_next: self.window_next,
            window_from_bell: self.window_from_bell,
        }
    }

    /// `b(t)`; `None` before genesis.
    pub fn bell_at(&self, t: i64) -> Option<u32> {
        beacon::bell_at(self.genesis_ts, t)
    }

    /// `T(b) = first_round_from(bell_end(b))`: the round a march arriving
    /// at `b` is sealed to and THE anchor of `b` carries.
    pub fn tlock_round(&self, b: u32) -> u64 {
        beacon::tlock_round(&self.beacon(), self.genesis_ts, b)
    }

    /// `W(b)`.
    pub fn window(&self, b: u32) -> u32 {
        beacon::window(&self.schedule(), b)
    }

    /// `close(b) = A + W(b)` for THE anchor of `b` landed at `a`.
    pub fn reveal_close(&self, b: u32, a: i64) -> i64 {
        beacon::reveal_close(a, self.window(b))
    }

    /// `S(b, r) = first_round_from(A + W(b) + Δ)`.
    pub fn seed_round(&self, b: u32, a: i64) -> u64 {
        beacon::seed_round(&self.beacon(), self.reveal_close(b, a), self.seed_margin)
    }

    /// `round_time(r)`.
    pub fn round_time(&self, r: u64) -> i64 {
        beacon::round_time(self.drand_genesis, self.drand_period, r)
    }
}

/// `genesis_round = first_round_from(t_create_min + 600 + Δ)` (CreateSeason).
pub fn genesis_round(drand_genesis: i64, drand_period: u32, t_create_min: i64, margin: u32) -> u64 {
    beacon::genesis_seed_round(
        &BeaconClock {
            genesis: drand_genesis,
            period: drand_period as i64,
        },
        t_create_min,
        margin,
    )
}

/// `genesis_ts = round_time(genesis_round) + 600` (CreateSeason).
pub fn genesis_ts(drand_genesis: i64, drand_period: u32, genesis_round: u64) -> i64 {
    beacon::genesis_ts(
        &BeaconClock {
            genesis: drand_genesis,
            period: drand_period as i64,
        },
        genesis_round,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock() -> SeasonClock {
        SeasonClock {
            genesis_ts: 1_790_000_123,
            drand_genesis: 1_692_803_367,
            drand_period: 3,
            reveal_window: 600,
            window_next: 1_200,
            window_from_bell: 500,
            seed_margin: 60,
        }
    }

    /// The program's rounds are the kernel's (CL-20, program side): T(b),
    /// S(A) and the windows over bells and every drand phase.
    #[test]
    fn rounds_are_the_kernels_over_every_phase() {
        for off in 0..3 {
            let mut c = clock();
            c.genesis_ts += off;
            let bc = c.beacon();
            for b in [0u32, 1, 143, 144, 499, 500, 1_007, 4_031] {
                let t = c.tlock_round(b);
                assert_eq!(t, beacon::tlock_round(&bc, c.genesis_ts, b));
                // T(b) is scheduled at or after the end of the bell, and it
                // is the first such round.
                let end = beacon::bell_end(c.genesis_ts, b);
                assert!(c.round_time(t) >= end);
                assert!(c.round_time(t - 1) < end);
                let w = if b >= 500 { 1_200 } else { 600 };
                assert_eq!(c.window(b), w);
                for a_off in [0i64, 1, 2, 59, 600] {
                    let a = end + a_off;
                    let s = c.seed_round(b, a);
                    assert!(c.round_time(s) >= a + w as i64 + 60);
                    assert!(c.round_time(s - 1) < a + w as i64 + 60);
                }
            }
        }
    }

    #[test]
    fn genesis_round_and_ts_follow_the_announcement() {
        let t = 1_790_000_000;
        let r = genesis_round(1_692_803_367, 3, t, 60);
        let rt = beacon::round_time(1_692_803_367, 3, r);
        assert!(rt >= t + 660 && rt - 3 < t + 660);
        assert_eq!(genesis_ts(1_692_803_367, 3, r), rt + 600);
    }

    #[test]
    fn season_fields_round_trip() {
        let c = clock();
        let mut d = [0u8; S::SIZE];
        let mut w = crate::layout::Rw(&mut d);
        w.set_i64(S::GENESIS_TS, c.genesis_ts).unwrap();
        w.set_i64(S::DRAND_GENESIS, c.drand_genesis).unwrap();
        w.set_u32(S::DRAND_PERIOD, c.drand_period).unwrap();
        w.set_u32(S::REVEAL_WINDOW, c.reveal_window).unwrap();
        w.set_u32(S::WINDOW_NEXT, c.window_next).unwrap();
        w.set_u32(S::WINDOW_FROM_BELL, c.window_from_bell).unwrap();
        w.set_u32(S::SEED_MARGIN, c.seed_margin).unwrap();
        assert_eq!(SeasonClock::read(&d), Ok(c));
        assert!(SeasonClock::read(&d[..100]).is_err());
    }
}
