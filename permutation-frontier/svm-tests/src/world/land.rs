//! World builders for rings, provinces, citizens, tickets and holdings
//! (W3-A, M1 contract §5.9). Everything goes through the program's own
//! instructions; the few crafted accounts (a season status after
//! EndSeason, which is W4-B's; an anchor and seed cache for the trace
//! build, which verifies real rounds only) are written byte for byte in the
//! frozen `frontier-abi` layouts and say so where they are used.
//!
//! Pinned rules the tests recompute natively (W3-A notes §4): the ticket
//! score [`ticket_score`], the site generation bump, the terrain digest
//! [`terrain_digest`].

use fclient::ix::{Displaced, Player, SeedSource, Site};
use frontier_abi::layout::beacon::{bell_anchor as BA, seed_cache as SC};
use frontier_abi::layout::player::{citizen as C, holding as H, ticket_site as TS};
use frontier_abi::layout::province as PV;
use frontier_abi::layout::world::{frontier as FR, ring_seed as RS, season as S};
use permutation_rules::frontier::beacon as kb;
use permutation_rules::frontier::clash::QUICKNET;
use permutation_rules::frontier::geometry::{region_of, ProvinceCoord, PROVINCE_TILES};
use permutation_rules::frontier::terrain::{generate_province, ProvinceTerrain};
use solana_address::Address;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_signer::Signer;

use super::World;
use crate::chain::{expect_lands, Chain, SendResult};
use crate::ix::{citizen as cix, map as mix};
use crate::records::le;

/// A joined player: its wallet (actor and payer) and faction.
pub struct Citizen {
    pub wallet: Keypair,
    pub faction: u8,
}

impl Citizen {
    /// Self-funded: the wallet is actor, payer and owner.
    pub fn player(&self) -> Player {
        Player {
            actor: self.wallet.pubkey(),
            payer: self.wallet.pubkey(),
            wallet: self.wallet.pubkey(),
        }
    }
    pub fn key(&self) -> Address {
        self.wallet.pubkey()
    }
}

/// The provinces of `ring` in `wedge` (`None`: every wedge), sorted.
pub fn provinces_of(ring: u32, wedge: Option<u8>) -> Vec<(i16, i16)> {
    let r = ring as i32;
    let mut v = vec![];
    for p in -r..=r {
        for q in -r..=r {
            let c = ProvinceCoord::new(p, q);
            if c.ring() == ring && (wedge.is_none() || c.wedge() == wedge) {
                v.push((p as i16, q as i16));
            }
        }
    }
    v
}

/// Beacon region of a province.
pub fn region(p: i16, q: i16) -> u8 {
    region_of(ProvinceCoord::new(p as i32, q as i32))
}

/// The kernel's terrain of a province under `ring_seed`.
pub fn terrain(ring_seed: &[u8; 32], p: i16, q: i16) -> ProvinceTerrain {
    generate_province(ring_seed, ProvinceCoord::new(p as i32, q as i32))
}

/// The pinned compact terrain block (the program's encoding, W3-A notes
/// §4): terrain classes, resources, sites, site count, the passable and
/// rough masks, road and explored zero.
pub fn terrain_block(t: &ProvinceTerrain) -> Vec<u8> {
    let mut b = vec![0u8; PV::province::SITE_MIRROR - PV::province::TERRAIN];
    let base = PV::province::TERRAIN;
    let mut passable = 0u64;
    let mut rough = 0u64;
    for i in 0..PROVINCE_TILES {
        b[PV::province::TERRAIN - base + i] = t.terrain[i] as u8;
        b[PV::province::RESOURCE - base + i] = t.resource[i].map_or(0, |r| 1 + r as u8);
        if t.terrain[i].is_passable() {
            passable |= 1 << i;
        }
        if t.terrain[i].info().defense_bps < 10_000 {
            rough |= 1 << i;
        }
    }
    b[PV::province::SITES - base..PV::province::SITES - base + 12].copy_from_slice(&t.sites);
    b[PV::province::SITE_COUNT - base] = t.site_count;
    b[PV::province::PASSABLE_MASK - base..PV::province::PASSABLE_MASK - base + 8]
        .copy_from_slice(&passable.to_le_bytes());
    b[PV::province::ROUGH_MASK - base..PV::province::ROUGH_MASK - base + 8]
        .copy_from_slice(&rough.to_le_bytes());
    b
}

