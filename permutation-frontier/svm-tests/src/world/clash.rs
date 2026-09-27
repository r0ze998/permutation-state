//! Province-bells ready to gather and resolve (W4-A, M1 contract §11 wave
//! 4): the SP-V2 fills and the 1,200-fill screen of `m1/lab/rfi-search`
//! (W1-A, CL-29) ported to M1's accounts, the I-43 storage fill, and the
//! beacon plumbing a gather, a resolve or a skip needs.
//!
//! Crafted accounts (every test that uses them says so), byte for byte in
//! the frozen `frontier-abi` layouts:
//!
//! - **The destination Province** of a [`Fill`]: the fill's terrain with
//!   its garrison tiles as the sites, the residents as roster entries
//!   (state 1, `from_bell = b − 1`, stamina clock at b), pending musters
//!   (state 2) and departed entries (state 3) as Muster and a resolve leave
//!   them, the garrisons as site mirrors in state 1 (walls committed on
//!   every third), no camp check due (`next_check_day = day(b) + 1`).
//! - **The arrivals** as a Reveal leaves an ArrivalSlot and a
//!   SettleDeparture leaves the transit record of the host's Holding
//!   (state 2, `troops_after`, `stamina_after`, `depart_bell = b − 2`), one
//!   Holding per arrival, and the ArrivalDay with b's bit.
//! - **THE anchor and a SeedCache** of `(b, region)` as PostAnchor and
//!   PostSeed leave them (release-build measurements cannot verify later
//!   rounds, W3-B's `craft_anchor`), with a chosen seed.
//!
//! Fill kinds (the lab's `Scenario::adversarial`): 0 dense, 1 pile-up, 2
//! random, 3 spread12, 4 spread12 with NEUTRAL garrisons, 5 wide. M1
//! changes: host ids are real host ids (tie keys differ from the lab's),
//! residents and garrisons fight at Hold, doctrine multipliers are the
//! factions' (asymmetric doctrines), and the storage room of I-43 applies
//! (48 residents leave 8 entries for arrivals).

use fclient::addr::{self, Addresses};
use fclient::ix::AnchorSource;
use frontier_abi::entry::{write_entry, Entry, EntryOp};
use frontier_abi::layout::beacon::seed_cache as SC;
use frontier_abi::layout::clash::arrival_slot as AS;
use frontier_abi::layout::player::{holding as H, transit as T};
use frontier_abi::layout::province::{camp as CP, entry as E, province as P, site as SM};
use frontier_abi::layout::{write_header, AccountKind};
use permutation_rules::fixed::{BPS_ONE, MILLI};
use permutation_rules::frontier::clash::{
    frontier_ruleset, resolve_clash, ClashInput, ClashOutcome, Fighter, Garrison, Occupancy,
    Relations,
};
use permutation_rules::frontier::doctrine::of_faction;
use permutation_rules::frontier::geometry::{region_of, ProvinceCoord, PROVINCE_TILES};
use permutation_rules::frontier::host::{Host, Stamina};
use permutation_rules::frontier::stance::{Posture, Stance};
use permutation_rules::frontier::terrain::{generate_province, ProvinceTerrain};
use permutation_rules::map::Terrain;
use permutation_rules::units::UnitType;
use solana_address::Address;
use solana_instruction::Instruction;
use solana_signer::Signer;

use crate::chain::Chain;
use crate::ix::clash as cix;
use crate::records::le;
use crate::world::holding::{u32_at, u64_at};
use crate::world::World;

/// Units a fill draws (Settlers never form hosts).
pub const UNITS: [UnitType; 7] = [
    UnitType::Spearman,
    UnitType::Archer,
    UnitType::Horseman,
    UnitType::Pikeman,
    UnitType::Crossbowman,
    UnitType::Knight,
    UnitType::Scout,
];

/// The lab's xorshift.
pub struct Rng(pub u64);
impl Rng {
    pub fn step(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.step() % n
    }
}

/// An entry crafted besides the roster: a pending muster (state 2) or a
/// departed host awaiting SettleDeparture (state 3).
#[derive(Clone, Copy, Debug)]
pub struct Extra {
    pub fighter: Fighter,
    pub state: u8,
}

