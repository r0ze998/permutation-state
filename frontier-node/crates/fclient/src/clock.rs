//! Rule times (M1 contract §5.1) and the `GameClock`.
//!
//! **Every rule time comes from the Clock sysvar, never from the wall
//! clock.** `GameClock` extrapolates the last observed Clock sysvar by the
//! detected scale (game seconds per wall second), so a keeper at 1× on a
//! validator and at 20× on `localnet` runs the same code. The wall clock is
//! only used to measure elapsed time between observations.
//!
//! **Integration note.** The formulas are the kernel's
//! (`permutation_rules::frontier::beacon`, W1-C, CL-19/20); this module is
//! their twin over the existing `clash::BeaconClock` and W2-F points it at
//! the kernel once merged.

use std::collections::VecDeque;
use std::time::Instant;

use crate::abi::BELL_SECS;
use crate::ports::ClockSysvar;

/// The kernel's clock functions (`permutation_rules::frontier::beacon`,
/// W1-C), re-exported (integ-W1 review: the first twin computed
/// `bell_end(u32::MAX)` as `b + 1` unchecked, and `bell_at` truncated past
/// bell `u32::MAX`; the kernel saturates and returns `None`).
/// `clock_vectors_match_the_kernels` runs `clock-vectors-v1.json`.
pub use permutation_rules::frontier::beacon::{
    bell_at, bell_end, bell_start, first_round_from, round_time,
};

/// The drand clock of a season.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drand {
    pub genesis: i64,
    pub period: u32,
}

impl Drand {
    pub const QUICKNET: Drand = Drand {
        genesis: crate::beacon::QUICKNET_GENESIS,
        period: crate::beacon::QUICKNET_PERIOD,
    };
    pub fn round_time(&self, r: u64) -> i64 {
        round_time(self.genesis, self.period, r)
    }
    pub fn first_round_from(&self, t: i64) -> u64 {
        first_round_from(self.genesis, self.period, t)
    }
    /// `T(b) = first_round_from(bell_end(b))`.
    pub fn tlock_round(&self, genesis_ts: i64, b: u32) -> u64 {
        self.first_round_from(bell_end(genesis_ts, b))
    }
    /// `S = first_round_from(close + Δ)`.
    pub fn seed_round(&self, close: i64, margin: u32) -> u64 {
        self.first_round_from(close + margin as i64)
    }
    /// `first_round_from(t_open + 600 + Δ)`.
    pub fn ring_seed_round(&self, t_open: i64, margin: u32) -> u64 {
        self.first_round_from(t_open + 600 + margin as i64)
    }
    /// `first_round_from(t_create_min + 600 + Δ)`.
    pub fn genesis_seed_round(&self, t_create_min: i64, margin: u32) -> u64 {
        self.first_round_from(t_create_min + 600 + margin as i64)
    }
}

/// `close(b, r) = A + W(b)` (saturating, the kernel's).
pub use permutation_rules::frontier::beacon::reveal_close;

/// A season's clock parameters as the off-chain side needs them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeasonClock {
    pub drand: Drand,
    pub genesis_ts: i64,
    pub reveal_window: u32,
    pub window_next: u32,
    pub window_from_bell: u32,
    pub seed_margin: u32,
    pub archive_after: u32,
}

impl SeasonClock {
    pub fn from_season(s: &crate::decode::Season) -> SeasonClock {
        SeasonClock {
            drand: Drand {
                genesis: s.drand_genesis,
                period: s.drand_period,
            },
            genesis_ts: s.genesis_ts,
            reveal_window: s.reveal_window,
            window_next: s.window_next,
            window_from_bell: s.window_from_bell,
            seed_margin: s.seed_margin,
            archive_after: s.archive_after,
        }
    }
    /// `W(b)`.
    pub fn window(&self, b: u32) -> u32 {
        if self.window_from_bell != u32::MAX && b >= self.window_from_bell {
            self.window_next
        } else {
            self.reveal_window
        }
    }
    pub fn bell_at(&self, t: i64) -> Option<u32> {
        bell_at(self.genesis_ts, t)
    }
    pub fn tlock_round(&self, b: u32) -> u64 {
        self.drand.tlock_round(self.genesis_ts, b)
    }
    /// `S(b, r)` for an anchor landed at `a`.
    pub fn seed_round(&self, b: u32, a: i64) -> u64 {
        self.drand
            .seed_round(reveal_close(a, self.window(b)), self.seed_margin)
    }
    pub fn reveal_close(&self, b: u32, a: i64) -> i64 {
        reveal_close(a, self.window(b))
    }
    /// Reveal open by time and the BeaconLog guard (§5.1): `now < close ∧ latest_round < S(A)`.
    pub fn reveal_open(&self, b: u32, a: i64, now: i64, latest_round: u64) -> bool {
        now < self.reveal_close(b, a) && latest_round < self.seed_round(b, a)
    }
    /// SettleTransit allowed: `now ≥ close(arrive) + 600`.
    pub fn settle_after(&self, arrive: u32, a: i64) -> i64 {
        self.reveal_close(arrive, a) + BELL_SECS
    }
    /// Archive allowed: `now ≥ A + archive_after`.
    pub fn archive_after(&self, a: i64) -> i64 {
        a + self.archive_after as i64
    }
}

