//! Transits for W4-B's tests (M1 contract §11 wave 4): marches in flight
//! (transit state 2) and the destination state SettleTransit reads, for
//! every branch (valid and bad seals before and after archive, fates,
//! bounces by rank, routs, drained recipients).
//!
//! A [`Trip`] is driven through the program where the instruction exists
//! on this branch: Depart and SettleDeparture (W3-B), PostAnchor and
//! PostSeed (W2-A), Reveal (W3-B), ArchiveAnchors (W4-B). **Crafted**
//! (GatherClash, ResolveFromInputs and SkipQuiet are W4-A's, same wave):
//!
//! - the origin's resolve of the departure bell ([`World::resolve_through`]
//!   of `world::holding`, W3-B's stand-in: the Spend settles and the entry
//!   becomes departed, state 3);
//! - a **resolved ClashInputs** ([`World::craft_resolved_inputs`]) in the
//!   frozen `frontier-abi` layout as ResolveFromInputs leaves it: every
//!   position gathered, flag 2, the fate table, the resolver, and the
//!   destination's `resolved_next = arrive + 1`;
//! - a **Stays/Withdrew entry** at the destination ([`World::craft_stayer`])
//!   as ResolveFromInputs writes it (state 1, `from_bell = arrive + 1`);
//! - a **skipped** destination ([`World::skip_dest`]): `resolved_next =
//!   arrive + 1` with no inputs (SkipQuiet's effect on an idle bell).
//!
//! Every test that relies on a crafted account says so.

use fclient::ix::{HoldingRef, SettleTransitArgs};
use frontier_abi::entry::{read_entry, unit_from_u8, write_entry, Entry};
use frontier_abi::layout::clash::{arrival as AR, clash_inputs as CI};
use frontier_abi::layout::player::holding as H;
use frontier_abi::layout::province::{entry as E, province as P};
use frontier_abi::layout::{write_header, AccountKind};
use permutation_rules::frontier::beacon as kb;
use permutation_rules::frontier::host::{Host, Stamina};
use solana_address::Address;
use solana_instruction::Instruction;
use solana_signer::Signer;

use crate::chain::{expect_lands, Chain, SendResult};
use crate::fixtures::tlock::SealCase;
use crate::ix::host as hix;
use crate::world::holding::{open_path, u32_at, Estate, March};
use crate::world::World;

/// The default straight march (8 steps east) of W3-B's reveal tests.
pub const EAST8: [u8; 8] = [0; 8];

/// A march in flight.
pub struct Trip {
    pub e: Estate,
    pub m: March,
    /// The destination's region.
    pub region: u8,
    pub tip: u64,
    /// The departure bell.
    pub depart_bell: u32,
    /// The host's troops at departure (milli-troops, = the departure mass).
    pub troops: u32,
}

impl Trip {
    pub fn dest(&self) -> (i32, i32) {
        self.m.dest
    }
    pub fn arrive(&self) -> u32 {
        self.m.arrive
    }
    pub fn faction(&self) -> u8 {
        self.e.faction
    }
    pub fn href(&self) -> HoldingRef {
        self.e.href()
    }
}

/// One record of a resolved final set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rec {
    pub faction: u8,
    pub i: u8,
    pub host_id: u64,
    pub citizen_tag: u64,
    pub dep_mass: u32,
    pub troops: u32,
    pub stamina: u16,
    pub fate: u8,
    pub troops_after: u32,
}

impl Rec {
    /// The record of `t`'s own host at slot `i` with `fate`.
    pub fn of(t: &Trip, i: u8, fate: u8, troops_after: u32) -> Rec {
        Rec {
            faction: t.faction(),
            i,
            host_id: t.m.host_id,
            citizen_tag: t.e.citizen_tag(),
            dep_mass: t.troops,
            troops: t.troops,
            stamina: 40,
            fate,
            troops_after,
        }
    }
    /// Another citizen's arrival of `faction` at slot `i`.
    pub fn other(faction: u8, i: u8, host_id: u64, tag: u64, mass: u32, fate: u8) -> Rec {
        Rec {
            faction,
            i,
            host_id,
            citizen_tag: tag,
            dep_mass: mass,
            troops: mass,
            stamina: 40,
            fate,
            troops_after: mass,
        }
    }
}

/// A copy of an estate (the wallet key cloned), to depart more hosts of
/// its holding.
pub fn clone_estate(e: &Estate) -> Estate {
    Estate {
        wallet: e.wallet.insecure_clone(),
        faction: e.faction,
        p: e.p,
        q: e.q,
        site: e.site,
        gen: e.gen,
        tile: e.tile,
        holding: e.holding,
        province: e.province,
        citizen: e.citizen,
    }
}

