//! Runs and the M0 measurement suite.

use std::fmt::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use crate::config::{Config, Emission, LateStake, OfficePay};
use crate::model::{doctrine_set, Arch};
use crate::report::*;
use crate::settle::{settle_run, Outcome};
use crate::sim::Sim;
use permutation_rules::frontier::index::{IndexParams, INDEX_ONE};

/// Play one season and settle it under the configured index.
pub fn play(cfg: &Config) -> (Sim, Outcome) {
    let mut sim = Sim::new(cfg);
    for b in 0..sim.end_bell {
        sim.step(b);
        if cfg.verbose && b % 144 == 143 {
            eprintln!(
                "day {:2}: ring {}, holdings {}, sessions {}, clashes {}, sieges {}/{}",
                b / 144,
                sim.open_ring,
                sim.used_sites,
                sim.stats.sessions,
                sim.stats.clashes,
                sim.stats.sieges_declared,
                sim.stats.sieges_completed
            );
        }
    }
    sim.finish();
    let o = settle_run(&sim, &cfg.index);
    (sim, o)
}

pub fn run_report(sim: &Sim, o: &Outcome, secs: f64) -> String {
    let mut s = String::new();
    let c = &sim.cfg;
    writeln!(
        s,
        "## Season: {} wallets, seed {}, sizes {:?}, bots {:.0}%, doctrines {}, γ {}/{}  ({secs:.1} s)\n",
        c.agents,
        c.seed,
        c.faction_weights,
        c.bot_share * 100.0,
        c.doctrines,
        c.index.gamma_num,
        c.index.gamma_den
    )
    .unwrap();
    writeln!(
        s,
        "### Payout multiple by archetype\n\n{}",
        payout_table(&[o])
    )
    .unwrap();
    writeln!(s, "### Claim parts\n\n{}", claim_parts_table(&[o])).unwrap();
    writeln!(s, "### By join day\n\n{}", join_day_table(&[o])).unwrap();
    writeln!(
        s,
        "### Laurels by join day (stakers, excluding idle)\n\n{}",
        join_laurel_table(&[o])
    )
    .unwrap();
    writeln!(s, "### Holdings\n\n{}", tier_table(&[o])).unwrap();
    writeln!(s, "### Factions\n\n{}", faction_table(sim, o)).unwrap();
    writeln!(
        s,
        "### Laurels and Works by archetype (per wallet)\n\n{}",
        laurel_table(&[o])
    )
    .unwrap();
    writeln!(s, "### Season\n\n{}", stats_table(sim, o)).unwrap();
    writeln!(s, "### Conservation\n\n{}", checks_table(o)).unwrap();
    writeln!(
        s,
        "World holding emission per day (laurels): {}\n",
        sim.stats
            .emission_by_day
            .iter()
            .map(|e| format!("{:.0}", *e as f64 / crate::sim::LAUREL as f64))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .unwrap();
    writeln!(s, "digest {}", hex(&o.digest)).unwrap();
    s
}

// ------------------------------------------------------------ runner

/// One finished job: the season settled under each requested index.
pub struct Done {
    pub cfg: Config,
    pub outs: Vec<Outcome>,
    pub secs: f64,
    pub stats: crate::sim::Stats,
    pub engine_stages: u8,
}

pub struct Job {
    pub cfg: Config,
    pub gammas: Vec<IndexParams>,
}

/// Run jobs on every core; results come back in job order.
pub fn run_all(jobs: Vec<Job>) -> Vec<Done> {
    let n = jobs.len();
    let threads = std::thread::available_parallelism()
        .map_or(4, |x| x.get())
        .min(n.max(1));
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<Done>>> = (0..n).map(|_| Mutex::new(None)).collect();
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= n {
                    break;
                }
                let job = &jobs[i];
                let t = Instant::now();
                let mut sim = Sim::new(&job.cfg);
                for b in 0..sim.end_bell {
                    sim.step(b);
                }
                sim.finish();
                let secs = t.elapsed().as_secs_f64();
                let mut gs = vec![job.cfg.index];
                gs.extend(job.gammas.iter().copied());
                let outs = gs.iter().map(|g| settle_run(&sim, g)).collect();
                *slots[i].lock().unwrap() = Some(Done {
                    cfg: job.cfg.clone(),
                    outs,
                    secs,
                    stats: sim.stats.clone(),
                    engine_stages: sim.engine_stages,
                });
            });
        }
    });
    slots
        .into_iter()
        .map(|m| m.into_inner().unwrap().expect("job"))
        .collect()
}

