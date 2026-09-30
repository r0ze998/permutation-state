//! The herald fixtures (`fixtures/herald`, one producer: `fixture::world`)
//! are fresh, parse into observations, and drive the policies to the
//! decisions §8.6 describes. Also parses W2-E's synthetic herald fixtures
//! (`permutation-gateway/test/fixtures/frontier`, the web's producer), so
//! the two readers of the herald format agree.

use std::collections::BTreeMap;
use std::path::Path;

use fclient::abi::layout::entry as le;
use fclient::seal::{validate, PlainError};
use fclient::Signer;
use frontier_agents::fixture::{self, FINAL, JOINED, PROVISIONAL, SEED, UNJOINED};
use frontier_agents::obs::{BellView, MeView, Observation, Overview, ProvinceView, SeasonView};
use frontier_agents::policy::{
    self, decide, Ctx, Intent, MarchMemo, Memory, RevealRoute, SealKind,
};
use frontier_agents::profile::{AgentSpec, Arch};
use frontier_agents::Persona;
use frontier_agents::{keys, recorded};
use permutation_rules::frontier::geometry::ProvinceCoord;
use serde_json::Value;

fn json(b: &[u8]) -> Value {
    serde_json::from_slice(b).expect("json")
}

#[test]
fn fixtures_are_fresh() {
    let w = fixture::world();
    let dir = fixture::dir();
    if std::env::var("FRONTIER_WRITE_FIXTURES").is_ok() {
        let _ = std::fs::remove_dir_all(&dir);
        fixture::write(&dir, &w).expect("write fixtures");
    }
    let mut on_disk = BTreeMap::new();
    fn walk(root: &Path, d: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(d).expect("fixtures dir (FRONTIER_WRITE_FIXTURES=1 writes it)") {
            let p = e.expect("entry").path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let k = p
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(k, std::fs::read(&p).unwrap());
            }
        }
    }
    walk(&dir, &dir, &mut on_disk);
    assert_eq!(
        on_disk.keys().collect::<Vec<_>>(),
        w.files.keys().collect::<Vec<_>>(),
        "fixture file set is stale: FRONTIER_WRITE_FIXTURES=1 cargo test -p agents --test herald_fixtures"
    );
    for (k, v) in &w.files {
        assert!(
            on_disk[k] == *v,
            "{k} is stale: FRONTIER_WRITE_FIXTURES=1 cargo test -p agents --test herald_fixtures"
        );
    }
}

/// Reads the fixture world into an observation for wallet `i`, as the
/// runner does (a missing `me` file = not joined).
fn observe(i: u32) -> Observation {
    let w = fixture::world();
    let f = |p: &str| w.files.get(p).cloned();
    let season = SeasonView::from_json(&json(&f("h/season.json").unwrap())).unwrap();
    let wallet = keys::wallet(SEED, i).pubkey();
    let me = match f(&format!("h/me/{wallet}.json")) {
        Some(b) => MeView::from_json(&json(&b)).unwrap(),
        None => MeView::empty(wallet),
    };
    let mut provinces = BTreeMap::new();
    let mut bells = BTreeMap::new();
    let mut overviews = vec![];
    for (k, b) in &w.files {
        if k.starts_with("h/province/") {
            let v = ProvinceView::from_json(&json(b)).unwrap();
            provinces.insert(v.coord(), v);
        } else if k.starts_with("h/overview/") {
            overviews.push(Overview::decode(b).unwrap());
        } else if k.starts_with("h/bell/") {
            let v = BellView::from_json(&json(b)).unwrap();
            bells.insert((v.bell, v.region), v);
        }
    }
    Observation {
        now: fixture::NOW,
        season,
        me,
        provinces,
        province_bells: BTreeMap::new(),
        overviews,
        bells,
    }
}

fn spec(i: u32, arch: Arch, persona: Option<Persona>) -> AgentSpec {
    AgentSpec {
        index: i,
        arch,
        faction: 0,
        join_day: 0,
        join_bell: 5,
        persona,
    }
}

fn ctx<'a>(s: &'a AgentSpec, mem: &'a Memory, direct: bool) -> Ctx<'a> {
    Ctx {
        spec: s,
        seed: SEED,
        wallet: keys::wallet(SEED, s.index).pubkey(),
        mem,
        reveal_loaded_limit: fixture::REVEAL_LOADED_LIMIT,
        direct,
        session: true,
    }
}

#[test]
fn the_world_parses_and_is_coherent() {
    let o = observe(FINAL);
    assert_eq!(o.bell(), fixture::BELL);
    assert_eq!(o.season.season_id, fixture::SEASON_ID);
    assert_eq!(o.season.program, fixture::program());
    let s = o.season.season.as_ref().expect("season bytes");
    assert_eq!(s.genesis_ts, fixture::GENESIS_TS);
    assert_eq!(o.provinces.len(), 6 + 12, "rings 1 and 2");
    assert_eq!(o.overviews.len(), 2);
    let (_, cz) = o.me.citizen.as_ref().expect("joined");
    assert_eq!(cz.wallet, keys::wallet(SEED, FINAL).pubkey());
    assert_eq!(o.me.holdings.len(), 1);
    let h = &o.me.holdings[0].1;
    assert_eq!((h.p, h.q), fixture::world().home);
    // The fixture's hosts are the holding's.
    let hosts = policy::own_hosts(&o, h);
    assert_eq!(hosts.len(), 2);
    // Every anchor verifies under the season's (test) key.
    for b in o.bells.values() {
        let a = b.anchor.as_ref().expect("anchor");
        assert!(fclient::beacon::verify(
            a.round,
            &a.sig48,
            &o.season.drand_pk
        ));
        assert_eq!(a.round, o.season.tlock_round(b.bell));
    }
    // The overview round-trips.
    for ov in &o.overviews {
        assert_eq!(Overview::decode(&ov.encode()).unwrap(), *ov);
    }
    // Tip presets (§8.3) from the Season account.
    let t = o.season.tip_presets(fixture::REVEAL_LOADED_LIMIT);
    assert!(t[0] > 0 && t[1] == (3 * t[0]).div_ceil(2) && t[2] == 2 * t[0]);
}