/// One province-bell's clash as crafted accounts will hold it.
#[derive(Clone, Debug)]
pub struct Fill {
    pub name: String,
    pub p: i32,
    pub q: i32,
    pub terrain: ProvinceTerrain,
    /// Roster residents (state 1).
    pub residents: Vec<Fighter>,
    /// Garrisons (site j = garrison j; walls on every third).
    pub garrisons: Vec<Garrison>,
    /// `(faction, i, arrival)`: slot position and contents.
    pub arrivals: Vec<(u8, u8, Fighter)>,
    pub extras: Vec<Extra>,
}

/// A doctrine multiplier as stored (u16).
pub fn dealt(faction: u8, posture: Posture, arrival: bool) -> u32 {
    match of_faction(faction) {
        Some(d) => d.dealt_bps(posture, arrival),
        None => BPS_ONE,
    }
}

/// Stances in seal order.
pub fn stance_of(v: u8) -> Stance {
    Stance::from_u8(v).expect("stance")
}

/// The unit byte of a unit type (entry order).
pub fn unit_u8(u: UnitType) -> u8 {
    frontier_abi::entry::unit_to_u8(u)
}

/// The host id a fill gives resident `n` of faction `f` (sites of the
/// destination, generation 1).
pub fn resident_id(p: i32, q: i32, f: u8, n: usize) -> u64 {
    let k = f as usize * 8 + n;
    addr::host_id(p, q, (k % 12) as u8, 1, 1 + k as u32).expect("host id")
}

/// The Holding province and site of arrival position `k` (one Holding per
/// arrival, in the two provinces "north" of the destination).
pub fn arrival_home(p: i32, q: i32, k: usize) -> (i32, i32, u8) {
    (p, q + 1 + (k / 12) as i32, (k % 12) as u8)
}

/// The host id of arrival position `k`.
pub fn arrival_id(p: i32, q: i32, k: usize) -> u64 {
    let (hp, hq, s) = arrival_home(p, q, k);
    addr::host_id(hp, hq, s, 1, 1).expect("host id")
}

/// The garrison id of site `j` (the holding key, generation 1).
pub fn garrison_id(p: i32, q: i32, j: usize) -> u64 {
    addr::host_id(p, q, j as u8, 1, 0).expect("host id") & !0xFFFF_FFFF
}

