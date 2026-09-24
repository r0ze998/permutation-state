//! Record a bot match for on-chain replay (permutation-chain/bench).
//!
//!     cargo run --release --bin ticklog -- out.bin
//!
//! Writes the genesis state root, then per tick: u32 length + borsh
//! `Vec<OrderBatch>` + the expected state root after the tick; and the
//! borsh genesis state to `out.bin.genesis`. Seeds and entries match the
//! program's dev genesis (6 civs, world [7;32], season [9;32]).
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::orders::OrderBatch;
use permutation_rules::state::{CivId, DeclaredKind};
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::{Preset, Ruleset};
use permutation_server::bots::{Bot, PERSONAS};
use permutation_server::fog::Fog;
use std::io::Write;
fn main() {
    let out = std::env::args().nth(1).unwrap();
    let rules = Ruleset::new(Preset::Blitz);
    let entries: Vec<Entry> = (0..6u8).map(|i| Entry { name: format!("civ-{i}"), declared_kind: DeclaredKind::Agent, payout_wallet: [i + 1; 32], exchange_deposit: 20_000_000 }).collect();
    let mut s = new_season(&rules, &[7; 32], &[9; 32], &entries).unwrap();
    let mut f = std::fs::File::create(out).unwrap();
    f.write_all(&s.state_root().unwrap()).unwrap();
    let g = borsh::to_vec(&s).unwrap();
    std::fs::write(format!("{}.genesis", std::env::args().nth(1).unwrap()), &g).unwrap();
    let mut bots: Vec<Bot> = (0..6).map(|i| Bot::new(i as CivId, PERSONAS[i])).collect();
    let mut fog = Fog::new(&s);
    while s.tick < rules.ticks_per_season {
        let batches: Vec<OrderBatch> = bots.iter_mut().map(|b| { let v = fog.belief(&s, b.civ); OrderBatch { civ: b.civ, tick: s.tick, decision_digest: [s.tick as u8; 32], orders: b.orders(&v, &rules, &fog.memory(b.civ).explored) } }).collect();
        let bytes = borsh::to_vec(&batches).unwrap();
        let mut vrf = [0u8; 32]; vrf[..2].copy_from_slice(&s.tick.to_le_bytes());
        resolve_tick(&mut s, &rules, &TickInput { vrf, batches }).unwrap();
        fog.update(&s);
        f.write_all(&(bytes.len() as u32).to_le_bytes()).unwrap();
        f.write_all(&bytes).unwrap();
        f.write_all(&s.state_root().unwrap()).unwrap();
    }
    eprintln!("wrote {} ticks", s.tick);
}
