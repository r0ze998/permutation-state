//! The keeper's play duties over `localnet` in process (W4-C; M1 contract
//! §5.11, §8.2, §12 Gate W4, §13.3 G7/G12, offchain design §6.5–§6.7).
//!
//! One scenario — a province-bell with eleven marches and a dissolve —
//! runs under five conditions:
//!
//! | test | condition | what it checks |
//! |---|---|---|
//! | `play_bell_pipeline` | one keeper | decrypt at T(b); reveals in rank order (4 slot creations for the full group, no displacement); the outranked and the second arrival of a citizen never revealed and bounced without loss; an owner self-reveal from the bell start; SettleDeparture before the gather; gather, resolve; every transit settled with the logged pair, bad seals with the stock opener's code; no Reveal lands at or after `A + W`; closes after the claim grace; the §21 return; every province caught up |
//! | `lag_gate_in_process` | the origin Province and the origin region's anchor held past the destination's close | the destination's inputs and outcome are byte-identical to the unheld run (the program-level lag gate, G7) |
//! | `crash_injection_every_journal_point` (ignored: 60 runs) | the keeper killed at the n-th journal write of each point × duty kind, restarted from its journal | outcome digests identical; ≤ 1 duplicate version per write in flight at the crash |
//! | `duplicate_keepers_race` | three keepers on one chain | identical outcomes; duplicate versions bounded |
//! | `reveals_stop_at_close_under_hold` | the slots of a group held above the keeper's cap through the close | no Reveal lands at or after `A + W`; the arrivals settle routed; liveness findings recorded |
//! | `claim_after_late_reveal` | the slots held below the cap for a few slots | the Reveal lands late (≥ `lateness_slots`), ClaimDefence pays the formula's refund, the slot is kept for the grace and closed after |
//!
//! The program is the native model (`tests/model/play.rs`) unless
//! `PSF_FRONTIER_SO` names a test-beacon `.so` (W4-A/W4-B's program).

mod common;
mod model;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use base64::Engine as _;
use serde_json::json;
use sha2::{Digest, Sha256};

use fclient::abi::layout as l;
use fclient::addr::citizen_tag_u64;
use fclient::decode::{ArrivalSlot, ClashInputs, Holding, Province};
use fclient::ix::{self, DepartArgs, HoldingRef, Player};
use fclient::seal::{self, Plain};
use fclient::{Address, Keypair, Signer};
use frontier_abi::log::{self as plog, transit_outcome as to, Kind};
use keeper_core::journal::{CrashAt, Journal, INJECTED_CRASH};
use keeper_core::Keeper;
use permutation_rules::frontier::geometry::{locate, tile_offset, ProvinceCoord};
use permutation_rules::hex::{Hex, DIRECTIONS};

use common::*;

type K = Keeper<localnet::InProcess, Gated>;

/// Unit-0 troops in every fixture Holding's reserve.
const RESERVE0: u32 = 10_000;

/// The bell every scenario departs in.
const DEPART_BELL: u32 = 4;

const PLAY_ROLES: [&str; 11] = [
    "beacon",
    "rings",
    "archive",
    "reveal",
    "settle-departure",
    "gather",
    "resolve",
    "skip",
    "settle",
    "close",
    "claims",
];

fn put(d: &mut [u8], o: usize, v: &[u8]) {
    d[o..o + v.len()].copy_from_slice(v);
}

// ------------------------------------------------------------------ fixtures

/// A player with one final Holding and hosts in its province.
struct Pl {
    wallet: Keypair,
    faction: u8,
    home: (i16, i16),
    site: u8,
}

impl Pl {
    fn player(&self) -> Player {
        Player {
            actor: self.wallet.pubkey(),
            payer: self.wallet.pubkey(),
            wallet: self.wallet.pubkey(),
        }
    }
    fn href(&self) -> HoldingRef {
        HoldingRef {
            p: self.home.0,
            q: self.home.1,
            site: self.site,
        }
    }
}

fn write_account(w: &World, k: Address, data: Vec<u8>) {
    w.ip.lock()
        .set_account(
            k,
            fclient::ports::Account {
                lamports: fclient::abi::rent(data.len()),
                data,
                owner: w.program,
                executable: false,
            },
        )
        .unwrap();
}

/// Writes the player's Citizen and final Holding (land is W3's; these
/// tests start from holdings).
fn fixture(w: &World, p: &Pl) {
    let s = w.season();
    let c_addr = w.addrs.citizen(&p.wallet.pubkey());
    let mut c = vec![0u8; fclient::abi::size::CITIZEN];
    put(&mut c, 0, fclient::abi::magic::CITIZEN);
    put(&mut c, 8, &s.h.season_id.to_le_bytes());
    put(&mut c, 16, &1u16.to_le_bytes());
    put(&mut c, l::citizen::WALLET, &p.wallet.pubkey().to_bytes());
    c[l::citizen::FACTION] = p.faction;
    c[l::citizen::FLAGS] = l::citizen::FLAG_JOINED | l::citizen::FLAG_FIRST_FINAL;
    put(
        &mut c,
        l::citizen::CITIZEN_TAG,
        &citizen_tag_u64(&c_addr).to_le_bytes(),
    );
    put(
        &mut c,
        l::citizen::RENT_PAYER,
        &p.wallet.pubkey().to_bytes(),
    );
    put(&mut c, l::citizen::TICKET_BELL, &u32::MAX.to_le_bytes());
    write_account(w, c_addr, c);
    let mut h = vec![0u8; fclient::abi::size::HOLDING];
    put(&mut h, 0, fclient::abi::magic::HOLDING);
    put(&mut h, 8, &s.h.season_id.to_le_bytes());
    put(&mut h, 16, &1u16.to_le_bytes());
    put(&mut h, l::holding::P, &p.home.0.to_le_bytes());
    put(&mut h, l::holding::Q, &p.home.1.to_le_bytes());
    h[l::holding::SITE] = p.site;
    h[l::holding::STATE] = l::holding::STATE_FINAL;
    put(&mut h, l::holding::OWNER_CITIZEN, &c_addr.to_bytes());
    h[l::holding::FACTION] = p.faction;
    put(&mut h, l::holding::HOST_SEQ, &1u32.to_le_bytes());
    put(
        &mut h,
        l::holding::RENT_PAYER,
        &p.wallet.pubkey().to_bytes(),
    );
    // Trained troops for the musters (a program Muster debits them).
    put(&mut h, l::holding::RESERVE, &RESERVE0.to_le_bytes());
    write_account(
        w,
        w.addrs.holding(p.home.0 as i32, p.home.1 as i32, p.site),
        h,
    );
    w.airdrop(&p.wallet.pubkey(), 2_000_000_000);
}