/// The PROVINCE_OPEN terrain digest: `sha256("PSF-TERRAIN-v1" ‖ block)`.
pub fn terrain_digest(t: &ProvinceTerrain) -> [u8; 32] {
    crate::sha256(&[b"PSF-TERRAIN-v1", &terrain_block(t)])
}

/// The pinned ticket score: `rand(S, "site", le32(P) ‖ le32(Q) ‖ site ‖
/// le64(citizen_tag))`.
pub fn ticket_score(seed: &[u8; 32], p: i16, q: i16, site: u8, citizen_tag: u64) -> u64 {
    let mut id = vec![];
    id.extend_from_slice(&(p as i32).to_le_bytes());
    id.extend_from_slice(&(q as i32).to_le_bytes());
    id.push(site);
    id.extend_from_slice(&citizen_tag.to_le_bytes());
    permutation_rules::rng::rand(seed, b"site", &id)
}

/// `(seq, head)` style readers over raw account data.
pub fn rd_u8(d: &[u8], off: usize) -> u8 {
    d[off]
}
pub fn rd_u16(d: &[u8], off: usize) -> u16 {
    le(&d[off..off + 2]) as u16
}
pub fn rd_u32(d: &[u8], off: usize) -> u32 {
    le(&d[off..off + 4]) as u32
}
pub fn rd_u64(d: &[u8], off: usize) -> u64 {
    le(&d[off..off + 8])
}
pub fn rd_i64(d: &[u8], off: usize) -> i64 {
    le(&d[off..off + 8]) as i64
}

impl World {
    /// The genesis ring `g`.
    pub fn genesis_ring(&self) -> u16 {
        self.params.season.genesis_ring as u16
    }

    pub fn now_bell(&self, c: &Chain) -> u32 {
        self.bell_at(c.now).unwrap_or(0)
    }

    // ------------------------------------------------------------ rings and provinces

    pub fn open_ring_ix(&self, d: u16) -> Instruction {
        mix::open_ring(&self.a, self.keeper.pubkey(), d)
    }

    /// OpenRing(d) paid by the keeper.
    pub fn open_ring(&self, c: &mut Chain, d: u16) -> SendResult {
        let ix = self.open_ring_ix(d);
        c.send(&[ix], &[&self.keeper])
    }

    /// OpenRing for every genesis ring `0..=g`.
    pub fn open_genesis_rings(&self, c: &mut Chain) {
        for d in 0..=self.genesis_ring() {
            expect_lands(self.open_ring(c, d), "OpenRing (genesis ring)");
        }
    }

    /// The RingSeed's stored seed.
    pub fn ring_seed_of(&self, c: &Chain, d: u16) -> [u8; 32] {
        let rd = c.data(&self.a.ring_seed(d));
        rd[RS::SEED..RS::SEED + 32].try_into().expect("32")
    }

    pub fn open_province_ix(&self, p: i16, q: i16) -> Instruction {
        mix::open_province(&self.a, self.keeper.pubkey(), p, q)
    }

    pub fn open_province(&self, c: &mut Chain, p: i16, q: i16) -> SendResult {
        let ix = self.open_province_ix(p, q);
        c.send(&[ix], &[&self.keeper])
    }

    /// Opens every province of `ring` in `wedge` (`None`: all wedges).
    pub fn open_provinces(&self, c: &mut Chain, ring: u32, wedge: Option<u8>) -> Vec<(i16, i16)> {
        let v = provinces_of(ring, wedge);
        for (p, q) in &v {
            expect_lands(self.open_province(c, *p, *q), "OpenProvince");
        }
        v
    }

    pub fn fold_ix(&self, part: u8) -> Instruction {
        mix::fold_occupancy(&self.a, self.keeper.pubkey(), part)
    }

    pub fn fold_part(&self, c: &mut Chain, part: u8) -> SendResult {
        let ix = self.fold_ix(part);
        c.send(&[ix], &[&self.keeper])
    }

    /// The three FoldOccupancy parts in one bell.
    pub fn fold(&self, c: &mut Chain) {
        for part in 0..3 {
            expect_lands(self.fold_part(c, part), "FoldOccupancy");
        }
    }