#[test]
fn w2e_gateway_fixtures_parse_with_the_same_readers() {
    let d = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../permutation-gateway/test/fixtures/frontier");
    let rd = |f: &str| std::fs::read(d.join(f)).unwrap_or_else(|e| panic!("{f}: {e}"));
    let s = SeasonView::from_json(&json(&rd("season.json"))).expect("season");
    assert_eq!(s.bell_secs, 600);
    let me = MeView::from_json(&json(&rd("me.json"))).expect("me");
    assert!(me.citizen.is_some());
    let pv = ProvinceView::from_json(&json(&rd("province-2,0-40.json"))).expect("province");
    assert_eq!(pv.coord(), (2, 0));
    let ov = Overview::decode(&rd("overview-2-40.bin")).expect("overview");
    assert_eq!(ov.ring, 2);
    assert_eq!(Overview::decode(&ov.encode()).unwrap(), ov);
    let b = BellView::from_json(&json(&rd("bell-40-region-13.json"))).expect("bell");
    assert_eq!((b.bell, b.region), (40, 13));
}

#[test]
fn an_unjoined_bot_joins_from_its_join_bell() {
    let o = observe(UNJOINED);
    let mem = Memory::default();
    let s = spec(UNJOINED, Arch::Daily, None);
    let v = decide(&o, &ctx(&s, &mem, false));
    assert!(
        matches!(v.as_slice(), [Intent::Join { faction: 0 }]),
        "{v:?}"
    );
    let late = AgentSpec {
        join_bell: 100,
        ..s
    };
    assert!(decide(&o, &ctx(&late, &mem, false)).is_empty());
}

#[test]
fn a_joined_bot_files_a_ticket_in_its_wedge() {
    let o = observe(JOINED);
    let mem = Memory::default();
    let s = spec(JOINED, Arch::Daily, None);
    let v = decide(&o, &ctx(&s, &mem, false));
    let Some(Intent::FileTicket { sites }) = v.first() else {
        panic!("{v:?}");
    };
    assert!((1..=3).contains(&sites.len()));
    for st in sites {
        let pc = ProvinceCoord::new(st.p as i32, st.q as i32);
        assert!(pc.ring() >= 2, "ring ≥ 2");
        assert_eq!(pc.wedge(), Some(0), "own wedge");
        let pv = o.province(st.p, st.q).expect("observed");
        assert!(st.site < pv.site_count);
        assert_eq!(pv.site_mirror[st.site as usize].state, 0, "free site");
    }
    // A refile waits for the next bell.
    let mem2 = Memory {
        ticket_sent_bell: Some(fixture::BELL),
        ..Memory::default()
    };
    assert!(decide(&o, &ctx(&s, &mem2, false)).is_empty());
}

#[test]
fn a_provisional_bot_waits_for_its_cohort() {
    let o = observe(PROVISIONAL);
    let mem = Memory::default();
    let s = spec(PROVISIONAL, Arch::Skilled, None);
    assert!(decide(&o, &ctx(&s, &mem, false)).is_empty());
    // ticket_holder holds its Province (direct port only).
    let th = spec(PROVISIONAL, Arch::Bot, Some(Persona::TicketHolder));
    let v = decide(&o, &ctx(&th, &mem, true));
    assert!(
        v.iter().any(|i| matches!(
            i,
            Intent::Hold {
                priority_milli: 500,
                bells: 3,
                ..
            }
        )),
        "{v:?}"
    );
    assert!(decide(&o, &ctx(&th, &mem, false)).is_empty());
}

fn departs(v: &[Intent]) -> Vec<&policy::DepartPlan> {
    v.iter()
        .filter_map(|i| match i {
            Intent::Depart(d) => Some(d.as_ref()),
            _ => None,
        })
        .collect()
}

/// Checks a march the way Reveal will (§5.11 checks 2 and 6): the
/// plaintext validates, the path walks passable hexes of observed
/// provinces (≤ 4) from the host's tile to the destination tile, and the
/// arrival bell is within the lead the kernel allows.
fn check_march(o: &Observation, d: &policy::DepartPlan) {
    use permutation_rules::frontier::geometry::locate;
    use permutation_rules::frontier::travel;
    use permutation_rules::hex::{Hex, DIRECTIONS};
    let p = &d.plain;
    if d.seal == SealKind::BadPlaintext {
        return;
    }
    validate(p, d.host_id, p.arrive_bell).expect("valid plaintext");
    let host = o.provinces[&d.host_at]
        .province
        .entries
        .iter()
        .find(|e| e.id == d.host_id && e.state == le::STATE_ROSTER)
        .expect("the host is in its province");
    let mut h = ProvinceCoord::new(d.host_at.0 as i32, d.host_at.1 as i32)
        .tile(host.tile)
        .unwrap();
    let mut steps = vec![];
    for i in 0..p.path_len {
        let dir = fclient::seal::step(&p.path, i);
        assert!(dir < 6);
        let (dq, dr) = DIRECTIONS[dir as usize];
        h = Hex::new(h.q + dq, h.r + dr);
        let (pc, idx) = locate(h);
        let pv = o
            .province(pc.p as i16, pc.q as i16)
            .expect("observed province");
        assert!(pv.passable_mask >> idx & 1 == 1, "passable");
        let terrain = [
            permutation_rules::map::Terrain::Grassland,
            permutation_rules::map::Terrain::Plains,
            permutation_rules::map::Terrain::Forest,
            permutation_rules::map::Terrain::Hills,
            permutation_rules::map::Terrain::Mountain,
            permutation_rules::map::Terrain::Water,
        ][pv.terrain[idx as usize] as usize];
        steps.push(travel::Step {
            hex: h,
            terrain,
            road: pv.road_mask >> idx & 1 == 1,
        });
    }
    let (dp, di) = locate(h);
    assert_eq!(
        (dp.p as i16, dp.q as i16, di),
        (p.dest_p, p.dest_q, p.dest_tile)
    );
    let start = ProvinceCoord::new(d.host_at.0 as i32, d.host_at.1 as i32)
        .tile(host.tile)
        .unwrap();
    let unit = permutation_rules::frontier::catalog::unit_of(host.unit).unwrap();
    let cost = travel::path_cost(start, &steps, unit).expect("the kernel prices the path");
    // The kernel's own check at the planned departure time.
    travel::check_arrival_bell(
        o.season.genesis_ts,
        o.now + policy::DEPART_SLACK_SECS,
        cost.secs,
        p.arrive_bell,
    )
    .expect("arrival bell");
    assert!(cost.provinces.len() <= 4);
}