/// One observation of the Clock sysvar with the wall instant it was read at.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub clock: ClockSysvar,
    pub at: Instant,
}

/// Extrapolates the chain's Clock between observations and detects the
/// time scale (1× on a validator, 20× in Mode A, 2,000× in the pre-season).
#[derive(Clone, Debug)]
pub struct GameClock {
    samples: VecDeque<Sample>,
    window: usize,
    /// Used until two samples with distinct instants exist.
    default_scale: f64,
}

impl Default for GameClock {
    fn default() -> Self {
        GameClock::new(1.0)
    }
}

impl GameClock {
    pub fn new(default_scale: f64) -> GameClock {
        GameClock {
            samples: VecDeque::new(),
            window: 32,
            default_scale,
        }
    }

    /// Records a Clock read. Samples that go backwards in slot are dropped
    /// (a lagging RPC node), so the clock is monotone.
    pub fn observe(&mut self, clock: ClockSysvar, at: Instant) {
        if let Some(last) = self.samples.back() {
            if clock.slot < last.clock.slot || clock.unix_timestamp < last.clock.unix_timestamp {
                return;
            }
            // A scale change (frontier_setScale) restarts the estimate.
            if let Some(s) = self.scale_between(last, &Sample { clock, at }) {
                if (s - self.scale()).abs() > 0.25 * self.scale().max(1.0)
                    && self.samples.len() >= 2
                {
                    let keep = *last;
                    self.samples.clear();
                    self.samples.push_back(keep);
                }
            }
        }
        self.samples.push_back(Sample { clock, at });
        while self.samples.len() > self.window {
            self.samples.pop_front();
        }
    }

    fn scale_between(&self, a: &Sample, b: &Sample) -> Option<f64> {
        let dw = b.at.checked_duration_since(a.at)?.as_secs_f64();
        let dg = (b.clock.unix_timestamp - a.clock.unix_timestamp) as f64;
        (dw >= 0.2 && dg >= 0.0).then(|| dg / dw)
    }

    /// Game seconds per wall second, over the sample window.
    pub fn scale(&self) -> f64 {
        match (self.samples.front(), self.samples.back()) {
            (Some(a), Some(b)) if self.samples.len() >= 2 => {
                self.scale_between(a, b).unwrap_or(self.default_scale)
            }
            _ => self.default_scale,
        }
    }

    /// Slots per wall second (2.5 on every cluster and on localnet, I-54).
    pub fn slot_rate(&self) -> f64 {
        match (self.samples.front(), self.samples.back()) {
            (Some(a), Some(b)) if self.samples.len() >= 2 => {
                let dw = b.at.saturating_duration_since(a.at).as_secs_f64();
                if dw > 0.0 {
                    (b.clock.slot - a.clock.slot) as f64 / dw
                } else {
                    2.5
                }
            }
            _ => 2.5,
        }
    }

    /// The Clock's `unix_timestamp` extrapolated to `at` (game seconds).
    pub fn now_at(&self, at: Instant) -> Option<i64> {
        let last = self.samples.back()?;
        let dw = at.saturating_duration_since(last.at).as_secs_f64();
        Some(last.clock.unix_timestamp + (dw * self.scale()).floor() as i64)
    }

    pub fn now(&self) -> Option<i64> {
        self.now_at(Instant::now())
    }

    /// Slot extrapolated to `at`.
    pub fn slot_at(&self, at: Instant) -> Option<u64> {
        let last = self.samples.back()?;
        let dw = at.saturating_duration_since(last.at).as_secs_f64();
        Some(last.clock.slot + (dw * self.slot_rate()).floor() as u64)
    }

    /// Wall seconds until the game clock reaches `t` (0 if past).
    pub fn wall_secs_until(&self, t: i64, at: Instant) -> Option<f64> {
        let now = self.now_at(at)?;
        let s = self.scale();
        Some(if t <= now || s <= 0.0 {
            0.0
        } else {
            (t - now) as f64 / s
        })
    }

