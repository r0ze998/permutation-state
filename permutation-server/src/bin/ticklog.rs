//! Record an AI season for on-chain replay and compute measurements.
//!
//!     cargo run --release --bin ticklog -- out.bin [members=3,3,2,2,1,0]
//!
//! Writes the state root after registration and the first election, then
//! per tick: u32 length + borsh `TickInput` + the expected state root after
//! the tick; and the borsh opening state to `out.bin.genesis`. Seeds match
//! the program's dev genesis (6 nations, world [7;32], season [9;32]).
use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::{Preset, Ruleset};
use permutation_server::driver::{seat_ai_members, AiSeason, Planner};
use permutation_server::ledger::Ledger;
use std::io::Write;

fn main() {
    let out = std::env::args().nth(1).expect("output path");
    let members: Vec<usize> = std::env::args()
        .nth(2)
        .map(|a| a.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![3, 3, 2, 2, 1, 0]);
    let rules = Ruleset::new(Preset::Blitz);
    let mut s = new_season(&rules, &[7; 32], &[9; 32], &nation_entries(6)).unwrap();
    seat_ai_members(&mut s, &rules, &members).unwrap();
    let mut f = std::fs::File::create(&out).unwrap();
    f.write_all(&s.state_root().unwrap()).unwrap();
    std::fs::write(format!("{out}.genesis"), borsh::to_vec(&s).unwrap()).unwrap();
    let mut season = AiSeason::new(rules, s, Planner::new(6), Ledger::seeded(b"ticklog"));
    let mut largest = 0;
    while !season.over() {
        let mut vrf = [0u8; 32];
        vrf[..2].copy_from_slice(&season.state.tick.to_le_bytes());
        let input = season.plan(vrf);
        let bytes = borsh::to_vec(&input).unwrap();
        let root = season.resolve(&input).unwrap();
        largest = largest.max(borsh::to_vec(&season.state).unwrap().len());
        f.write_all(&(bytes.len() as u32).to_le_bytes()).unwrap();
        f.write_all(&bytes).unwrap();
        f.write_all(&root).unwrap();
    }
    let s = &season.state;
    eprintln!(
        "wrote {} ticks; largest world {} bytes with {} members",
        s.tick,
        largest,
        s.members.len()
    );
}