#[test]
fn a_final_bot_runs_its_economy_and_marches() {
    let o = observe(FINAL);
    let mem = Memory::default();
    let s = spec(FINAL, Arch::VerySkilled, None);
    let v = decide(&o, &ctx(&s, &mem, false));
    let names: Vec<&str> = v.iter().map(|i| i.name()).collect();
    assert!(names.contains(&"build"), "{names:?}");
    assert!(names.contains(&"explore"), "{names:?}");
    // Determinism: same seed and observation, same intents.
    let again = decide(&o, &ctx(&s, &mem, false));
    assert_eq!(format!("{v:?}"), format!("{again:?}"));
    // Some bell in the session day brings a march (aggression 0.6): scan
    // a few observation times, checking every march planned.
    let mut marched = 0;
    for k in 0..24 {
        let mut ok = o.clone();
        ok.now = fixture::NOW + k * 600;
        for d in departs(&decide(&ok, &ctx(&s, &mem, false))) {
            check_march(&ok, d);
            assert_eq!(d.seal, SealKind::Honest);
            assert!(ok
                .season
                .tip_presets(fixture::REVEAL_LOADED_LIMIT)
                .contains(&d.tip));
            marched += 1;
        }
    }
    assert!(
        marched > 0,
        "a very skilled bot marches at least once in 24 bells"
    );
}

#[test]
fn personas_shape_their_marches() {
    let o = observe(FINAL);
    let mem = Memory::default();
    let presets = o.season.tip_presets(fixture::REVEAL_LOADED_LIMIT);
    let first = |p: Persona, direct: bool| -> Vec<Intent> {
        let s = spec(FINAL, Arch::Bot, Some(p));
        decide(&o, &ctx(&s, &mem, direct))
    };
    // min_tip: tip exactly tip_min.
    let v = first(Persona::MinTip, false);
    let d = departs(&v);
    assert_eq!(d.len(), 1, "{v:?}");
    assert_eq!(d[0].tip, presets[0]);
    check_march(&o, d[0]);
    // self_tip: 2 × tip_min.
    assert_eq!(departs(&first(Persona::SelfTip, false))[0].tip, presets[2]);
    // zero_tip: 0, and the same march direct when a direct port exists.
    let v = first(Persona::ZeroTip, true);
    let d = departs(&v);
    assert_eq!(d.len(), 2);
    assert!(d.iter().all(|x| x.tip == 0));
    assert_eq!(d[1].route, policy::Route::Direct);
    // garbage_seal / settle_racer: garbage; bad_plaintext: bad.
    assert_eq!(
        departs(&first(Persona::GarbageSeal, false))[0].seal,
        SealKind::Garbage
    );
    let racer = first(Persona::SettleRacer, false);
    let rd = departs(&racer);
    assert_eq!(rd[0].seal, SealKind::Garbage);
    assert_eq!(rd[0].why, "stay", "the racer's host must stay");
    assert_eq!(
        departs(&first(Persona::BadPlaintext, false))[0].seal,
        SealKind::BadPlaintext
    );
    // prefunder: lamports to the march's future accounts first.
    let v = first(Persona::Prefunder, true);
    let Some(Intent::Prefund { targets, .. }) =
        v.iter().find(|i| matches!(i, Intent::Prefund { .. }))
    else {
        panic!("{v:?}");
    };
    let d = departs(&v)[0];
    let a = fixture::addresses();
    let (p, q) = d.dest();
    assert!(targets.contains(&a.arrival_slot(p, q, d.plain.arrive_bell, 0, 0)));
    assert!(targets.contains(&a.clash_inputs(p, q, d.plain.arrive_bell)));
    // spammer: a burst of sponsored actions once a day.
    assert!(first(Persona::Spammer, false)
        .iter()
        .any(|i| matches!(i, Intent::Spam { n, .. } if *n > 60)));
    let spammed = Memory {
        spam_day: Some(0),
        ..Memory::default()
    };
    let s = spec(FINAL, Arch::Bot, Some(Persona::Spammer));
    assert!(!decide(&o, &ctx(&s, &spammed, false))
        .iter()
        .any(|i| matches!(i, Intent::Spam { .. })));
}

fn memo(o: &Observation, persona: Persona, arrive: u32) -> MarchMemo {
    let s = spec(FINAL, Arch::Bot, Some(persona));
    let mem = Memory::default();
    let v = decide(o, &ctx(&s, &mem, true));
    let d = departs(&v)[0].clone();
    let salt = [3u8; 32];
    let plain = fclient::seal::pack(&d.plain);
    let commit = fclient::seal::commit(&plain, &salt);
    let seal = vec![0x80; 165];
    MarchMemo {
        key: (d.host_id, o.bell()),
        h: d.h,
        transit_slot: d.transit_slot,
        arrive_bell: arrive,
        dest: d.dest(),
        path_others: d.path_others.clone(),
        plain,
        salt,
        commit,
        ct_hash: fclient::seal::ct_hash(&seal),
        seal,
        tip: d.tip,
        kind: d.seal,
        sent: true,
        reveal_tries: 0,
        revealed: false,
        accepted: false,
        last_code: None,
        late_done: false,
        settled: false,
        redeparted: false,
    }
}

#[test]
fn owners_reveal_in_the_arrival_bell_by_persona() {
    let o = observe(FINAL);
    let reveals = |p: Persona, arrive: u32, direct: bool| -> Vec<(RevealRoute, bool)> {
        let m = memo(&o, p, arrive);
        let mem = Memory {
            marches: vec![m],
            ..Memory::default()
        };
        let s = spec(FINAL, Arch::Bot, Some(p));
        let mut c = ctx(&s, &mem, direct);
        c.session = false;
        decide(&o, &c)
            .into_iter()
            .filter_map(|i| match i {
                Intent::Reveal { route, late, .. } => Some((route, late)),
                _ => None,
            })
            .collect()
    };
    let bell = fixture::BELL;
    // min_tip never self-reveals (keepers do).
    assert!(reveals(Persona::MinTip, bell, true).is_empty());
    // A plain bot (persona-free behaviour via double_arrival) reveals through the keeper in its arrival bell only.
    assert_eq!(
        reveals(Persona::DoubleArrival, bell, false),
        vec![(RevealRoute::Keeper, false)]
    );
    assert!(
        reveals(Persona::DoubleArrival, bell + 3, false).is_empty(),
        "not before the arrival bell"
    );
    assert!(
        reveals(Persona::DoubleArrival, bell - 3, false).is_empty(),
        "not long after it"
    );
    // self_tip reveals itself, beneficiary = its wallet.
    assert_eq!(
        reveals(Persona::SelfTip, bell, true),
        vec![(RevealRoute::DirectSelf, false)]
    );
    // late_revealer: only after the window, both routes.
    assert!(reveals(Persona::LateRevealer, bell, true).is_empty());
    assert_eq!(
        reveals(Persona::LateRevealer, bell - 2, true),
        vec![(RevealRoute::Keeper, true), (RevealRoute::DirectSelf, true)]
    );
    // W6-C: judged on the time the herald observed, not the runner's clock:
    // a runner clock ahead of the chain does not make it reveal early.
    {
        let mut o2 = o.clone();
        o2.season.latest_unix = o2.now - 3 * 600;
        let m = memo(&o2, Persona::LateRevealer, bell - 2);
        let mem = Memory {
            marches: vec![m],
            ..Memory::default()
        };
        let s = spec(FINAL, Arch::Bot, Some(Persona::LateRevealer));
        let mut c = ctx(&s, &mem, true);
        c.session = false;
        assert!(
            !decide(&o2, &c)
                .iter()
                .any(|i| matches!(i, Intent::Reveal { .. })),
            "the herald has not seen the window close yet"
        );
    }
    // forger: a forged direct reveal, and the honest one.
    assert_eq!(
        reveals(Persona::Forger, bell, true),
        vec![
            (RevealRoute::DirectForged, false),
            (RevealRoute::Keeper, false)
        ]
    );
}

