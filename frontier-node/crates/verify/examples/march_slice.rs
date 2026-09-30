//! Cuts a verifier input down to the marches of one host (W6T-3, the
//! fixture `w6s7-shielded-march.json.gz`): the season's CreateSeason, and
//! for each settled march of the host its DEPART, the transaction that
//! carried the round `T(arrive)` signature, every failed Reveal of the
//! host, the SettleTransit, and the last writer, before the arrival bell's
//! end, of the Holding and of every Province the path enters (the states
//! V5's §5.11 step-6 judge reads).
//!
//! `cargo run --release -p verify --example march_slice -- IN OUT HOST [ARRIVE …]`

use std::collections::BTreeSet;

use frontier_abi::ix as aix;
use frontier_abi::log::Kind;
use frontier_abi::tags::Ix;
use permutation_rules::frontier::geometry::{locate, ProvinceCoord};
use permutation_rules::hex::{Hex, DIRECTIONS};
use verify_core::{world::World, Input};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let inp = Input::load(std::path::Path::new(&a[1])).expect("load");
    let host: u64 = a[3].parse().expect("host id");
    let only: Vec<u32> = a[4..].iter().map(|s| s.parse().expect("bell")).collect();
    let w = World::parse(
        &inp.cfg.program,
        &inp.txs,
        inp.finals.clone(),
        inp.final_slot,
    );
    let f = verify_core::facts::Facts::build(&w, &inp.cfg);
    let mut keep = BTreeSet::new();
    keep.extend(f.create_tx);
    if let Some(n) = f.created_rec {
        keep.insert(w.recs[n].tx);
    }
    // Every failed Reveal of the host (the per-march count must split them).
    for (i, t) in w.txs.iter().enumerate().filter(|(_, t)| !t.ok) {
        if let Some(x) = t
            .ix(Ix::Reveal.tag())
            .and_then(|i| aix::Reveal::decode(&i.data).ok())
        {
            if fclient::seal::unpack(&x.plain).host_id == host {
                keep.insert(i);
            }
        }
    }
    let last_before = |k: &[u8; 32], at: usize| -> Option<usize> {
        let v = w.posts.get(k)?;
        let i = v.partition_point(|&t| t < at);
        v.get(i.checked_sub(1)?).copied()
    };
    for (n, r) in w.recs.iter().enumerate() {
        if r.kind != Kind::TRANSIT_SETTLED || r.ku64("host_id") != host {
            continue;
        }
        let Some(d) = f.depart_of(&w, host, n, None) else {
            continue;
        };
        let dr = &w.recs[d];
        let arrive = dr.pu32("arrive_bell");
        if !only.is_empty() && !only.contains(&arrive) {
            continue;
        }
        keep.insert(dr.tx);
        keep.insert(r.tx);
        if let Some(s) = f.sigs.get(&f.tlock_round(arrive)).and_then(|v| v.first()) {
            keep.insert(s.tx);
        }
        let end = permutation_rules::frontier::beacon::bell_end(f.genesis_ts, arrive);
        let at = w
            .txs
            .partition_point(|t| t.time < end)
            .min(r.tx)
            .max(dr.tx + 1);
        let hk = f.ctx.holding_of_host(host).expect("host id");
        keep.extend(last_before(&hk, at));
        // The path's provinces, from the SettleTransit's opened plaintext:
        // the Reveals carried it.
        let plain = w
            .txs
            .iter()
            .filter_map(|t| t.ix(Ix::Reveal.tag()))
            .filter_map(|i| aix::Reveal::decode(&i.data).ok())
            .map(|x| fclient::seal::unpack(&x.plain))
            .find(|p| p.host_id == host && p.arrive_bell == arrive)
            .expect("a Reveal attempt with the plaintext");
        let mut h = ProvinceCoord::new(dr.pi32("origin_p"), dr.pi32("origin_q"))
            .tile(dr.pu8("origin_tile"))
            .expect("origin");
        let mut provinces = BTreeSet::new();
        for i in 0..plain.path_len {
            let (dq, dr_) = DIRECTIONS[fclient::seal::step(&plain.path, i) as usize];
            h = Hex::new(h.q + dq, h.r + dr_);
            let (pc, _) = locate(h);
            provinces.insert((pc.p, pc.q));
        }
        for (p, q) in &provinces {
            keep.extend(last_before(&f.ctx.province(*p, *q), at));
        }
        eprintln!(
            "host {host} arrive {arrive}: depart tx {}, settle tx {}, provinces {provinces:?}",
            dr.tx, r.tx
        );
    }
    let out = Input {
        txs: keep.iter().map(|&i| inp.txs[i].clone()).collect(),
        finals: Default::default(),
        provenance: format!(
            "w6-s7 slice (7dcacdf, .so 072b1205): the settled marches of host {host} with their DEPART, T(arrive) carrier, failed Reveals, SettleTransit and the Holding and path Provinces the Reveals read"
        ),
        scenarios: vec![],
        ..inp
    };
    eprintln!("{} transactions", out.txs.len());
    out.save(std::path::Path::new(&a[2])).expect("save");
}
