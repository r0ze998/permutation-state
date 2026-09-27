//! Restated C4 inputs from the simulator (m0c review of SP-FEE).
//!
//! C4 (owner decision O7): excluding every reveal bid at the default keeper
//! price for one bell must cost at least 10× the value of all clashes
//! resolving in that bell for the targeted side. SP-FEE priced the value at
//! a placeholder $7 per clash the busiest faction took part in, on one
//! 50k-wallet seed. This module measures both sides of that from the
//! simulator instead:
//!
//! * **Participation** (the per-bell metric SP-FEE asked for): for every
//!   bell, the clashes the busiest faction takes part in (arrivals, resident
//!   hosts or garrisons) and the clashes in which it has arrivals, the only
//!   ones an excluded reveal changes (besides committed postures). Tail
//!   bells are grouped into **episodes** (runs of consecutive bells at or
//!   above the p99), so a tail that rests on one war shows as one episode.
//! * **Value** (counterfactual): the same seed is re-played with that
//!   faction's reveals excluded in one bell (`Config::attack`: its arrivals
//!   routed at 50%, its committed postures in Disarray), and the value is
//!   the faction's loss of claims (USDC) against the unattacked season. The
//!   largest gain of any other faction is reported too. A re-play can
//!   diverge after a change (agents decide on what they see), so one
//!   counterfactual is noisy; the table spans several seeds and bells, and
//!   the **minimal attack** (the bell in which the faction has the smallest
//!   non-zero arrival) shows how far a tiny exclusion propagates.

use std::fmt::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use crate::config::Config;
use crate::settle::{settle_run, Outcome};
use crate::sim::{ClashRow, Sim};
use permutation_rules::frontier::pools::USDC;

pub struct Played {
    pub claims: [f64; 6],
    pub log: Vec<ClashRow>,
    pub routed: u64,
    pub disarray: u64,
    pub end_bell: u32,
    pub checks_ok: bool,
}

fn play(cfg: &Config) -> Played {
    let mut sim = Sim::new(cfg);
    for b in 0..sim.end_bell {
        sim.step(b);
    }
    sim.finish();
    let o: Outcome = settle_run(&sim, &cfg.index);
    let mut claims = [0f64; 6];
    for a in &o.agents {
        claims[a.faction as usize % 6] += a.claim.paid as f64 / USDC as f64;
    }
    Played {
        claims,
        log: std::mem::take(&mut sim.clash_log),
        routed: sim.attack_routed,
        disarray: sim.attack_disarray,
        end_bell: sim.end_bell,
        checks_ok: o.checks.iter().all(|c| c.ok),
    }
}

fn run_par(cfgs: Vec<Config>) -> Vec<Played> {
    let n = cfgs.len();
    let threads = std::thread::available_parallelism()
        .map_or(4, |x| x.get())
        .min(n.max(1));
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<Played>>> = (0..n).map(|_| Mutex::new(None)).collect();
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= n {
                    break;
                }
                *slots[i].lock().unwrap() = Some(play(&cfgs[i]));
            });
        }
    });
    slots
        .into_iter()
        .map(|m| m.into_inner().unwrap().expect("played"))
        .collect()
}

/// Per-bell participation of every faction.
pub struct Bells {
    /// `part[b][f]`: clashes faction f takes part in during bell b.
    pub part: Vec<[u32; 6]>,
    /// `arr[b][f]`: clashes in which f has admitted arrivals.
    pub arr: Vec<[u32; 6]>,
    pub troops: Vec<[u64; 6]>,
    /// Player-vs-player clashes (≥ 2 player factions) f takes part in.
    pub pvp: Vec<[u32; 6]>,
}

pub fn bells(p: &Played) -> Bells {
    let n = p.end_bell as usize + 1;
    let mut bl = Bells {
        part: vec![[0; 6]; n],
        arr: vec![[0; 6]; n],
        troops: vec![[0; 6]; n],
        pvp: vec![[0; 6]; n],
    };
    for r in &p.log {
        let b = r.bell as usize;
        let any = r.arr_mask | r.def_mask;
        let is_pvp = any.count_ones() >= 2;
        for f in 0..6 {
            if any & (1 << f) != 0 {
                bl.part[b][f] += 1;
                if is_pvp {
                    bl.pvp[b][f] += 1;
                }
            }
            if r.arr_mask & (1 << f) != 0 {
                bl.arr[b][f] += 1;
                bl.troops[b][f] += r.arr_troops[f] as u64;
            }
        }
    }
    bl
}