#[test]
fn bad_plaintexts_and_errors_are_detected() {
    // The kernel's rules the bad_plaintext persona relies on.
    let o = observe(FINAL);
    let s = spec(FINAL, Arch::Bot, Some(Persona::BadPlaintext));
    let mem = Memory::default();
    let d = departs(&decide(&o, &ctx(&s, &mem, false)))[0].clone();
    assert_eq!(d.seal, SealKind::BadPlaintext);
    // The runner corrupts the stance; the plan itself is valid.
    let mut p = d.plain;
    assert!(validate(&p, d.host_id, p.arrive_bell).is_ok());
    p.stance = 9;
    assert_eq!(
        validate(&p, d.host_id, p.arrive_bell),
        Err(PlainError::Stance)
    );
}

#[test]
fn the_roster_follows_the_mix() {
    use frontier_agents::profile::{arch_counts, roster};
    let m = Mix::for_season_days(7);
    let r = roster(1_000, 42, &m);
    assert_eq!(r.len(), 1_000);
    // 50 bots, then the humans by cumulative rounding: 143 / 427 / 285 / 86 / 9.
    assert_eq!(arch_counts(&r), [143, 427, 285, 86, 9, 50]);
    // Stratified factions: each archetype spread evenly.
    for a in frontier_agents::ARCHS {
        let mut f = [0usize; 6];
        for x in r.iter().filter(|x| x.arch == a) {
            f[x.faction as usize] += 1;
        }
        let (mn, mx) = (f.iter().min().unwrap(), f.iter().max().unwrap());
        assert!(mx - mn <= 1, "{a:?}: {f:?}");
    }
    // Joins before the preset's join_close_bell (wave-3 review, W3-E: the
    // program refuses Join from bell 756 of M1_LOCAL_7D); ~60% on day 0.
    let close = 756; // frontier_abi::presets::M1_LOCAL_7D.join_close_bell
    assert_eq!(m.join_close_bell, Some(close));
    assert!(r
        .iter()
        .all(|x| x.join_bell < close && x.join_bell / 144 == x.join_day));
    assert!(
        r.iter().any(|x| x.join_bell >= 5 * 144),
        "late days are used"
    );
    let day0 = r.iter().filter(|x| x.join_day == 0).count();
    assert!((550..=700).contains(&day0), "{day0}");
    // Personas: 5 each (≤ 1%), never idle, early on day 0.
    for p in Persona::ALL {
        let n: Vec<&AgentSpec> = r.iter().filter(|x| x.persona == Some(p)).collect();
        assert_eq!(n.len(), 5, "{p:?}");
        assert!(n.iter().all(|x| x.arch != Arch::Idle && x.join_bell < 12));
    }
    // Deterministic in the seed.
    assert_eq!(roster(1_000, 42, &m), r);
    assert_ne!(roster(1_000, 43, &m), r);
    // 100 bots: one per persona.
    let small = roster(100, 1, &m);
    assert_eq!(small.iter().filter(|x| x.persona.is_some()).count(), 13);
}

use frontier_agents::Mix;

#[test]
fn double_arrival_and_squatter_send_hosts_to_one_province_bell() {
    let mut o = observe(FINAL);
    let home = fixture::world().home;
    // A second and third combat host of the same holding on the home tile.
    let (h_p, h_q) = home;
    let pv = &mut o.provinces.get_mut(&home).unwrap().province;
    let first = pv
        .entries
        .iter()
        .position(|e| e.state == le::STATE_ROSTER && e.unit != 6)
        .unwrap();
    for seq in [3u32, 4] {
        let mut e = pv.entries[first];
        e.id = fclient::addr::host_id(h_p as i32, h_q as i32, 0, 1, seq).unwrap();
        e.troops = 100;
        let free = pv
            .entries
            .iter()
            .position(|x| x.state == le::STATE_FREE)
            .unwrap();
        pv.entries[free] = e;
    }
    let mem = Memory::default();
    for (p, want) in [(Persona::DoubleArrival, 2usize), (Persona::Squatter, 3)] {
        let s = spec(FINAL, Arch::Bot, Some(p));
        let v = decide(&o, &ctx(&s, &mem, false));
        let d = departs(&v);
        assert_eq!(d.len(), want, "{p:?}: {v:?}");
        let bells: Vec<u32> = d.iter().map(|x| x.plain.arrive_bell).collect();
        let dests: Vec<(i32, i32)> = d.iter().map(|x| x.dest()).collect();
        assert!(bells.iter().all(|&b| b == bells[0]), "one bell");
        assert!(dests.iter().all(|&x| x == dests[0]), "one province");
        let mut slots: Vec<u8> = d.iter().map(|x| x.transit_slot).collect();
        slots.sort();
        slots.dedup();
        assert_eq!(slots.len(), want, "distinct transit slots");
        for x in &d {
            check_march(&o, x);
        }
    }
}

