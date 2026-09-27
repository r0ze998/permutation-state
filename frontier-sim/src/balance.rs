//! Doctrine balance harness (design §4.1 / D17, owner decision O5).
//!
//! Every seed is played six times, once per rotation, so every doctrine
//! sits in every wedge and on every faction's population once per seed
//! (a Latin square). With `paired` (the default) the six rotations of a
//! seed share its seed, hence its wallets, archetypes and faction draws:
//! each season is distributed exactly as in an unpaired run, but the
//! population noise cancels between doctrines. The M0 suite used unpaired
//! seeds (`1000 + 7k + rotation`); `paired: false` reproduces that layout.
//!
//! "Win" is the highest `s_k` at the Reckoning (ties to the lower slot, as
//! in M0). The band is 16.7% ± 2 points.

use std::fmt::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use crate::config::Config;
use crate::model::{doctrine_table, DoctrineSet};
use crate::report::{fx, Mult};
use crate::settle::settle_run;
use crate::sim::Sim;

/// Win-rate band: 1/6 ± this many points.
pub const BAND_POINTS: f64 = 2.0;

#[derive(Clone, Debug)]
pub struct Spec {
    pub agents: usize,
    /// Seeds 1..=seeds (paired) or M0's layout (unpaired); 6 seasons each.
    pub seeds: u64,
    /// Offset added to every seed (a disjoint seed set).
    pub first_seed: u64,
    pub set: DoctrineSet,
    pub tweaks: String,
    pub paired: bool,
}

impl Spec {
    pub fn new(agents: usize, seeds: u64, set: DoctrineSet) -> Spec {
        Spec {
            agents,
            seeds,
            first_seed: 0,
            set,
            tweaks: String::new(),
            paired: true,
        }
    }

    fn seed(&self, k: u64, rot: usize) -> u64 {
        if self.paired {
            self.first_seed + k
        } else {
            self.first_seed + 1000 + k * 7 + rot as u64
        }
    }
}

/// What one season contributes (the full `Outcome` is dropped at once, so
/// thousands of 10k-wallet seasons fit in memory).
#[derive(Clone, Debug)]
struct Season {
    seed_k: u64,
    rot: usize,
    /// By doctrine (not by faction slot).
    raw: [f64; 6],
    winner: usize,
    path: [[f64; 4]; 6],
    mult: [Mult; 6],
    checks_ok: bool,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub name: &'static str,
    pub win_pct: f64,
    pub in_band: bool,
    /// Mean undamped index and its sd across seasons.
    pub raw_mean: f64,
    pub raw_sd: f64,
    /// Mean of (raw − the season's mean over doctrines), in % of index,
    /// and its standard error across seeds (paired) or seasons.
    pub delta_pct: f64,
    pub delta_se_pct: f64,
    /// Per-capita path value ÷ civilization, by path.
    pub path: [f64; 4],
    pub claims_per_paid: f64,
}

#[derive(Clone, Debug)]
pub struct Balance {
    pub spec: Spec,
    pub seasons: usize,
    pub rows: Vec<Row>,
    pub in_band: usize,
    /// Binomial standard error of one win rate, points.
    pub se_points: f64,
    pub checks_ok: usize,
    pub secs: f64,
}

impl Balance {
    pub fn max_abs_delta_pct(&self) -> f64 {
        self.rows
            .iter()
            .map(|r| r.delta_pct.abs())
            .fold(0.0, f64::max)
    }
    pub fn max_win_gap_points(&self) -> f64 {
        self.rows
            .iter()
            .map(|r| (r.win_pct - 100.0 / 6.0).abs())
            .fold(0.0, f64::max)
    }
}

fn play_season(cfg: &Config) -> (Sim, crate::settle::Outcome) {
    let mut sim = Sim::new(cfg);
    for b in 0..sim.end_bell {
        sim.step(b);
    }
    sim.finish();
    let o = settle_run(&sim, &cfg.index);
    (sim, o)
}

fn season(cfg: &Config, seed_k: u64) -> Season {
    let (_, o) = play_season(cfg);
    let rot = cfg.doctrine_rotation;
    let dk = |k: usize| (k + rot) % 6;
    let mut raw = [0f64; 6];
    let mut path = [[0f64; 4]; 6];
    let mut mult: [Mult; 6] = Default::default();
    let best = (0..6)
        .max_by_key(|&k| (o.index[k], std::cmp::Reverse(k)))
        .unwrap();
    let act_all: f64 = o.facts.iter().map(|f| f.active as f64).sum();
    for k in 0..6 {
        raw[dk(k)] = fx(o.raw[k]);
        for (p, v) in path[dk(k)].iter_mut().enumerate() {
            let all: f64 = o.facts.iter().map(|f| f.path[p] as f64).sum();
            let pc = o.facts[k].path[p] as f64 / o.facts[k].active.max(1) as f64;
            *v = pc / (all / act_all).max(f64::MIN_POSITIVE);
        }
    }
    for a in o.agents.iter().filter(|a| !a.shade) {
        mult[dk(a.faction as usize)].add(a);
    }
    Season {
        seed_k,
        rot,
        raw,
        winner: dk(best),
        path,
        mult,
        checks_ok: o.checks.iter().all(|c| c.ok),
    }
}

