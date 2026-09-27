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
use frontier_agents::keys;
use frontier_agents::obs::{BellView, MeView, Observation, Overview, ProvinceView, SeasonView};
use frontier_agents::policy::{
    self, decide, Ctx, Intent, MarchMemo, Memory, RevealRoute, SealKind,
};
use frontier_agents::profile::{AgentSpec, Arch};
use frontier_agents::Persona;
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
