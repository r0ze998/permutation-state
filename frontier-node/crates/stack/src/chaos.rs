//! Chaos (M1 contract §13.4, offchain design §11.4): `kill -9` a random
//! component every 2–6 game hours and restart it after 0–60 game seconds.
//! The plan is a pure function of the seed and the play window, so a run
//! can be repeated; the supervisor converts game seconds to wall time at
//! the run's scale (the Clock stops while the chain itself is down, so a
//! game-second delay cannot be measured on it).

use serde_json::{json, Value};

/// Components chaos may kill (every long-running one of the stack).
pub const TARGETS: &[&str] = &[
    "localnet",
    "drand-replay",
    "keeper-a",
    "keeper-b",
    "relay",
    "herald",
    "bots",
];

#[derive(Clone, Debug, PartialEq)]
pub struct Kill {
    /// Game time of the kill.
    pub at: i64,
    pub component: String,
    /// Game seconds until the restart.
    pub restart_after: f64,
}

impl Kill {
    pub fn to_json(&self) -> Value {
        json!({"at": self.at, "component": self.component, "restart_after_game_secs": self.restart_after})
    }
}

/// xorshift64* (seeded; no new crate).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
}

/// The kill schedule over `[start, end)` game time among `targets`.
pub fn plan(
    seed: u64,
    start: i64,
    end: i64,
    min_hours: f64,
    max_hours: f64,
    restart_max_secs: f64,
    targets: &[String],
) -> Vec<Kill> {
    let mut r = Rng::new(seed ^ 0xC4A0_5EED);
    let mut out = vec![];
    if targets.is_empty() || end <= start {
        return out;
    }
    let mut t = start as f64;
    loop {
        t += (min_hours + r.unit() * (max_hours - min_hours)) * 3_600.0;
        if t >= end as f64 {
            break;
        }
        out.push(Kill {
            at: t as i64,
            component: targets[r.below(targets.len())].clone(),
            restart_after: r.unit() * restart_max_secs,
        });
    }
    out
}

/// Forced kills (`--chaos-force component:hours`, W6T-4): each at
/// `genesis_ts + (viewer_start_hours + hours) × 3,600`, restarted after the
/// chaos restart maximum (the worst case the viewers' recovery budget must
/// cover). Sorted by time.
pub fn forced(
    genesis_ts: i64,
    viewer_start_hours: f64,
    force: &[(String, f64)],
    restart_max_secs: f64,
) -> Vec<Kill> {
    let _ = (genesis_ts, viewer_start_hours, force, restart_max_secs);
    vec![] // W6T-4 failing-first stub
}

/// The plan with the forced kills merged in (sorted by time).
pub fn merge(mut kills: Vec<Kill>, forced: Vec<Kill>) -> Vec<Kill> {
    kills.extend(forced);
    kills.sort_by_key(|k| k.at);
    kills
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Vec<String> {
        TARGETS.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn gaps_and_delays_in_range_and_repeatable() {
        let p = plan(7, 1_000, 1_000 + 7 * 86_400, 2.0, 6.0, 60.0, &t());
        assert_eq!(p, plan(7, 1_000, 1_000 + 7 * 86_400, 2.0, 6.0, 60.0, &t()));
        assert_ne!(p, plan(8, 1_000, 1_000 + 7 * 86_400, 2.0, 6.0, 60.0, &t()));
        // 7 days / 2–6 h ≈ 28–84 kills.
        assert!((28..=84).contains(&p.len()), "{}", p.len());
        let mut prev = 1_000;
        for k in &p {
            let gap = k.at - prev;
            assert!((7_199..=21_601).contains(&gap), "{gap}");
            assert!((0.0..60.0).contains(&k.restart_after));
            assert!(TARGETS.contains(&k.component.as_str()));
            prev = k.at;
        }
        let comps: std::collections::BTreeSet<_> = p.iter().map(|k| k.component.clone()).collect();
        assert!(
            comps.len() >= 5,
            "kills spread over the components: {comps:?}"
        );
    }

    /// W6T-4: `--chaos-force herald:2 --chaos-force herald:7` puts two
    /// herald kills 2 and 7 game hours into the viewer window, restarted
    /// after the chaos maximum, merged in time order with the random plan.
    #[test]
    fn forced_herald_kills_fall_in_the_viewer_window() {
        let g = 1_000_000;
        let f = forced(
            g,
            1.0,
            &[("herald".into(), 7.0), ("herald".into(), 2.0)],
            60.0,
        );
        assert_eq!(
            f,
            vec![
                Kill {
                    at: g + 3 * 3_600,
                    component: "herald".into(),
                    restart_after: 60.0
                },
                Kill {
                    at: g + 8 * 3_600,
                    component: "herald".into(),
                    restart_after: 60.0
                },
            ]
        );
        let p = plan(1, g, g + 86_400, 2.0, 6.0, 60.0, &t());
        let m = merge(p.clone(), f);
        assert_eq!(m.len(), p.len() + 2);
        assert!(m.windows(2).all(|w| w[0].at <= w[1].at));
    }

    #[test]
    fn empty_windows() {
        assert!(plan(1, 10, 10, 2.0, 6.0, 60.0, &t()).is_empty());
        assert!(
            plan(1, 0, 3_600, 2.0, 6.0, 60.0, &t()).is_empty(),
            "shorter than the first gap"
        );
        assert!(plan(1, 0, 86_400, 2.0, 6.0, 60.0, &[]).is_empty());
    }
}