/// The first free entry of a Province's data.
pub fn free_entry(d: &[u8]) -> usize {
    (0..P::ENTRIES_N)
        .find(|&i| read_entry(d, i).expect("entry").state == E::STATE_FREE)
        .expect("a free entry")
}

/// What a settlement names besides the trip (defaults: the keeper settles
/// for itself, the slot is the host's own or index 0, THE anchor present).
#[derive(Clone, Copy, Debug)]
pub struct Settle {
    pub slot_i: u8,
    pub anchor_present: bool,
    pub slot_beneficiary: Address,
    pub resolver: Address,
    pub beneficiary: Address,
    /// v1.7: the owner's Citizen (the camp's Works), position 13.
    pub camp_citizen: Option<Address>,
}

impl World {
    /// A march in flight (transit state 2): estate `label` of `faction` on
    /// site `site` of `home`, a Spearman host of `troops` whole troops on
    /// the holding's tile, departing now along `dirs` (crafted provinces
    /// opened on the way) to arrive at `now_bell + lead`, sealed as `case`
    /// to `T(arrive)`, tip `tip_min`, transit slot 0; the origin resolved
    /// through the departure bell (stand-in, module note) and
    /// SettleDeparture landed.
    #[allow(clippy::too_many_arguments)]
    pub fn trip(
        &self,
        c: &mut Chain,
        label: &str,
        faction: u8,
        home: (i16, i16),
        site: u8,
        troops: u32,
        dirs: &[u8],
        lead: u32,
        case: SealCase,
    ) -> Trip {
        let e = self.craft_estate(c, label, faction, home, site);
        self.trip_from(c, e, 0, troops, dirs, lead, case)
    }

    /// Another march of an estate `e` already has (a clone of a trip's
    /// estate, [`clone_estate`]) on transit slot `transit_slot`, with a
    /// fresh host (as [`World::trip`]).
    #[allow(clippy::too_many_arguments)]
    pub fn trip_from(
        &self,
        c: &mut Chain,
        e: Estate,
        transit_slot: u8,
        troops: u32,
        dirs: &[u8],
        lead: u32,
        case: SealCase,
    ) -> Trip {
        let slot = free_entry(&c.data(&e.province));
        let seq = u32_at(&c.data(&e.holding), H::HOST_SEQ);
        let id = self.craft_host(c, &e, &e.province, slot, seq, 0, troops, e.tile);
        let origin = (e.p as i32, e.q as i32, e.tile);
        open_path(self, c, origin, dirs);
        let b = self.bell(c);
        let m = self.plan_march(id, transit_slot, origin, dirs, b + lead, 1, 0, case);
        self.depart_trip(c, e, m, b, troops)
    }

    /// Departs `m` (tip `tip_min`), resolves the origin through `b`
    /// (stand-in) and lands SettleDeparture.
    pub fn depart_trip(&self, c: &mut Chain, e: Estate, m: March, b: u32, troops: u32) -> Trip {
        let tip = self.tip_min(c);
        expect_lands(
            c.send(&[self.depart_ix(&e, (e.p, e.q), &m, tip)], &[&e.wallet]),
            "Depart",
        );
        self.resolve_through(c, &e.province, b);
        let settle = hix::settle_departure(
            &self.a,
            self.keeper.pubkey(),
            (e.p, e.q),
            e.href(),
            m.transit_slot,
        );
        expect_lands(c.send(&[settle], &[&self.keeper]), "SettleDeparture");
        let region = World::region(m.dest.0, m.dest.1);
        Trip {
            e,
            m,
            region,
            tip,
            depart_bell: b,
            troops: troops * 1_000,
        }
    }

    /// PostSeed of THE anchor of `t`'s arrival (nonce 0), the Clock at `A +
    /// archive_after`, and ArchiveAnchors of the bell (lands; THE anchor
    /// closes). A bell already archived is left as it is.
    pub fn archive_trip(&self, c: &mut Chain, t: &Trip) {
        let (bell, region) = (t.arrive(), t.region);
        let Some(a) = self.anchor_a(c, bell, region) else {
            return;
        };
        if c.is_absent(&self.a.seed_cache(bell, region, 0)) {
            expect_lands(self.post_seed(c, bell, region, 0), "PostSeed");
        }
        let at = a + self.params.season.archive_after as i64;
        if c.now < at {
            c.set_time(at);
        }
        let ix = crate::ix::beacon::archive_anchors(
            &self.a,
            self.keeper.pubkey(),
            region,
            crate::world::archive_part(bell),
            &[crate::ix::beacon::ArchiveItem {
                bell,
                cache_nonce: 0,
                anchor_rent_to: self.keeper.pubkey(),
            }],
        );
        expect_lands(c.send(&[ix], &[&self.keeper]), "ArchiveAnchors");
    }

