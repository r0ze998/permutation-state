//! Balance measurement for Game Design V5: many Blitz seasons of AI-run
//! nations, one line per season and a summary against the calibration
//! targets of V5 §6.5.
//!
//!     cargo run --release --bin sim -- [seeds=20] [members=3,3,2,2,1,0]
//!
//! Every nation is run by hosted AI members (`driver`): they stand for
//! office, vote, propose, and the elected officers order within their
//! office. A nation with 0 members is run by the acting official alone and
//! takes no share of the pool. Personas rotate over the start positions
//! with the seed. Every decision is made from the full state (perfect
//! information).
//!
//! Environment (read once, `env.rs`):
//! - `SIM_SET="key=v,v;key=v"`: calibration overrides of the Blitz ruleset.
//! - `SIM_PERSONA=Builder` (etc.): every nation plays that persona.
//! - `SIM_TREASURY=usdc`: every nation's opening treasury (contracts).
//! - `SIM_AI=k` (default 1): the first k members of each nation are the
//!   operator's AI members (V5 §18).
//! - `SIM_BOUNTY=usdc` (default 5 USDC): the bounty per AI home.
//! - `SIM_HOMEAWARE=1`: armies know the others' AI home cities once drawn.
//! - `SIM_FROM=i`: start at seed i (averages still divide by `seeds`).
//! - `SIM_SKIPS=1`: print the most frequent skipped orders per slot.
//! - `SIM_DEBUG="seed:tick"`: check the invariants after every phase of
//!   that tick.
//! - `SIM_ROTATE=k` (diagnostic): turn the genesis world by k × 60°.
//! - `SIM_EQUIV=1` (diagnostic): the rotational equivariance check instead
//!   (`equiv.rs`), turning by `SIM_EQUIV_K` (default 1) × 60°;
//!   `SIM_EQUIV_DEBUG=1` reports the first divergence.

mod env;
mod equiv;
mod season;
mod totals;

use env::Env;
use permutation_rules::rng::Seed;
use season::play;
use totals::Totals;

const ENTRY_FEE: u64 = 10_000_000;

fn seed_bytes(tag: &str, i: u32) -> Seed {
    let mut s = [0u8; 32];
    let t = format!("permutation-state/{tag}/sim-{i:04}");
    s[..t.len().min(32)].copy_from_slice(&t.as_bytes()[..t.len().min(32)]);
    s
}

/// A tick's randomness: the tick, then the seed.
fn tick_vrf(tick: u16, i: u32) -> [u8; 32] {
    let mut vrf = [0u8; 32];
    vrf[..2].copy_from_slice(&tick.to_le_bytes());
    vrf[2..6].copy_from_slice(&i.to_le_bytes());
    vrf
}

/// Each value's share of the total (all 0 when the total is 0).
fn fractions(v: &[u64]) -> Vec<f64> {
    let total: u64 = v.iter().sum();
    v.iter()
        .map(|x| {
            if total == 0 {
                0.0
            } else {
                *x as f64 / total as f64
            }
        })
        .collect()
}

/// Members per nation from a comma-separated argument, or `default`.
fn parse_members(arg: Option<&String>, default: &[usize]) -> Vec<usize> {
    arg.map(|a| a.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_else(|| default.to_vec())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let env = Env::read();
    if env.equiv {
        let seeds: u32 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(5);
        let members = parse_members(args.get(2), &[3, 3, 3, 3, 3, 3]);
        let ok = (0..seeds)
            .filter(|i| equiv::equivariance(*i, &members, &env))
            .count();
        println!(
            "{ok}/{seeds} seasons end the same when the world and the orders are turned by 60°"
        );
        return;
    }
    let seeds: u32 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(20);
    let members = parse_members(args.get(2), &[3, 3, 2, 2, 1, 0]);
    let mut totals = Totals::new(members.len());
    println!("members per nation: {members:?}");
    println!("seed | eras | points | pool share % | tiers (H/P/S/C) | cities | captured held");
    for i in env.from..seeds {
        let mut r = play(i, &members, &env);
        totals.add(i, &mut r);
        println!(
            "{i:4} | {:?} | {:?} | {:?} | {} | {:?} | {:?}",
            r.era,
            r.points,
            r.share
                .iter()
                .map(|x| (x * 100.0).round() as u32)
                .collect::<Vec<_>>(),
            r.tiers
                .iter()
                .map(|t| format!("{}{}{}{}", t[0], t[1], t[2], t[3]))
                .collect::<Vec<_>>()
                .join(" "),
            r.cities,
            r.captured,
        );
    }
    totals.print(seeds, &env);
}