/// The holding flips provisional → final lazily, in the prologue of the
/// owner's first resident action once `now ≥ final_ts` and its cohort is
/// closed (I-29, I-47, §5.6 step 5). A bot that waited for the herald to
/// show `final` would wait forever: it must act on a holding that is final
/// by rule (found by W4-F's first in-process day: 35 holdings, no economy).
#[test]
fn a_provisional_holding_final_by_rule_acts() {
    let s = spec(PROVISIONAL, Arch::VerySkilled, None);
    let mem = Memory::default();
    let o = observe(PROVISIONAL);
    let h = o.me.holdings[0].1.clone();
    assert_eq!(h.state, fclient::abi::layout::holding::STATE_PROVISIONAL);
    assert!(o.now >= h.final_ts, "the fixture's final_ts passed");
    // Cohort open (a rival ticket unsettled): waits.
    assert!(decide(&o, &ctx(&s, &mem, false)).is_empty());
    // Cohort closed: the owner acts (its first resident action flips it).
    let mut closed = o.clone();
    let pv = closed
        .provinces
        .get_mut(&(h.p, h.q))
        .expect("home observed");
    for c in pv
        .province
        .cohorts
        .iter_mut()
        .filter(|c| c.bell == h.ticket_bell)
    {
        c.settled = c.filed;
    }
    let v = decide(&closed, &ctx(&s, &mem, false));
    assert!(!v.is_empty(), "a final-by-rule holding runs its economy");
    // Cohort closed but final_ts not reached: waits.
    let mut early = closed.clone();
    early.me.holdings[0].1.final_ts = early.now + 1;
    assert!(decide(&early, &ctx(&s, &mem, false)).is_empty());
    // The cohort expires 24 bells after the ticket bell even if unsettled.
    let mut expired = o.clone();
    expired.now = o.season.genesis_ts + (h.ticket_bell as i64 + 24) * 600 + 5;
    assert!(!decide(&expired, &ctx(&s, &mem, false)).is_empty());
}

/// A Muster never counts troops trained in the same step: the relay
/// simulates each transaction against the landed state, so a Muster sent
/// right behind its Train is refused `Insufficient` (W4-F's in-process day:
/// 13 of 16 musters). The next step musters them.
#[test]
fn a_bot_musters_only_troops_already_in_reserve() {
    let s = spec(FINAL, Arch::VerySkilled, None);
    let mem = Memory::default();
    let mut o = observe(FINAL);
    for (_, h) in o.me.holdings.iter_mut() {
        h.reserve = [0; 8];
    }
    // Drop the combat host so the bot wants one.
    let (_, h) = o.me.holdings[0].clone();
    let pv = o.provinces.get_mut(&(h.p, h.q)).expect("home");
    pv.province.entries.retain(|e| e.unit == 6);
    for k in 0..24 {
        let mut ok = o.clone();
        ok.now = fixture::NOW + k * 600;
        let v = decide(&ok, &ctx(&s, &mem, false));
        let trains = v
            .iter()
            .filter(|i| matches!(i, Intent::Train { .. }))
            .count();
        let musters: Vec<&Intent> = v
            .iter()
            .filter(|i| matches!(i, Intent::Muster { .. }))
            .collect();
        assert!(
            musters.is_empty(),
            "bell +{k}: trains {trains}, musters {musters:?}"
        );
    }
}

/// A Hamlet has two queue slots (`Tier::queue_slots`): with two items
/// running the bot does not Build (it did, and got `QueueFull`).
#[test]
fn the_build_queue_counts_the_tiers_slots() {
    let mut o = observe(FINAL);
    let now = o.now;
    let h = &mut o.me.holdings[0].1;
    h.tier = 0;
    for q in h.queue.iter_mut() {
        *q = Default::default();
    }
    assert!(policy::queue_free(h, now));
    h.queue[0] = fclient::decode::QueueItem {
        done_at: now + 600,
        kind: 1,
        arg: 0,
        delta: 1,
    };
    assert!(policy::queue_free(h, now), "one of two running");
    h.queue[1] = h.queue[0];
    assert!(!policy::queue_free(h, now), "Hamlet: two of two running");
    h.tier = 2;
    assert!(policy::queue_free(h, now), "City: two of four");
    h.tier = 0;
    h.queue[1].done_at = now;
    assert!(policy::queue_free(h, now), "a finished item frees its slot");
}