impl Fill {
    /// The lab's adversarial fills (module note).
    pub fn adversarial(kind: u8, seed: u64, p: i32, q: i32) -> Fill {
        let mut terrain = generate_province(
            &crate::sha256(&[&seed.to_le_bytes()]),
            ProvinceCoord::new(p, q),
        );
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let passable: Vec<u8> = (0..PROVINCE_TILES as u8)
            .filter(|&t| terrain.terrain[t as usize].is_passable())
            .collect();
        assert!(passable.len() >= 20, "too little passable land");
        let k = match kind {
            2 => 8 + rng.below(5) as usize,
            3 | 4 => 12,
            5 => 8 + rng.below(7) as usize,
            _ => 8,
        };
        let step = passable.len() / k;
        let hexes: Vec<u8> = (0..k).map(|j| passable[j * step]).collect();
        let troops = |r: &mut Rng| (5_000 + r.below(25_000) as u32) * 1000;
        let mut residents = vec![];
        let mut per_hex = vec![0usize; k];
        for f in 0..6u8 {
            let spread: Vec<usize> = (0..k)
                .filter(|&h| (f as usize + 6 - h % 6) % 6 < 4)
                .collect();
            #[allow(clippy::needless_range_loop)]
            for n in 0..8usize {
                let h = match kind {
                    2 | 5 => loop {
                        let h = rng.below(k as u64) as usize;
                        if per_hex[h] < 6 {
                            break h;
                        }
                    },
                    3 | 4 => spread[n],
                    _ => n,
                };
                per_hex[h] += 1;
                let unit = UNITS[rng.below(7) as usize];
                let tr = troops(&mut rng);
                let st = 60 + rng.below(61) as u16;
                residents.push(Fighter {
                    id: resident_id(p, q, f, n),
                    faction: f,
                    unit,
                    troops: tr,
                    stamina: st,
                    tile: hexes[h],
                    posture: Posture::Stance(Stance::Hold),
                    retreat_bps: None,
                    dealt_bps: dealt(f, Posture::Stance(Stance::Hold), false),
                });
            }
        }
        let mut gtiles: Vec<u8> = if kind == 3 { vec![] } else { hexes.clone() };
        let mut pool: Vec<u8> = passable.clone();
        if kind == 5 {
            gtiles.clear();
            for j in (1..pool.len()).rev() {
                pool.swap(j, rng.below(j as u64 + 1) as usize);
            }
        }
        for t in pool.iter() {
            if gtiles.len() >= 12 {
                break;
            }
            if !gtiles.contains(t) && (kind != 3 || !hexes.contains(t)) {
                gtiles.push(*t);
            }
        }
        gtiles.truncate(12);
        let gfac: Vec<u8> = (0..gtiles.len())
            .map(|j| match kind {
                4 => 6,
                5 => rng.below(7) as u8,
                _ => (j % 6) as u8,
            })
            .collect();
        let garrisons: Vec<Garrison> = gtiles
            .iter()
            .enumerate()
            .map(|(j, &t)| Garrison {
                id: garrison_id(p, q, j),
                faction: gfac[j],
                tile: t,
                troops: (2_000 + rng.below(8_000) as u32) * 1000,
                walls: j % 3 == 0,
                posture: Posture::Stance(Stance::Hold),
            })
            .collect();
        let mut arrivals = vec![];
        for f in 0..6u8 {
            let spread: Vec<usize> = (0..k)
                .filter(|&h| (f as usize + 6 - h % 6) % 6 >= 4)
                .collect();
            for i in 0..4u8 {
                let n = (f as usize) * 4 + i as usize;
                let tile = match kind {
                    1 => hexes[n % 2],
                    2 | 5 => hexes[rng.below(k as u64) as usize],
                    3 | 4 => hexes[spread[i as usize]],
                    _ => hexes[n % 8],
                };
                let retreat = match (kind, n % 4) {
                    (3 | 4, _) => None,
                    (_, 0) => Some(5_000 + rng.below(20_000) as u32),
                    (_, 1) => Some(60_000),
                    _ => None,
                };
                let unit = UNITS[rng.below(7) as usize];
                let tr = troops(&mut rng);
                let st = 40 + rng.below(81) as u16;
                let stance = stance_of(rng.below(4) as u8);
                arrivals.push((
                    f,
                    i,
                    Fighter {
                        id: arrival_id(p, q, n),
                        faction: f,
                        unit,
                        troops: tr,
                        stamina: st,
                        tile,
                        posture: Posture::Stance(stance),
                        retreat_bps: retreat,
                        dealt_bps: dealt(f, Posture::Stance(stance), true),
                    },
                ));
            }
        }
        // The garrison tiles are the province's sites.
        terrain.sites = [0; 12];
        for (j, g) in garrisons.iter().enumerate() {
            terrain.sites[j] = g.tile;
        }
        terrain.site_count = garrisons.len() as u8;
        Fill {
            name: format!(
                "{}#{seed}",
                ["dense", "pileup", "random", "spread12", "spread12n", "wide"][kind as usize]
            ),
            p,
            q,
            terrain,
            residents,
            garrisons,
            arrivals,
            extras: vec![],
        }
    }