/// The global hex of `tile` in province `(p, q)`.
fn hex_of(p: i32, q: i32, tile: u8) -> Hex {
    let c = ProvinceCoord::new(p, q).centre();
    let o = tile_offset(tile).unwrap();
    Hex::new(c.q + o.q, c.r + o.r)
}

/// A greedy hex walk from `(origin, tile)` to `(dest, tile)`.
fn path(origin: (i32, i32, u8), dest: (i32, i32, u8)) -> Vec<u8> {
    let mut h = hex_of(origin.0, origin.1, origin.2);
    let goal = hex_of(dest.0, dest.1, dest.2);
    let mut dirs = vec![];
    while h != goal {
        let (d, n) = DIRECTIONS
            .iter()
            .enumerate()
            .map(|(i, (dq, dr))| (i as u8, Hex::new(h.q + dq, h.r + dr)))
            .min_by_key(|(_, n)| n.distance(goal))
            .unwrap();
        dirs.push(d);
        h = n;
        assert!(dirs.len() <= 32, "path too long");
    }
    let (pc, t) = locate(h);
    assert_eq!((pc.p, pc.q, t), (dest.0, dest.1, dest.2));
    dirs
}

/// What kind of march.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarchKind {
    Valid,
    /// Random 165 B with a valid commitment; the owner reveals in-bell.
    Garbage,
    /// A valid seal over an invalid plaintext (stance 9).
    BadPlain,
    /// Valid; the owner submits the material from the bell start.
    OwnerReveal,
}

#[derive(Clone)]
struct March {
    pl: usize,
    host: u64,
    troops: u32,
    kind: MarchKind,
    /// Filled at Depart.
    commit: [u8; 32],
    plain: [u8; 37],
    salt: [u8; 32],
    ct_hash: [u8; 32],
    transit_slot: u8,
}

// ------------------------------------------------------------------ the scenario

