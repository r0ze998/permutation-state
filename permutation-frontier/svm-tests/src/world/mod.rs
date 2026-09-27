//! Worlds: a season driven through the program's own instructions, plus
//! crafted accounts for states later waves' instructions create.
//!
//! [`World`] holds one season's addresses, keys, parameters and beacon
//! source. `World::running(&mut chain, id)` goes AnnounceSeason →
//! CreateSeason → InitBeaconLogs → InitShards × 6 → ConsumeGenesisSeed →
//! Clock at `genesis_ts`, every step through the program (W2-A's
//! instructions), so a later test starts from state the program wrote.
//!
//! **Time.** The season is placed so its genesis round is a beacon the
//! build can verify: on the release binary the first SP-V2 fixture round
//! (32,551,361, ≈ 2026-09-25), on the test-beacon build any round (the same
//! placement is used so the two builds see the same Clock). AnnounceSeason
//! lands 24 h + 60 s before `t_create_min`.
//!
//! **Crafted accounts** ([`World::craft_archive`] …) stand in for accounts
//! whose creating instruction belongs to a later wave (ArchiveAnchors is
//! W4-B's); each is written byte for byte in the frozen `frontier-abi`
//! layout and says so in the test that uses it.
//!
//! Area builders: [`land`] (W3-A), [`holding`] (W3-B), [`clash`] (W4-A),
//! [`transit`] (W4-B): stubs, handed over (§11).

pub mod clash;
pub mod holding;
pub mod land;
pub mod transit;

use fclient::addr::{self, Addresses};
use fclient::ix::BeaconArg;
use frontier_abi::layout::beacon::{anchor_archive as AA, bell_anchor as BA, seed_cache as SC};
use frontier_abi::layout::world::{beacon_log as BL, season as S};
use frontier_abi::presets::{self, SeasonParams};
use permutation_rules::frontier::beacon as kb;
use permutation_rules::frontier::clash::QUICKNET;
use solana_address::Address;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_signer::Signer;

use crate::chain::{expect_lands, Chain, SendResult};
use crate::fixtures::Beacons;
use crate::ix::{beacon as bix, season as six};
use crate::records::le;

/// The fixture round the season's genesis round is placed on.
pub const GENESIS_FIXTURE_ROUND: u64 = 32_551_361;

/// One season.
pub struct World {
    pub a: Addresses,
    pub id: u64,
    /// The program's upgrade authority (AnnounceSeason, I-51) and the
    /// season's authority.
    pub authority: Keypair,
    /// A keeper fee payer (K prologue).
    pub keeper: Keypair,
    pub params: six::Params,
    pub beacons: Beacons,
    pub t_create_min: i64,
    pub bond: u64,
}

/// The M1 7-day preset with the build's beacon key hash.
pub fn params_for(c: &Chain) -> SeasonParams {
    let mut p = presets::M1_LOCAL_7D;
    p.quicknet_pk_hash = Beacons::for_build(c.build).pk_hash();
    p
}

impl World {
    /// A world for season `id` with the 7-day preset (nothing sent yet;
    /// the authority and keeper are funded).
    pub fn new(c: &mut Chain, id: u64) -> World {
        World::with_params(c, id, params_for(c))
    }

    pub fn with_params(c: &mut Chain, id: u64, p: SeasonParams) -> World {
        let authority = c.upgrade_authority.insecure_clone();
        c.airdrop(&authority.pubkey(), 1_000_000_000_000);
        let keeper = crate::keypair(format!("keeper-{id}").as_bytes());
        c.airdrop(&keeper.pubkey(), 100_000_000_000);
        let t_create_min = Beacons::align(GENESIS_FIXTURE_ROUND, p.seed_margin);
        World {
            a: Addresses::new(c.program, id),
            id,
            authority,
            keeper,
            params: six::Params::new(p),
            beacons: Beacons::for_build(c.build),
            t_create_min,
            bond: presets::MIN_CREATION_BOND,
        }
    }

    // ------------------------------------------------------------ times

    pub fn announce_time(&self) -> i64 {
        self.t_create_min - presets::MIN_ANNOUNCE_LEAD_SECS - 60
    }
    pub fn genesis_round(&self) -> u64 {
        kb::genesis_seed_round(&QUICKNET, self.t_create_min, self.params.season.seed_margin)
    }
    pub fn genesis_ts(&self) -> i64 {
        kb::genesis_ts(&QUICKNET, self.genesis_round())
    }
    /// `T(b)`.
    pub fn tlock_round(&self, bell: u32) -> u64 {
        kb::tlock_round(&QUICKNET, self.genesis_ts(), bell)
    }
    pub fn bell_end(&self, bell: u32) -> i64 {
        kb::bell_end(self.genesis_ts(), bell)
    }
    /// Unix time of quicknet round `r`.
    pub fn round_time(r: u64) -> i64 {
        crate::fixtures::beacons::round_time(r)
    }
    /// The bell of `t` (None before genesis).
    pub fn bell_at(&self, t: i64) -> Option<u32> {
        kb::bell_at(self.genesis_ts(), t)
    }