    /// A running season with parameters `p`, through the program's own
    /// lifecycle instructions (as [`World::running`]).
    pub fn running_with(c: &mut Chain, id: u64, p: frontier_abi::presets::SeasonParams) -> World {
        let w = World::with_params(c, id, p);
        expect_lands(w.announce(c), "AnnounceSeason");
        expect_lands(w.create(c), "CreateSeason");
        expect_lands(w.init_logs(c), "InitBeaconLogs");
        for f in 0..6 {
            expect_lands(w.init_shards(c, f), "InitShards");
        }
        expect_lands(w.consume_genesis(c), "ConsumeGenesisSeed");
        let g = w.genesis_ts();
        if c.now < g {
            c.set_time(g);
        }
        w
    }

    /// The 7-day preset stretched to the longest season (28 days, joins
    /// until day 21), for the week-scale rules (vigil changes, dormancy).
    pub fn long_params(c: &Chain) -> frontier_abi::presets::SeasonParams {
        let mut p = super::params_for(c);
        p.end_bell = 4_032;
        p.join_close_bell = 3_024;
        p
    }

    /// [`World::land`] on the long season.
    pub fn land_long(c: &mut Chain, id: u64, wedge: u8) -> (World, Vec<(i16, i16)>) {
        let p = World::long_params(c);
        let w = World::running_with(c, id, p);
        w.open_genesis_rings(c);
        let v = w.open_provinces(c, 2, Some(wedge));
        (w, v)
    }

    /// A running world with the genesis rings open and the provinces of
    /// ring 2 in `wedge` open.
    pub fn land(c: &mut Chain, id: u64, wedge: u8) -> (World, Vec<(i16, i16)>) {
        let w = World::running(c, id);
        w.open_genesis_rings(c);
        let v = w.open_provinces(c, 2, Some(wedge));
        (w, v)
    }

    // ------------------------------------------------------------ citizens

    pub fn join_ix(&self, wallet: &Keypair, faction: u8) -> Instruction {
        cix::join(
            &self.a,
            wallet.pubkey(),
            wallet.pubkey(),
            faction,
            &Address::default(),
            0,
            None,
        )
    }

    pub fn join(&self, c: &mut Chain, wallet: &Keypair, faction: u8) -> SendResult {
        let ix = self.join_ix(wallet, faction);
        c.send(&[ix], &[wallet])
    }

    /// A funded, joined citizen.
    pub fn citizen(&self, c: &mut Chain, label: &str, faction: u8) -> Citizen {
        let wallet = c.funded(format!("citizen-{}-{label}", self.id).as_bytes(), 10);
        expect_lands(self.join(c, &wallet, faction), "Join");
        Citizen { wallet, faction }
    }

    pub fn citizen_data(&self, c: &Chain, who: &Citizen) -> Vec<u8> {
        c.data(&self.a.citizen(&who.key()))
    }

    /// The Citizen's `citizen_tag` (first 8 bytes of its address, LE).
    pub fn citizen_tag(&self, who: &Citizen) -> u64 {
        le(&self.a.citizen(&who.key()).as_ref()[..8])
    }

    // ------------------------------------------------------------ tickets

    pub fn file_ticket_ix(&self, who: &Citizen, sites: &[Site]) -> Instruction {
        cix::file_ticket(&self.a, &who.player(), sites)
    }

    pub fn file_ticket(&self, c: &mut Chain, who: &Citizen, sites: &[Site]) -> SendResult {
        let ix = self.file_ticket_ix(who, sites);
        c.send(&[ix], &[&who.wallet])
    }

    /// The citizen's open ticket: `(ticket_bell, sites)` as stored.
    pub fn ticket_of(&self, c: &Chain, who: &Citizen) -> (u32, Vec<Site>) {
        let d = self.citizen_data(c, who);
        let bell = rd_u32(&d, C::TICKET_BELL);
        let mut v = vec![];
        for i in 0..3 {
            let o = C::TICKET_SITES + i * TS::SIZE;
            let s = Site {
                p: rd_u16(&d, o + TS::P) as i16,
                q: rd_u16(&d, o + TS::Q) as i16,
                site: d[o + TS::SITE],
            };
            if (s.p, s.q, s.site) == (0, 0, 0) {
                break;
            }
            v.push(s);
        }
        (bell, v)
    }