    /// The I-43 storage fill (§13.1): the first `r` residents of a dense
    /// fill spread over factions 0–4, 8 musters of faction 5 pending, `d`
    /// departed entries (faction 0), and 24 arrivals that would stay.
    pub fn storage(seed: u64, p: i32, q: i32, r: usize, d: usize) -> Fill {
        let mut f = Fill::adversarial(0, seed, p, q);
        f.name = format!("storage-r{r}-d{d}#{seed}");
        let mut keep = vec![];
        for x in f.residents.iter() {
            if x.faction < 5 && keep.len() < r {
                keep.push(*x);
            }
        }
        f.residents = keep;
        let tile = f.residents[0].tile;
        for n in 0..8 {
            f.extras.push(Extra {
                fighter: Fighter {
                    id: resident_id(p, q, 5, n),
                    faction: 5,
                    unit: UnitType::Spearman,
                    troops: 1_000_000,
                    stamina: 120,
                    tile,
                    posture: Posture::Stance(Stance::Hold),
                    retreat_bps: None,
                    dealt_bps: dealt(5, Posture::Stance(Stance::Hold), false),
                },
                state: E::STATE_MUSTER_PENDING,
            });
        }
        for n in 0..d {
            f.extras.push(Extra {
                fighter: Fighter {
                    id: addr::host_id(p, q, (n % 12) as u8, 1, 500 + n as u32).unwrap(),
                    faction: 0,
                    unit: UnitType::Spearman,
                    troops: 900_000,
                    stamina: 50,
                    tile,
                    posture: Posture::Stance(Stance::Hold),
                    retreat_bps: None,
                    dealt_bps: dealt(0, Posture::Stance(Stance::Hold), false),
                },
                state: E::STATE_DEPARTED,
            });
        }
        // Arrivals that would all stay: large, no retreat order, on hexes
        // no one holds against them (the garrison-free tiles far from the
        // residents when possible).
        for (_, _, a) in f.arrivals.iter_mut() {
            a.retreat_bps = None;
            a.troops = 29_000_000;
            a.stamina = 120;
        }
        f.garrisons.clear();
        f.terrain.site_count = 0;
        f
    }

    /// The storage room the program derives from the entries (I-43).
    pub fn occupancy(&self) -> Occupancy {
        let mut o = Occupancy::EMPTY;
        let used = self.residents.len() + self.extras.len();
        o.storage_free = (56 - used) as u8;
        for x in &self.extras {
            if x.state == E::STATE_MUSTER_PENDING {
                o.pending[x.fighter.faction as usize] += 1;
            }
        }
        o
    }

    /// The kernel run natively on the inputs the accounts hold.
    pub fn native(&self, seed: &[u8; 32], bell: u32) -> ClashOutcome {
        let arrivals: Vec<Fighter> = self.arrivals.iter().map(|(_, _, a)| *a).collect();
        let inp = ClashInput {
            province: ProvinceCoord::new(self.p, self.q),
            bell,
            seed: *seed,
            terrain: &self.terrain,
            residents: &self.residents,
            garrisons: &self.garrisons,
            arrivals: &arrivals,
            relations: Relations::ALL_HOSTILE,
            occupancy: self.occupancy(),
        };
        resolve_clash(&frontier_ruleset(), &inp).expect("native kernel")
    }

    /// The region of the destination.
    pub fn region(&self) -> u8 {
        region_of(ProvinceCoord::new(self.p, self.q))
    }

    /// The destination as `(P, Q)`.
    pub fn dest(&self) -> (i32, i32) {
        (self.p, self.q)
    }
}

/// An entry of a fighter as Muster and a resolve leave it (stamina clock
/// at `bell`).
pub fn entry_of(f: &Fighter, state: u8, bell: u32) -> Entry {
    let mut h = Host::muster(
        f.id,
        frontier_abi::addr::holding_key_of_host(f.id),
        f.faction,
        f.unit,
        f.troops.max(100_000),
        bell,
    )
    .expect("host");
    h.troops = f.troops;
    h.stamina = Stamina {
        value: f.stamina,
        bell,
    };
    Entry::from_host(
        &h,
        f.tile,
        state,
        f.dealt_bps as u16,
        bell.saturating_sub(1),
    )
}

/// The seed a crafted SeedCache carries for `(bell, region)`.
pub fn bell_seed(bell: u32, region: u8) -> [u8; 32] {
    crate::sha256(&[b"W4-A bell seed", &bell.to_le_bytes(), &[region]])
}