/// Run the harness on every core. `base` supplies everything but the
/// seed, the doctrine fields and the agent count.
pub fn run(base: &Config, spec: &Spec) -> Balance {
    let t0 = Instant::now();
    let mut cfgs: Vec<(u64, Config)> = Vec::new();
    for k in 1..=spec.seeds {
        for rot in 0..6 {
            cfgs.push((
                k,
                Config {
                    seed: spec.seed(k, rot),
                    agents: spec.agents,
                    doctrines: true,
                    doctrine_set: spec.set,
                    doctrine_tweaks: spec.tweaks.clone(),
                    doctrine_rotation: rot,
                    stratified: false,
                    verbose: false,
                    ..base.clone()
                },
            ));
        }
    }
    let n = cfgs.len();
    let threads = std::thread::available_parallelism()
        .map_or(4, |x| x.get())
        .min(n.max(1));
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<Season>>> = (0..n).map(|_| Mutex::new(None)).collect();
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= n {
                    break;
                }
                let (k, cfg) = &cfgs[i];
                *slots[i].lock().unwrap() = Some(season(cfg, *k));
            });
        }
    });
    let seasons: Vec<Season> = slots
        .into_iter()
        .map(|m| m.into_inner().unwrap().expect("season"))
        .collect();
    summarise(spec, &seasons, t0.elapsed().as_secs_f64())
}

fn summarise(spec: &Spec, ss: &[Season], secs: f64) -> Balance {
    let table = doctrine_table(spec.set, &spec.tweaks);
    let nd = ss.len() as f64;
    let mut rows = Vec::new();
    // Per-seed doctrine deltas (paired: averaged over the seed's six
    // rotations; unpaired: every season is its own unit).
    let unit = |s: &Season| {
        if spec.paired {
            s.seed_k
        } else {
            s.seed_k * 6 + s.rot as u64
        }
    };
    let mut units: Vec<u64> = ss.iter().map(unit).collect();
    units.sort_unstable();
    units.dedup();
    let mut in_band = 0;
    for (d, doc) in table.iter().enumerate() {
        let wins = ss.iter().filter(|s| s.winner == d).count() as u64;
        let win_pct = 100.0 * wins as f64 / nd;
        let ok = (win_pct - 100.0 / 6.0).abs() <= BAND_POINTS;
        in_band += ok as usize;
        let raw: Vec<f64> = ss.iter().map(|s| s.raw[d]).collect();
        let raw_mean = raw.iter().sum::<f64>() / nd;
        let raw_sd = (raw.iter().map(|x| (x - raw_mean).powi(2)).sum::<f64>() / nd).sqrt();
        let per_unit: Vec<f64> = units
            .iter()
            .map(|&u| {
                let mine: Vec<&Season> = ss.iter().filter(|s| unit(s) == u).collect();
                mine.iter()
                    .map(|s| s.raw[d] - s.raw.iter().sum::<f64>() / 6.0)
                    .sum::<f64>()
                    / mine.len() as f64
            })
            .collect();
        let nu = per_unit.len() as f64;
        let dm = per_unit.iter().sum::<f64>() / nu;
        let dsd =
            (per_unit.iter().map(|x| (x - dm).powi(2)).sum::<f64>() / (nu - 1.0).max(1.0)).sqrt();
        let mut path = [0f64; 4];
        for s in ss {
            for (p, v) in path.iter_mut().enumerate() {
                *v += s.path[d][p] / nd;
            }
        }
        let mut m = Mult::default();
        for s in ss {
            m.merge(&s.mult[d]);
        }
        rows.push(Row {
            name: doc.name(),
            win_pct,
            in_band: ok,
            raw_mean,
            raw_sd,
            delta_pct: 100.0 * dm,
            delta_se_pct: 100.0 * dsd / nu.sqrt(),
            path,
            claims_per_paid: m.x(),
        });
    }
    Balance {
        spec: spec.clone(),
        seasons: ss.len(),
        rows,
        in_band,
        se_points: (1.0f64 / 6.0 * 5.0 / 6.0 / nd).sqrt() * 100.0,
        checks_ok: ss.iter().filter(|s| s.checks_ok).count(),
        secs,
    }
}