    /// THE anchor of the arrival (PostAnchor; the Clock moves to `T(arrive)`'s
    /// time). A present anchor is left as it is.
    pub fn trip_anchor(&self, c: &mut Chain, t: &Trip) {
        if self.anchor_a(c, t.arrive(), t.region).is_none() {
            expect_lands(self.post_anchor(c, t.arrive(), t.region), "PostAnchor");
        }
    }

    /// The keeper reveals `t` into slot `target_i` (THE anchor posted first).
    pub fn trip_reveal(&self, c: &mut Chain, t: &Trip, target_i: u8) -> SendResult {
        self.trip_anchor(c, t);
        // The ArrivalDay writable: required on the first reveal of the
        // province-bell, allowed on the others (`r|w`).
        let ix = self.reveal_ix(&self.keeper.pubkey(), &t.e, &t.m, target_i, true);
        c.send(&[ix], &[&self.keeper])
    }

    /// The reveal close `A + W(arrive)` of THE anchor (present).
    pub fn trip_close(&self, c: &Chain, t: &Trip) -> i64 {
        let a = self
            .anchor_a(c, t.arrive(), t.region)
            .expect("THE anchor is present");
        kb::reveal_close(a, self.window(c, t.arrive()))
    }

    /// The Clock at `close + 600 + extra` (never back).
    pub fn to_settle(&self, c: &mut Chain, t: &Trip, extra: i64) {
        let at = self.trip_close(c, t) + 600 + extra;
        if c.now < at {
            c.set_time(at);
        }
    }

    /// A resolved ClashInputs of `t`'s arrival (crafted, module note) with
    /// `recs`, the resolver given; the destination's `resolved_next =
    /// arrive + 1`.
    pub fn craft_resolved_inputs(
        &self,
        c: &mut Chain,
        t: &Trip,
        recs: &[Rec],
        resolver: &Address,
    ) -> Address {
        let (p, q) = t.dest();
        let k = self.a.clash_inputs(p, q, t.arrive());
        let mut d = vec![0u8; CI::SIZE];
        assert!(write_header(&mut d, AccountKind::ClashInputs, self.id));
        d[CI::P..CI::P + 2].copy_from_slice(&(p as i16).to_le_bytes());
        d[CI::Q..CI::Q + 2].copy_from_slice(&(q as i16).to_le_bytes());
        d[CI::BELL..CI::BELL + 4].copy_from_slice(&t.arrive().to_le_bytes());
        d[CI::ARRIVALS_MASK..CI::ARRIVALS_MASK + 4]
            .copy_from_slice(&CI::ALL_GATHERED.to_le_bytes());
        d[CI::FLAGS] = CI::FLAG_RESOLVED;
        d[CI::N_PRESENT] = recs.len() as u8;
        for r in recs {
            let o = CI::arrival(CI::position(r.faction, r.i));
            let put =
                |d: &mut [u8], f: usize, v: &[u8]| d[o + f..o + f + v.len()].copy_from_slice(v);
            put(&mut d, AR::HOST_ID, &r.host_id.to_le_bytes());
            put(&mut d, AR::CITIZEN_TAG, &r.citizen_tag.to_le_bytes());
            put(&mut d, AR::DEP_MASS, &r.dep_mass.to_le_bytes());
            put(&mut d, AR::TROOPS, &r.troops.to_le_bytes());
            put(&mut d, AR::STAMINA, &r.stamina.to_le_bytes());
            put(&mut d, AR::FACTION, &[r.faction]);
            put(&mut d, AR::UNIT, &[0]);
            put(&mut d, AR::TILE, &[t.m.dest_tile]);
            put(&mut d, AR::STANCE, &[1]);
            put(&mut d, AR::PRESENT, &[1]);
            put(&mut d, AR::FATE, &[r.fate]);
            put(&mut d, AR::TROOPS_AFTER, &r.troops_after.to_le_bytes());
        }
        d[CI::RESOLVER..CI::RESOLVER + 32].copy_from_slice(resolver.as_ref());
        d[CI::RENT_TO..CI::RENT_TO + 32].copy_from_slice(self.keeper.pubkey().as_ref());
        c.put_program_account(k, d);
        self.skip_dest(c, t);
        k
    }

