//! Vision, memory and the belief state (§7.3, §7.4).

use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::gov;
use permutation_rules::hex::Hex;
use permutation_rules::map::Terrain;
use permutation_rules::orders::Good;
use permutation_rules::rng::Seed;
use permutation_rules::state::{Delivery, Skip};
use permutation_rules::state::{Owner, Relation, WorldState};
use permutation_rules::tech::Tech;
use permutation_rules::vision::{belief, between, sees, visible, Memory};
use permutation_rules::{Preset, Ruleset};

const WORLD: Seed = [31; 32];
const SEASON: Seed = [41; 32];

fn setup() -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let entries: Vec<Entry> = (0..6)
        .map(|i| Entry {
            name: format!("civ-{i}"),
            treasury: 0,
        })
        .collect();
    let state = new_season(&rules, &WORLD, &SEASON, &entries).expect("genesis");
    (rules, state)
}

fn idx(s: &WorldState, h: Hex) -> usize {
    s.map.index_of(h).expect("on map")
}

/// Clear the map to grassland so a test controls every line of sight.
fn flatten(s: &mut WorldState) {
    for t in &mut s.map.tiles {
        t.terrain = Terrain::Grassland;
    }
}

#[test]
fn hex_lines_are_contiguous_and_exclude_the_ends() {
    let pairs = [
        (Hex::new(0, 0), Hex::new(4, -2)),
        (Hex::new(-3, 3), Hex::new(2, -1)),
        (Hex::new(1, 1), Hex::new(1, 5)),
    ];
    for (a, b) in pairs {
        let line = between(a, b);
        assert_eq!(line.len() as u32, a.distance(b) - 1);
        let mut prev = a;
        for h in line.iter().copied().chain([b]) {
            assert_eq!(prev.distance(h), 1, "{a:?}->{b:?} jumps at {h:?}");
            prev = h;
        }
    }
    // A tie (exactly between two hexes) always resolves the same way.
    assert_eq!(
        between(Hex::new(0, 0), Hex::new(2, -1)),
        between(Hex::new(0, 0), Hex::new(2, -1))
    );
    assert!(between(Hex::new(0, 0), Hex::new(1, 0)).is_empty());
}

#[test]
fn mountains_block_sight_beyond_but_are_themselves_seen() {
    let (_, mut s) = setup();
    flatten(&mut s);
    let (a, m, b) = (Hex::new(0, 0), Hex::new(1, 0), Hex::new(2, 0));
    assert!(sees(&s, a, 2, b));
    let i = idx(&s, m);
    s.map.tiles[i].terrain = Terrain::Mountain;
    assert!(sees(&s, a, 2, m), "the mountain itself is visible");
    assert!(!sees(&s, a, 2, b), "nothing behind it is");
    assert!(!sees(&s, a, 1, b), "and range still applies");
}

#[test]
fn civs_see_around_their_units_and_cities_and_share_with_allies() {
    let (_, mut s) = setup();
    flatten(&mut s);
    let seen = visible(&s, 0);
    let cap = s.cities[s.civs[0].capital.unwrap() as usize].hex;
    for (i, t) in s.map.tiles.iter().enumerate() {
        if t.hex.distance(cap) <= 2 {
            assert!(seen[i], "capital sees {:?}", t.hex);
        }
    }
    // Another civ's capital is far away and unseen until allied.
    let other = s.cities[s.civs[1].capital.unwrap() as usize].hex;
    assert!(!seen[idx(&s, other)]);
    s.set_relation(0, 1, Relation::Alliance { leaving_at: None });
    assert!(visible(&s, 0)[idx(&s, other)], "allies share vision");
}