/// The **recorded** herald fixtures (`fixtures/herald-recorded`, one
/// producer: `FRONTIER_RECORD_FIXTURES=1 cargo test --release -p itest
/// --test inproc_day -- --include-ignored`, the strict run): the answers the
/// real herald gave two thirds into the in-process day (`DayCfg::
/// record_bell`, while the bots still hold hosts) (W3-E notes §2: the
/// synthetic set is re-recorded from a real herald, and the readers must
/// accept both). Every file parses with the same readers; the anchors
/// verify under the season's (test) key at `tlock_round(bell)`; every
/// recorded wallet is `keys::wallet(seed, index)` and joined; and the
/// policies decide on the recorded world with every planned march passing
/// the kernel's own checks.
#[test]
fn the_recorded_herald_parses_and_drives_the_policies() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/herald-recorded");
    let rd = |p: &Path| std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let idx = json(&rd(&dir.join("index.json")));
    let seed = idx["seed"].as_u64().expect("seed");
    let season = SeasonView::from_json(&json(&rd(&dir.join("h/season.json")))).expect("season");
    assert_eq!(season.bell_secs, 600);
    assert_eq!(
        season.drand_pk,
        fclient::beacon::TestKey::new().pk96,
        "the in-process day seals to the test key"
    );
    let mut provinces = BTreeMap::new();
    let mut bells = BTreeMap::new();
    let mut overviews = vec![];
    fn files(d: &Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            if e.path().is_dir() {
                files(&e.path(), out);
            } else {
                out.push(e.path());
            }
        }
    }
    let mut all = vec![];
    files(&dir.join("h"), &mut all);
    let mut mes = 0;
    for p in &all {
        let rel = p
            .strip_prefix(&dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let b = rd(p);
        if rel.starts_with("h/overview/") {
            let ov = Overview::decode(&b).unwrap_or_else(|e| panic!("{rel}: {e:?}"));
            assert_eq!(Overview::decode(&ov.encode()).unwrap(), ov, "{rel}");
            overviews.push(ov);
        } else if rel.starts_with("h/province/") {
            let v = ProvinceView::from_json(&json(&b)).unwrap_or_else(|e| panic!("{rel}: {e:?}"));
            let (pp, qq) = v.coord();
            assert!(rel.contains(&format!("/{pp},{qq}/")), "{rel}");
            provinces.insert(v.coord(), v);
        } else if rel.starts_with("h/bell/") {
            let v = BellView::from_json(&json(&b)).unwrap_or_else(|e| panic!("{rel}: {e:?}"));
            let a = v.anchor.as_ref().expect("a bell file has THE anchor");
            assert_eq!(a.round, season.tlock_round(v.bell), "{rel}");
            assert!(
                fclient::beacon::verify(a.round, &a.sig48, &season.drand_pk),
                "{rel}: anchor signature"
            );
            bells.insert((v.bell, v.region), v);
        } else if rel.starts_with("h/me/") {
            MeView::from_json(&json(&b)).unwrap_or_else(|e| panic!("{rel}: {e:?}"));
            mes += 1;
        }
    }
    assert!(!overviews.is_empty() && !provinces.is_empty() && !bells.is_empty());
    let wallets = idx["wallets"].as_array().expect("wallets");
    assert_eq!(mes, wallets.len(), "one me file per recorded wallet");
    let recorded_bell = idx["recordedBell"].as_u64().expect("recordedBell") as u32;
    // The decisions (W6-C: one rule with the recorder, `recorded::plans`).
    // An idle province may sit up to a keeper skip batch behind the
    // recording's bell; a bot then nudges instead of acting (the residency
    // gate, integ-W4 review), so the plans are taken with the provinces
    // caught up (as after the nudge). W5-C: the bots march only a host with
    // the stamina Depart charges; a wallet that plans nothing is decided
    // again with its roster rested, which still checks the plan against the
    // recorded herald.
    let rec = recorded::load(&dir).expect("the recording loads");
    assert_eq!(rec.wallets.len(), wallets.len());
    let now_bell = season.bell_at(season.latest_unix);
    for (w, rw) in wallets.iter().zip(&rec.wallets) {
        assert_eq!(w["wallet"].as_str().unwrap(), rw.wallet.to_string());
        assert_eq!(rw.wallet, keys::wallet(seed, rw.spec.index).pubkey());
        // The recording is taken mid-play: a wallet whose join bell has
        // not come yet (or came within the last few bells) may still be
        // outside; every other one has joined.
        if rw.me.citizen.is_none() {
            assert!(
                rw.spec.join_bell + 6 >= recorded_bell,
                "{}: not joined at bell {recorded_bell} (join bell {})",
                rw.wallet,
                rw.spec.join_bell
            );
        }
    }
    let plans = recorded::plans(&rec, fixture::REVEAL_LOADED_LIMIT);
    for m in &plans.marches {
        check_march(&m.obs, &m.plan);
        let host = m.obs.provinces[&m.plan.host_at]
            .province
            .entries
            .iter()
            .find(|e| e.id == m.plan.host_id)
            .expect("the host");
        let st = permutation_rules::frontier::host::Stamina {
            value: host.stamina_value,
            bell: host.stamina_bell,
        };
        assert!(
            st.at(now_bell)
                >= permutation_rules::frontier::travel::march_stamina(
                    permutation_rules::frontier::travel::MAX_PATH_STEPS as u32
                ),
            "a planned march has the stamina Depart charges"
        );
    }
    let (marches, rested_marches) = (plans.unrested(), plans.rested());
    println!(
        "recorded herald (bell {recorded_bell}): {} provinces, {} bell-region files, {} overviews, {} wallets decided, {marches} marches planned with the recorded stamina ({rested_marches} more with rested hosts)",
        provinces.len(),
        bells.len(),
        overviews.len(),
        plans.decided
    );
    // Integ-W4 review (W4-F): a recording of the strict day, not of the
    // stub world (whose provinces never moved: every latest file was the
    // OpenProvince bytes). W6-C (DECISIONS O11): one that drives a march
    // **with the hosts' recorded stamina**, not only once rested.
    assert!(
        marches > 0,
        "the recorded herald drives at least one unrested march ({rested_marches} with rested hosts only)"
    );
    for (pq, v) in &provinces {
        assert!(
            v.province.resolved_next + 30 >= recorded_bell,
            "{pq:?}: resolved through {} at the recording's bell {recorded_bell}",
            v.province.resolved_next
        );
    }
}

/// Every province opens with a barbarian camp, so on day 0 a camp-free
/// "stays" province is rare: the settle racer then takes a free tile of a
/// province with a camp (away from it) rather than never marching (W4-F's
/// in-process day: its garbage seal never departed).
#[test]
fn the_settle_racer_marches_when_every_province_has_a_camp() {
    let s = spec(FINAL, Arch::Bot, Some(Persona::SettleRacer));
    let mem = Memory::default();
    let mut o = observe(FINAL);
    for pv in o.provinces.values_mut() {
        if pv.province.camp.state == 0 {
            pv.province.camp.state = 1;
            pv.province.camp.troops = 100;
            pv.province.camp.tile = pv.province.sites[0];
        }
    }
    let mut marched = 0;
    for k in 0..24 {
        let mut ok = o.clone();
        ok.now = fixture::NOW + k * 600;
        for d in departs(&decide(&ok, &ctx(&s, &mem, true))) {
            assert_eq!(d.seal, SealKind::Garbage);
            assert_eq!(d.why, "stay");
            let pv = ok
                .province(d.plain.dest_p, d.plain.dest_q)
                .expect("observed");
            assert_ne!(d.plain.dest_tile, pv.camp.tile, "not the camp tile");
            marched += 1;
        }
    }
    assert!(marched > 0, "the settle racer marches");
}

/// A building's copy number counts the Production items of its resource
/// (the queue item's `arg` is the resource, W3-B P1), running or finished
/// but not yet settled into `production` — the program's `copy_number`.
/// Matching the building index instead mispriced Builds (`Insufficient`).
#[test]
fn building_copies_count_production_items_by_resource() {
    use permutation_rules::frontier::catalog;
    let o = observe(FINAL);
    let now = o.now;
    let mut h = o.me.holdings[0].1.clone();
    for q in h.queue.iter_mut() {
        *q = Default::default();
    }
    // A building whose resource index differs from its item index.
    let item = (0..catalog::BUILDINGS.len() as u8)
        .find(|&i| catalog::BUILDINGS[i as usize].resource as u8 != i)
        .expect("some building's resource is not its index");
    let res = catalog::BUILDINGS[item as usize].resource as u8;
    let base = policy::copies(&h, item, now);
    h.queue[0] = fclient::decode::QueueItem {
        done_at: now + 600,
        kind: policy::QUEUE_PRODUCTION,
        arg: res,
        delta: 1,
    };
    assert_eq!(policy::copies(&h, item, now), base + 1, "running");
    h.queue[0].done_at = now - 1;
    assert_eq!(
        policy::copies(&h, item, now),
        base + 1,
        "finished, not settled"
    );
    h.queue[0].arg = item;
    h.queue[0].kind = policy::QUEUE_PRODUCTION;
    if item != res {
        assert_eq!(policy::copies(&h, item, now), base, "another resource");
    }
}