    // ------------------------------------------------------------ lifecycle

    pub fn announce_ix(&self) -> Instruction {
        six::announce(
            &self.a,
            self.authority.pubkey(),
            &self.params,
            self.t_create_min,
            self.bond,
        )
    }

    /// AnnounceSeason at `announce_time` (the Clock moves there if earlier).
    pub fn announce(&self, c: &mut Chain) -> SendResult {
        if c.now < self.announce_time() {
            c.set_time(self.announce_time());
        }
        let ix = self.announce_ix();
        c.send(&[ix], &[&self.authority])
    }

    pub fn create_ix(&self) -> Instruction {
        six::create(&self.a, self.authority.pubkey(), &self.params)
    }

    /// CreateSeason at `t_create_min` (the Clock moves there if earlier).
    pub fn create(&self, c: &mut Chain) -> SendResult {
        if c.now < self.t_create_min {
            c.set_time(self.t_create_min);
        }
        let ix = self.create_ix();
        c.send(&[ix], &[&self.authority])
    }

    pub fn init_logs(&self, c: &mut Chain) -> SendResult {
        let ix = six::init_beacon_logs(&self.a, self.authority.pubkey());
        c.send(&[ix], &[&self.authority])
    }

    pub fn init_shards(&self, c: &mut Chain, faction: u8) -> SendResult {
        let ix = six::init_shards(&self.a, self.authority.pubkey(), faction);
        c.send(&[ix], &[&self.authority])
    }

    pub fn genesis_arg(&self) -> BeaconArg {
        self.beacons.must(self.genesis_round())
    }

    /// ConsumeGenesisSeed with the genesis round (the Clock moves to its
    /// publication if earlier).
    pub fn consume_genesis(&self, c: &mut Chain) -> SendResult {
        let t = World::round_time(self.genesis_round());
        if c.now < t {
            c.set_time(t);
        }
        let ix = six::consume_genesis_seed(&self.a, self.keeper.pubkey(), &self.genesis_arg());
        c.send(&[ix], &[&self.keeper])
    }

    /// Announced.
    pub fn announced(c: &mut Chain, id: u64) -> World {
        let w = World::new(c, id);
        expect_lands(w.announce(c), "AnnounceSeason");
        w
    }

    /// Created (status Created).
    pub fn created(c: &mut Chain, id: u64) -> World {
        let w = World::announced(c, id);
        expect_lands(w.create(c), "CreateSeason");
        w
    }

    /// Created, 16 BeaconLogs, 48 JoinShards, genesis seed consumed (Seeded).
    pub fn seeded(c: &mut Chain, id: u64) -> World {
        let w = World::created(c, id);
        expect_lands(w.init_logs(c), "InitBeaconLogs");
        for f in 0..6 {
            expect_lands(w.init_shards(c, f), "InitShards");
        }
        expect_lands(w.consume_genesis(c), "ConsumeGenesisSeed");
        w
    }

    /// Seeded with the Clock at `genesis_ts` (effective status Running).
    pub fn running(c: &mut Chain, id: u64) -> World {
        let w = World::seeded(c, id);
        let g = w.genesis_ts();
        if c.now < g {
            c.set_time(g);
        }
        w
    }

    // ------------------------------------------------------------ beacons

    /// PostAnchor for `(bell, region)` with round `T(bell)`.
    pub fn anchor_ix(&self, bell: u32, region: u8) -> Instruction {
        let arg = self.beacons.must(self.tlock_round(bell));
        bix::post_anchor(
            &self.a,
            self.keeper.pubkey(),
            region,
            bell,
            &arg,
            &self.keeper.pubkey(),
        )
    }

    /// The Clock at `round_time(T(bell))` or later (never moves back).
    pub fn to_anchor_time(&self, c: &mut Chain, bell: u32) {
        let t = World::round_time(self.tlock_round(bell));
        if c.now < t {
            c.set_time(t);
        }
    }

    /// PostAnchor for `(bell, region)`; the Clock moves to the round's
    /// publication first if it is earlier (a test-beacon build refuses a
    /// round the Clock has not reached).
    pub fn post_anchor(&self, c: &mut Chain, bell: u32, region: u8) -> SendResult {
        self.to_anchor_time(c, bell);
        let ix = self.anchor_ix(bell, region);
        c.send(&[ix], &[&self.keeper])
    }

    /// `W(b)` from the Season account's schedule.
    pub fn window(&self, c: &Chain, bell: u32) -> u32 {
        let d = c.data(&self.a.season);
        let s = kb::WindowSchedule {
            reveal_window: le(&d[S::REVEAL_WINDOW..S::REVEAL_WINDOW + 4]) as u32,
            window_next: le(&d[S::WINDOW_NEXT..S::WINDOW_NEXT + 4]) as u32,
            window_from_bell: le(&d[S::WINDOW_FROM_BELL..S::WINDOW_FROM_BELL + 4]) as u32,
        };
        kb::window(&s, bell)
    }