impl World {
    /// The destination Province of `f` (module note), resolved through
    /// `bell − 1`.
    pub fn fill_province_bytes(&self, f: &Fill, bell: u32) -> Vec<u8> {
        let mut d = vec![0u8; P::SIZE];
        assert!(write_header(&mut d, AccountKind::Province, self.id));
        let pc = ProvinceCoord::new(f.p, f.q);
        d[P::P..P::P + 2].copy_from_slice(&(f.p as i16).to_le_bytes());
        d[P::Q..P::Q + 2].copy_from_slice(&(f.q as i16).to_le_bytes());
        d[P::RING..P::RING + 2].copy_from_slice(&(pc.ring() as u16).to_le_bytes());
        d[P::WEDGE] = pc.wedge().unwrap_or(6);
        d[P::REGION] = region_of(pc);
        d[P::RESOLVED_NEXT..P::RESOLVED_NEXT + 4].copy_from_slice(&bell.to_le_bytes());
        let (mut pass, mut rough) = (0u64, 0u64);
        for i in 0..P::TILES {
            let tr = f.terrain.terrain[i];
            d[P::TERRAIN + i] = tr as u8;
            d[P::RESOURCE + i] = f.terrain.resource[i].map_or(0, |r| 1 + r as u8);
            if tr.is_passable() {
                pass |= 1 << i;
            }
            if matches!(tr, Terrain::Forest | Terrain::Hills) {
                rough |= 1 << i;
            }
        }
        d[P::PASSABLE_MASK..P::PASSABLE_MASK + 8].copy_from_slice(&pass.to_le_bytes());
        d[P::ROUGH_MASK..P::ROUGH_MASK + 8].copy_from_slice(&rough.to_le_bytes());
        d[P::SITES..P::SITES + 12].copy_from_slice(&f.terrain.sites);
        d[P::SITE_COUNT] = f.terrain.site_count;
        for s in 0..P::SITES_N {
            let o = P::site(s);
            d[o + SM::FACTION] = 6;
            d[o + SM::PEND0_BELL..o + SM::PEND0_BELL + 4]
                .copy_from_slice(&SM::NO_BELL.to_le_bytes());
            d[o + SM::PEND1_BELL..o + SM::PEND1_BELL + 4]
                .copy_from_slice(&SM::NO_BELL.to_le_bytes());
        }
        for (j, g) in f.garrisons.iter().enumerate() {
            let o = P::site(j);
            d[o + SM::STATE] = SM::STATE_HOLDING;
            d[o + SM::FACTION] = g.faction;
            d[o + SM::ORDER] = 1;
            d[o + SM::GEN] = 1;
            d[o + SM::GARRISON..o + SM::GARRISON + 4].copy_from_slice(&g.troops.to_le_bytes());
            if g.walls {
                d[o + SM::WALLS_COMMITTED..o + SM::WALLS_COMMITTED + 4]
                    .copy_from_slice(&100u32.to_le_bytes());
            }
        }
        let mut i = 0;
        for r in &f.residents {
            write_entry(&mut d, i, &entry_of(r, E::STATE_ROSTER, bell)).unwrap();
            i += 1;
        }
        for x in &f.extras {
            let mut e = entry_of(&x.fighter, x.state, bell);
            if x.state == E::STATE_MUSTER_PENDING {
                e.from_bell = bell + 1;
            }
            write_entry(&mut d, i, &e).unwrap();
            i += 1;
        }
        d[P::N_ENTRIES] = i as u8;
        let next = bell / 144 + 1;
        d[P::CAMP + CP::NEXT_CHECK_DAY..P::CAMP + CP::NEXT_CHECK_DAY + 4]
            .copy_from_slice(&next.to_le_bytes());
        d
    }