/// The settle racer re-departs its host from the bell after its arrival
/// bell, for `RACE_BELLS` bells, until a try counts (integ-W6t review: it does not wait to see the
/// destination's resolve, whose window before the keepers' SettleTransit is
/// shorter than the herald's lag at 20×; the relay refuses the early tries
/// `NotResident` in simulation); before the arrival bell a Depart tests
/// nothing.
#[test]
fn the_settle_racer_redeparts_from_the_arrival_bell() {
    let o = observe(FINAL);
    let w = fixture::world();
    let h = o.me.holdings[0].1.clone();
    let (_, t) = h
        .transit
        .iter()
        .enumerate()
        .find(|(_, t)| t.state != 0)
        .expect("the fixture's march in transit");
    let mut m = memo(&o, Persona::SettleRacer, fixture::IN_TRANSIT_ARRIVE);
    m.key = (t.host_id, t.depart_bell);
    m.dest = (w.enemy_home.0 as i32, w.enemy_home.1 as i32);
    let s = spec(FINAL, Arch::Bot, Some(Persona::SettleRacer));
    let redeparts = |o: &Observation, m: &policy::MarchMemo| {
        let mem = Memory {
            marches: vec![m.clone()],
            ..Memory::default()
        };
        decide(o, &ctx(&s, &mem, true))
            .iter()
            .filter(|i| matches!(i, Intent::Redepart { .. }))
            .count()
    };
    let bell = o.bell();
    // From the bell after the arrival bell (the resolve needs the close),
    // whether or not the resolve is seen yet.
    let mut racing = m.clone();
    racing.arrive_bell = bell - 1;
    assert_eq!(redeparts(&o, &racing), 1);
    let mut early = o.clone();
    early
        .provinces
        .get_mut(&w.enemy_home)
        .expect("destination observed")
        .province
        .resolved_next = bell - 1;
    assert_eq!(redeparts(&early, &racing), 1);
    // In the arrival bell or before it: nothing to race yet.
    let mut before = m.clone();
    before.arrive_bell = bell;
    assert_eq!(redeparts(&o, &before), 0);
    let m = racing;
    // Past the race (`RACE_BELLS`), or once a try counted: done.
    let mut late = o.clone();
    late.now += (policy::RACE_BELLS as i64 + 3) * 600;
    assert_eq!(redeparts(&late, &m), 0);
    let mut done = m.clone();
    done.redeparted = true;
    assert_eq!(redeparts(&o, &done), 0);
}

// ------------------------------------------------------ W6T-3 (w6-s7)

/// The FINAL bot's combat host, its holding and every target it could
/// plan a march to (end bell far), longest path first.
/// `march_setup`'s answer: the holding, the host's province, the host and
/// its targets.
type MarchSetup = (
    fclient::decode::Holding,
    (i16, i16),
    fclient::decode::Entry,
    Vec<(policy::Target, u32)>,
);

fn march_setup(o: &Observation) -> MarchSetup {
    let (_, h) = o.me.holdings[0].clone();
    let (at, host) = policy::own_hosts(o, &h)
        .into_iter()
        .find(|(_, e)| e.unit != 6)
        .expect("a combat host");
    let mut ts: Vec<(policy::Target, u32)> = policy::targets(o, 0, u32::MAX)
        .into_iter()
        .filter_map(|t| {
            policy::plan_march(o, &h, at, &host, t, 0, 0, 0).map(|(p, _, _)| (t, p.secs))
        })
        .collect();
    ts.sort_by_key(|x| std::cmp::Reverse(x.1));
    (h, at, host, ts)
}

fn set_end_bell(o: &mut Observation, end: u32) {
    o.season
        .season
        .as_mut()
        .expect("the Season account")
        .end_bell = end;
}

/// Failing first on 7dcacdf (w6-s7 criterion 1: 9 honest marches arrived
/// at 1008–1011 with `end_bell` 1008): the arrival is clamped to
/// `end_bell − 1`, and there is no Depart when the earliest arrival is at
/// or after `end_bell`.
#[test]
fn plan_near_end_never_arrives_at_or_after_end_bell() {
    let mut o = observe(FINAL);
    let (h, at, host, ts) = march_setup(&o);
    let dep = o.season.bell_at(o.now + policy::DEPART_SLACK_SECS);
    // The longest path whose earliest arrival is still before `dep + 4`.
    let earliest = |t: policy::Target| {
        policy::plan_march(&o, &h, at, &host, t, 0, 0, 0)
            .unwrap()
            .1
            .arrive_bell
            - 1
    };
    let t = ts
        .iter()
        .map(|x| x.0)
        .find(|&t| earliest(t) == dep + 3)
        .expect("a target arriving at dep + 3 at the earliest");
    // Unclamped, the planned arrival would pass `dep + 3` (7dcacdf).
    assert_eq!(
        policy::plan_march(&o, &h, at, &host, t, 2, 0, 0)
            .unwrap()
            .1
            .arrive_bell,
        dep + 6
    );
    for extra in 0..3 {
        set_end_bell(&mut o, dep + 4);
        let (_, p, _) = policy::plan_march(&o, &h, at, &host, t, extra, 0, 0)
            .expect("a march that can still arrive before the end");
        assert!(
            p.arrive_bell <= dep + 3,
            "extra {extra}: arrive {}",
            p.arrive_bell
        );
        assert!(p.arrive_bell >= dep + 2);
        set_end_bell(&mut o, dep + 2);
        assert!(
            policy::plan_march(&o, &h, at, &host, t, extra, 0, 0).is_none(),
            "earliest ≥ end_bell: no Depart"
        );
    }
    // And the whole policy near the end: every march it plans arrives
    // before the end.
    let s = spec(FINAL, Arch::VerySkilled, Some(Persona::MinTip));
    let mem = Memory::default();
    for k in 0..6 {
        let mut ok = o.clone();
        ok.now = fixture::NOW + k * 600;
        let dep = ok.season.bell_at(ok.now + policy::DEPART_SLACK_SECS);
        set_end_bell(&mut ok, dep + 3);
        for d in departs(&decide(&ok, &ctx(&s, &mem, false))) {
            assert!(d.plain.arrive_bell < dep + 3, "{d:?}");
        }
    }
}