fn quant(v: &[u32], q: f64) -> u32 {
    let mut s = v.to_vec();
    s.sort_unstable();
    s[((q * s.len() as f64) as usize).min(s.len() - 1)]
}

fn stat_line(v: &[u32]) -> String {
    let mean = v.iter().map(|&x| x as f64).sum::<f64>() / v.len() as f64;
    format!(
        "mean {:.1}, p50 {}, p90 {}, p99 {}, max {}",
        mean,
        quant(v, 0.5),
        quant(v, 0.9),
        quant(v, 0.99),
        v.iter().max().copied().unwrap_or(0)
    )
}

pub struct Spec {
    pub agents: usize,
    pub seeds: u64,
    pub first_seed: u64,
}

/// Run the C4 measurement; returns markdown and a JSON summary.
pub fn run(base: &Config, spec: &Spec) -> (String, String) {
    let t0 = Instant::now();
    let seeds: Vec<u64> = (1..=spec.seeds).map(|k| spec.first_seed + k).collect();
    let bases: Vec<Config> = seeds
        .iter()
        .map(|&s| Config {
            seed: s,
            agents: spec.agents,
            clash_log: true,
            verbose: false,
            ..base.clone()
        })
        .collect();
    let base_runs = run_par(bases.clone());
    let mut md = String::new();
    let mut js = String::from("{\"seeds\": [");
    writeln!(
        md,
        "## C4 from the simulator: per-bell participation and counterfactual value\n"
    )
    .unwrap();
    writeln!(md, "{} wallets, seeds {:?}, doctrines off, the default (K3) economy. \"Busiest faction\" = the faction with the most clashes in that bell. A counterfactual re-plays the seed with that faction's reveals excluded in that one bell (arrivals routed at 50%, committed postures in Disarray) [sim].\n", spec.agents, seeds).unwrap();
    // Counterfactual targets per seed.
    let mut attacks: Vec<(usize, &'static str, u32, u8)> = Vec::new();
    let mut per_seed_bells = Vec::new();
    for (i, p) in base_runs.iter().enumerate() {
        let bl = bells(p);
        let nb = p.end_bell as usize;
        let m: Vec<u32> = (0..nb).map(|b| *bl.part[b].iter().max().unwrap()).collect();
        let ma: Vec<u32> = (0..nb).map(|b| *bl.arr[b].iter().max().unwrap()).collect();
        let mp: Vec<u32> = (0..nb).map(|b| *bl.pvp[b].iter().max().unwrap()).collect();
        let busiest = |b: usize| -> u8 {
            (0..6)
                .max_by_key(|&f| (bl.part[b][f], std::cmp::Reverse(f)))
                .unwrap() as u8
        };
        // Bells at the quantiles of m (the first bell reaching each value).
        let pick = |q: f64| -> usize {
            let v = if q >= 1.0 {
                *m.iter().max().unwrap()
            } else {
                quant(&m, q)
            };
            (0..nb).find(|&b| m[b] == v).unwrap()
        };
        for (name, q) in [
            ("p50 bell", 0.5),
            ("p90 bell", 0.9),
            ("p99 bell", 0.99),
            ("max bell", 1.0),
        ] {
            let b = pick(q);
            attacks.push((i, name, b as u32, busiest(b)));
        }
        // Minimal attack: the busiest faction of the max bell, in the bell
        // where it has the smallest non-zero arriving troops.
        let fm = busiest(pick(1.0)) as usize;
        if let Some(b) = (0..nb)
            .filter(|&b| bl.troops[b][fm] > 0)
            .min_by_key(|&b| (bl.troops[b][fm], b))
        {
            attacks.push((i, "minimal attack", b as u32, fm as u8));
        }
        // Tail episodes: runs of bells with m >= p99.
        let thr = quant(&m, 0.99);
        let mut eps: Vec<(usize, usize, u32)> = Vec::new();
        let mut b = 0;
        while b < nb {
            if m[b] >= thr {
                let s = b;
                let mut mx = 0;
                while b < nb && m[b] >= thr {
                    mx = mx.max(m[b]);
                    b += 1;
                }
                eps.push((s, b - 1, mx));
            } else {
                b += 1;
            }
        }
        writeln!(md, "### Seed {}\n", seeds[i]).unwrap();
        writeln!(
            md,
            "- Clashes resolved: {}; conservation {}.",
            p.log.len(),
            if p.checks_ok { "PASS" } else { "**FAIL**" }
        )
        .unwrap();
        writeln!(
            md,
            "- Busiest faction's clashes per bell (all it takes part in): {}.",
            stat_line(&m)
        )
        .unwrap();
        writeln!(md, "- Busiest faction's clashes **with its own arrivals** per bell (what an exclusion changes): {}.", stat_line(&ma)).unwrap();
        writeln!(
            md,
            "- Busiest faction's player-vs-player clashes per bell: {}.",
            stat_line(&mp)
        )
        .unwrap();
        writeln!(
            md,
            "- Tail episodes (consecutive bells ≥ p99 = {thr}): {} — {}.\n",
            eps.len(),
            eps.iter()
                .take(8)
                .map(|(s, e, mx)| format!(
                    "bells {s}–{e} (day {:.1}–{:.1}, {} bells, max {mx})",
                    *s as f64 / 144.0,
                    *e as f64 / 144.0,
                    e - s + 1
                ))
                .collect::<Vec<_>>()
                .join("; ")
        )
        .unwrap();
        let _ = write!(
            js,
            "{}{{\"seed\": {}, \"part\": {{\"mean\": {:.2}, \"p90\": {}, \"p99\": {}, \"max\": {}}}, \"arr\": {{\"mean\": {:.2}, \"p90\": {}, \"p99\": {}, \"max\": {}}}, \"episodes\": {}}}",
            if i > 0 { ", " } else { "" },
            seeds[i],
            m.iter().map(|&x| x as f64).sum::<f64>() / nb as f64,
            quant(&m, 0.9),
            quant(&m, 0.99),
            m.iter().max().unwrap(),
            ma.iter().map(|&x| x as f64).sum::<f64>() / nb as f64,
            quant(&ma, 0.9),
            quant(&ma, 0.99),
            ma.iter().max().unwrap(),
            eps.len()
        );
        per_seed_bells.push(bl);
    }
    js.push_str("], \"attacks\": [");
    let cf: Vec<Config> = attacks
        .iter()
        .map(|&(i, _, b, f)| Config {
            attack: Some((f, b, b)),
            clash_log: false,
            ..bases[i].clone()
        })
        .collect();
    let cf_runs = run_par(cf);
    writeln!(md, "### Counterfactual value of one attacked bell\n").unwrap();
    writeln!(md, "| seed | bell | faction | its clashes (all / with arrivals) | its arriving troops | troops routed (all arrivals of that bell) | postures in Disarray | **target's loss of claims (USDC)** | largest gain of another faction | faction's claims (unattacked) |").unwrap();
    writeln!(md, "|---|---|---|---|---|---|---|---|---|---|").unwrap();
    for (k, (&(i, name, b, f), r)) in attacks.iter().zip(&cf_runs).enumerate() {
        let base = &base_runs[i];
        let bl = &per_seed_bells[i];
        let loss = base.claims[f as usize] - r.claims[f as usize];
        let gain = (0..6)
            .filter(|&g| g != f as usize)
            .map(|g| r.claims[g] - base.claims[g])
            .fold(f64::MIN, f64::max);
        writeln!(
            md,
            "| {} | {name}: {b} (day {:.1}) | {} | {} / {} | {} | {} | {} | **{:+.2}** | {:+.2} | {:.0} |",
            seeds[i],
            b as f64 / 144.0,
            (b'A' + f) as char,
            bl.part[b as usize][f as usize],
            bl.arr[b as usize][f as usize],
            bl.troops[b as usize][f as usize] / 1000,
            r.routed / 1000,
            r.disarray,
            loss,
            gain,
            base.claims[f as usize]
        )
        .unwrap();
        let _ = write!(
            js,
            "{}{{\"seed\": {}, \"kind\": \"{name}\", \"bell\": {b}, \"faction\": {f}, \"part\": {}, \"arr\": {}, \"routed_troops\": {}, \"loss_usdc\": {:.3}, \"max_gain_usdc\": {:.3}, \"conserved\": {}}}",
            if k > 0 { ", " } else { "" },
            seeds[i],
            bl.part[b as usize][f as usize],
            bl.arr[b as usize][f as usize],
            r.routed / 1000,
            loss,
            gain,
            r.checks_ok
        );
    }
    js.push_str("]}");
    writeln!(
        md,
        "\nWall time {:.0} s [measured].",
        t0.elapsed().as_secs_f64()
    )
    .unwrap();
    (md, js)
}
