//! Frontier host balance simulator (open-world design §12 M0).
//!
//! ```text
//! frontier-sim run   [--agents N] [--seed S] [--sizes 3,1,1,1,1,1] [--bots 0.05]
//!                    [--doctrines] [--rotation K] [--gamma 3/5] [--day0 0.6]
//! frontier-sim suite [--agents N] [--seeds K] [--out PATH]
//! frontier-sim doctrines [--agents N] [--seeds K] [--set kernel|draft|m0]
//!                    [--dx "C.arrival=10500,D.drill=10500"] [--unpaired]
//!                    [--first-seed N] [--gate]
//!                    [--gate-index 0.12]   (exit 1 if a mean index is off by more)
//! frontier-sim doctrine-gate [--set kernel|draft|m0] [--controls]
//!                    (the CI gate's harness; --controls: the negative controls must fail)
//! frontier-sim criterion [--seeds K] [--rev2-economy] [economy knobs]   (bot criterion, O4)
//! frontier-sim c4 [--agents 50000] [--seeds K] [--first-seed N] [--out PATH]
//!                    (restated C4: per-bell participation, tail episodes and
//!                    the counterfactual value of one attacked bell)
//! frontier-sim criterion --best-response [--seeds K] [--gate]
//!                    (the criterion as the max over the bot's join window,
//!                    stake and office choices; exit 1 with --gate if any ≥ 1.0)
//! ```
//!
//! `run` plays one season and prints its report; `suite` runs every M0
//! measurement and writes the markdown results.

mod balance;
mod c4;
mod config;
mod model;
mod report;
mod rng;
mod settle;
mod sim;
mod suite;

use config::Config;
use permutation_rules::frontier::index::IndexParams;