    /// THE anchor of `(bell, region)` and a seed cache (nonce 0), each
    /// posted if absent; the Clock moves to `S(A)` if earlier.
    pub fn seed_ready(&self, c: &mut Chain, bell: u32, region: u8) {
        if c.is_absent(&self.a.anchor(bell, region)) {
            expect_lands(self.post_anchor(c, bell, region), "PostAnchor");
        }
        if c.is_absent(&self.a.seed_cache(bell, region, 0)) {
            expect_lands(self.post_seed(c, bell, region, 0), "PostSeed");
        } else {
            let t = World::round_time(self.seed_round(c, bell, region));
            if c.now < t {
                c.set_time(t);
            }
        }
    }

    /// The stored seed of `S(bell, region)` (cache nonce 0).
    pub fn bell_seed(&self, c: &Chain, bell: u32, region: u8) -> [u8; 32] {
        self.cache_seed(c, bell, region, 0)
    }

    /// SettleTicket(k) for `who`'s open ticket, the seed from cache nonce 0,
    /// with the displaced holder when given.
    pub fn settle_ticket_ix(
        &self,
        c: &Chain,
        who: &Citizen,
        k: u8,
        displaced: Option<&Displaced>,
    ) -> Instruction {
        self.settle_ticket_ix_src(c, who, k, displaced, SeedSource::Cache { nonce: 0 })
    }

    pub fn settle_ticket_ix_src(
        &self,
        c: &Chain,
        who: &Citizen,
        k: u8,
        displaced: Option<&Displaced>,
        src: SeedSource,
    ) -> Instruction {
        let (bell, sites) = self.ticket_of(c, who);
        let site = sites[k as usize];
        cix::settle_ticket(
            &self.a,
            self.keeper.pubkey(),
            &who.key(),
            who.faction,
            k,
            site,
            bell,
            &sites,
            src,
            displaced,
        )
    }

    pub fn settle_ticket(
        &self,
        c: &mut Chain,
        who: &Citizen,
        k: u8,
        displaced: Option<&Displaced>,
    ) -> SendResult {
        let ix = self.settle_ticket_ix(c, who, k, displaced);
        c.send(&[ix], &[&self.keeper])
    }

    /// The displaced-holder accounts of the holding at a site.
    pub fn displaced_at(&self, c: &Chain, p: i16, q: i16, site: u8, faction: u8) -> Displaced {
        let h = c.data(&self.a.holding(p as i32, q as i32, site));
        let owner = Address::new_from_array(
            h[H::OWNER_CITIZEN..H::OWNER_CITIZEN + 32]
                .try_into()
                .unwrap(),
        );
        let rent_payer =
            Address::new_from_array(h[H::RENT_PAYER..H::RENT_PAYER + 32].try_into().unwrap());
        let cd = c.data(&owner);
        let wallet = Address::new_from_array(cd[C::WALLET..C::WALLET + 32].try_into().unwrap());
        Displaced {
            rent_payer,
            wallet,
            faction,
        }
    }

    /// Ticket score of `who` for a site under `seed`.
    pub fn score_of(&self, seed: &[u8; 32], who: &Citizen, s: Site) -> u64 {
        ticket_score(seed, s.p, s.q, s.site, self.citizen_tag(who))
    }

    /// A province's site count as stored.
    pub fn site_count(&self, c: &Chain, p: i16, q: i16) -> u8 {
        c.data(&self.a.province(p as i32, q as i32))[PV::province::SITE_COUNT]
    }

    /// The Frontier's `rings_opened`.
    pub fn rings_opened(&self, c: &Chain) -> u16 {
        rd_u16(&c.data(&self.a.frontier()), FR::RINGS_OPENED)
    }

    // ------------------------------------------------------------ crafted state

    /// Crafted: the Season's stored status (EndSeason and AbortSeason are
    /// W4-B's). Byte `STATUS` of the frozen layout only.
    pub fn craft_status(&self, c: &mut Chain, status: u8) {
        c.edit(&self.a.season, |d| d[S::STATUS] = status);
    }

    /// `end = bell_start(end_bell)`.
    pub fn end_ts(&self) -> i64 {
        kb::bell_start(self.genesis_ts(), self.params.season.end_bell)
    }