    /// A Holding with one transit record as SettleDeparture leaves it
    /// (module note).
    #[allow(clippy::too_many_arguments)]
    pub fn craft_transit_holding(
        &self,
        c: &mut Chain,
        home: (i32, i32, u8),
        faction: u8,
        a: &Fighter,
        bell: u32,
        state: u8,
    ) -> Address {
        let (hp, hq, site) = home;
        let k = self.a.holding(hp, hq, site);
        let mut d = vec![0u8; H::SIZE];
        assert!(write_header(&mut d, AccountKind::Holding, self.id));
        d[H::P..H::P + 2].copy_from_slice(&(hp as i16).to_le_bytes());
        d[H::Q..H::Q + 2].copy_from_slice(&(hq as i16).to_le_bytes());
        d[H::SITE] = site;
        d[H::GEN] = 1;
        d[H::STATE] = H::STATE_FINAL;
        d[H::FACTION] = faction;
        d[H::RENT_PAYER..H::RENT_PAYER + 32].copy_from_slice(self.keeper.pubkey().as_ref());
        let o = H::transit(0);
        d[o + T::STATE] = state;
        d[o + T::UNIT] = unit_u8(a.unit);
        d[o + T::FACTION] = faction;
        d[o + T::HOST_ID..o + T::HOST_ID + 8].copy_from_slice(&a.id.to_le_bytes());
        d[o + T::DEPART_BELL..o + T::DEPART_BELL + 4].copy_from_slice(&(bell - 2).to_le_bytes());
        d[o + T::ARRIVE_BELL..o + T::ARRIVE_BELL + 4].copy_from_slice(&bell.to_le_bytes());
        d[o + T::DEP_MASS..o + T::DEP_MASS + 4].copy_from_slice(&a.troops.to_le_bytes());
        d[o + T::TROOPS_AFTER..o + T::TROOPS_AFTER + 4].copy_from_slice(&a.troops.to_le_bytes());
        let st = a.stamina.saturating_sub(1);
        d[o + T::STAMINA_AFTER..o + T::STAMINA_AFTER + 2].copy_from_slice(&st.to_le_bytes());
        c.put_program_account(k, d);
        k
    }

    /// An ArrivalSlot of arrival `a` at `(dest, bell, f, i)` as Reveal
    /// leaves it.
    #[allow(clippy::too_many_arguments)]
    pub fn craft_full_slot(
        &self,
        c: &mut Chain,
        dest: (i32, i32),
        bell: u32,
        f: u8,
        i: u8,
        a: &Fighter,
        citizen_tag: u64,
    ) -> Address {
        let k = self.a.arrival_slot(dest.0, dest.1, bell, f, i);
        let mut d = vec![0u8; AS::SIZE];
        assert!(write_header(&mut d, AccountKind::ArrivalSlot, self.id));
        d[AS::P..AS::P + 2].copy_from_slice(&(dest.0 as i16).to_le_bytes());
        d[AS::Q..AS::Q + 2].copy_from_slice(&(dest.1 as i16).to_le_bytes());
        d[AS::BELL..AS::BELL + 4].copy_from_slice(&bell.to_le_bytes());
        d[AS::FACTION] = f;
        d[AS::I] = i;
        d[AS::UNIT] = unit_u8(a.unit);
        d[AS::STANCE] = match a.posture {
            Posture::Stance(s) => s as u8,
            _ => 0,
        };
        d[AS::TILE] = a.tile;
        let r = a.retreat_bps.unwrap_or(0) as u16;
        d[AS::RETREAT_BPS..AS::RETREAT_BPS + 2].copy_from_slice(&r.to_le_bytes());
        d[AS::HOST_ID..AS::HOST_ID + 8].copy_from_slice(&a.id.to_le_bytes());
        d[AS::CITIZEN_TAG..AS::CITIZEN_TAG + 8].copy_from_slice(&citizen_tag.to_le_bytes());
        d[AS::DEP_MASS..AS::DEP_MASS + 4].copy_from_slice(&a.troops.to_le_bytes());
        d[AS::DEALT_BPS..AS::DEALT_BPS + 2].copy_from_slice(&(a.dealt_bps as u16).to_le_bytes());
        d[AS::RENT_TO..AS::RENT_TO + 32].copy_from_slice(self.keeper.pubkey().as_ref());
        c.put_program_account(k, d);
        k
    }

    /// Crafts everything a fill's gathers and resolve read at `bell`:
    /// Province, slots, Holdings, ArrivalDay (module note).
    pub fn craft_fill(&self, c: &mut Chain, f: &Fill, bell: u32) {
        let d = self.fill_province_bytes(f, bell);
        c.put_program_account(self.a.province(f.p, f.q), d);
        for (fa, i, a) in &f.arrivals {
            let k = *fa as usize * 4 + *i as usize;
            let home = arrival_home(f.p, f.q, k);
            self.craft_transit_holding(c, home, *fa, a, bell, T::STATE_SETTLED);
            self.craft_full_slot(c, f.dest(), bell, *fa, *i, a, 0x1000 + k as u64);
        }
        if !f.arrivals.is_empty() {
            self.craft_day(c, f.dest(), bell / 144, &[bell]);
        }
    }