    /// The destination resolved (or skipped) through the arrival bell:
    /// `resolved_next = arrive + 1` (crafted, module note).
    pub fn skip_dest(&self, c: &mut Chain, t: &Trip) {
        let (p, q) = t.dest();
        let k = self.a.province(p, q);
        self.set_resolved_next(c, &k, t.arrive() + 1);
    }

    /// `t`'s host as a destination resident (state 1, `from_bell = arrive +
    /// 1`) with `troops` milli-troops, as ResolveFromInputs writes a
    /// Stays/Withdrew arrival (crafted, module note). Returns the entry.
    pub fn craft_stayer(&self, c: &mut Chain, t: &Trip, troops: u32) -> usize {
        let (p, q) = t.dest();
        let k = self.a.province(p, q);
        let d = c.data(&k);
        let i = free_entry(&d);
        let h = Host {
            id: t.m.host_id,
            owner: frontier_abi::addr::holding_key_of_host(t.m.host_id),
            faction: t.faction(),
            unit: unit_from_u8(0).expect("unit 0"),
            troops,
            stamina: Stamina {
                value: 40,
                bell: t.arrive(),
            },
            ready_bell: t.arrive() + 2,
            pending: None,
        };
        let e = Entry::from_host(&h, t.m.dest_tile, E::STATE_ROSTER, 10_000, t.arrive() + 1);
        c.edit(&k, |d| {
            write_entry(d, i, &e).expect("entry");
            d[P::N_ENTRIES] += 1;
        });
        i
    }

    /// SettleTransit of `t` sent by `payer` with the accounts of `s`.
    pub fn settle_ix(&self, t: &Trip, payer: &Address, s: &Settle) -> Instruction {
        fclient::ix::settle_transit(
            &self.a,
            *payer,
            &SettleTransitArgs {
                holding: t.href(),
                transit_slot: t.m.transit_slot,
                commit: t.m.made.commit,
                seal: t.m.made.seal,
                beneficiary: s.beneficiary,
                dest: t.dest(),
                arrive: t.arrive(),
                faction: t.faction(),
                slot_i: s.slot_i,
                home: (t.e.p as i32, t.e.q as i32),
                anchor_present: s.anchor_present,
                slot_beneficiary: s.slot_beneficiary,
                resolver: s.resolver,
                holding_rent_payer: t.e.wallet.pubkey(),
                camp_citizen: s.camp_citizen,
            },
        )
    }

    /// The default settlement accounts: the keeper everywhere.
    pub fn settle_default(&self) -> Settle {
        let k = self.keeper.pubkey();
        Settle {
            slot_i: 0,
            anchor_present: true,
            slot_beneficiary: k,
            resolver: k,
            beneficiary: k,
            camp_citizen: None,
        }
    }
}

/// `g01_loaded_limit_*` for W4-B's instructions (§13.1, I-45): `ixs` sent
/// with `L(kind)` (`frontier_abi::budgets::loaded_limit_for` at the
/// deployed programdata length) load and land, and one page below the
/// tight limit they fail `MaxLoadedAccountsDataSizeExceeded`, charged. The
/// account sets are the tests' (W5-A runs the worst sets). Returns the
/// SIMD-0186 need.
pub fn loaded_check(
    c: &Chain,
    ix: frontier_abi::tags::Ix,
    ixs: &[Instruction],
    signers: &[&solana_keypair::Keypair],
) -> u64 {
    use crate::chain::{assert_loaded_exceeded, Profile, PAGE};
    let pd = c.programdata_len();
    let l = frontier_abi::budgets::loaded_limit_for(ix, pd);
    let p = Profile::ladder(ix, pd).with_loaded(l);
    let t = c.transaction(&p, ixs, signers);
    let need = c.loaded_size(&t.message);
    assert!(
        need <= l as u64,
        "{}: need {need} B > L(kind) {l} B",
        ix.name()
    );
    let tight = (need.div_ceil(PAGE as u64) * PAGE as u64) as u32;
    println!(
        "g01 L({}) = {l} B at programdata {pd} B: need {need} B, tight {tight} B",
        ix.name()
    );
    let mut f = c.fork();
    let landed = expect_lands(f.send_with(&p, ixs, signers), ix.name());
    assert_eq!(landed.loaded, need);
    let mut f = c.fork();
    assert_loaded_exceeded(f.send_with(&p.with_loaded(tight - PAGE), ixs, signers));
    need
}