/// w6-s7 criterion 4 (27 honest marches refused `Shielded`, routed): while
/// the bot's own holding is shielded at the arrival bell's start, no war
/// march is planned; once the shield lapses, war marches come back.
#[test]
fn shielded_holding_offers_no_war_target() {
    let o = observe(FINAL);
    let (h0, at, host, ts) = march_setup(&o);
    let war = ts
        .iter()
        .map(|x| x.0)
        .find(|t| t.why == "war")
        .expect("a war target in view");
    let camp = ts
        .iter()
        .map(|x| x.0)
        .find(|t| t.why == "camp")
        .expect("a camp in view");
    let pick = |o: &Observation, h: &fclient::decode::Holding, c: Vec<policy::Target>| {
        policy::first_plan(o, h, at, &host, c, 0, 0, 0).map(|x| x.0)
    };
    // The control: an unshielded holding marches to war.
    assert_eq!(pick(&o, &h0, vec![war, camp]), Some(war));
    let mut shielded = o.clone();
    for (_, h) in shielded.me.holdings.iter_mut() {
        h.shield_until = fixture::NOW + 7 * 86_400;
    }
    let h = shielded.me.holdings[0].1.clone();
    assert_eq!(pick(&shielded, &h, vec![war]), None);
    assert_eq!(pick(&shielded, &h, vec![war, camp]), Some(camp));
    let (_, p, _) = policy::plan_march(&shielded, &h, at, &host, war, 0, 0, 0).unwrap();
    assert!(policy::shield_refuses(&shielded, &h, &war, p.arrive_bell));
    // A shield that ends before the arrival bell starts does not refuse.
    let mut lapsing = h.clone();
    lapsing.shield_until = shielded.season.genesis_ts + p.arrive_bell as i64 * 600;
    assert!(!policy::shield_refuses(
        &shielded,
        &lapsing,
        &war,
        p.arrive_bell
    ));
    // Over a session day the whole policy never plans a war march.
    let s = spec(FINAL, Arch::VerySkilled, Some(Persona::MinTip));
    let mem = Memory::default();
    for k in 0..48 {
        let mut ok = shielded.clone();
        ok.now = fixture::NOW + k * 600;
        for d in departs(&decide(&ok, &ctx(&s, &mem, false))) {
            assert_ne!(d.why, "war", "{d:?}");
        }
    }
    // A dormant holding's shield does not count (§5.11 step 6).
    let mut hd = h.clone();
    hd.flags |= fclient::abi::layout::holding::FLAG_DORMANT_CACHE;
    assert!(!policy::shield_refuses(&shielded, &hd, &war, p.arrive_bell));
    // Camps are not holding sites: never refused by the shield rule.
    for (t, _) in ts.iter().filter(|x| x.0.why == "camp") {
        assert!(!policy::shield_refuses(&shielded, &h, t, p.arrive_bell));
    }
}

/// The destination's shield is judged at the planned arrival bell (the
/// program's `shield_until_bell > arrive`), not at `bell + 4`: a site
/// whose shield ends between the two is a legal target.
#[test]
fn dest_shield_judged_at_arrival() {
    let mut o = observe(FINAL);
    let (h, at, host, ts) = march_setup(&o);
    let war = ts
        .iter()
        .map(|x| x.0)
        .find(|t| t.why == "war")
        .expect("a war target in view");
    let (_, p, _) = policy::plan_march(&o, &h, at, &host, war, 2, 0, 0).unwrap();
    let arrive = p.arrive_bell;
    assert!(
        arrive > fixture::BELL + 4,
        "the test needs arrive > bell + 4"
    );
    let set = |o: &mut Observation, b: u32| {
        let pv = &mut o.provinces.get_mut(&(war.p, war.q)).unwrap().province;
        let k = (0..pv.site_count as usize)
            .find(|&k| pv.sites[k] == war.tile)
            .unwrap();
        pv.site_mirror[k].shield_until_bell = b;
    };
    set(&mut o, arrive);
    assert!(
        policy::targets(&o, 0, fixture::BELL + 73).contains(&war),
        "a shield ending at the arrival keeps the target"
    );
    assert!(!policy::shield_refuses(&o, &h, &war, arrive));
    set(&mut o, arrive + 1);
    assert!(policy::shield_refuses(&o, &h, &war, arrive));
    set(&mut o, fixture::BELL + 74);
    assert!(!policy::targets(&o, 0, fixture::BELL + 73).contains(&war));
}

/// w6-s7: 111 Musters refused `ProvinceFull`. With the home province's
/// roster at 48 (other factions' hosts), or no free entry, the bot does
/// not muster; the control musters.
#[test]
fn muster_skips_full_province() {
    let s = spec(FINAL, Arch::VerySkilled, None);
    let mem = Memory::default();
    let musters = |o: &Observation| -> usize {
        (0..24)
            .map(|k| {
                let mut ok = o.clone();
                ok.now = fixture::NOW + k * 600;
                decide(&ok, &ctx(&s, &mem, false))
                    .iter()
                    .filter(|i| matches!(i, Intent::Muster { .. }))
                    .count()
            })
            .sum()
    };
    let mut o = observe(FINAL);
    // Drop the combat host so the bot wants one (as the reserve test).
    let (_, h) = o.me.holdings[0].clone();
    o.provinces
        .get_mut(&(h.p, h.q))
        .unwrap()
        .province
        .entries
        .retain(|e| e.unit == 6);
    let mut pad = o.clone();
    let pv = &mut o.provinces.get_mut(&(h.p, h.q)).unwrap().province;
    while pv.entries.len() < fclient::abi::ENTRIES {
        pv.entries.push(Default::default());
    }
    assert!(musters(&o) > 0, "the control musters");
    assert!(policy::muster_room(pv_of(&o, h.p, h.q), h.faction) > 0);
    // 48 other-faction hosts on the roster.
    let pv = &mut pad.provinces.get_mut(&(h.p, h.q)).unwrap().province;
    pv.entries.resize(fclient::abi::ENTRIES, Default::default());
    let mut n = pv
        .entries
        .iter()
        .filter(|e| e.state == le::STATE_ROSTER || e.state == le::STATE_MUSTER_PENDING)
        .count();
    for e in pv.entries.iter_mut() {
        if n >= 48 {
            break;
        }
        if e.state == le::STATE_FREE {
            e.state = le::STATE_ROSTER;
            e.faction = 3;
            n += 1;
        }
    }
    assert_eq!(policy::muster_room(pv_of(&pad, h.p, h.q), h.faction), 0);
    assert_eq!(musters(&pad), 0);
    // No free entry (departed hosts fill the rest).
    let mut full = pad.clone();
    for e in full
        .provinces
        .get_mut(&(h.p, h.q))
        .unwrap()
        .province
        .entries
        .iter_mut()
    {
        if e.state == le::STATE_ROSTER && e.faction == 3 {
            e.state = 3;
        }
        if e.state == le::STATE_FREE {
            e.state = 3;
        }
    }
    assert_eq!(policy::muster_room(pv_of(&full, h.p, h.q), h.faction), 0);
    assert_eq!(musters(&full), 0);
}

fn pv_of(o: &Observation, p: i16, q: i16) -> &fclient::decode::Province {
    o.province(p, q).expect("province")
}