    /// A SeedCache of THE anchor `(bell, region)` as PostSeed leaves it,
    /// carrying `seed` (the anchor must be present).
    pub fn craft_cache(
        &self,
        c: &mut Chain,
        bell: u32,
        region: u8,
        nonce: u8,
        seed: [u8; 32],
    ) -> Address {
        let anchor = self.a.anchor(bell, region);
        let a_ts = self.anchor_a(c, bell, region).expect("THE anchor");
        let k = self.a.seed_cache(bell, region, nonce);
        let mut d = vec![0u8; SC::SIZE];
        assert!(write_header(&mut d, AccountKind::SeedCache, self.id));
        d[SC::BELL..SC::BELL + 4].copy_from_slice(&bell.to_le_bytes());
        d[SC::REGION] = region;
        d[SC::NONCE] = nonce;
        let round = self.seed_round(c, bell, region);
        d[SC::ROUND..SC::ROUND + 8].copy_from_slice(&round.to_le_bytes());
        d[SC::SEED..SC::SEED + 32].copy_from_slice(&seed);
        d[SC::ANCHOR_KEY..SC::ANCHOR_KEY + 32].copy_from_slice(anchor.as_ref());
        d[SC::A..SC::A + 8].copy_from_slice(&a_ts.to_le_bytes());
        d[SC::RENT_TO..SC::RENT_TO + 32].copy_from_slice(self.keeper.pubkey().as_ref());
        c.put_program_account(k, d);
        k
    }

    /// `close(bell)` of THE anchor `(bell, region)`.
    pub fn close_of(&self, c: &Chain, bell: u32, region: u8) -> i64 {
        let a = self.anchor_a(c, bell, region).expect("THE anchor");
        permutation_rules::frontier::beacon::reveal_close(a, self.window(c, bell))
    }

    /// THE anchor of `(bell, region)` (crafted at `bell_end + 1` when
    /// absent) and, with `seed`, a SeedCache; the Clock moves past the
    /// reveal close. Returns the seed.
    pub fn ready_bell(
        &self,
        c: &mut Chain,
        bell: u32,
        region: u8,
        seed: Option<[u8; 32]>,
    ) -> [u8; 32] {
        if self.anchor_a(c, bell, region).is_none() {
            let a = self.bell_end(bell) + 1;
            self.craft_anchor(c, bell, region, a, c.slot);
        }
        let s = seed.unwrap_or_else(|| bell_seed(bell, region));
        if seed.is_some() || c.is_absent(&self.a.seed_cache(bell, region, 0)) {
            self.craft_cache(c, bell, region, 0, s);
        }
        let close = self.close_of(c, bell, region);
        if c.now < close {
            c.set_time(close);
        }
        s
    }

    /// GatherClash of positions `start..start + n` of `(dest, bell)` from
    /// the slots on chain (their Holdings from the slots' host ids).
    pub fn gather_ix(
        &self,
        c: &Chain,
        dest: (i32, i32),
        bell: u32,
        start: u8,
        n: u8,
    ) -> Instruction {
        let mut slots = vec![];
        let mut holdings = vec![];
        let mut bitmap = 0u32;
        for k in start..start + n {
            let (f, i) = (k / 4, k % 4);
            slots.push((f, i));
            let sd = c.data(&self.a.arrival_slot(dest.0, dest.1, bell, f, i));
            if sd.len() == AS::SIZE {
                let id = u64_at(&sd, AS::HOST_ID);
                let parts = frontier_abi::addr::split_host_id(id).expect("host id");
                holdings.push(
                    self.a
                        .holding(parts.province.p, parts.province.q, parts.site),
                );
                bitmap |= 1 << k;
            }
        }
        cix::gather(
            &self.a,
            self.keeper.pubkey(),
            dest,
            bell,
            AnchorSource::Anchor,
            start,
            &slots,
            &holdings,
            bitmap,
            &self.keeper.pubkey(),
        )
    }