/// Markdown table of one balance run.
pub fn table(b: &Balance) -> String {
    let mut s = String::new();
    let sp = &b.spec;
    writeln!(
        s,
        "{} seasons: {} ({}), every doctrine in every wedge ({} seeds × 6 rotations, {}), {} wallets, equal expected sizes with each wallet's faction drawn at random. \"Win\" = highest s_k at the Reckoning. Band: 16.7% ± {BAND_POINTS} points [sim].\n",
        b.seasons,
        sp.set.label(),
        if sp.tweaks.is_empty() { "no overrides".to_string() } else { format!("overrides `{}`", sp.tweaks) },
        sp.seeds,
        if sp.paired { format!("paired: the six rotations share seed {}+k", sp.first_seed) } else { format!("unpaired: seed {}+1000+7k+rotation", sp.first_seed) },
        sp.agents
    )
    .unwrap();
    writeln!(s, "| Doctrine | win rate | in band | Δ index vs mean (± SE) | mean undamped index | sd across seasons | Dominion/cap ÷ civ | Prosperity/cap ÷ civ | Knowledge/cap ÷ civ | Concord/cap ÷ civ | claims / paid |").unwrap();
    writeln!(s, "|---|---|---|---|---|---|---|---|---|---|---|").unwrap();
    for r in &b.rows {
        writeln!(
            s,
            "| {} | {:.1}% | {} | {:+.3}% (± {:.3}) | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.3} |",
            r.name,
            r.win_pct,
            if r.in_band { "yes" } else { "**no**" },
            r.delta_pct,
            r.delta_se_pct,
            r.raw_mean,
            r.raw_sd,
            r.path[0],
            r.path[1],
            r.path[2],
            r.path[3],
            r.claims_per_paid
        )
        .unwrap();
    }
    writeln!(
        s,
        "\n**{} of 6 doctrines in the band.** Largest win-rate gap {:.1} points; largest |Δ index| {:.3}%. Binomial standard error of one win rate at this sample: {:.1} points. Conservation: {} of {} seasons passed every check. Wall time {:.0} s [measured].",
        b.in_band,
        b.max_win_gap_points(),
        b.max_abs_delta_pct(),
        b.se_points,
        b.checks_ok,
        b.seasons,
        b.secs
    )
    .unwrap();
    s
}

/// CI gate sizes: wallets per season and seeds (× 6 rotations).
///
/// **What the gate is.** A proxy for O5, not O5 itself. It bounds each
/// doctrine's mean undamped index (the systematic quantity) at a size a CI
/// job can afford: 30 paired seeds × 6 rotations = 180 seasons, index SE
/// ≈ 0.05% [sim], bound ±0.2% (≈ 4 SE). It catches an edge of about 0.25%
/// of index or more (≈ 4 win points at 10k wallets), which is what the
/// negative controls below are: the draft table (+8%), F on the Knight line
/// (−0.22% on these seeds with the final table: it fails, but only just)
/// and a within-bounds A boost (+0.38%). It does
/// **not** catch an edge under ≈ 0.15% of index, and the win-rate bound
/// (±10 points at SE 2.8) is only a guard against a gross outlier. The
/// O5 band itself (every doctrine 16.7% ± 2 points) is checked on ≥ 1,500
/// paired seasons by the scheduled workflow `doctrine-balance.yml`
/// (`frontier-sim doctrines --seeds 250 --first-seed 10000 --gate`).
///
/// The seeds are fixed (1..=30) so CI is deterministic; the scheduled
/// workflow also runs a fresh seed set every night, so a table tuned to
/// the CI seeds is caught there.
pub const GATE_AGENTS: usize = 10_000;
pub const GATE_SEEDS: u64 = 30;
/// CI gate bounds: mean undamped index within this many % of the mean over
/// doctrines, and win rate within 16.7 ± this many points.
pub const GATE_MAX_DELTA_PCT: f64 = 0.2;
pub const GATE_MAX_WIN_GAP: f64 = 10.0;
/// Seeds of the draft control (a 10-point outlier needs no more).
pub const GATE_DRAFT_SEEDS: u64 = 12;

/// The CI gate on a finished run: conservation, every doctrine's mean
/// index within `GATE_MAX_DELTA_PCT` of the mean and its win rate within
/// 16.7 ± `GATE_MAX_WIN_GAP` points.
pub fn gate_check(b: &Balance) -> Result<(), String> {
    gate_check_with(b, GATE_MAX_DELTA_PCT, GATE_MAX_WIN_GAP)
}

