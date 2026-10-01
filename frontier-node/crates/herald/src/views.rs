//! The live (not per-bell) answers built from the fold's captures:
//! `/h/season`, `/h/province/{P},{Q}/latest` and `/h/me/{wallet}`
//! (contract §8.4, §9.2; the shapes the web page reads are pinned by
//! `permutation-gateway/test/fixtures/frontier/*.json`).

use serde_json::{json, Value};
use solana_address::Address;

use fclient::decode::{Citizen, Frontier, Holding, Province};
use fclient::ports::ChainInfo;
use frontier_abi::addr::{holding_key_of_host, host_id};
use frontier_abi::layout::AccountKind;

use crate::fold::Fold;
use crate::records::B64;
use base64::Engine;

/// What the season record states besides the chain: the cluster name, the
/// drand network the season seals to, the relay's quotas.
#[derive(Clone, Debug)]
pub struct SeasonStatic {
    pub cluster: String,
    pub drand: ChainInfo,
    pub quotas: Value,
}

/// `GET /h/season` (`None` until the Season account is captured).
pub fn season_json(f: &Fold, s: &SeasonStatic) -> Option<Value> {
    let addr = f.season_address();
    let c = f.account(&addr)?;
    let se = f.season.as_ref()?;
    let mut rings: std::collections::BTreeMap<u16, [u8; 32]> = f.st.ring_seeds.clone();
    for d in 0..=se.r_max.min(128) {
        if let Some(rc) = f.account(&f.ctx.ring_seed(d)) {
            // status 2 = seeded; the seed at 40.
            if rc.data.get(18) == Some(&2) {
                if let Some(seed) = rc.data.get(40..72).and_then(|x| x.try_into().ok()) {
                    rings.insert(d, seed);
                }
            }
        }
    }
    // The Frontier account's per-wedge site counters (§5.9 overflow rule: a wedge is
    // full when WEDGE_OPEN − WEDGE_OCCUPIED = 0). The page's automatic site ticket reads
    // them (DECISIONS V2); null until the account is folded.
    let frontier = f
        .account(&f.ctx.frontier())
        .and_then(|c| Frontier::decode(&c.data).ok());
    let (wedge_open, wedge_occupied) = match &frontier {
        Some(fr) => (json!(fr.wedge_open), json!(fr.wedge_occupied)),
        None => (Value::Null, Value::Null),
    };
    Some(json!({
        "v": 1,
        "programId": f.cfg.program.to_string(),
        "cluster": s.cluster,
        "season": f.cfg.season_id.to_string(),
        "seasonAddress": Address::new_from_array(addr).to_string(),
        "genesisTs": se.genesis_ts,
        "bellSecs": se.bell_secs,
        "W": se.reveal_window,
        "windowNext": se.window_next,
        "windowFromBell": se.window_from_bell,
        "delta": se.seed_margin,
        "drand": {
            "chainHash": hex::encode(s.drand.chain_hash),
            "publicKey": hex::encode(s.drand.public_key),
            "period": s.drand.period,
            "genesis": s.drand.genesis_time,
        },
        "rulesetHash": hex::encode(se.ruleset_hash),
        "rMax": se.r_max,
        "wedgeOpen": wedge_open,
        "wedgeOccupied": wedge_occupied,
        "rings": rings.iter().map(|(d, seed)| json!({"d": d, "seed": hex::encode(seed)})).collect::<Vec<_>>(),
        "tipPriorityMilli": se.min_reveal_priority_milli,
        "revealCuLimit": se.reveal_cu_limit,
        "revealLoadedLimit": se.reveal_loaded_limit,
        "marchFee": se.march_fee.to_string(),
        "sealBond": se.seal_bond.to_string(),
        "quotas": s.quotas,
        "headSeq": f.st.events.to_string(),
        "latestSlot": f.live.map_or(f.st.last_slot, |l| l.0.max(f.st.last_slot)),
        "latestUnix": f.live.map_or(f.st.last_time, |l| l.1),
        "slot": c.slot,
        "bytes_b64": B64.encode(&c.data),
    }))
}