    /// The gathers of every position in `parts` (`(start, n)`).
    pub fn gather_parts(
        &self,
        c: &Chain,
        dest: (i32, i32),
        bell: u32,
        parts: &[(u8, u8)],
    ) -> Vec<Instruction> {
        parts
            .iter()
            .map(|&(s, n)| self.gather_ix(c, dest, bell, s, n))
            .collect()
    }

    /// ResolveFromInputs of `(dest, bell)` from THE anchor's cache (nonce 0).
    pub fn resolve_ix(&self, dest: (i32, i32), bell: u32) -> Instruction {
        cix::resolve(
            &self.a,
            self.keeper.pubkey(),
            dest,
            bell,
            &self.keeper.pubkey(),
        )
    }

    /// SkipQuiet of `n` bells from `b0` (every anchor present).
    pub fn skip_ix(&self, dest: (i32, i32), b0: u32, n: u8) -> Instruction {
        cix::skip(&self.a, self.keeper.pubkey(), dest, b0, n)
    }

    /// `resolved_next` of a Province.
    pub fn resolved_next(&self, c: &Chain, dest: (i32, i32)) -> u32 {
        u32_at(&c.data(&self.a.province(dest.0, dest.1)), P::RESOLVED_NEXT)
    }

    /// Entries of a Province (index, entry) that are not free.
    pub fn entries(&self, c: &Chain, dest: (i32, i32)) -> Vec<(usize, Entry)> {
        let d = c.data(&self.a.province(dest.0, dest.1));
        (0..P::ENTRIES_N)
            .map(|i| (i, frontier_abi::entry::read_entry(&d, i).unwrap()))
            .filter(|(_, e)| e.state != E::STATE_FREE)
            .collect()
    }

    /// The camp record of a Province: `(tile, state, troops, next_check_day, gen)`.
    pub fn camp(&self, c: &Chain, dest: (i32, i32)) -> (u8, u8, u32, u32, u32) {
        let d = c.data(&self.a.province(dest.0, dest.1));
        let o = P::CAMP;
        (
            d[o + CP::TILE],
            d[o + CP::STATE],
            u32_at(&d, o + CP::TROOPS),
            u32_at(&d, o + CP::NEXT_CHECK_DAY),
            u32_at(&d, o + CP::GEN),
        )
    }

    /// The last outcome digest of a Province.
    pub fn last_digest(&self, c: &Chain, dest: (i32, i32)) -> [u8; 32] {
        let d = c.data(&self.a.province(dest.0, dest.1));
        d[P::LAST_DIGEST..P::LAST_DIGEST + 32].try_into().unwrap()
    }
}

/// Parts that cover the 24 positions, 8 at a time.
pub const THIRDS: [(u8, u8); 3] = [(0, 8), (8, 8), (16, 8)];

/// Whether a pending op is set (tests).
pub fn busy(e: &Entry) -> bool {
    e.op != EntryOp::None
}

/// A little-endian u64 of a slice (re-export for tests).
pub fn le64(b: &[u8]) -> u64 {
    le(b)
}

/// An arrival-less Province at `dest` with `residents`, resolved through
/// `bell − 1` (SkipQuiet worlds).
pub fn roster_fill(p: i32, q: i32, seed: u64, residents: Vec<Fighter>) -> Fill {
    let mut f = Fill::adversarial(0, seed, p, q);
    f.name = format!("roster#{seed}");
    f.residents = residents;
    f.arrivals.clear();
    f.garrisons.clear();
    f.terrain.site_count = 0;
    f
}

/// The AnchorSource of every bell of a skip (anchors present).
pub fn anchors(n: u8) -> Vec<AnchorSource> {
    vec![AnchorSource::Anchor; n as usize]
}

/// The address of the MILLI constant's unit (whole troops) for tests.
pub const MILLI_U32: u32 = MILLI as u32;

/// The day of a bell.
pub fn day(bell: u32) -> u32 {
    addr::day_of(bell)
}

/// Every address a fill's resolve reads (tests that forge).
pub fn fill_addresses(a: &Addresses, f: &Fill, bell: u32) -> Vec<Address> {
    let mut v = vec![a.province(f.p, f.q), a.clash_inputs(f.p, f.q, bell)];
    for k in 0..24u8 {
        v.push(a.arrival_slot(f.p, f.q, bell, k / 4, k % 4));
    }
    v
}