/// [`gate_check`] with explicit bounds (the scheduled fresh-seed run uses
/// a tighter index bound at 1,500 seasons).
pub fn gate_check_with(b: &Balance, max_delta_pct: f64, max_win_gap: f64) -> Result<(), String> {
    if b.checks_ok != b.seasons {
        return Err(format!("conservation: {} of {}", b.checks_ok, b.seasons));
    }
    for r in &b.rows {
        if r.delta_pct.abs() > max_delta_pct {
            return Err(format!(
                "{}: mean index {:+.3}% off the mean (gate ±{max_delta_pct}%)",
                r.name, r.delta_pct
            ));
        }
        if (r.win_pct - 100.0 / 6.0).abs() > max_win_gap {
            return Err(format!(
                "{}: win rate {:.1}% (gate 16.7 ± {max_win_gap})",
                r.name, r.win_pct
            ));
        }
    }
    Ok(())
}

/// The CI gate's harness: the kernel table (or `set`) at the gate's size.
pub fn gate_run(set: DoctrineSet) -> Balance {
    gate_run_with(set, "", GATE_SEEDS)
}

/// The gate's harness with doctrine overrides (`--dx` syntax) and a seed
/// count: the negative controls.
pub fn gate_run_with(set: DoctrineSet, tweaks: &str, seeds: u64) -> Balance {
    let mut spec = Spec::new(GATE_AGENTS, seeds, set);
    spec.tweaks = tweaks.to_string();
    run(&Config::default(), &spec)
}

/// Negative controls: tables the gate must reject. The Knight line for F
/// (the K3 tuning's outlier, −0.22% on the gate seeds with the final
/// table) and an A boost that stays inside every doctrine bound (+0.38%).
pub const GATE_KNIGHT: &str = "F.unit=knight,F.variant=10000";
pub const GATE_A_BOOST: &str = "A.walls=5000,A.drill=11500,A.travel=7000";

#[cfg(test)]
mod tests {
    use super::*;
    use permutation_rules::frontier::doctrine::{validate_table, DOCTRINES};

    /// CI gate (design §4.1: "a CI check that no doctrine's win rate in the
    /// balance simulation leaves 16.7% ± 2 points"), reduced to fit a CI
    /// job. See `GATE_*` and DOCTRINES.md for the sizes and bounds.
    #[test]
    #[cfg_attr(debug_assertions, ignore = "release only: cargo test --release")]
    fn doctrine_balance_gate() {
        assert_eq!(validate_table(&DOCTRINES), Ok(()));
        let b = gate_run(DoctrineSet::Kernel);
        eprintln!("{}", table(&b));
        if let Err(e) = gate_check(&b) {
            panic!("{e}\n{}", table(&b));
        }
    }

    /// The gate has power: it rejects the draft table (Lumen's ×1.2
    /// Knowledge weight and +20% science).
    #[test]
    #[cfg_attr(debug_assertions, ignore = "release only: cargo test --release")]
    fn doctrine_balance_gate_rejects_the_draft() {
        let b = gate_run_with(DoctrineSet::Draft, "", GATE_DRAFT_SEEDS);
        assert!(
            gate_check(&b).is_err(),
            "draft table passed the gate: {}",
            table(&b)
        );
    }

    /// Negative control (review of K3): F on the Knight line, which the K3
    /// gate (12 seeds, ±0.3%) let through on some seed sets.
    #[test]
    #[cfg_attr(debug_assertions, ignore = "release only: cargo test --release")]
    fn doctrine_balance_gate_rejects_the_knight() {
        let b = gate_run_with(DoctrineSet::Kernel, GATE_KNIGHT, GATE_SEEDS);
        assert!(
            gate_check(&b).is_err(),
            "the Knight table passed the gate: {}",
            table(&b)
        );
    }

    /// Negative control: a quiet edge inside every doctrine bound (walls
    /// ×0.5, a ×1.15 drill, marches ×0.7 for A).
    #[test]
    #[cfg_attr(debug_assertions, ignore = "release only: cargo test --release")]
    fn doctrine_balance_gate_rejects_a_quiet_a_boost() {
        let t = crate::model::doctrine_table(DoctrineSet::Kernel, GATE_A_BOOST);
        assert!(
            t.iter().all(|d| d.k.validate().is_ok()),
            "the control must be a valid table"
        );
        let b = gate_run_with(DoctrineSet::Kernel, GATE_A_BOOST, GATE_SEEDS);
        assert!(
            gate_check(&b).is_err(),
            "the A boost passed the gate: {}",
            table(&b)
        );
    }
}