fn gamma(num: u32, den: u32) -> IndexParams {
    IndexParams {
        gamma_num: num,
        gamma_den: den,
        ..IndexParams::REV2
    }
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

fn range(v: &[f64]) -> (f64, f64) {
    v.iter()
        .fold((f64::MAX, f64::MIN), |(a, b), &x| (a.min(x), b.max(x)))
}

fn all_pass(done: &[Done]) -> (usize, usize) {
    let mut n = 0;
    let mut ok = 0;
    for d in done {
        for o in &d.outs {
            n += 1;
            if o.checks.iter().all(|c| c.ok) {
                ok += 1;
            }
        }
    }
    (ok, n)
}

// ------------------------------------------------------------ the suite

pub fn suite(base: &Config, seeds: u64, only: Option<&str>) -> String {
    let t0 = Instant::now();
    let want = |k: &str| only.is_none_or(|o| o.split(',').any(|x| x == k));
    let mut s = String::new();
    let mut checks_ok = 0;
    let mut checks_n = 0;
    writeln!(
        s,
        "<!-- generated by frontier-sim suite: {} wallets, {} seeds per cell -->\n",
        base.agents, seeds
    )
    .unwrap();

    // ---- §A baseline payout table
    if want("payout") {
        let jobs: Vec<Job> = (1..=seeds)
            .map(|k| Job {
                cfg: Config {
                    seed: k,
                    ..base.clone()
                },
                gammas: vec![],
            })
            .collect();
        let done = run_all(jobs);
        let (ok, n) = all_pass(&done);
        checks_ok += ok;
        checks_n += n;
        let outs: Vec<&Outcome> = done.iter().map(|d| &d.outs[0]).collect();
        writeln!(
            s,
            "## A. Payout multiple by archetype and join day (baseline)\n"
        )
        .unwrap();
        writeln!(
            s,
            "{} seasons × {} wallets, equal factions, bots {:.0}%, Shades {:.1}%, doctrines off, γ = 0.6, **K3 economy** (officer-pay ceiling 95%, 105 Works per USDC, stake priced by accrual left, order-weighted emission, no Relic Site laurels, Mandate reserve to staker completers). Pooled over seeds 1..={}. Multiples are Σ claims / Σ paid for the group [sim].\n",
            done.len(),
            base.agents,
            base.bot_share * 100.0,
            base.shade_bps as f64 / 100.0,
            seeds
        )
        .unwrap();
        writeln!(s, "{}", payout_table(&outs)).unwrap();
        writeln!(s, "Per-seed spread of key cells:\n").unwrap();
        writeln!(s, "| Cell | per seed | mean |").unwrap();
        writeln!(s, "|---|---|---|").unwrap();
        for (label, arch, st) in [
            ("scripted bot, with stake", Arch::Bot, true),
            ("scripted bot, fee only", Arch::Bot, false),
            ("very skilled, with stake", Arch::VerySkilled, true),
            ("skilled, with stake", Arch::Skilled, true),
            ("daily, with stake", Arch::Daily, true),
            ("daily, fee only", Arch::Daily, false),
            ("casual, fee only", Arch::Casual, false),
            ("idle, fee only", Arch::Idle, false),
        ] {
            let v: Vec<f64> = outs.iter().map(|o| arch_mult(o, arch, st).x()).collect();
            writeln!(
                s,
                "| {label} | {} | {:.3} |",
                v.iter()
                    .map(|x| format!("{x:.3}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                mean(&v)
            )
            .unwrap();
        }
        writeln!(
            s,
            "\n### A.1 Claim parts (USDC per wallet)\n\n{}",
            claim_parts_table(&outs)
        )
        .unwrap();
        writeln!(s, "### A.2 By join day\n\n{}", join_day_table(&outs)).unwrap();
        writeln!(
            s,
            "### A.3 Laurels by join day (stakers, idle excluded)\n\n{}",
            join_laurel_table(&outs)
        )
        .unwrap();
        writeln!(
            s,
            "### A.4 Where first holdings sit at T_end (stakers, idle excluded)\n\n{}",
            neighbourhood_table(&outs)
        )
        .unwrap();
        writeln!(s, "### A.5 First holding at T_end\n\n{}", tier_table(&outs)).unwrap();
        writeln!(
            s,
            "### A.6 Laurel sources, Works, sessions (per wallet)\n\n{}",
            laurel_table(&outs)
        )
        .unwrap();
        let d0 = &done[0];
        writeln!(s, "### A.7 One season in numbers (seed 1)\n").unwrap();
        writeln!(s, "{}", stats_line(d0)).unwrap();
        writeln!(
            s,
            "### A.8 Conservation (seed 1)\n\n{}",
            checks_table(&d0.outs[0])
        )
        .unwrap();
        let secs: Vec<f64> = done.iter().map(|d| d.secs).collect();
        let (lo, hi) = range(&secs);
        writeln!(
            s,
            "Season wall time [measured]: {:.1}–{:.1} s per {}-wallet season (release build, one thread each, {} in parallel).\n",
            lo,
            hi,
            base.agents,
            done.len().min(std::thread::available_parallelism().map_or(4, |x| x.get()))
        )
        .unwrap();
    }

    // ---- §B the M0 (revision 2) economy on the same seeds
    if want("variant") {
        let jobs: Vec<Job> = (1..=seeds)
            .map(|k| {
                let mut cfg = Config {
                    seed: k,
                    ..base.clone()
                };
                cfg.set_rev2_economy();
                Job {
                    cfg,
                    gammas: vec![],
                }
            })
            .collect();
        let done = run_all(jobs);
        let (ok, n) = all_pass(&done);
        checks_ok += ok;
        checks_n += n;
        let outs: Vec<&Outcome> = done.iter().map(|d| &d.outs[0]).collect();
        writeln!(
            s,
            "## B. For comparison: the M0 (revision 2) economy on the same seeds\n"
        )
        .unwrap();
        writeln!(s, "Same seeds and settings as A, with the economy of `cea89be`: every holding emits 1/12 a bell, Relic Sites pay 1 laurel a bell, 140 Works per USDC, stakes priced by days left, officer pay without a ceiling, Mandate reserve split among every completer [sim].\n").unwrap();
        writeln!(s, "{}", payout_table(&outs)).unwrap();
        writeln!(s, "### B.1 By join day\n\n{}", join_day_table(&outs)).unwrap();
        writeln!(
            s,
            "### B.2 Laurels by join day\n\n{}",
            join_laurel_table(&outs)
        )
        .unwrap();
        writeln!(
            s,
            "### B.3 Where first holdings sit\n\n{}",
            neighbourhood_table(&outs)
        )
        .unwrap();
    }

    // ---- §C herding and γ
    if want("herding") {
        let gammas: Vec<(u32, u32)> = vec![
            (0, 1),
            (3, 10),
            (3, 5),
            (4, 5),
            (1, 1),
            (6, 5),
            (3, 2),
            (2, 1),
        ];
        let sizes: Vec<(f64, [u32; 6])> = vec![
            (1.0, [1, 1, 1, 1, 1, 1]),
            (1.5, [3, 2, 2, 2, 2, 2]),
            (2.0, [2, 1, 1, 1, 1, 1]),
            (3.0, [3, 1, 1, 1, 1, 1]),
        ];
        let mut jobs = Vec::new();
        for (_, w) in &sizes {
            for k in 1..=seeds {
                jobs.push(Job {
                    cfg: Config {
                        seed: 100 + k,
                        faction_weights: *w,
                        ..base.clone()
                    },
                    gammas: gammas.iter().map(|&(a, b)| gamma(a, b)).collect(),
                });
            }
        }
        let done = run_all(jobs);
        let (ok, n) = all_pass(&done);
        checks_ok += ok;
        checks_n += n;
        writeln!(s, "## C. Herding: faction size, β and γ\n").unwrap();
        writeln!(
            s,
            "Faction 0 has m× the members of each other faction (same archetype mix in every faction, stratified). {} seeds per size, {} wallets. β is measured as `1 + ln(r_big / r_small) / ln m`, where r is the per-active-member path value (per path) or the undamped index `clamp(mean ratio)` [sim].\n",
            seeds, base.agents
        )
        .unwrap();
        writeln!(s, "### C.1 β by path (mean over seeds; min–max)\n").unwrap();
        writeln!(
            s,
            "| m | Dominion | Prosperity | Knowledge | Concord | index (undamped) |"
        )
        .unwrap();
        writeln!(s, "|---|---|---|---|---|---|").unwrap();
        for (m, w) in sizes.iter().skip(1) {
            let ds: Vec<&Done> = done
                .iter()
                .filter(|d| d.cfg.faction_weights == *w)
                .collect();
            let mut row = format!("| {m} |");
            for p in 0..5 {
                let v: Vec<f64> = ds.iter().map(|d| beta(&d.outs[0], p, *m)).collect();
                let (lo, hi) = range(&v);
                write!(row, " {:.2} ({:.2}–{:.2}) |", mean(&v), lo, hi).unwrap();
            }
            writeln!(s, "{row}").unwrap();
        }
        writeln!(s, "\n### C.2 Herding ratio: per-capita payout (Σ claims / Σ paid) of the big faction ÷ the small factions\n").unwrap();
        write!(s, "| m |").unwrap();
        for (a, b) in &gammas {
            write!(s, " γ = {:.2} |", *a as f64 / *b as f64).unwrap();
        }
        writeln!(s).unwrap();
        write!(s, "|---|").unwrap();
        for _ in &gammas {
            write!(s, "---|").unwrap();
        }
        writeln!(s).unwrap();
        let mut chosen: Option<(u32, u32)> = None;
        for (m, w) in &sizes {
            let ds: Vec<&Done> = done
                .iter()
                .filter(|d| d.cfg.faction_weights == *w)
                .collect();
            write!(s, "| {m} |").unwrap();
            for (gi, g) in gammas.iter().enumerate() {
                let v: Vec<f64> = ds
                    .iter()
                    .map(|d| herding_ratio(&d.outs[gi + 1]).0)
                    .collect();
                let (_, hi) = range(&v);
                write!(s, " {:.3} (max {:.3}) |", mean(&v), hi).unwrap();
                if *m == 3.0 && hi <= 1.0 && chosen.is_none() {
                    chosen = Some(*g);
                }
            }
            writeln!(s).unwrap();
        }
        writeln!(
            s,
            "\n### C.3 Stakers and fee-only citizens separately (big ÷ small)\n"
        )
        .unwrap();
        writeln!(
            s,
            "| m | γ | fee only | with stake | s_big | s_small (mean) | h_big |"
        )
        .unwrap();
        writeln!(s, "|---|---|---|---|---|---|---|").unwrap();
        for (m, w) in sizes.iter().skip(1) {
            let ds: Vec<&Done> = done
                .iter()
                .filter(|d| d.cfg.faction_weights == *w)
                .collect();
            for (gi, (a, b)) in gammas.iter().enumerate() {
                if !matches!((a, b), (0, 1) | (3, 5) | (1, 1)) {
                    continue;
                }
                let r: Vec<(f64, f64, f64)> =
                    ds.iter().map(|d| herding_ratio(&d.outs[gi + 1])).collect();
                let sb: Vec<f64> = ds.iter().map(|d| fx(d.outs[gi + 1].index[0])).collect();
                let ss: Vec<f64> = ds
                    .iter()
                    .map(|d| (1..6).map(|k| fx(d.outs[gi + 1].index[k])).sum::<f64>() / 5.0)
                    .collect();
                let hb: Vec<f64> = ds.iter().map(|d| fx(d.outs[gi + 1].herd[0])).collect();
                writeln!(
                    s,
                    "| {m} | {:.2} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |",
                    *a as f64 / *b as f64,
                    mean(&r.iter().map(|x| x.1).collect::<Vec<_>>()),
                    mean(&r.iter().map(|x| x.2).collect::<Vec<_>>()),
                    mean(&sb),
                    mean(&ss),
                    mean(&hb)
                )
                .unwrap();
            }
        }
        // Extrapolation [estimate]: at m = 3 the payout ratio moves as
        // s_big^α; fit α from the measured γ grid, then ask which γ keeps
        // the ratio ≤ 1 if the true β were higher than the simulated one.
        {
            let ds: Vec<&Done> = done
                .iter()
                .filter(|d| d.cfg.faction_weights == [3, 1, 1, 1, 1, 1])
                .collect();
            let r = |gi: usize| {
                mean(
                    &ds.iter()
                        .map(|d| herding_ratio(&d.outs[gi + 1]).0)
                        .collect::<Vec<_>>(),
                )
            };
            let hb = |gi: usize| {
                mean(
                    &ds.iter()
                        .map(|d| fx(d.outs[gi + 1].herd[0]))
                        .collect::<Vec<_>>(),
                )
            };
            let r0 = r(0);
            // γ = 1 is index 4 on the grid.
            let alpha = (r(4) / r0).ln() / hb(4).ln();
            let beta_sim = mean(
                &ds.iter()
                    .map(|d| beta(&d.outs[0], 4, 3.0))
                    .collect::<Vec<_>>(),
            );
            writeln!(
                s,
                "\n### C.4 If real players scale better than the simulated ones [estimate]\n\nAt m = 3 the payout ratio moves as h_big^α with α = {alpha:.2} (fitted from the γ grid; α is between ½ for the citizen pool's √s and 1 for the laurel pool's s). With the simulated β = {beta_sim:.2} and ratio {r0:.3} at γ = 0, a population with a higher β needs γ ≥ (ln r0 + α (β − β_sim) ln 3) / (α ln 2.25):\n"
            )
            .unwrap();
            writeln!(s, "| assumed β | minimum γ for ratio ≤ 1.0 at m = 3 |").unwrap();
            writeln!(s, "|---|---|").unwrap();
            for b in [1.0, 1.2, 1.4, 1.6] {
                let g = (r0.ln() + alpha * (b - beta_sim) * 3f64.ln()) / (alpha * 2.25f64.ln());
                writeln!(s, "| {b:.1} | {:.2} |", g.max(0.0)).unwrap();
            }
            let raws: Vec<f64> = done
                .iter()
                .flat_map(|d| d.outs[0].raw.iter().map(|&x| fx(x)))
                .collect();
            let (lo, hi) = range(&raws);
            writeln!(
                s,
                "\nThe undamped index stays within {lo:.3}–{hi:.3} in every herding run, inside the clamp [0.5, 2], so the clamp never binds here."
            )
            .unwrap();
        }
        match chosen {
            Some((a, b)) => writeln!(
                s,
                "\n**Smallest γ on the grid with the ratio ≤ 1.0 at m = 3 in every seed: γ = {}/{} = {:.2}.**\n",
                a,
                b,
                a as f64 / b as f64
            )
            .unwrap(),
            None => writeln!(s, "\n**No γ on the grid keeps the ratio ≤ 1.0 at m = 3 in every seed.**\n").unwrap(),
        }
    }

    // ---- §D bots
    if want("bots") {
        let shares = [0.01, 0.02, 0.05, 0.10, 0.20, 0.30, 0.50];
        let mut jobs = Vec::new();
        for (vi, (q, ag)) in [(None, None), (Some(0.9), Some(0.8))].iter().enumerate() {
            for &b in &shares {
                if vi == 1 && !matches!(b, 0.05 | 0.10 | 0.30) {
                    continue;
                }
                for k in 1..=seeds {
                    jobs.push(Job {
                        cfg: Config {
                            seed: 200 + k,
                            bot_share: b,
                            bot_q: *q,
                            bot_aggression: *ag,
                            ..base.clone()
                        },
                        gammas: vec![],
                    });
                }
            }
        }
        let done = run_all(jobs);
        let (ok, n) = all_pass(&done);
        checks_ok += ok;
        checks_n += n;
        writeln!(s, "## D. Scripted bots: return vs bot share\n").unwrap();
        writeln!(
            s,
            "Bot share of all wallets; the rest is the baseline human mix. {} seeds per row, {} wallets. SDK default: decision quality 0.6, aggression 0.5, hourly sessions, never withholds; tuned script: quality 0.9, aggression 0.8 [sim].\n",
            seeds, base.agents
        )
        .unwrap();
        writeln!(s, "| strategy | bot share | bot with stake | (min–max) | bot fee only | very skilled + stake | skilled + stake | daily + stake | daily fee only | casual fee only | bots' share of laurels |").unwrap();
        writeln!(s, "|---|---|---|---|---|---|---|---|---|---|---|").unwrap();
        for (vi, label) in ["SDK default", "tuned script"].iter().enumerate() {
            for &b in &shares {
                let ds: Vec<&Done> = done
                    .iter()
                    .filter(|d| d.cfg.bot_share == b && d.cfg.bot_q.is_some() == (vi == 1))
                    .collect();
                if ds.is_empty() {
                    continue;
                }
                let cell = |arch: Arch, st: bool| {
                    let mut m = Mult::default();
                    for d in &ds {
                        m.merge(&arch_mult(&d.outs[0], arch, st));
                    }
                    m
                };
                let per: Vec<f64> = ds
                    .iter()
                    .map(|d| arch_mult(&d.outs[0], Arch::Bot, true).x())
                    .collect();
                let (lo, hi) = range(&per);
                let lshare: Vec<f64> = ds
                    .iter()
                    .map(|d| {
                        let o = &d.outs[0];
                        let tot: f64 = o
                            .agents
                            .iter()
                            .filter(|a| !a.shade && a.stake > 0)
                            .map(|a| a.laurels as f64)
                            .sum();
                        let bots: f64 = o
                            .agents
                            .iter()
                            .filter(|a| !a.shade && a.stake > 0 && a.arch == Arch::Bot)
                            .map(|a| a.laurels as f64)
                            .sum();
                        bots / tot
                    })
                    .collect();
                writeln!(
                    s,
                    "| {label} | {:.0}% | **{}** | {:.2}–{:.2} | {} | {} | {} | {} | {} | {} | {:.1}% |",
                    b * 100.0,
                    cell(Arch::Bot, true).cell(),
                    lo,
                    hi,
                    cell(Arch::Bot, false).cell(),
                    cell(Arch::VerySkilled, true).cell(),
                    cell(Arch::Skilled, true).cell(),
                    cell(Arch::Daily, true).cell(),
                    cell(Arch::Daily, false).cell(),
                    cell(Arch::Casual, false).cell(),
                    100.0 * mean(&lshare)
                )
                .unwrap();
            }
        }
        // Bots by join day at 5% (the acceptance criterion's cell).
        let ds: Vec<&Outcome> = done
            .iter()
            .filter(|d| d.cfg.bot_share == 0.05 && d.cfg.bot_q.is_none())
            .map(|d| &d.outs[0])
            .collect();
        let g = grid(&ds);
        writeln!(
            s,
            "\nSDK-default bots at 5%, with stake, by join day: {}.\n",
            DAY_BUCKETS
                .iter()
                .enumerate()
                .map(|(i, (_, _, n))| format!("{n} {}", g[Arch::Bot.idx()][1][i].cell()))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
    }

    // ---- §G what drives the bot edge
    if want("decompose") {
        type Tweak = fn(&mut Config);
        let variants: Vec<(&str, Tweak)> = vec![
            ("K3 economy (baseline)", |_| {}),
            ("Relic Sites pay again (rev2)", |c| c.relics = true),
            ("every holding emits 1/12 (rev2)", |c| {
                c.emission = Emission::Full
            }),
            ("holdings 2-3 do not emit", |c| {
                c.emission = Emission::FirstOnly
            }),
            ("140 Works per USDC (rev2)", |c| {
                c.payout.works_per_usdc = 140
            }),
            ("Works cap 60/day", |c| c.works_cap = 60),
            ("stake priced by days left (D3)", |c| c.stake_ramp_bps = 0),
            ("Mandate reserve to every completer (M0)", |c| {
                c.mandate_stakers_only = false
            }),
            ("bots add their stake late (day 21)", |c| {
                c.late_stake = LateStake::Bots
            }),
            ("every staker adds its stake late (day 21)", |c| {
                c.late_stake = LateStake::Stakers
            }),
            ("bots stand for office", |c| c.bot_officers = true),
            ("bots stand for office, no officer ceiling (rev2)", |c| {
                c.bot_officers = true;
                c.payout.office_ceiling_bps = u32::MAX;
            }),
        ];
        let shares = [0.02, 0.05, 0.10];
        let mut jobs = Vec::new();
        for (vi, (_, f)) in variants.iter().enumerate() {
            for &b in &shares {
                for k in 1..=seeds {
                    let mut cfg = Config {
                        seed: 300 + k,
                        bot_share: b,
                        ..base.clone()
                    };
                    f(&mut cfg);
                    cfg.doctrine_rotation = vi; // tag only (doctrines off)
                    jobs.push(Job {
                        cfg,
                        gammas: vec![],
                    });
                }
            }
        }
        let done = run_all(jobs);
        let (ok, n) = all_pass(&done);
        checks_ok += ok;
        checks_n += n;
        writeln!(s, "## G. What drives the bot edge\n").unwrap();
        writeln!(s, "SDK-default bots; each row switches one K3 change back (or adds one behaviour) [sim variants]. {} seeds per cell, {} wallets.\n", seeds, base.agents).unwrap();
        writeln!(s, "| variant | bot share | bot + stake | bot fee only | very skilled + stake | skilled + stake | daily + stake | daily fee only | casual fee only | late-join daily + stake (days 1-21) vs day 0 |").unwrap();
        writeln!(s, "|---|---|---|---|---|---|---|---|---|---|").unwrap();
        for (vi, (label, _)) in variants.iter().enumerate() {
            for &b in &shares {
                let ds: Vec<&Outcome> = done
                    .iter()
                    .filter(|d| d.cfg.doctrine_rotation == vi && d.cfg.bot_share == b)
                    .map(|d| &d.outs[0])
                    .collect();
                let cell = |arch: Arch, st: bool| {
                    let mut m = Mult::default();
                    for o in &ds {
                        m.merge(&arch_mult(o, arch, st));
                    }
                    m.cell()
                };
                let g = grid(&ds);
                let mut late = Mult::default();
                for bk in 1..4 {
                    late.merge(&g[Arch::Daily.idx()][1][bk]);
                }
                writeln!(
                    s,
                    "| {label} | {:.0}% | **{}** | {} | {} | {} | {} | {} | {} | {:.2} vs {:.2} |",
                    b * 100.0,
                    cell(Arch::Bot, true),
                    cell(Arch::Bot, false),
                    cell(Arch::VerySkilled, true),
                    cell(Arch::Skilled, true),
                    cell(Arch::Daily, true),
                    cell(Arch::Daily, false),
                    cell(Arch::Casual, false),
                    late.x(),
                    g[Arch::Daily.idx()][1][0].x()
                )
                .unwrap();
            }
        }
        writeln!(s).unwrap();
    }

    // ---- §K the O4 ladder and the O3 alternatives
    if want("ladder") {
        let mk = |f: &dyn Fn(&mut Config)| {
            let mut c = base.clone();
            f(&mut c);
            c
        };
        let m0 = |c: &mut Config| c.set_rev2_economy();
        let k3base = |c: &mut Config| {
            c.set_rev2_economy();
            c.mandate_stakers_only = true;
            c.payout.office_ceiling_bps = 9_500;
        };
        let ladder: Vec<(String, Config)> = vec![
            ("0. M0 economy (rev2)".into(), mk(&m0)),
            (
                "1. + O10 Mandate to staker completers + O3 ceiling 95%".into(),
                mk(&k3base),
            ),
            (
                "2. step 1: + 70 Works per USDC".into(),
                mk(&|c| {
                    k3base(c);
                    c.payout.works_per_usdc = 70;
                }),
            ),
            (
                "2b. step 1 alt: 35 Works per USDC".into(),
                mk(&|c| {
                    k3base(c);
                    c.payout.works_per_usdc = 35;
                }),
            ),
            (
                "3. step 2: + stake priced by accrual left (ramp 2.0)".into(),
                mk(&|c| {
                    k3base(c);
                    c.payout.works_per_usdc = 70;
                    c.stake_ramp_bps = 20_000;
                }),
            ),
            (
                "4. step 3: + order-weighted emission".into(),
                mk(&|c| {
                    k3base(c);
                    c.payout.works_per_usdc = 70;
                    c.stake_ramp_bps = 20_000;
                    c.emission = Emission::OrderWeighted;
                }),
            ),
            (
                "5. step 4: + no Relic Site laurels".into(),
                mk(&|c| {
                    k3base(c);
                    c.payout.works_per_usdc = 70;
                    c.stake_ramp_bps = 20_000;
                    c.emission = Emission::OrderWeighted;
                    c.relics = false;
                }),
            ),
            (
                "6. back-off: steps 2-4 with 140 Works per USDC".into(),
                mk(&|c| {
                    c.payout.works_per_usdc = 140;
                }),
            ),
            (
                "7. back-off: step 4 alone (140, D3 stake, full emission)".into(),
                mk(&|c| {
                    c.payout.works_per_usdc = 140;
                    c.stake_ramp_bps = 0;
                    c.emission = Emission::Full;
                }),
            ),
            (
                "8. back-off: 70 Works + step 4 (no ramp, full emission)".into(),
                mk(&|c| {
                    c.payout.works_per_usdc = 70;
                    c.stake_ramp_bps = 0;
                    c.emission = Emission::Full;
                }),
            ),
            (
                "9. **K3 choice**: steps 2-4 with 105 Works per USDC".into(),
                base.clone(),
            ),
        ];
        let rows = criterion_rows(base, seeds, &ladder, &[false, true]);
        writeln!(s, "## K. The bot criterion, step by step (O4) \n").unwrap();
        writeln!(s, "SDK-default staking bots at 1, 2, 5 and 10% of wallets, {} seeds per cell (seeds 201..), {} wallets; each economy is run with bots barred from office and with bots standing for office (the most engaged wallet wins, as in M0 §G). Steps are cumulative in the owner's order (O4); rows 6-9 back steps off to find the passing set that costs honest players least. The honest cells are the same seasons' multiples. \"swept\" is what the officer-pay ceiling (and the 5× cap) carries to the next season [sim].\n", seeds, base.agents).unwrap();
        writeln!(s, "{}", criterion_table(&rows)).unwrap();
        let alts: Vec<(String, Config)> = vec![
            (
                "a. no bound (rev2 rows)".into(),
                mk(&|c| c.payout.office_ceiling_bps = u32::MAX),
            ),
            (
                "b. rows capped at 25% of what the officer paid".into(),
                mk(&|c| {
                    c.payout.office_ceiling_bps = u32::MAX;
                    c.office_pay = OfficePay::ShareOfPaid(2_500);
                }),
            ),
            (
                "c. laurels from the Mandate budget (Minister 2, Warden 1 shares)".into(),
                mk(&|c| {
                    c.payout.office_ceiling_bps = u32::MAX;
                    c.office_pay = OfficePay::Laurels;
                }),
            ),
            (
                "d. ceiling 90% of what the wallet paid".into(),
                mk(&|c| c.payout.office_ceiling_bps = 9_000),
            ),
            ("e. **ceiling 95%** (K3 choice)".into(), base.clone()),
            (
                "f. ceiling 100% (break-even)".into(),
                mk(&|c| c.payout.office_ceiling_bps = 10_000),
            ),
        ];
        let rows = criterion_rows(base, seeds, &alts, &[true]);
        writeln!(s, "### K.1 Officer pay (O3): the alternatives on the K3 economy, bots standing for office\n").unwrap();
        writeln!(s, "{}", criterion_table(&rows)).unwrap();
    }

    // ---- §E doctrines
    if want("doctrines") {
        let dseeds = seeds * 20;
        for tuned in [false, true] {
            let mut jobs = Vec::new();
            for rot in 0..6 {
                for k in 1..=dseeds {
                    jobs.push(Job {
                        cfg: Config {
                            seed: 1000 + k * 7 + rot as u64,
                            doctrines: true,
                            doctrines_tuned: tuned,
                            doctrine_rotation: rot,
                            stratified: false,
                            ..base.clone()
                        },
                        gammas: vec![],
                    });
                }
            }
            let done = run_all(jobs);
            let (ok, n) = all_pass(&done);
            checks_ok += ok;
            checks_n += n;
            let set = doctrine_set(tuned);
            writeln!(
                s,
                "## E{}. Doctrines: {}\n",
                if tuned { 2 } else { 1 },
                if tuned {
                    "tuned proposal (no direct multipliers on scored facts)"
                } else {
                    "draft table of design §4.1"
                }
            )
            .unwrap();
            writeln!(
                s,
                "{} seasons: every doctrine in every wedge (6 rotations × {} seeds), {} wallets, equal expected sizes with each wallet's faction drawn at random (so faction composition varies as it would in a real season). \"Win\" = highest s_k at the Reckoning. CI band: 16.7% ± 2 points [sim].\n",
                done.len(),
                dseeds,
                base.agents
            )
            .unwrap();
            writeln!(s, "| Doctrine | win rate | in band | mean undamped index | sd across seasons | Dominion/cap ÷ civ | Prosperity/cap ÷ civ | Knowledge/cap ÷ civ | Concord/cap ÷ civ | claims / paid |").unwrap();
            writeln!(s, "|---|---|---|---|---|---|---|---|---|---|").unwrap();
            let mut wins = [0u32; 6];
            let mut idx = [0f64; 6];
            let mut idx2 = [0f64; 6];
            let mut path = [[0f64; 4]; 6];
            let mut mult = [Mult::default(); 6];
            for d in &done {
                let o = &d.outs[0];
                let best = (0..6)
                    .max_by_key(|&k| (o.index[k], std::cmp::Reverse(k)))
                    .unwrap();
                let dk = |k: usize| (k + d.cfg.doctrine_rotation) % 6;
                wins[dk(best)] += 1;
                let act_all: f64 = o.facts.iter().map(|f| f.active as f64).sum();
                for k in 0..6 {
                    idx[dk(k)] += fx(o.raw[k]);
                    idx2[dk(k)] += fx(o.raw[k]) * fx(o.raw[k]);
                    for p in 0..4 {
                        let all: f64 = o.facts.iter().map(|f| f.path[p] as f64).sum();
                        let pc = o.facts[k].path[p] as f64 / o.facts[k].active.max(1) as f64;
                        path[dk(k)][p] += pc / (all / act_all);
                    }
                }
                for a in o.agents.iter().filter(|a| !a.shade) {
                    mult[dk(a.faction as usize)].add(a);
                }
            }
            let nd = done.len() as f64;
            let mut in_band = 0;
            for k in 0..6 {
                let w = 100.0 * wins[k] as f64 / nd;
                let ok = (w - 100.0 / 6.0).abs() <= 2.0;
                in_band += ok as u32;
                writeln!(
                    s,
                    "| {} | {:.1}% | {} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.4} | {:.3} |",
                    set[k].name,
                    w,
                    if ok { "yes" } else { "**no**" },
                    idx[k] / nd,
                    (idx2[k] / nd - (idx[k] / nd).powi(2)).max(0.0).sqrt(),
                    path[k][0] / nd,
                    path[k][1] / nd,
                    path[k][2] / nd,
                    path[k][3] / nd,
                    mult[k].x()
                )
                .unwrap();
            }
            let se = (1.0f64 / 6.0 * 5.0 / 6.0 / nd).sqrt() * 100.0;
            writeln!(
                s,
                "\n{in_band} of 6 doctrines in the band. Binomial standard error of one win rate at this sample: {se:.1} points.\n"
            )
            .unwrap();
        }
    }

    // ---- §F determinism
    if want("determinism") {
        let cfg = Config {
            seed: 1,
            ..base.clone()
        };
        let a = play(&cfg).1.digest;
        let jobs = vec![
            Job {
                cfg: cfg.clone(),
                gammas: vec![],
            },
            Job {
                cfg: Config {
                    seed: 2,
                    ..cfg.clone()
                },
                gammas: vec![],
            },
        ];
        let done = run_all(jobs);
        let b = done[0].outs[0].digest;
        let c = done[1].outs[0].digest;
        writeln!(s, "## F. Determinism\n").unwrap();
        writeln!(
            s,
            "Seed 1 on the main thread: `{}`; seed 1 on a worker thread: `{}` ({}); seed 2: `{}` (differs: {}).\n",
            hex(&a),
            hex(&b),
            if a == b { "identical" } else { "**DIFFERENT**" },
            hex(&c),
            a != c
        )
        .unwrap();
    }

    writeln!(
        s,
        "## Conservation over the whole suite\n\nEvery settlement (each season × each γ) ran all conservation checks: **{checks_ok} of {checks_n} passed every check**.\n\nSuite wall time [measured]: {:.0} s.",
        t0.elapsed().as_secs_f64()
    )
    .unwrap();
    s
}

fn stats_line(d: &Done) -> String {
    let st = &d.stats;
    format!(
        "Final ring {}, rings opened {}; sessions {}; clashes {} ({} engagements, {} kernel refusals); sieges declared {} / completed {} / failed {} (never held {}, lost {}); occupations {}, liberations {}, captures {}, Free City captures {}; camps beaten {}; routs {}; Disarray postures {}; Engine stages {} on days {:?}; Relic Sites {} minting {:.0} laurels; dormancies {}, first holdings released {}; 2nd/3rd holdings founded {}; withdrawn joins {}; Mandate shares {} (fee-only completions without a share {}), Mandate laurels paid {:.0}, left in reserve {:.0}; office-terms Minister {} / paid Warden {}.\n",
        st.final_ring,
        st.rings_opened,
        st.sessions,
        st.clashes,
        st.engagements,
        st.clash_errors,
        st.sieges_declared,
        st.sieges_completed,
        st.sieges_failed,
        st.fail_never_held,
        st.fail_lost,
        st.occupations,
        st.liberations,
        st.captures,
        st.free_city_captures,
        st.camps_won,
        st.routs,
        st.disarray,
        d.engine_stages,
        st.engine_stage_days,
        st.relics_spawned,
        lau(st.relic_minted),
        st.dormancies,
        st.releases,
        st.second_holdings,
        st.withdrawn_joins,
        st.mandate_shares,
        st.mandate_unshared,
        lau(st.mandate_paid),
        lau(st.mandate_left),
        st.minister_terms,
        st.warden_terms,
    )
}

/// The bot criterion of O4 (and O3's office variant) for a list of
/// configurations: SDK-default staking bots at 1, 2, 5 and 10% of wallets,
/// with bots barred from office and with bots standing for office, plus
/// the honest archetypes' multiples in the same seasons [sim].
pub const CRITERION_SHARES: [f64; 4] = [0.01, 0.02, 0.05, 0.10];

pub struct CriterionRow {
    pub label: String,
    pub offices: bool,
    pub share: f64,
    pub bot_stake: Mult,
    pub bot_stake_max: f64,
    pub cells: Vec<Mult>,
    pub swept_pct: f64,
}

/// Honest cells reported next to the bot: (label, archetype, staker).
pub const HONEST_CELLS: [(&str, Arch, bool); 8] = [
    ("bot fee only", Arch::Bot, false),
    ("very skilled + stake", Arch::VerySkilled, true),
    ("skilled + stake", Arch::Skilled, true),
    ("daily + stake", Arch::Daily, true),
    ("daily fee only", Arch::Daily, false),
    ("casual + stake", Arch::Casual, true),
    ("casual fee only", Arch::Casual, false),
    ("idle fee only", Arch::Idle, false),
];

pub fn criterion_rows(
    base: &Config,
    seeds: u64,
    variants: &[(String, Config)],
    offices: &[bool],
) -> Vec<CriterionRow> {
    let mut jobs = Vec::new();
    let mut tags = Vec::new();
    for (vi, (_, cfg)) in variants.iter().enumerate() {
        for &off in offices {
            for &b in &CRITERION_SHARES {
                for k in 1..=seeds {
                    jobs.push(Job {
                        cfg: Config {
                            seed: 200 + k,
                            bot_share: b,
                            bot_officers: off,
                            agents: base.agents,
                            ..cfg.clone()
                        },
                        gammas: vec![],
                    });
                    tags.push((vi, off, b));
                }
            }
        }
    }
    let done = run_all(jobs);
    let mut rows = Vec::new();
    for (vi, (label, _)) in variants.iter().enumerate() {
        for &off in offices {
            for &b in &CRITERION_SHARES {
                let ds: Vec<&Done> = done
                    .iter()
                    .zip(&tags)
                    .filter(|(_, t)| **t == (vi, off, b))
                    .map(|(d, _)| d)
                    .collect();
                let mut bot = Mult::default();
                let mut mx = f64::MIN;
                let mut cells = vec![Mult::default(); HONEST_CELLS.len()];
                let (mut swept, mut prize) = (0u128, 0u128);
                for d in &ds {
                    let o = &d.outs[0];
                    assert!(o.checks.iter().all(|c| c.ok), "conservation failed");
                    let m = arch_mult(o, Arch::Bot, true);
                    mx = mx.max(m.x());
                    bot.merge(&m);
                    for (i, (_, a, st)) in HONEST_CELLS.iter().enumerate() {
                        cells[i].merge(&arch_mult(o, *a, *st));
                    }
                    swept += o.ledger.swept as u128;
                    prize += o.ledger.prize as u128;
                }
                rows.push(CriterionRow {
                    label: label.clone(),
                    offices: off,
                    share: b,
                    bot_stake: bot,
                    bot_stake_max: mx,
                    cells,
                    swept_pct: 100.0 * swept as f64 / prize.max(1) as f64,
                });
            }
        }
    }
    rows
}

pub fn criterion_table(rows: &[CriterionRow]) -> String {
    let mut s = String::new();
    write!(
        s,
        "| variant | bots in office | bot share | **bot + stake** | max seed |"
    )
    .unwrap();
    for (l, _, _) in HONEST_CELLS {
        write!(s, " {l} |").unwrap();
    }
    writeln!(s, " swept % of prize |").unwrap();
    writeln!(
        s,
        "|---|---|---|---|---|{}---|",
        "---|".repeat(HONEST_CELLS.len())
    )
    .unwrap();
    for r in rows {
        write!(
            s,
            "| {} | {} | {:.0}% | **{:.3}** | {:.3} |",
            r.label,
            if r.offices { "yes" } else { "no" },
            r.share * 100.0,
            r.bot_stake.x(),
            r.bot_stake_max
        )
        .unwrap();
        for c in &r.cells {
            write!(s, " {} |", c.cell()).unwrap();
        }
        writeln!(s, " {:.2}% |", r.swept_pct).unwrap();
    }
    // Verdict per variant.
    let mut labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
    labels.dedup();
    writeln!(s).unwrap();
    for l in labels {
        for off in [false, true] {
            let rs: Vec<&CriterionRow> = rows
                .iter()
                .filter(|r| r.label == l && r.offices == off)
                .collect();
            if rs.is_empty() {
                continue;
            }
            let worst = rs.iter().map(|r| r.bot_stake.x()).fold(f64::MIN, f64::max);
            writeln!(
                s,
                "- {l}, bots {} office: worst pooled bot + stake {:.3} → **{}**",
                if off { "in" } else { "barred from" },
                worst,
                if worst < 1.0 {
                    "passes (< 1.0 at 1, 2, 5, 10%)"
                } else {
                    "fails"
                }
            )
            .unwrap();
        }
    }
    s
}

/// β for path p (0..4) or the undamped index (p = 4), faction 0 vs 1..6.
fn beta(o: &Outcome, p: usize, m: f64) -> f64 {
    let r = if p == 4 {
        let small = (1..6).map(|k| o.raw[k] as f64).sum::<f64>() / 5.0;
        o.raw[0] as f64 / small
    } else {
        let big = o.facts[0].path[p] as f64 / o.facts[0].active.max(1) as f64;
        let sp: f64 = (1..6).map(|k| o.facts[k].path[p] as f64).sum();
        let sa: f64 = (1..6).map(|k| o.facts[k].active as f64).sum();
        big / (sp / sa)
    };
    let _ = INDEX_ONE;
    1.0 + r.ln() / m.ln()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Emission;

    fn small(seed: u64) -> Config {
        Config {
            seed,
            agents: 600,
            ..Config::default()
        }
    }

    #[test]
    fn a_small_season_conserves_money_and_laurels() {
        let mut rev2 = small(3);
        rev2.set_rev2_economy();
        let cfgs = [
            small(3),
            rev2,
            Config {
                emission: Emission::FirstOnly,
                relics: true,
                ..small(3)
            },
            Config {
                bot_officers: true,
                office_pay: OfficePay::Laurels,
                ..small(3)
            },
            Config {
                bot_officers: true,
                office_pay: OfficePay::ShareOfPaid(2_500),
                ..small(3)
            },
        ];
        for cfg in cfgs {
            let (sim, o) = play(&cfg);
            for c in &o.checks {
                assert!(c.ok, "{:?}: {} ({})", cfg.emission, c.name, c.detail);
            }
            assert!(o.ledger.claimed > 0);
            assert!(sim.stats.mandate_paid > 0);
        }
    }

    #[test]
    fn the_mandate_reserve_pays_only_staker_completers() {
        let (sim, o) = play(&small(4));
        assert!(sim.stats.mandate_unshared > 0 && sim.stats.mandate_shares > 0);
        for a in o.agents.iter().filter(|a| a.stake == 0) {
            assert_eq!(a.earned.mandate, 0, "a fee-only wallet got Mandate pay");
        }
        assert!(o.agents.iter().any(|a| a.earned.mandate > 0));
    }

    #[test]
    fn a_seed_replays_bit_for_bit() {
        let a = play(&small(5)).1.digest;
        let b = run_all(vec![Job {
            cfg: small(5),
            gammas: vec![],
        }])[0]
            .outs[0]
            .digest;
        let c = play(&small(6)).1.digest;
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn doctrines_and_unequal_sizes_settle() {
        let (_, o) = play(&Config {
            doctrines: true,
            stratified: false,
            faction_weights: [3, 1, 1, 1, 1, 1],
            bot_share: 0.2,
            ..small(7)
        });
        assert!(o.checks.iter().all(|c| c.ok));
        assert!(o.facts[0].members > o.facts[1].members);
    }
}