    /// Crafted: THE anchor of `(bell, region)` landed at `a` and a seed
    /// cache (nonce 0) of round `S(A)` holding `seed`, written exactly as
    /// PostAnchor and PostSeed leave them (used on the trace build, which
    /// verifies real rounds only).
    pub fn craft_seed(&self, c: &mut Chain, bell: u32, region: u8, a: i64, seed: [u8; 32]) {
        let ak = self.a.anchor(bell, region);
        let mut d = vec![0u8; BA::SIZE];
        d[..8].copy_from_slice(&BA::MAGIC);
        d[8..16].copy_from_slice(&self.id.to_le_bytes());
        d[BA::BELL..BA::BELL + 4].copy_from_slice(&bell.to_le_bytes());
        d[BA::REGION] = region;
        d[BA::NET] = S::NETWORK_QUICKNET;
        d[BA::ROUND..BA::ROUND + 8].copy_from_slice(&self.tlock_round(bell).to_le_bytes());
        d[BA::A..BA::A + 8].copy_from_slice(&a.to_le_bytes());
        d[BA::RENT_TO..BA::RENT_TO + 32].copy_from_slice(self.keeper.pubkey().as_ref());
        c.put_program_account(ak, d);
        let close = kb::reveal_close(a, self.window(c, bell));
        let round = kb::seed_round(&QUICKNET, close, self.params.season.seed_margin);
        let ck = self.a.seed_cache(bell, region, 0);
        let mut d = vec![0u8; SC::SIZE];
        d[..8].copy_from_slice(&SC::MAGIC);
        d[8..16].copy_from_slice(&self.id.to_le_bytes());
        d[SC::BELL..SC::BELL + 4].copy_from_slice(&bell.to_le_bytes());
        d[SC::REGION] = region;
        d[SC::ROUND..SC::ROUND + 8].copy_from_slice(&round.to_le_bytes());
        d[SC::SEED..SC::SEED + 32].copy_from_slice(&seed);
        d[SC::ANCHOR_KEY..SC::ANCHOR_KEY + 32].copy_from_slice(ak.as_ref());
        d[SC::A..SC::A + 8].copy_from_slice(&a.to_le_bytes());
        d[SC::RENT_TO..SC::RENT_TO + 32].copy_from_slice(self.keeper.pubkey().as_ref());
        c.put_program_account(ck, d);
    }
}

/// A chained account followed through one transaction's records, matching
/// links by continuation: a record may carry several links of one entity
/// kind (SETTLE with a displacement chains two Citizens), so the link that
/// continues this account's chain (`seq + 1`, `head = sha256(prev ‖
/// le64(seq) ‖ body_without_tail)`) is the one taken. The account must end
/// on its last link, or be closed with `CLOSE` as its last record. (The
/// harness's `records::ChainWatch` takes the first link of a kind; W2-B's
/// file, so this lives here: integ-W2 "ChainWatch link matching".)
pub struct ChainTrack {
    pub address: Address,
    pub entity: frontier_abi::log::EntityKind,
    pub before: (u64, [u8; 32]),
}

impl ChainTrack {
    pub fn new(c: &Chain, address: Address, entity: frontier_abi::log::EntityKind) -> ChainTrack {
        ChainTrack {
            address,
            entity,
            before: crate::records::head_of(c, &address),
        }
    }

    #[track_caller]
    pub fn check(
        &self,
        c: &Chain,
        logs: &[String],
        min_links: usize,
    ) -> Vec<frontier_abi::log::Link> {
        use frontier_abi::log::{next_head, Kind};
        let (mut seq, mut head) = self.before;
        let mut seen = vec![];
        let mut last_kind = None;
        for r in crate::records::records(logs) {
            let hit = r.links.iter().copied().find(|l| {
                l.entity == self.entity
                    && l.seq == seq + 1
                    && l.head == next_head(&head, l.seq, &r.body_without_tail)
            });
            if let Some(l) = hit {
                seq = l.seq;
                head = l.head;
                seen.push(l);
                last_kind = Some(r.kind);
            }
        }
        assert!(
            seen.len() >= min_links,
            "{:?} {}: {} links continue its chain, want ≥ {min_links}",
            self.entity,
            self.address,
            seen.len()
        );
        if c.is_absent(&self.address) {
            assert_eq!(
                last_kind,
                Some(Kind::CLOSE),
                "a closed account's last record is CLOSE"
            );
        } else {
            assert_eq!(
                crate::records::head_of(c, &self.address),
                (seq, head),
                "{:?} {}: the header ends on its last link",
                self.entity,
                self.address
            );
        }
        seen
    }
}