/// `GET /h/province/{P},{Q}/latest`: the current Province bytes; `bell` is
/// the next bell it resolves, with that bell's slots, day and inputs as
/// they stand.
pub fn province_latest(f: &Fold, p: i16, q: i16) -> Option<Value> {
    let c = f.account(&f.ctx.province(p as i32, q as i32))?;
    let pv = Province::decode(&c.data).ok()?;
    f.envelope((p, q), pv.resolved_next)
}

fn acct(f: &Fold, a: &[u8; 32]) -> Value {
    match f.account(a) {
        Some(c) => {
            let (seq, head) = c.head().unwrap_or((0, [0; 32]));
            json!({"address": Address::new_from_array(*a).to_string(), "slot": c.slot,
                "seq": seq.to_string(), "head": hex::encode(head), "bytes_b64": B64.encode(&c.data)})
        }
        None => Value::Null,
    }
}

/// `GET /h/me/{wallet}`: the Citizen, its ≤ 3 Holdings (bytes), their
/// hosts wherever they stand, transit records, open ArrivalSlots, settled
/// seal codes, and the relay quota (`quota`, fetched by the caller).
pub fn me_json(f: &Fold, wallet: &Address, quota: Value) -> Value {
    let ca = f.ctx.citizen(&wallet.to_bytes());
    let citizen = f.account(&ca).and_then(|c| Citizen::decode(&c.data).ok());
    let mut holdings = vec![];
    let mut keys = vec![];
    let mut transits = vec![];
    if let Some(ci) = &citizen {
        for s in ci.holding.iter().take(ci.holdings_n.min(3) as usize) {
            let ha = f.ctx.holding(s.p as i32, s.q as i32, s.site);
            holdings.push(acct(f, &ha));
            if let Some(hk) = host_id(s.p as i32, s.q as i32, s.site, s.gen, 0) {
                keys.push(holding_key_of_host(hk));
            }
            if let Some(h) = f.account(&ha).and_then(|c| Holding::decode(&c.data).ok()) {
                for (i, t) in h.transit.iter().enumerate().filter(|(_, t)| t.state != 0) {
                    transits.push(json!({
                        "holding": [s.p, s.q, s.site], "transitSlot": i, "state": t.state,
                        "host": t.host_id.to_string(), "departBell": t.depart_bell, "arriveBell": t.arrive_bell,
                        "sealRoot": hex::encode(t.seal_root), "tip": t.tip.to_string(),
                    }));
                }
            }
        }
    }
    let mut hosts = vec![];
    let mut slots = vec![];
    if !keys.is_empty() {
        for c in f.st.accounts.values() {
            match c
                .data
                .get(..8)
                .and_then(|m| AccountKind::from_magic(m.try_into().ok()?))
            {
                Some(AccountKind::Province) => {
                    let Ok(pv) = Province::decode(&c.data) else {
                        continue;
                    };
                    for e in pv.entries.iter().filter(|e| e.state != 0) {
                        if keys.contains(&holding_key_of_host(e.id)) {
                            hosts.push(json!({
                                "id": e.id.to_string(), "province": [pv.p, pv.q], "state": e.state,
                                "faction": e.faction, "unit": e.unit, "tile": e.tile, "troops": e.troops,
                                "readyBell": e.ready_bell, "fromBell": e.from_bell,
                            }));
                        }
                    }
                }
                Some(AccountKind::ArrivalSlot) => {
                    let tag = citizen.as_ref().map(|c| c.citizen_tag);
                    let st = c
                        .data
                        .get(40..48)
                        .map(|b| u64::from_le_bytes(b.try_into().unwrap_or_default()));
                    if tag.is_some() && st == tag {
                        let key =
                            crate::records::account_key(AccountKind::ArrivalSlot, &c.data, "");
                        slots.push(
                            json!({"key": key, "slot": c.slot, "bytes_b64": B64.encode(&c.data)}),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    let mut seals = vec![];
    for k in &keys {
        for (h, s) in f.st.seals.range(*k..=(*k | 0xFFFF_FFFF)) {
            seals.push(json!({"host": h.to_string(), "outcome": s.outcome, "code": s.code, "bell": s.bell}));
        }
    }
    json!({
        "v": 1,
        "wallet": wallet.to_string(),
        "citizen": acct(f, &ca),
        "holdings": holdings,
        "hosts": hosts,
        "transits": transits,
        "slots": slots,
        "seals": seals,
        "quota": quota,
        "latestSlot": f.st.last_slot,
    })
}