#[derive(Clone, Default)]
struct Opts {
    hold_origin: bool,
    crash: Option<(&'static str, &'static [&'static str], u32)>,
    keepers: usize,
    /// Hold the faction-0 slots of the destination bell: `(priority_milli, slots after T(arrive))`.
    hold_slots: Option<(u64, u64)>,
    /// `--peace-start` of the keepers (W writes start at this × p_tip).
    peace_start: Option<f64>,
}

struct Run {
    w: World,
    keepers: Vec<K>,
    marches: Vec<March>,
    players: Vec<Pl>,
    dest: (i16, i16),
    arrive: u32,
    digest: [u8; 32],
    /// `(host, outcome, code)`.
    settled: BTreeMap<u64, (u8, u8)>,
    dup_versions: u64,
    crashes: u32,
    inflight_at_crash: usize,
    dissolved: (usize, u64),
    reserve_at_dissolve: u32,
    /// The destination's gathered arrival records and CLASH digest.
    dest_inputs: Vec<u8>,
    dest_clash: [u8; 32],
    dest_anchor_a: i64,
    tag_dups: TagDups,
    wall: std::time::Duration,
}

fn keeper_cfg(w: &World, beneficiary: Address) -> keeper_core::config::KeeperConfig {
    let mut c = keeper_config(w);
    c.beneficiary = beneficiary;
    c.roles = PLAY_ROLES.iter().map(|s| s.to_string()).collect();
    c
}

fn new_keeper(
    w: &World,
    i: usize,
    jpath: Option<&PathBuf>,
    crash: Option<CrashAt>,
    peace: Option<f64>,
) -> K {
    let ben = Keypair::new_from_array([0xB0 + i as u8; 32]);
    let mut journal = jpath.map(|p| Journal::open(p).unwrap());
    if let (Some(j), Some(c)) = (journal.as_mut(), crash) {
        j.crash = Some(c);
    }
    let mut cfg = keeper_cfg(w, ben.pubkey());
    cfg.peace_start = peace;
    let mut k = Keeper::new(
        cfg,
        w.ip.clone(),
        w.drand.clone(),
        &[0x70 + i as u8; 32],
        journal,
    )
    .unwrap();
    k.set_claim_key(ben).unwrap();
    // Keepers besides the first race with the design's start jitter.
    if i > 0 {
        k.cfg.race_jitter_slots = 2;
    }
    k
}

/// One tick of keeper `k`; with a crash hook armed, a panic is the crash:
/// the keeper is gone and is rebuilt from its journal (same slot).
async fn tick(
    w: &World,
    k: K,
    i: usize,
    jpath: Option<&PathBuf>,
    crashes: &mut u32,
    inflight: &mut usize,
) -> K {
    // A local task: the keeper (its SQLite journal) is not `Sync`; its
    // JoinHandle still reports the panic.
    let local = tokio::task::LocalSet::new();
    let h = local
        .run_until(async move {
            tokio::task::spawn_local(async move {
                let mut k = k;
                let r = k.tick().await;
                (k, r)
            })
            .await
        })
        .await;
    match h {
        Ok((k, r)) => {
            r.unwrap();
            k
        }
        Err(e) if e.is_panic() => {
            let msg = e
                .into_panic()
                .downcast::<String>()
                .map(|s| *s)
                .unwrap_or_default();
            assert!(msg.contains(INJECTED_CRASH), "unexpected panic: {msg}");
            *crashes += 1;
            let mut k = new_keeper(w, i, jpath, None, None);
            *inflight += k.start().await.unwrap();
            k.tick().await.unwrap();
            k
        }
        Err(e) => panic!("{e}"),
    }
}

/// `None` when the program under test lacks the play instructions (a
/// `PSF_FRONTIER_SO` build before W4-A/W4-B): the run is not applicable.
async fn scenario(o: Opts) -> Option<Run> {
    let started = std::time::Instant::now();
    let w = world_with(|p| {
        p.clash_close_grace = 2;
    })
    .await;
    let dir = std::env::temp_dir().join(format!(
        "w4c-play-{}-{}",
        std::process::id(),
        rand::random::<u64>()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let jpath = dir.join("keeper.sqlite");
    let n_keepers = o.keepers.max(1);
    let mut ks: Vec<K> = (0..n_keepers)
        .map(|i| {
            let crash = (i == 0)
                .then_some(o.crash)
                .flatten()
                .map(|(p, kinds, n)| CrashAt::new(p, kinds, n));
            new_keeper(&w, i, (i == 0).then_some(&jpath), crash, o.peace_start)
        })
        .collect();
    for k in &ks {
        fund(&w, &k.payers);
        w.airdrop(&k.cfg.beneficiary, 5_000_000_000);
    }
    for k in ks.iter_mut() {
        k.start().await.unwrap();
    }
    let (mut crashes, mut inflight) = (0u32, 0usize);
    macro_rules! step_all {
        () => {{
            let mut next = vec![];
            for (i, k) in ks.drain(..).enumerate() {
                next.push(
                    tick(
                        &w,
                        k,
                        i,
                        (i == 0).then_some(&jpath),
                        &mut crashes,
                        &mut inflight,
                    )
                    .await,
                );
            }
            ks = next;
            w.step();
        }};
    }
    // Genesis, rings 0–3 and their provinces.
    let genesis_ts = w.season().genesis_ts;
    while w.now() < genesis_ts + 600 || ks[0].rings.opened.len() < 37 {
        step_all!();
        assert!(w.now() < genesis_ts + 6 * 3_600, "genesis stalled");
    }
    // The destination D = (2, 0) (ring 2); origins are its neighbours.
    let dest: (i16, i16) = (2, 0);
    let nb: Vec<(i16, i16)> = ProvinceCoord::new(2, 0)
        .neighbors()
        .iter()
        .map(|c| (c.p as i16, c.q as i16))
        .filter(|&(p, q)| {
            let r = (p.abs() + q.abs() + (p + q).abs()) / 2;
            (1..=3).contains(&r)
        })
        .collect();
    assert_eq!(nb.len(), 6);
    let mk = |seed: u8, faction: u8, home: (i16, i16), site: u8| Pl {
        wallet: Keypair::new_from_array([seed; 32]),
        faction,
        home,
        site,
    };
    // One faction per home province (a home of two factions would fight
    // every bell), the destination's defender, a far province for the
    // dissolve.
    let players = vec![
        mk(0x11, 0, nb[0], 0),   // 0: A1 900 + A6 400 (second arrival)
        mk(0x12, 0, nb[0], 1),   // 1: A2 800
        mk(0x13, 0, nb[1], 0),   // 2: A3 700
        mk(0x14, 0, nb[1], 1),   // 3: A4 600
        mk(0x15, 0, nb[2], 0),   // 4: A5 500 (outranked)
        mk(0x16, 1, nb[3], 0),   // 5: B1 300 (destroyed)
        mk(0x17, 1, dest, 0),    // 6: R1 200 (defender, stays home)
        mk(0x18, 2, nb[4], 0),   // 7: G1 garbage seal, owner reveals
        mk(0x19, 3, nb[5], 0),   // 8: X1 bad plaintext
        mk(0x1A, 4, (0, 0), 0),  // 9: W1 250 owner self-reveal from the bell start
        mk(0x1B, 5, (-2, 0), 0), // 10: H1 dissolves (the §21 return)
    ];
    let o1 = nb[0];
    for p in &players {
        fixture(&w, p);
    }
    // Hosts: (player, troops, kind, dest tile); seq from 1 upwards per holding.
    let spec: Vec<(usize, u32, Option<MarchKind>, u8)> = vec![
        (0, 900, Some(MarchKind::Valid), 10),
        (1, 800, Some(MarchKind::Valid), 11),
        (2, 700, Some(MarchKind::Valid), 12),
        (3, 600, Some(MarchKind::Valid), 13),
        (4, 500, Some(MarchKind::Valid), 14),
        (0, 400, Some(MarchKind::Valid), 15),
        (5, 300, Some(MarchKind::Valid), 16),
        (6, 200, None, 30),
        (7, 350, Some(MarchKind::Garbage), 17),
        (8, 450, Some(MarchKind::BadPlain), 18),
        (9, 250, Some(MarchKind::OwnerReveal), 19),
        (10, 150, None, 30),
    ];
    let mut seq: BTreeMap<usize, u32> = BTreeMap::new();
    let mut hosts: Vec<(usize, u64, u32, Option<MarchKind>, u8)> = vec![];
    for &(pi, troops, kind, tile) in &spec {
        let p = &players[pi];
        let s = seq.entry(pi).or_insert(1);
        let host = fclient::addr::host_id(p.home.0 as i32, p.home.1 as i32, p.site, 0, *s).unwrap();
        *s += 1;
        w.send(
            &[ix::muster(&w.addrs, &p.player(), p.href(), 0, troops, 30)],
            &[&p.wallet],
        )
        .await;
        hosts.push((pi, host, troops, kind, tile));
    }
    // The players nudge their provinces (the web's `/f/nudge`) and act
    // once each is resolved through b − 2, early in a bell.
    let homes: BTreeSet<(i16, i16)> = players.iter().map(|p| p.home).chain([dest]).collect();
    let sc = fclient::clock::SeasonClock::from_season(&w.season());
    let mut nudged_at = u32::MAX;
    loop {
        let b = sc.bell_at(w.now()).unwrap();
        if nudged_at != b {
            nudged_at = b;
            for k in &ks {
                let mut s = k.shared.lock().unwrap();
                for &(p, q) in &homes {
                    s.nudges.push(json!({"province": [p, q], "bell": b}));
                }
            }
        }
        step_all!();
        if lacks_play(&ks[0]) {
            println!(
                "program {}: play instructions NotImplemented; not applicable",
                w.which.label()
            );
            return None;
        }
        let b = sc.bell_at(w.now()).unwrap();
        let caught = homes.iter().all(|&(p, q)| {
            w.ip.lock()
                .account(&w.addrs.province(p as i32, q as i32))
                .and_then(|a| Province::decode(&a.data).ok())
                .is_some_and(|pv| pv.resolved_next + 1 >= b)
        });
        let early = w.now() - sc.genesis_ts - 600 * b as i64 <= 240;
        // A fixed departure bell, so every run (crashed, held, raced) has
        // the same timeline.
        if caught && early && b >= DEPART_BELL {
            assert_eq!(b, DEPART_BELL, "homes caught up too late");
            break;
        }
        if w.now() >= genesis_ts + 6 * 3_600 {
            let rns: Vec<_> = homes
                .iter()
                .map(|&(p, q)| {
                    (
                        (p, q),
                        w.ip.lock()
                            .account(&w.addrs.province(p as i32, q as i32))
                            .and_then(|a| Province::decode(&a.data).ok())
                            .map(|pv| pv.resolved_next),
                    )
                })
                .collect();
            panic!(
                "homes not caught up at bell {b}: {rns:?}\n{}\nalerts {:?}\npending {:?}",
                ks[0].play.status(),
                ks[0].alerts.iter().rev().take(10).collect::<Vec<_>>(),
                ks[0]
                    .engine
                    .pending_keys()
                    .iter()
                    .take(10)
                    .collect::<Vec<_>>()
            );
        }
    }
    let b0 = sc.bell_at(w.now()).unwrap();
    let arrive = b0 + 2;
    let round = sc.tlock_round(arrive);
    let pk96 = w.drand.key.pk96;
    let mut marches: Vec<March> = vec![];
    let mut dissolved = (0usize, 0u64);
    let mut reserve_at_dissolve = 0u32;
    let mut slot_of: BTreeMap<usize, u8> = BTreeMap::new();
    for &(pi, host, troops, kind, tile) in &hosts {
        let p = &players[pi];
        let Some(kind) = kind else {
            if pi == 10 {
                w.send(
                    &[ix::dissolve(&w.addrs, &p.player(), p.href(), host)],
                    &[&p.wallet],
                )
                .await;
                dissolved = (pi, host);
                reserve_at_dissolve =
                    w.ip.lock()
                        .account(&w.addrs.holding(p.home.0 as i32, p.home.1 as i32, p.site))
                        .and_then(|a| Holding::decode(&a.data).ok())
                        .map_or(0, |h| h.reserve[0]);
            }
            continue;
        };
        let dirs = path(
            (p.home.0 as i32, p.home.1 as i32, 30),
            (dest.0 as i32, dest.1 as i32, tile),
        );
        let mut plain = Plain {
            version: 1,
            host_id: host,
            arrive_bell: arrive,
            dest_p: dest.0,
            dest_q: dest.1,
            dest_tile: tile,
            stance: 0,
            retreat_bps: 0,
            path_len: dirs.len() as u8,
            path: seal::path_of(&dirs),
            ..Default::default()
        };
        if kind == MarchKind::BadPlain {
            plain.stance = 9;
        }
        let pt = seal::pack(&plain);
        let (seal_b, commit, salt, ct) = if kind == MarchKind::Garbage {
            let k = [0x42u8; 16];
            let salt = seal::salt_of(&k);
            let commit = seal::commit(&pt, &salt);
            let mut g = [0u8; 165];
            for (i, b) in g.iter_mut().enumerate() {
                *b = (i as u8).wrapping_mul(37).wrapping_add(host as u8);
            }
            g[0] |= 0x80;
            (g, commit, salt, seal::ct_hash(&g))
        } else {
            let s = seal::seal(&pt, &pk96, round).unwrap();
            (s.seal, s.commit, s.salt, s.ct_hash)
        };
        let ts = slot_of.entry(pi).or_insert(0);
        let transit_slot = *ts;
        *ts += 1;
        let tip = fclient::fees::min_tip_lamports(433, 26_000, 1_048_576);
        w.send(
            &[ix::depart(
                &w.addrs,
                &p.player(),
                p.href(),
                p.home,
                &DepartArgs {
                    host_id: host,
                    commit,
                    seal: seal_b,
                    arrive_bell: arrive,
                    tip,
                    transit_slot,
                },
            )],
            &[&p.wallet],
        )
        .await;
        marches.push(March {
            pl: pi,
            host,
            troops,
            kind,
            commit,
            plain: pt,
            salt,
            ct_hash: ct,
            transit_slot,
        });
    }
    // The lag gate: hold the origin Province and the origin region's anchor
    // of the departure bell past the destination's close.
    if o.hold_origin {
        let r_o = ix::region_of(o1.0 as i32, o1.1 as i32);
        let keys = vec![
            w.addrs.province(o1.0 as i32, o1.1 as i32),
            w.addrs.anchor(b0, r_o),
        ];
        let slots = (((arrive as i64 + 2) * 600 + sc.genesis_ts + 900 - w.now()) / 8) as u64;
        w.ip.lock().hold(keys, 1_000_000, slots).unwrap();
    }
    // Owner material: the self-reveal and the garbage seal's owner.
    let push_owner = |ks: &mut Vec<K>, m: &March, p: &Pl| {
        let v = json!({
            "holding": w.addrs.holding(p.home.0 as i32, p.home.1 as i32, p.site).to_string(),
            "transit_slot": m.transit_slot,
            "plain_b64": base64::engine::general_purpose::STANDARD.encode(m.plain),
            "salt_b64": base64::engine::general_purpose::STANDARD.encode(m.salt),
            "ct_hash_b64": base64::engine::general_purpose::STANDARD.encode(m.ct_hash),
            "checked": true,
        });
        let mut s = ks[0].shared.lock().unwrap();
        s.next_track += 1;
        let id = format!("r{}", s.next_track);
        s.tracks.insert(id.clone(), json!({"state": "queued"}));
        s.reveals.push((id, v));
    };
    let mut owner_pushed = false;
    let mut held_slots = false;
    let end_by = sc.genesis_ts + (arrive as i64 + 14) * 600;
    loop {
        let b = sc.bell_at(w.now()).unwrap();
        if !owner_pushed && b >= arrive {
            for m in marches
                .iter()
                .filter(|m| matches!(m.kind, MarchKind::OwnerReveal | MarchKind::Garbage))
            {
                push_owner(&mut ks, m, &players[m.pl]);
            }
            owner_pushed = true;
        }
        if let (Some((prio, n)), false) = (o.hold_slots, held_slots) {
            let t_arrive = sc.drand.round_time(sc.tlock_round(arrive));
            if w.now() >= t_arrive {
                let keys: Vec<Address> = (0..4u8)
                    .map(|i| {
                        w.addrs
                            .arrival_slot(dest.0 as i32, dest.1 as i32, arrive, 0, i)
                    })
                    .collect();
                w.ip.lock().hold(keys, prio, n).unwrap();
                held_slots = true;
            }
        }
        step_all!();
        if lacks_play(&ks[0]) {
            println!(
                "program {}: play instructions NotImplemented; not applicable",
                w.which.label()
            );
            return None;
        }
        let done = ks[0].play.index.departs.len() == marches.len()
            && ks[0].play.index.settled.len() == marches.len();
        let closes_done = ks[0].play.index.slots.keys().all(|s| {
            w.ip.lock()
                .account(&w.addrs.arrival_slot(s.0 as i32, s.1 as i32, s.2, s.3, s.4))
                .is_none_or(|a| a.data.is_empty())
        });
        // The §21 return landed: the dissolved host's entry is gone from
        // its province (chain state: a restarted keeper's counters start at 0).
        let returned = {
            let p = &players[dissolved.0];
            w.ip.lock()
                .account(&w.addrs.province(p.home.0 as i32, p.home.1 as i32))
                .and_then(|a| Province::decode(&a.data).ok())
                .is_some_and(|pv| {
                    !pv.entries
                        .iter()
                        .any(|e| e.state != 0 && e.id == dissolved.1)
                })
        };
        if done && closes_done && returned {
            break;
        }
        if w.now() >= end_by {
            let k = &ks[0];
            let uns: Vec<_> = k
                .play
                .index
                .departs
                .keys()
                .filter(|x| !k.play.index.settled.contains_key(x))
                .map(|x| {
                    (
                        x,
                        k.play
                            .transits
                            .get(x)
                            .map(|t| (t.tstate, t.opened.map(|o| o.code), t.refused, t.missed)),
                    )
                })
                .collect();
            panic!(
                "scenario did not finish: settled {}/{} returns {} status {}\nunsettled {uns:?}\nalerts {:?}\npending {:?}",
                k.play.index.settled.len(),
                marches.len(),
                k.play.stats.returns,
                k.play.status(),
                k.alerts.iter().filter(|a| a.1 != "reveal-pool-low").rev().take(12).collect::<Vec<_>>(),
                k.engine.pending_keys()
            );
        }
    }
    // Ride out a few more bells (closes of inputs, catch-up).
    let until = w.now() + 3 * 600;
    while w.now() < until {
        step_all!();
    }
    // Outcomes from the chain feed.
    let (digest, settled, dup, dest_clash, tag_dups) = outcomes(&w, &players);
    let ci =
        w.ip.lock()
            .account(&w.addrs.clash_inputs(dest.0 as i32, dest.1 as i32, arrive));
    let dest_inputs = ci.map(|a| a.data).unwrap_or_default();
    let r_d = ix::region_of(dest.0 as i32, dest.1 as i32);
    let dest_anchor_a = ks[0].play.anchors.get(&(arrive, r_d)).map_or(0, |a| a.a);
    let _ = std::fs::remove_dir_all(&dir);
    Some(Run {
        w,
        keepers: ks,
        marches,
        players,
        dest,
        arrive,
        digest,
        settled,
        dup_versions: dup,
        crashes,
        inflight_at_crash: inflight,
        dissolved,
        reserve_at_dissolve,
        dest_inputs,
        dest_clash,
        dest_anchor_a,
        tag_dups,
        wall: started.elapsed(),
    })
}

/// The program answered `NotImplemented` for a play role (W4-A/W4-B not
/// in the `.so` under test).
fn lacks_play(k: &K) -> bool {
    [
        "gather",
        "resolve",
        "skip",
        "settle",
        "settle-departure",
        "reveal",
    ]
    .iter()
    .any(|r| k.unsupported_roles.contains(r))
}

/// The outcome digest (CLASH outcome digests, TRANSIT_SETTLED outcomes,
/// seal codes and troops, DEPARTURE_SETTLED troops, the fixtures' final
/// reserves), the settled map, the duplicate versions (landed versions of
/// one object after its first success) and the destination's CLASH digest.
/// Per instruction tag: `(objects that succeeded, duplicate versions)`.
type TagDups = BTreeMap<u8, (u64, u64)>;

#[allow(clippy::type_complexity)]
fn outcomes(
    w: &World,
    players: &[Pl],
) -> ([u8; 32], BTreeMap<u64, (u8, u8)>, u64, [u8; 32], TagDups) {
    let c = w.ip.lock();
    let mut recs: BTreeSet<Vec<u8>> = BTreeSet::new();
    let mut settled = BTreeMap::new();
    let mut dest_clash = [0u8; 32];
    // object id → (first success seq, versions landed after it)
    let mut objects: BTreeMap<[u8; 32], (bool, u64)> = BTreeMap::new();
    let mut dup_tags: BTreeMap<u8, u64> = BTreeMap::new();
    let mut tags: TagDups = BTreeMap::new();
    for tx in c.feed(0, usize::MAX, Some(&w.program)) {
        // Object identity of the Frontier instruction: tag, data and every
        // account but the fee payer.
        if let Ok(t) = fclient::tx::from_wire(&tx.wire) {
            let keys = &t.message.account_keys;
            for ci in &t.message.instructions {
                if keys.get(ci.program_id_index as usize) != Some(&w.program) {
                    continue;
                }
                let mut h = Sha256::new();
                h.update(&ci.data);
                for &a in &ci.accounts {
                    if a != 0 {
                        h.update(keys[a as usize].as_ref());
                    }
                }
                let id: [u8; 32] = h.finalize().into();
                let e = objects.entry(id).or_insert((false, 0));
                let tag = ci.data.first().copied().unwrap_or(0);
                if e.0 {
                    e.1 += 1;
                    *dup_tags.entry(tag).or_default() += 1;
                    tags.entry(tag).or_default().1 += 1;
                } else if tx.err.is_none() {
                    e.0 = true;
                    tags.entry(tag).or_default().0 += 1;
                }
            }
        }
        if tx.err.is_some() {
            continue;
        }
        let Ok(bodies) = fclient::log::bodies_from_logs(&tx.logs, &w.program) else {
            continue;
        };
        for b in bodies {
            let Ok(r) = plog::decode(&b) else { continue };
            let f = |n: &str| -> Vec<u8> {
                plog::field(r.kind, n, false)
                    .and_then(|(o, wd)| r.key.get(o..o + wd))
                    .or_else(|| {
                        plog::field(r.kind, n, true).and_then(|(o, wd)| r.payload.get(o..o + wd))
                    })
                    .unwrap_or(&[])
                    .to_vec()
            };
            match r.kind {
                Kind::CLASH => {
                    if std::env::var("W4C_DUMP").is_ok() {
                        eprintln!(
                            "CLASHIN {} in {} out {}",
                            hex::encode(r.key),
                            hex::encode(f("input_digest")),
                            hex::encode(f("outcome_digest"))
                        );
                    }
                    let mut v = vec![41u8];
                    v.extend(f("p"));
                    v.extend(f("q"));
                    v.extend(f("bell"));
                    v.extend(f("outcome_digest"));
                    v.extend(f("fates"));
                    if f("p") == 2i32.to_le_bytes() && f("q") == 0i32.to_le_bytes() {
                        dest_clash = f("outcome_digest").try_into().unwrap_or([0; 32]);
                    }
                    recs.insert(v);
                }
                Kind::TRANSIT_SETTLED => {
                    let host = u64::from_le_bytes(f("host_id").try_into().unwrap());
                    settled.insert(host, (f("outcome")[0], f("seal_code")[0]));
                    let mut v = vec![34u8];
                    v.extend(f("host_id"));
                    v.extend(f("outcome"));
                    v.extend(f("seal_code"));
                    v.extend(f("troops"));
                    recs.insert(v);
                }
                Kind::ANCHOR | Kind::SEED if std::env::var("W4C_DUMP").is_ok() => {
                    eprintln!(
                        "BEACON {} {} {} slot {}",
                        r.kind.name(),
                        hex::encode(r.key),
                        hex::encode(r.payload),
                        tx.slot
                    );
                }
                Kind::DEPARTURE_SETTLED => {
                    let mut v = vec![32u8];
                    v.extend(f("host_id"));
                    v.extend(f("troops_after"));
                    recs.insert(v);
                }
                _ => {}
            }
        }
    }
    for p in players {
        if let Some(a) = c.account(&w.addrs.holding(p.home.0 as i32, p.home.1 as i32, p.site)) {
            if let Ok(h) = Holding::decode(&a.data) {
                let mut v = vec![0xFFu8];
                for r in h.reserve {
                    v.extend(r.to_le_bytes());
                }
                recs.insert(v);
            }
        }
    }
    let mut h = Sha256::new();
    for r in &recs {
        h.update((r.len() as u32).to_le_bytes());
        h.update(r);
    }
    let dup = objects.values().map(|x| x.1).sum();
    if std::env::var("W4C_DUMP").is_ok() || std::env::var("W4C_DUPS").is_ok() {
        eprintln!("DUPS {dup_tags:?}");
    }
    if std::env::var("W4C_DUMP").is_ok() {
        for r in &recs {
            eprintln!("REC {}", hex::encode(r));
        }
    }
    (h.finalize().into(), settled, dup, dest_clash, tags)
}

fn judge_code(w: &World, m: &March, seal_b: &[u8; 165], commit: &[u8; 32], arrive: u32) -> u8 {
    let sc = fclient::clock::SeasonClock::from_season(&w.season());
    let b = w.drand.key.beacon(sc.tlock_round(arrive));
    seal::judge(seal_b, commit, &b.sig48, m.host, arrive).0
}

// ------------------------------------------------------------------ tests

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn play_bell_pipeline() {
    let Some(r) = scenario(Opts::default()).await else {
        return;
    };
    let w = &r.w;
    let k = &r.keepers[0];
    println!("program: {}", w.which.label());
    let by_host: BTreeMap<u64, &March> = r.marches.iter().map(|m| (m.host, m)).collect();
    let host_of = |troops: u32| r.marches.iter().find(|m| m.troops == troops).unwrap().host;
    // Every transit settled, with the expected branch.
    assert_eq!(r.settled.len(), r.marches.len(), "{:?}", r.settled);
    let expect: Vec<(u32, u8)> = vec![
        (900, to::STAYS),
        (800, to::STAYS),
        (700, to::STAYS),
        (600, to::STAYS),
        (500, to::BOUNCED_UNRANKED),
        (400, to::BOUNCED_UNRANKED),
        (300, to::DESTROYED),
        (350, to::BAD_SEAL),
        (450, to::BAD_SEAL),
        (250, to::DESTROYED),
    ];
    for (troops, want) in &expect {
        let h = host_of(*troops);
        assert_eq!(
            r.settled[&h].0, *want,
            "host of {troops} troops: {:?}",
            r.settled[&h]
        );
    }
    // Every DEPART logged the pair the player committed to.
    let idx = &k.play.index;
    for m in &r.marches {
        assert_eq!(idx.transit(m.host).unwrap().commit, m.commit);
    }
    // Bad seals: the stock opener's code (garbage: FO/point; bad plaintext: 5).
    for m in r
        .marches
        .iter()
        .filter(|m| matches!(m.kind, MarchKind::Garbage | MarchKind::BadPlain))
    {
        let d = idx.transit(m.host).unwrap();
        let code = judge_code(w, m, &d.seal, &d.commit, r.arrive);
        assert_eq!(r.settled[&m.host].1, code, "seal code of host {}", m.host);
        assert!(code > 0);
    }
    assert_eq!(r.settled[&host_of(450)].1, 5);
    // Reveals: the faction-0 group in rank order, 4 creations, no displacement;
    // the outranked and the second arrival never revealed.
    let f0: Vec<_> = idx
        .reveals
        .iter()
        .filter(|x| x.dest == r.dest && x.arrive == r.arrive && x.faction == 0)
        .collect();
    assert_eq!(f0.len(), 4, "{f0:?}");
    assert!(f0.iter().all(|x| x.displaced.is_none()));
    let order: Vec<u32> = f0.iter().map(|x| by_host[&x.host].troops).collect();
    assert_eq!(order, vec![900, 800, 700, 600], "descending mass");
    assert!(!idx
        .reveals
        .iter()
        .any(|x| x.host == host_of(500) || x.host == host_of(400)));
    // The owner self-reveal landed before the keeper's decrypt could (T(b)).
    let sc = fclient::clock::SeasonClock::from_season(&w.season());
    let own = idx.reveals.iter().find(|x| x.host == host_of(250)).unwrap();
    let t_round = sc.drand.round_time(sc.tlock_round(r.arrive));
    let own_time = w.ip.lock().block_time(own.slot).unwrap();
    assert!(
        own_time < t_round + 30,
        "owner reveal at {own_time}, T(b) at {t_round}"
    );
    // The garbage seal was revealed by its owner (valid commitment) and
    // destroyed at settlement anyway.
    assert!(idx.reveals.iter().any(|x| x.host == host_of(350)));
    // No Reveal landed at or after A + W.
    for x in &idx.reveals {
        let rg = ix::region_of(x.dest.0 as i32, x.dest.1 as i32);
        let a = k.play.anchors[&(x.arrive, rg)].a;
        let t = w.ip.lock().block_time(x.slot).unwrap();
        assert!(t < sc.reveal_close(x.arrive, a), "Reveal after close");
    }
    // T(b) anchor → last keeper Reveal of the bell (E5 criterion 3: ≤ 4
    // slots at p99; one bell here).
    let r_d = ix::region_of(r.dest.0 as i32, r.dest.1 as i32);
    let an_slot = k.play.anchors[&(r.arrive, r_d)].slot.unwrap();
    let last = idx
        .reveals
        .iter()
        .filter(|x| x.arrive == r.arrive && x.beneficiary == k.cfg.beneficiary)
        .map(|x| x.slot)
        .max()
        .unwrap();
    let reveal_lat = last.saturating_sub(an_slot);
    assert!(reveal_lat <= 4, "anchor → last reveal {reveal_lat} slots");
    // SettleDeparture before the gather; one CLASH at (D, arrive).
    assert!(k.play.stats.departures_settled >= 10);
    assert!(idx.clashes.contains_key(&(r.dest.0, r.dest.1, r.arrive)));
    // The §21 return: the dissolved host's troops are back in reserve.
    let p = &r.players[r.dissolved.0];
    let h = Holding::decode(
        &w.ip
            .lock()
            .account(&w.addrs.holding(p.home.0 as i32, p.home.1 as i32, p.site))
            .unwrap()
            .data,
    )
    .unwrap();
    assert_eq!(h.reserve[0] - r.reserve_at_dissolve, 150, "returned troops");
    // Closes: every slot of the bell closed after the grace; the inputs
    // closed after `clash_close_grace`.
    for s in idx.slots.keys() {
        assert!(w
            .ip
            .lock()
            .account(&w.addrs.arrival_slot(s.0 as i32, s.1 as i32, s.2, s.3, s.4))
            .is_none_or(|a| a.data.is_empty()));
    }
    assert!(
        r.dest_inputs.is_empty(),
        "ClashInputs closed after the grace"
    );
    // Every province caught up (no stuck province-bell): within the
    // 24-bell batch of an idle province, and the busy ones at the head.
    let now_bell = sc.bell_at(w.now()).unwrap();
    for (pq, s) in &k.play.provinces {
        let pv = Province::decode(
            &w.ip
                .lock()
                .account(&w.addrs.province(pq.0 as i32, pq.1 as i32))
                .unwrap()
                .data,
        )
        .unwrap();
        assert!(
            pv.resolved_next + 26 >= now_bell,
            "{pq:?} at {} (now {now_bell}), {s:?}",
            pv.resolved_next
        );
    }
    let d = Province::decode(
        &w.ip
            .lock()
            .account(&w.addrs.province(r.dest.0 as i32, r.dest.1 as i32))
            .unwrap()
            .data,
    )
    .unwrap();
    assert!(d.resolved_next + 2 >= now_bell, "destination at the head");
    // Owner tracks moved.
    let tracks = k.shared.lock().unwrap().tracks.clone();
    assert!(
        tracks.values().any(|v| v["state"] == "landed"),
        "{tracks:?}"
    );
    // No failed or dead alert besides the expected refusals.
    let bad: Vec<_> = k
        .alerts
        .iter()
        .filter(|a| a.1 == "dead" || a.1 == "not-implemented")
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
    println!(
        "play: {} transits settled {:?}; anchor → last reveal {reveal_lat} slots; stats {}; findings {:?}; dup versions {}; wall {:?}",
        r.settled.len(),
        r.settled.values().fold(BTreeMap::<u8, u32>::new(), |mut m, x| {
            *m.entry(x.0).or_default() += 1;
            m
        }),
        k.play.status(),
        k.play.findings,
        r.dup_versions,
        r.wall
    );
    let _ = ArrivalSlot::decode;
    let _ = ClashInputs::decode;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lag_gate_in_process() {
    let Some(a) = scenario(Opts::default()).await else {
        return;
    };
    let b = scenario(Opts {
        hold_origin: true,
        ..Default::default()
    })
    .await
    .expect("same program");
    // The held origin delayed its SettleDepartures past the destination's
    // close; the destination waited and resolved from the same values.
    assert_eq!(a.dest_anchor_a, b.dest_anchor_a, "same THE anchor time");
    assert_eq!(a.dest_clash, b.dest_clash, "destination outcome digest");
    assert_ne!(a.dest_clash, [0u8; 32]);
    assert_eq!(a.digest, b.digest, "every outcome");
    let res_a = &a.keepers[0].play.stats.resolve_latency;
    let res_b = &b.keepers[0].play.stats.resolve_latency;
    println!(
        "lag gate: outcome digests equal ({}), destination CLASH {} ; resolve latency slots unheld {:?} held {:?}; wall {:?} + {:?}",
        hex::encode(a.digest),
        hex::encode(a.dest_clash),
        res_a.iter().max(),
        res_b.iter().max(),
        a.wall,
        b.wall
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn duplicate_keepers_race() {
    let Some(one) = scenario(Opts::default()).await else {
        return;
    };
    let three = scenario(Opts {
        keepers: 3,
        ..Default::default()
    })
    .await
    .expect("same program");
    assert_eq!(one.digest, three.digest, "identical outcomes");
    // Waste: a racing keeper pays at most one losing version per object
    // (its version sent before it saw the winner's): per instruction tag,
    // duplicates ≤ (keepers − 1) × objects.
    for (tag, (objs, dups)) in &three.tag_dups {
        assert!(
            *dups <= 2 * objs,
            "tag {tag:#x}: {dups} duplicate versions over {objs} objects"
        );
    }
    let objects = three
        .w
        .ip
        .lock()
        .feed(0, usize::MAX, Some(&three.w.program))
        .len() as u64;
    println!(
        "duplicate keepers: digest equal; duplicate versions 1 keeper {} / 3 keepers {}; program txs {}; per tag (objects, duplicates) {:?}",
        one.dup_versions, three.dup_versions, objects, three.tag_dups
    );
}

/// ≈ 20 journal points × 3 duty kinds.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "60 scenario runs (Gate W4 runs it with --include-ignored crash_injection)"]
async fn crash_injection_every_journal_point() {
    let Some(base) = scenario(Opts::default()).await else {
        return;
    };
    let kinds: [(&str, &[&str]); 3] = [
        ("reveal", &["reveal"]),
        ("clash", &["gather", "resolve", "skip"]),
        ("settle", &["settle-transit", "settle-departure", "return"]),
    ];
    let mut report = vec![];
    let mut reached: BTreeMap<&str, u32> = BTreeMap::new();
    for (name, ks) in kinds {
        let only = std::env::var("W4C_CRASH_ONLY").ok();
        let points: &[&str] = if name == "reveal" {
            &["attempt", "status", "plaintext"]
        } else {
            &["attempt", "status"]
        };
        for &point in points {
            for nth in 1..=10u32 {
                if only
                    .as_deref()
                    .is_some_and(|o| o != format!("{name}/{point}#{nth}"))
                {
                    continue;
                }
                let r = scenario(Opts {
                    crash: Some((point, ks, nth)),
                    ..Default::default()
                })
                .await
                .expect("same program");
                if r.crashes == 0 {
                    report.push(format!("{name}/{point}#{nth}: not reached"));
                    continue;
                }
                *reached.entry(name).or_default() += 1;
                assert_eq!(
                    r.digest, base.digest,
                    "{name}/{point}#{nth}: outcome digest"
                );
                let extra = r.dup_versions.saturating_sub(base.dup_versions);
                // The version sent but not journalled at the crash, plus
                // one per write the restart adopted from the journal.
                let bound = r.inflight_at_crash as u64 + 1;
                assert!(
                    extra <= bound,
                    "{name}/{point}#{nth}: {extra} duplicate versions > {bound} in flight"
                );
                report.push(format!(
                    "{name}/{point}#{nth}: digest ok, extra versions {extra} (in flight {})",
                    r.inflight_at_crash
                ));
            }
        }
    }
    println!(
        "crash injection (points reached per duty kind {reached:?}):\n{}",
        report.join("\n")
    );
    for (kind, n) in &reached {
        assert!(*n >= 17, "{kind}: only {n} crash points reached");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reveals_stop_at_close_under_hold() {
    // The faction-0 slots held far above the keeper's cap (P_def 2.0) from
    // T(arrive) for 150 slots: no Reveal can land before the close.
    let Some(r) = scenario(Opts {
        hold_slots: Some((1_000_000, 150)),
        ..Default::default()
    })
    .await
    else {
        return;
    };
    let w = &r.w;
    let k = &r.keepers[0];
    let sc = fclient::clock::SeasonClock::from_season(&w.season());
    for x in &k.play.index.reveals {
        let rg = ix::region_of(x.dest.0 as i32, x.dest.1 as i32);
        let a = k.play.anchors[&(x.arrive, rg)].a;
        let t = w.ip.lock().block_time(x.slot).unwrap();
        assert!(t < sc.reveal_close(x.arrive, a), "Reveal after close");
        assert!(
            x.faction != 0 || x.arrive != r.arrive,
            "held group revealed"
        );
    }
    // The held group's valid arrivals settle routed (no final set).
    for m in r
        .marches
        .iter()
        .filter(|m| m.pl <= 4 && m.kind == MarchKind::Valid)
    {
        let o = r.settled[&m.host].0;
        assert!(
            o == to::ROUTED || o == to::BOUNCED_UNRANKED,
            "host of {} troops: {o}",
            m.troops
        );
    }
    assert!(k.play.stats.reveals_missed >= 4, "{}", k.play.status());
    println!(
        "hold through the close: {} missed, findings {:?}",
        k.play.stats.reveals_missed, k.play.findings
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn claim_after_late_reveal() {
    // Held at 1.9 (below the keeper's 2.0) for 10 slots after T(arrive),
    // the keeper starting at a quarter of the tip level (`--peace-start
    // 0.25`): the Reveals escalate past the hold and land ≥ 4 slots after
    // THE anchor, at a price above the tip level.
    let Some(r) = scenario(Opts {
        hold_slots: Some((1_900, 10)),
        peace_start: Some(0.25),
        ..Default::default()
    })
    .await
    else {
        return;
    };
    let k = &r.keepers[0];
    let claims: Vec<_> = k
        .play
        .index
        .claims
        .iter()
        .filter(|c| c.0 == k.cfg.beneficiary)
        .collect();
    if claims.is_empty() {
        let r_d = ix::region_of(r.dest.0 as i32, r.dest.1 as i32);
        let an = k.play.anchors.get(&(r.arrive, r_d)).copied();
        for x in k.play.index.reveals.iter().filter(|x| x.arrive == r.arrive) {
            println!(
                "reveal f{} i{} slot {} ev? an {:?}",
                x.faction, x.i, x.slot, an
            );
        }
        for i in 0..4u8 {
            if let Some(a) = r.w.ip.lock().account(&r.w.addrs.arrival_slot(
                r.dest.0 as i32,
                r.dest.1 as i32,
                r.arrive,
                0,
                i,
            )) {
                if let Ok(sl) = ArrivalSlot::decode(&a.data) {
                    println!(
                        "slot {i}: ev_slot {} price {} limit {} claimed {} flags {}",
                        sl.ev_slot, sl.ev_price, sl.ev_limit, sl.claimed, sl.flags
                    );
                }
            }
        }
        panic!("no claim: {}", k.play.status());
    }
    assert!(claims.iter().all(|c| c.2 > 0));
    println!("claims: {claims:?}");
}