    pub fn last(&self) -> Option<ClockSysvar> {
        self.samples.back().map(|s| s.clock)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn rule_times() {
        let d = Drand::QUICKNET;
        let g = 1_800_000_000;
        // T(b) is the first round at or after the end of bell b.
        let t = d.tlock_round(g, 0);
        assert!(d.round_time(t) >= g + 600 && d.round_time(t - 1) < g + 600);
        // S rounds up (never before close + Δ).
        let s = d.seed_round(g + 1_234, 60);
        assert!(d.round_time(s) >= g + 1_294 && d.round_time(s - 1) < g + 1_294);
        assert_eq!(bell_at(g, g - 1), None);
        assert_eq!(bell_at(g, g + 1_199), Some(1));
        assert_eq!(bell_end(g, 1), g + 1_200);
        assert_eq!(first_round_from(100, 3, 99), 1);
        assert_eq!(first_round_from(100, 3, 101), 2);
    }

    #[test]
    fn season_windows() {
        let c = SeasonClock {
            drand: Drand::QUICKNET,
            genesis_ts: 1_800_000_000,
            reveal_window: 600,
            window_next: 900,
            window_from_bell: 200,
            seed_margin: 60,
            archive_after: 172_800,
        };
        assert_eq!(c.window(199), 600);
        assert_eq!(c.window(200), 900);
        let a = 1_800_000_000 + 600 + 2;
        assert!(c.reveal_open(0, a, a + 599, 0));
        assert!(!c.reveal_open(0, a, a + 600, 0));
        assert!(!c.reveal_open(0, a, a + 10, c.seed_round(0, a)));
    }

    #[test]
    fn detects_scale_and_extrapolates() {
        let t0 = Instant::now();
        let mut g = GameClock::new(1.0);
        // 20×: 8 game seconds per 400-ms slot.
        for i in 0..6u64 {
            let c = ClockSysvar {
                slot: 100 + i,
                unix_timestamp: 1_000 + 8 * i as i64,
                ..Default::default()
            };
            g.observe(c, t0 + Duration::from_millis(400 * i));
        }
        assert!((g.scale() - 20.0).abs() < 1e-6, "{}", g.scale());
        assert!((g.slot_rate() - 2.5).abs() < 1e-6);
        let at = t0 + Duration::from_millis(400 * 5 + 1_000);
        assert_eq!(g.now_at(at), Some(1_040 + 20));
        assert_eq!(g.wall_secs_until(1_060 + 40, at), Some(2.0));
        // A backwards read is ignored.
        g.observe(
            ClockSysvar {
                slot: 99,
                unix_timestamp: 0,
                ..Default::default()
            },
            at,
        );
        assert_eq!(g.last().unwrap().slot, 105);
    }

    fn kernel_vectors(name: &str) -> serde_json::Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../permutation-rules/vectors")
            .join(name);
        serde_json::from_str(&std::fs::read_to_string(path).expect("vectors")).expect("json")
    }

    /// The kernel's clock vectors (W1-C) through fclient's clock, including
    /// the bell `u32::MAX` rows (the first twin panicked there in debug).
    #[test]
    fn clock_vectors_match_the_kernels() {
        let j = kernel_vectors("clock-vectors-v1.json");
        let d = Drand {
            genesis: j["clock"]["genesis"].as_i64().unwrap(),
            period: j["clock"]["period"].as_u64().unwrap() as u32,
        };
        assert_eq!(d, Drand::QUICKNET);
        let rows = j["bells"].as_array().unwrap();
        assert!(rows
            .iter()
            .any(|r| r["bell"].as_u64() == Some(u32::MAX as u64)));
        for r in rows {
            let g = r["genesis_ts"].as_i64().unwrap();
            let b = r["bell"].as_u64().unwrap() as u32;
            assert_eq!(bell_start(g, b), r["bell_start"].as_i64().unwrap());
            assert_eq!(bell_end(g, b), r["bell_end"].as_i64().unwrap());
            assert_eq!(d.tlock_round(g, b), r["tlock_round"].as_u64().unwrap());
            assert_eq!(bell_at(g, bell_start(g, b)), Some(b));
        }
        // Past bell u32::MAX: None, not a truncated bell.
        assert_eq!(bell_at(0, (u32::MAX as i64 + 1) * 600), None);
        for r in j["seed_rounds"].as_array().unwrap() {
            let close = r["close"].as_i64().unwrap();
            let m = r["margin"].as_u64().unwrap() as u32;
            assert_eq!(d.seed_round(close, m), r["seed_round"].as_u64().unwrap());
        }
    }
}