    /// `A` of THE anchor `(bell, region)` as stored.
    pub fn anchor_a(&self, c: &Chain, bell: u32, region: u8) -> Option<i64> {
        let d = c.data(&self.a.anchor(bell, region));
        (d.len() == BA::SIZE).then(|| le(&d[BA::A..BA::A + 8]) as i64)
    }

    /// `S(A)` of THE anchor `(bell, region)` (reads its A and the window).
    pub fn seed_round(&self, c: &Chain, bell: u32, region: u8) -> u64 {
        let a = self
            .anchor_a(c, bell, region)
            .expect("THE anchor is present");
        let close = kb::reveal_close(a, self.window(c, bell));
        kb::seed_round(&QUICKNET, close, self.params.season.seed_margin)
    }

    pub fn seed_ix(&self, bell: u32, region: u8, nonce: u8, round: u64) -> Instruction {
        let arg = self.beacons.must(round);
        bix::post_seed(
            &self.a,
            self.keeper.pubkey(),
            region,
            bell,
            nonce,
            &arg,
            &self.keeper.pubkey(),
        )
    }

    /// PostSeed with THE anchor's `S(A)`.
    pub fn post_seed(&self, c: &mut Chain, bell: u32, region: u8, nonce: u8) -> SendResult {
        let r = self.seed_round(c, bell, region);
        let t = World::round_time(r);
        if c.now < t {
            c.set_time(t);
        }
        let ix = self.seed_ix(bell, region, nonce, r);
        c.send(&[ix], &[&self.keeper])
    }

    pub fn beacon_ix(&self, region: u8, round: u64) -> Instruction {
        let arg = self.beacons.must(round);
        bix::post_beacon(&self.a, self.keeper.pubkey(), region, &arg)
    }

    /// PostBeacon of `round` (the Clock moves to its publication if earlier).
    pub fn post_beacon(&self, c: &mut Chain, region: u8, round: u64) -> SendResult {
        let t = World::round_time(round);
        if c.now < t {
            c.set_time(t);
        }
        let ix = self.beacon_ix(region, round);
        c.send(&[ix], &[&self.keeper])
    }

    /// The BeaconLog's `latest_round`.
    pub fn latest_round(&self, c: &Chain, region: u8) -> u64 {
        let d = c.data(&self.a.beacon_log(region));
        le(&d[BL::LATEST_ROUND..BL::LATEST_ROUND + 8])
    }

    /// The SeedCache's stored seed.
    pub fn cache_seed(&self, c: &Chain, bell: u32, region: u8, nonce: u8) -> [u8; 32] {
        let d = c.data(&self.a.seed_cache(bell, region, nonce));
        d[SC::SEED..SC::SEED + 32].try_into().expect("32")
    }

    // ------------------------------------------------------------ season fields

    pub fn season_u8(&self, c: &Chain, off: usize) -> u8 {
        c.data(&self.a.season)[off]
    }
    pub fn season_u64(&self, c: &Chain, off: usize) -> u64 {
        le(&c.data(&self.a.season)[off..off + 8])
    }
    pub fn season_u32(&self, c: &Chain, off: usize) -> u32 {
        le(&c.data(&self.a.season)[off..off + 4]) as u32
    }
    pub fn status(&self, c: &Chain) -> u8 {
        self.season_u8(c, S::STATUS)
    }

    // ------------------------------------------------------------ crafted accounts

    /// A region-half-day AnchorArchive (`part` = [`archive_part`], v1.3)
    /// written as ArchiveAnchors (W4-B) leaves it: `tombstone` and
    /// `archived` bits set for `bells`, entries empty.
    pub fn craft_archive(&self, c: &mut Chain, region: u8, part: u32, bells: &[u32]) -> Address {
        let k = self.a.archive(region, part);
        let mut d = vec![0u8; AA::SIZE];
        d[..8].copy_from_slice(&AA::MAGIC);
        d[8..16].copy_from_slice(&self.id.to_le_bytes());
        d[AA::REGION] = region;
        d[AA::PART..AA::PART + 4].copy_from_slice(&part.to_le_bytes());
        for b in bells {
            for base in [AA::TOMBSTONE, AA::ARCHIVED] {
                let (i, m) = AA::bit(base, *b);
                d[i] |= m;
            }
        }
        d[AA::RENT_TO..AA::RENT_TO + 32].copy_from_slice(self.keeper.pubkey().as_ref());
        c.put_program_account(k, d);
        k
    }
}

/// `day(b)`.
pub fn day_of(bell: u32) -> u32 {
    addr::day_of(bell)
}

/// The AnchorArchive part of bell `b` (`b / 72`, v1.3 half-day archives).
pub fn archive_part(bell: u32) -> u32 {
    addr::archive_part(bell)
}