#[test]
fn belief_hides_units_remembers_cities_and_strips_private_data() {
    let (_, mut s) = setup();
    flatten(&mut s);
    let me = 0;
    let their_city = s.civs[1].capital.unwrap();
    let their_hex = s.cities[their_city as usize].hex;
    let their_unit = s
        .units
        .iter()
        .find(|u| u.owner == Owner::Civ(1))
        .unwrap()
        .id;
    s.civs[1].gold = 77_000;
    s.civs[1].techs.insert(Tech::Writing);

    // Never seen: the foreign city and unit are absent, borders unknown.
    let mut mem = Memory::new(&s);
    let seen = visible(&s, me);
    mem.update(&s, me, &seen);
    let b = belief(&s, me, &seen, &mem);
    assert!(!b.cities[their_city as usize].alive);
    assert!(!b.units[their_unit as usize].alive);
    assert_eq!(b.map.tiles[idx(&s, their_hex)].owner_city, None);
    assert_eq!(b.civs[1].gold, 0);
    assert!(!b.civs[1].techs.has(Tech::Writing));
    assert_eq!(
        b.civs[me as usize].gold, s.civs[me as usize].gold,
        "own data is live"
    );

    // Walk our scout next to their capital: city, border and unit become visible.
    let scout = s
        .units
        .iter()
        .find(|u| {
            u.owner == Owner::Civ(me) && u.unit_type == permutation_rules::units::UnitType::Scout
        })
        .unwrap()
        .id;
    s.units[scout as usize].hex = their_hex.neighbors()[0];
    s.units[their_unit as usize].hex = their_hex;
    let seen = visible(&s, me);
    mem.update(&s, me, &seen);
    let b = belief(&s, me, &seen, &mem);
    assert!(b.cities[their_city as usize].alive);
    assert!(b.units[their_unit as usize].alive);
    assert_eq!(b.map.tiles[idx(&s, their_hex)].owner_city, Some(their_city));
    assert!(
        b.cities[their_city as usize].queue.is_empty(),
        "foreign queues stay private"
    );

    // Leave: the unit vanishes, the city stays as last seen.
    let seen_pop = s.cities[their_city as usize].pop;
    s.units[scout as usize].hex = s.cities[s.civs[me as usize].capital.unwrap() as usize].hex;
    s.cities[their_city as usize].pop = seen_pop + 3;
    s.tick += 5;
    let seen = visible(&s, me);
    mem.update(&s, me, &seen);
    let b = belief(&s, me, &seen, &mem);
    assert!(!b.units[their_unit as usize].alive);
    assert!(b.cities[their_city as usize].alive);
    assert_eq!(
        b.cities[their_city as usize].pop, seen_pop,
        "memory, not the live value"
    );
    assert_eq!(mem.city_seen(their_city), Some(0));
    assert_eq!(
        b.map.tiles[idx(&s, their_hex)].owner_city,
        Some(their_city),
        "borders remembered"
    );
}

#[test]
fn belief_keeps_other_nations_merit_deliveries_and_skips_private() {
    let (rules, mut s) = setup();
    let me = gov::join(&mut s, &rules, 0, [1; 32]).unwrap();
    let them = gov::join(&mut s, &rules, 1, [2; 32]).unwrap();
    s.members[me as usize].merit = [1, 2, 3, 4, 5];
    s.members[them as usize].merit = [9, 9, 9, 9, 9];
    s.deliveries.push(Delivery {
        civ: 0,
        good: Good::Iron,
        qty: 1,
        due: 3,
    });
    s.deliveries.push(Delivery {
        civ: 1,
        good: Good::Iron,
        qty: 7,
        due: 3,
    });
    s.last_skipped.push(Skip {
        civ: 0,
        role: 0,
        index: 0,
        reason: 1,
    });
    s.last_skipped.push(Skip {
        civ: 1,
        role: 3,
        index: 2,
        reason: 5,
    });
    let mut mem = Memory::new(&s);
    let seen = visible(&s, 0);
    mem.update(&s, 0, &seen);
    let b = belief(&s, 0, &seen, &mem);
    assert_eq!(
        b.members[me as usize].merit,
        [1, 2, 3, 4, 5],
        "own members' merit is visible"
    );
    assert_eq!(b.members[them as usize].merit, [0; 5]);
    assert_eq!(
        b.deliveries.iter().map(|d| d.civ).collect::<Vec<_>>(),
        vec![0]
    );
    assert_eq!(
        b.last_skipped.iter().map(|k| k.civ).collect::<Vec<_>>(),
        vec![0]
    );
}