fn parse_gamma(s: &str) -> IndexParams {
    let (n, d) = s.split_once('/').unwrap_or((s, "1"));
    IndexParams {
        gamma_num: n.parse().expect("gamma numerator"),
        gamma_den: d.parse().expect("gamma denominator"),
        ..IndexParams::REV2
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("run");
    let mut cfg = Config::default();
    let mut seeds = 3u64;
    let mut out: Option<String> = None;
    let mut only: Option<String> = None;
    let mut paired = true;
    let mut gate = false;
    let mut first_seed = 0u64;
    let mut best = false;
    let mut gate_index: Option<f64> = None;
    let mut controls = false;
    let mut i = 2;
    while i < args.len() {
        let v = args.get(i + 1).cloned().unwrap_or_default();
        match args[i].as_str() {
            "--agents" => cfg.agents = v.parse().expect("--agents"),
            "--seed" => cfg.seed = v.parse().expect("--seed"),
            "--seeds" => seeds = v.parse().expect("--seeds"),
            "--bots" => cfg.bot_share = v.parse().expect("--bots"),
            "--day0" => cfg.day0_share = v.parse().expect("--day0"),
            "--rotation" => cfg.doctrine_rotation = v.parse().expect("--rotation"),
            "--bot-q" => cfg.bot_q = Some(v.parse().expect("--bot-q")),
            "--bot-aggression" => cfg.bot_aggression = Some(v.parse().expect("--bot-aggression")),
            "--works-cap" => cfg.works_cap = v.parse().expect("--works-cap"),
            "--gamma" => cfg.index = parse_gamma(&v),
            "--works-per-usdc" => cfg.payout.works_per_usdc = v.parse().expect("--works-per-usdc"),
            "--stake-ramp" => cfg.stake_ramp_bps = v.parse().expect("--stake-ramp"),
            "--office-ceiling" => {
                cfg.payout.office_ceiling_bps = if v == "none" {
                    u32::MAX
                } else {
                    v.parse().expect("--office-ceiling")
                }
            }
            "--office-pay" => {
                cfg.office_pay = match v.split_once(':') {
                    Some(("share", b)) => config::OfficePay::ShareOfPaid(b.parse().expect("bps")),
                    _ if v == "usdc" => config::OfficePay::Usdc,
                    _ if v == "laurels" => config::OfficePay::Laurels,
                    _ => panic!("--office-pay {v}"),
                }
            }
            "--out" => out = Some(v),
            "--only" => only = Some(v),
            "--sizes" => {
                let w: Vec<u32> = v.split(',').map(|x| x.parse().expect("--sizes")).collect();
                cfg.faction_weights.copy_from_slice(&w[..6]);
            }
            "--set" => {
                cfg.doctrines = true;
                cfg.doctrine_set = model::DoctrineSet::parse(&v);
            }
            "--dx" => {
                cfg.doctrines = true;
                cfg.doctrine_tweaks = v;
            }
            "--first-seed" => first_seed = v.parse().expect("--first-seed"),
            "--unpaired" => {
                paired = false;
                i += 1;
                continue;
            }
            "--gate-index" => gate_index = Some(v.parse().expect("--gate-index")),
            "--controls" => {
                controls = true;
                i += 1;
                continue;
            }
            "--best-response" => {
                best = true;
                i += 1;
                continue;
            }
            "--bot-mandates" => cfg.bot_mandates = Some(v.parse().expect("--bot-mandates")),
            "--office-term-limit" => {
                cfg.office_term_limit = Some(v.parse().expect("--office-term-limit"))
            }
            "--bot-join-days" => {
                let (lo, hi) = v.split_once('-').unwrap_or((&v, &v));
                cfg.bot_join_days = Some((lo.parse().expect("day"), hi.parse().expect("day")));
            }
            "--gate" => {
                gate = true;
                i += 1;
                continue;
            }
            "--doctrines" => {
                cfg.doctrines = true;
                i += 1;
                continue;
            }
            "--emission" => {
                cfg.emission = match v.as_str() {
                    "full" => config::Emission::Full,
                    "first-only" => config::Emission::FirstOnly,
                    "order-weighted" => config::Emission::OrderWeighted,
                    x => panic!("--emission {x}"),
                }
            }
            "--late-stake" => {
                cfg.late_stake = match v.as_str() {
                    "none" => config::LateStake::None,
                    "bots" => config::LateStake::Bots,
                    "stakers" => config::LateStake::Stakers,
                    x => panic!("--late-stake {x}"),
                }
            }
            "--bot-officers" => {
                cfg.bot_officers = true;
                i += 1;
                continue;
            }
            "--no-mandate-floor" => {
                cfg.mandate_floor = false;
                i += 1;
                continue;
            }
            "--mandate-all" => {
                cfg.mandate_stakers_only = false;
                i += 1;
                continue;
            }
            "--relics" => {
                cfg.relics = true;
                i += 1;
                continue;
            }
            "--rev2-economy" => {
                cfg.set_rev2_economy();
                i += 1;
                continue;
            }
            "--no-relics" => {
                cfg.relics = false;
                i += 1;
                continue;
            }
            "--verbose" => {
                cfg.verbose = true;
                i += 1;
                continue;
            }
            other => panic!("unknown argument {other}"),
        }
        i += 2;
    }
    match cmd {
        "run" => {
            let t = std::time::Instant::now();
            let (sim, o) = suite::play(&cfg);
            println!("{}", suite::run_report(&sim, &o, t.elapsed().as_secs_f64()));
        }
        "suite" => {
            let text = suite::suite(&cfg, seeds, only.as_deref());
            match out {
                Some(p) => {
                    std::fs::write(&p, &text).expect("write results");
                    eprintln!("wrote {p}");
                }
                None => println!("{text}"),
            }
        }
        "doctrines" => {
            let spec = balance::Spec {
                agents: cfg.agents,
                seeds,
                first_seed,
                set: cfg.doctrine_set,
                tweaks: cfg.doctrine_tweaks.clone(),
                paired,
            };
            let b = balance::run(&cfg, &spec);
            let text = balance::table(&b);
            match out {
                Some(p) => {
                    std::fs::write(&p, &text).expect("write results");
                    eprintln!("wrote {p}");
                }
                None => println!("{text}"),
            }
            if gate && b.in_band < 6 {
                eprintln!("gate failed: {} of 6 doctrines in the band", b.in_band);
                std::process::exit(1);
            }
            if let Some(x) = gate_index {
                if let Err(e) = balance::gate_check_with(&b, x, 100.0) {
                    eprintln!("index gate failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        "doctrine-gate" => {
            let b = balance::gate_run(cfg.doctrine_set);
            println!("{}", balance::table(&b));
            if let Err(e) = balance::gate_check(&b) {
                eprintln!("gate failed: {e}");
                std::process::exit(1);
            }
            if controls {
                // The negative controls must each fail the gate.
                for (name, set, dx, seeds) in [
                    (
                        "draft",
                        model::DoctrineSet::Draft,
                        "",
                        balance::GATE_DRAFT_SEEDS,
                    ),
                    (
                        "Knight",
                        model::DoctrineSet::Kernel,
                        balance::GATE_KNIGHT,
                        balance::GATE_SEEDS,
                    ),
                    (
                        "A boost",
                        model::DoctrineSet::Kernel,
                        balance::GATE_A_BOOST,
                        balance::GATE_SEEDS,
                    ),
                ] {
                    let b = balance::gate_run_with(set, dx, seeds);
                    println!("### control: {name}\n\n{}", balance::table(&b));
                    match balance::gate_check(&b) {
                        Err(e) => println!("control {name} rejected as it must be: {e}\n"),
                        Ok(()) => {
                            eprintln!(
                                "control {name} PASSED the gate: the gate has lost its power"
                            );
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
        "criterion" if best => {
            let rows = suite::criterion_best(&cfg, seeds, &[false, true]);
            let (text, worst) = suite::criterion_best_table(&rows);
            println!("{text}");
            if gate && worst >= 1.0 {
                std::process::exit(1);
            }
        }
        "criterion" => {
            let rows = suite::criterion_rows(
                &cfg,
                seeds,
                &[("this config".to_string(), cfg.clone())],
                &[false, true],
            );
            println!("{}", suite::criterion_table(&rows));
        }
        "c4" => {
            let spec = c4::Spec {
                agents: cfg.agents,
                seeds,
                first_seed,
            };
            let (md, js) = c4::run(&cfg, &spec);
            match out {
                Some(p) => {
                    std::fs::write(&p, &md).expect("write results");
                    std::fs::write(format!("{p}.json"), &js).expect("write json");
                    eprintln!("wrote {p} and {p}.json");
                }
                None => println!("{md}\n{js}"),
            }
        }
        other => panic!("unknown command {other}"),
    }
}
