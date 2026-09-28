//! The clash at the bell (design §6.3, §6.4).
//!
//! Everything that arrives at a province in bell b resolves together
//! against the province's **roster frozen at the start of b** (hosts and
//! garrisons whose `host::Presence` covers b). [`resolve_clash`] is a pure
//! function of that roster, the bell's revealed arrivals and postures, the
//! diplomatic relations, the province's storage room ([`Occupancy`]) and
//! the bell seed `S(b, r)`. Nothing in it depends on when the reveals or
//! the resolve landed, or on the order of any list it is given, so lag only
//! waits and never changes an outcome (§8.4).
//!
//! Steps:
//! 1. **Retreat orders.** An arrival with a `retreat_ratio` r withdraws
//!    without fighting if the frozen hostile strength on its target hex
//!    exceeds r × its own. (Evaluated against the frozen roster only, so it
//!    is done first and frees its slot.)
//! 2. **Merge** by the province caps (≤ 48 hosts, ≤ 8 per faction) and the
//!    hex fair-share rule, allocated by **side**, not by faction (allied
//!    factions cannot pool slots against a third, §6.1): the factions on a
//!    hex fall into sides, the connected groups of factions that are not
//!    hostile to each other. Each side is guaranteed
//!    `min(its hosts, ⌊6 / sides⌋)` slots and the rest go by mass. On a
//!    holding's hex the sides hostile to the owner together get at most 3
//!    (shared by the same rule) and the owner's side the rest, the owner's
//!    own hosts first. Ties go by `tie_key(seed, host_id)`. Hosts that find
//!    no room bounce home with no loss.
//!    - **(2a) The room** (I-43): the province caps count the residents and
//!      the musters still pending (they join after this clash), and an
//!      arrival is admitted only while the province stores fewer than
//!      `Occupancy::storage_free` new entries, so a Province never holds
//!      more than its 56 entries and a faction never more than 8 hosts once
//!      its musters join. Arrivals are taken in mass order.
//!    - **(2b)** the hex fair share above.
//!    - **(2c) The recount** (CL-10): a host the hex fair share bounced
//!      frees its province, faction and storage slot; the arrivals the room
//!      refused are re-admitted in mass order into the free slots of their
//!      hexes (never displacing a host the fair share admitted), in one
//!      pass.
//! 3. **Engagements.** On every hex each hostile pair of combatants fights
//!    one `combat::resolve_engagement`, reused unchanged **with both of its
//!    halves** (the attack and the defender's retaliation, with v9's
//!    retaliation modifiers: a garrison fighting as `Combatant::City`
//!    retaliates at ×0.5 and never attacks, a ranged defender retaliates at
//!    ×0.5), **from pre-clash counts**. The arrival attacks a resident; a
//!    garrison is always the defender; two arrivals or two residents
//!    engage both ways at half weight each. Every clash engagement is on
//!    one hex, so none is a ranged attack (`RANGED_ATTACK` applies to v9's
//!    attacks from range only). What a combatant deals in each of its
//!    engagements is divided by its number of engagements on the hex, then
//!    scaled by the stance table (`stance`). The variance dice of every
//!    engagement are drawn from the bell seed.
//! 4. **All damage applies at once.** Hosts below 0.5 troops are destroyed.
//! 5. **One side holds the field;** hosts hostile to it withdraw to an
//!    adjacent friendly hex of the province, or bounce home. Units with no
//!    attack and no defence (Scouts; CL-10) never contest a hex, never
//!    count in the ranking and never hold a holding's hex for a siege or
//!    defend it; they withdraw when their faction loses the hex or when
//!    they are hostile to the side that holds it.
//! 6. **Damage-ratio refund:** a faction whose damage ratio on the hex is
//!    ≥ 10 pays no engagement stamina.
//!
//! **Inputs as of the bell's start.** Everything the clash of bell b reads
//! is its value at the start of b, whenever the clash is resolved: the
//! roster (`host::Presence`), the hosts' and garrisons' troops and stamina
//! (`host::Host::values_at`, `host::GarrisonState::at`), walls
//! (`holding::Holding::walls_at`) and relations ([`RelationsLog::at`]).
//!
//! **Bounds** (CL-01, CL-06, CL-14, I-27): [`resolve_clash`] refuses input
//! an honest program never builds (troops above `MAX_HOST_TROOPS`, stamina
//! above `STAMINA_CAP`, a doctrine multiplier outside
//! `[BPS_ONE, COMBAT_MAX_BPS]`, a retreat ratio above [`RETREAT_MAX_BPS`],
//! a faction above [`NEUTRAL`]), so the damage arithmetic cannot overflow
//! ([`MAX_DAMAGE_PRODUCT_BPS`]); honest outcomes are unchanged.
//!
//! **Quiet bells.** A bell with no arrivals in which resolving would change
//! nothing ([`is_quiet`]: no hostile combatants share a hex and no hex is
//! over its fair share) needs no resolution: skipping it gives the same
//! state, and sieges count a run of quiet bells in closed form
//! (`siege::Siege::advance_quiet`). Hostile residents sharing a hex (for
//! example besiegers and a garrison that still has troops) fight every
//! bell, so such a bell is never quiet.
//!
//! **Two bodies, one rule set** (I-14): [`resolve_clash`] is the M1 lab's
//! Phase A rewrite (tile buckets, per-hex damage arrays, an occupancy
//! table for the withdraw search), digest-identical to the step-by-step
//! reference [`resolve_clash_ref`], which host builds keep as the oracle
//! (`tests/frontier_clash_equiv.rs`).

/// Version of this kernel, bound into `RULESET_HASH` (`super::KERNEL_VERSIONS`):
/// bump it whenever an honest outcome changes. v2: M1 W1-A CL-10 (one-pass cap recount, civilians never contest), I-43 storage room, Phase A body (digest-identical to the reference).
pub const CLASH_VERSION: u16 = 2;

#[cfg(any(
    test,
    feature = "std",
    not(any(target_os = "solana", target_arch = "wasm32"))
))]
use super::geometry::{tile_index, tile_offset};
use super::geometry::{ProvinceCoord, PROVINCE_TILES};
use super::host::{
    city_strength, strength, BATTLE_COOLDOWN_BELLS, DESTROYED_BELOW, ENGAGE_STAMINA,
    FACTION_RESIDENT_CAP, HEX_HOST_CAP, MAX_HOST_TROOPS, OWNER_HEX_SLOTS, PROVINCE_HOST_CAP,
    STAMINA_CAP,
};
use super::stance::{damage_bps, Posture, Stance};
use super::terrain::ProvinceTerrain;
use crate::combat::{resolve_engagement, variance, Combatant, Situation};
use crate::fixed::{Bps, MilliTroops, BPS_ONE};
use crate::hash::{sha256, Digest32};
#[cfg(any(
    test,
    feature = "std",
    not(any(target_os = "solana", target_arch = "wasm32"))
))]
use crate::hex::Hex;
use crate::params::{Preset, Ruleset};
use crate::rng::{rand, rand_id, tie_key, Seed};
use crate::units::UnitType;
#[cfg(any(
    test,
    feature = "std",
    not(any(target_os = "solana", target_arch = "wasm32"))
))]
use alloc::vec;
use alloc::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};

/// A sort key (M1 W5-A): `(a, b, c)` compared lexicographically, then the
/// element's position, so equal keys keep their order (a stable sort).
/// Three `u64` words, not a `u128`: SBF has no 128-bit compare.
#[derive(Clone, Copy)]
pub(crate) struct SortKey {
    a: u64,
    b: u64,
    c: u64,
    pos: u32,
}

impl SortKey {
    #[inline(always)]
    fn lt(&self, o: &SortKey) -> bool {
        if self.a != o.a {
            return self.a < o.a;
        }
        if self.b != o.b {
            return self.b < o.b;
        }
        if self.c != o.c {
            return self.c < o.c;
        }
        self.pos < o.pos
    }
}

/// A key for [`sort_by_key3`]: three words compared in order.
pub(crate) type Key3 = (u64, u64, u64);

/// The kernel's one sort body (M1 W5-A, `.so` size): a natural merge sort
/// of precomputed [`SortKey`]s. Every on-chain ordering of more than
/// [`SMALL_SORT`] elements encodes its key into three words and shares this
/// body, instead of one monomorphized `core::slice::sort` per key type
/// (≈ 186 KB of the release `.so` before). The keys are computed once per
/// element; ascending runs are found first (a roster passed in id order,
/// or two sorted lists one after the other, costs one scan and at most one
/// merge). The position makes every key distinct, so the result is the
/// stable order `sort_by_key` gave (the host-only reference
/// [`resolve_clash_ref`] keeps the core sorts; the equivalence test and
/// `sort_keys_equal_the_core_stable_sort` compare the two).
#[inline(never)]
pub(crate) fn sort_keys(v: &mut [SortKey]) {
    const MIN_RUN: usize = 8;
    let n = v.len();
    let mut runs: Vec<usize> = Vec::with_capacity(n / MIN_RUN + 2);
    let mut s = 0;
    while s < n {
        let mut e = s + 1;
        while e < n && v[e - 1].lt(&v[e]) {
            e += 1;
        }
        let want = if s + MIN_RUN < n { s + MIN_RUN } else { n };
        while e < want {
            let x = v[e];
            let mut j = e;
            while j > s && x.lt(&v[j - 1]) {
                v[j] = v[j - 1];
                j -= 1;
            }
            v[j] = x;
            e += 1;
        }
        runs.push(e);
        s = e;
    }
    if runs.len() < 2 {
        return;
    }
    let mut buf: Vec<SortKey> = Vec::with_capacity(n);
    while runs.len() > 1 {
        let (mut out, mut k, mut lo) = (0, 0, 0);
        while k < runs.len() {
            if k + 1 < runs.len() {
                let (mid, hi) = (runs[k], runs[k + 1]);
                merge_runs(v, lo, mid, hi, &mut buf);
                runs[out] = hi;
                lo = hi;
                k += 2;
            } else {
                runs[out] = runs[k];
                k += 1;
            }
            out += 1;
        }
        runs.truncate(out);
    }
}

/// Merges the sorted runs `v[lo..mid]` and `v[mid..hi]` (left first on a
/// tie, which cannot happen: positions differ). Only the part of the left
/// run that moves is copied out; the right run's tail already in place
/// stays.
fn merge_runs(v: &mut [SortKey], lo: usize, mid: usize, hi: usize, buf: &mut Vec<SortKey>) {
    let mut lo = lo;
    while lo < mid && !v[mid].lt(&v[lo]) {
        lo += 1;
    }
    if lo == mid {
        return;
    }
    let mut hi = hi;
    while hi > mid && !v[hi - 1].lt(&v[mid - 1]) {
        hi -= 1;
    }
    buf.clear();
    buf.extend_from_slice(&v[lo..mid]);
    let (mut i, mut j, mut w) = (0, mid, lo);
    let l = buf.len();
    while i < l && j < hi {
        if v[j].lt(&buf[i]) {
            v[w] = v[j];
            j += 1;
        } else {
            v[w] = buf[i];
            i += 1;
        }
        w += 1;
    }
    while i < l {
        v[w] = buf[i];
        i += 1;
        w += 1;
    }
}

/// Up to this many elements [`sort_by_key3`] sorts in place by insertion
/// (keys recomputed per comparison, no allocation), like the core sort's
/// small-sort path.
pub(crate) const SMALL_SORT: usize = 12;

/// `v` sorted stably by `key` (three words, compared lexicographically):
/// by insertion up to [`SMALL_SORT`] elements, else through [`sort_keys`].
#[inline(always)]
pub(crate) fn sort_by_key3<T: Copy>(v: &mut [T], key: impl Fn(&T) -> Key3) {
    let n = v.len();
    if n <= SMALL_SORT {
        for i in 1..n {
            let x = v[i];
            let kx = key(&x);
            let mut j = i;
            while j > 0 && kx < key(&v[j - 1]) {
                v[j] = v[j - 1];
                j -= 1;
            }
            v[j] = x;
        }
        return;
    }
    // Already in order (a roster passed in id order): one key per element
    // and no allocation.
    let mut prev = key(&v[0]);
    let mut i = 1;
    while i < n {
        let k = key(&v[i]);
        if k < prev {
            break;
        }
        prev = k;
        i += 1;
    }
    if i == n {
        return;
    }
    let mut ks: Vec<SortKey> = Vec::with_capacity(n);
    for (pos, x) in v.iter().enumerate() {
        let (a, b, c) = key(x);
        ks.push(SortKey {
            a,
            b,
            c,
            pos: pos as u32,
        });
    }
    sort_keys(&mut ks);
    if ks.iter().enumerate().all(|(i, k)| k.pos as usize == i) {
        return;
    }
    let out: Vec<T> = ks.iter().map(|k| v[k.pos as usize]).collect();
    v.copy_from_slice(&out);
}

/// [`Unit::order`] as a [`sort_by_key3`] key: fewer troops later, then
/// the tie key, then the id.
#[inline(always)]
fn order_key(u: &Unit) -> Key3 {
    ((u32::MAX - u.troops) as u64, u.tie, u.id)
}

/// Arrival slots per (province, bell): 4 per faction.
pub const MAX_ARRIVALS: usize = 24;
/// Holdings (sites) per province.
pub const MAX_GARRISONS: usize = 12;
/// Barbarians and Free Cities: hostile to everyone, never at peace. The
/// largest faction id a clash accepts (factions 0..=5 are the six
/// doctrines, CL-06).
pub const NEUTRAL: u8 = 6;
/// Faction ids are below this (array sizes; valid ids stop at [`NEUTRAL`]).
pub const FACTION_LIMIT: u8 = 8;
/// A side whose damage ratio reaches this pays no stamina.
pub const REFUND_RATIO: u64 = 10;

/// Whether `f` is a faction id the clash accepts from data: a player
/// faction `0..=5`, or also [`NEUTRAL`] when `allow_neutral`. A private
/// copy of `geometry::valid_faction` (contract §7; the clash keeps no
/// dependency on geometry's faction rule), pinned to it for every `u8` by
/// `frontier_clash_bounds::clash_faction_rule_equals_geometry`.
/// Rule 6: a faction that dealt damage on a hex and at least
/// [`REFUND_RATIO`] times what it took there pays no engagement stamina
/// (the one definition both bodies use).
const fn refunded(dealt: u64, taken: u64) -> bool {
    dealt > 0 && dealt >= REFUND_RATIO.saturating_mul(taken)
}

pub(crate) const fn valid_faction(f: u8, allow_neutral: bool) -> bool {
    f < NEUTRAL || (allow_neutral && f == NEUTRAL)
}

/// The largest retreat ratio (bps) a march may carry: `retreat_bps` in the
/// sealed plaintext is 0 for "never retreat" and `1..=RETREAT_MAX_BPS` for
/// a ratio; anything above is an invalid plaintext (I-27, re-exported by
/// `frontier::seal`).
pub const RETREAT_MAX_BPS: u16 = 60_000;

/// Largest stance multiplier (bps) `stance::damage_bps` gives any pairing
/// (a stance against a defender in Disarray, ×1.25).
pub const MAX_STANCE_BPS: Bps = {
    let all = [
        Posture::Stance(Stance::Hold),
        Posture::Stance(Stance::Assault),
        Posture::Stance(Stance::Flank),
        Posture::Stance(Stance::Brace),
        Posture::Disarray,
    ];
    let mut m = 0;
    let mut i = 0;
    while i < all.len() {
        let mut j = 0;
        while j < all.len() {
            let d = damage_bps(all[i], all[j]);
            if d > m {
                m = d;
            }
            j += 1;
        }
        i += 1;
    }
    m
};

/// Largest damage multiplier (bps) of one engagement half (CL-14): the
/// stance maximum times the largest doctrine multiplier (`Fighter::
/// dealt_bps`, which already folds the variant weight, the drill and the
/// arrival bonus) that `validate` accepts. `doctrine::validate_table`
/// refuses a table whose doctrines could deal more.
pub const MAX_DAMAGE_PRODUCT_BPS: u64 =
    MAX_STANCE_BPS as u64 * super::doctrine::bounds::COMBAT_MAX_BPS as u64 / BPS_ONE as u64;

// The damage arithmetic of step 3 cannot overflow: the largest host at the
// largest product fits (the CL-14 statement), and so does each intermediate
// product of `scale` for any raw damage a `MilliTroops` can hold.
const _: () = assert!(
    MAX_HOST_TROOPS as u128 * MAX_DAMAGE_PRODUCT_BPS as u128 <= u64::MAX as u128,
    "CL-14: MAX_HOST_TROOPS × MAX_DAMAGE_PRODUCT_BPS must fit u64"
);
const _: () = assert!(
    MilliTroops::MAX as u128 * MAX_STANCE_BPS as u128 / BPS_ONE as u128
        * super::doctrine::bounds::COMBAT_MAX_BPS as u128
        <= u64::MAX as u128,
    "CL-14: raw × stance × doctrine must fit u64"
);
const _: () = assert!(
    RETREAT_MAX_BPS as u32 >= BPS_ONE,
    "a ratio ≥ 1 must be expressible"
);

/// The combat constants the clash reuses from `combat` (identical in
/// every preset).
pub fn frontier_ruleset() -> Ruleset {
    Ruleset::new(Preset::Season)
}

/// Who is hostile to whom this bell: a symmetric matrix over factions
/// 0..8; a set bit means "not hostile" (Peace, NAP, Alliance). Different
/// factions are hostile by default (Rivalry, War); `NEUTRAL` always is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Relations {
    pub peaceful: u64,
}

impl Relations {
    pub const ALL_HOSTILE: Relations = Relations { peaceful: 0 };

    const fn bit(a: u8, b: u8) -> u64 {
        1 << ((a as u32 % 8) * 8 + b as u32 % 8)
    }

    pub const fn hostile(self, a: u8, b: u8) -> bool {
        if a == b {
            return false;
        }
        if a == NEUTRAL || b == NEUTRAL {
            return true;
        }
        self.peaceful & Self::bit(a, b) == 0
    }

    /// Mark a pair peaceful (or hostile again). Only the six doctrine
    /// factions 0..=5 can be: `NEUTRAL` and ids above it are refused
    /// (a no-op, CL-06).
    pub fn set_peaceful(&mut self, a: u8, b: u8, peaceful: bool) {
        if a == b || !valid_faction(a, false) || !valid_faction(b, false) {
            return;
        }
        let m = Self::bit(a, b) | Self::bit(b, a);
        if peaceful {
            self.peaceful |= m;
        } else {
            self.peaceful &= !m;
        }
    }
}

/// A Peace, NAP or Alliance decree issued during bell c takes effect at the
/// start of c + 1 …
pub const PEACE_LEAD_BELLS: u32 = 1;
/// … and a declaration of War (or breaking a treaty) after its 36-bell horn.
pub const WAR_HORN_BELLS: u32 = 36;

/// One diplomatic decree between two factions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Decree {
    /// First bell whose clash sees it.
    pub from_bell: u32,
    /// Issue order (ties at one `from_bell`: the later decree wins).
    pub seq: u64,
    pub a: u8,
    pub b: u8,
    pub peaceful: bool,
}

/// Relations by bell: the clash of bell b reads `at(b)`, the matrix in
/// force at the start of b, however late it is resolved (design §6.3,
/// §8.4). Decrees are kept until every province has resolved past them
/// (`prune`); the program stores them per change, like the bell anchors.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RelationsLog {
    /// Relations in force before every kept decree.
    pub base: Relations,
    pub decrees: Vec<Decree>,
    next_seq: u64,
}

impl RelationsLog {
    pub fn new(base: Relations) -> RelationsLog {
        RelationsLog {
            base,
            decrees: Vec::new(),
            next_seq: 0,
        }
    }

    /// A decree issued during bell `issued`; returns the bell it takes
    /// effect at (`issued + 1` for peace, `issued + 36` for hostility).
    pub fn decree(&mut self, issued: u32, a: u8, b: u8, peaceful: bool) -> u32 {
        let lead = if peaceful {
            PEACE_LEAD_BELLS
        } else {
            WAR_HORN_BELLS
        };
        let from_bell = issued.saturating_add(lead);
        self.decrees.push(Decree {
            from_bell,
            seq: self.next_seq,
            a,
            b,
            peaceful,
        });
        self.next_seq += 1;
        from_bell
    }

    /// Relations in force at the start of bell `bell`.
    pub fn at(&self, bell: u32) -> Relations {
        let mut ds: Vec<&Decree> = self
            .decrees
            .iter()
            .filter(|d| d.from_bell <= bell)
            .collect();
        sort_by_key3(&mut ds, |d| (d.from_bell as u64, d.seq, 0));
        let mut r = self.base;
        for d in ds {
            r.set_peaceful(d.a, d.b, d.peaceful);
        }
        r
    }

    /// Fold every decree in force by bell `bell` into the base, once every
    /// province has resolved through `bell − 1` (no clash reads older).
    pub fn prune(&mut self, bell: u32) {
        self.base = self.at(bell);
        self.decrees.retain(|d| d.from_bell > bell);
    }
}

/// A host in the clash: a resident of the frozen roster, or an arrival.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Fighter {
    pub id: u64,
    pub faction: u8,
    pub unit: UnitType,
    pub troops: MilliTroops,
    /// Stamina at this bell (residents: refilled to it; arrivals: after
    /// the march).
    pub stamina: u16,
    /// Tile index in the province (arrivals: the revealed target hex).
    pub tile: u8,
    pub posture: Posture,
    /// Arrivals only: withdraw without fighting if the frozen hostile
    /// strength on the target hex exceeds this share (bps) of its own.
    /// At most [`RETREAT_MAX_BPS`].
    pub retreat_bps: Option<Bps>,
    /// Doctrine hook on damage dealt (`BPS_ONE` = none), in
    /// `[BPS_ONE, doctrine::bounds::COMBAT_MAX_BPS]`.
    pub dealt_bps: Bps,
}

/// A holding's garrison and walls, fighting as a virtual host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Garrison {
    /// The holding's id.
    pub id: u64,
    pub faction: u8,
    pub tile: u8,
    pub troops: MilliTroops,
    pub walls: bool,
    pub posture: Posture,
}

/// The province's storage room at the clash (I-43): what the Province's
/// entries hold besides the frozen roster. Musters pending at this bell
/// (entry state 2) join the roster after the clash, and departed entries
/// (state 3) keep their slot until SettleDeparture, so both limit the
/// arrivals that can stay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Occupancy {
    /// Pending musters per faction (entries in state 2): they count against
    /// the province cap and the faction cap as if they were residents.
    pub pending: [u8; FACTION_LIMIT as usize],
    /// Entries the Province can still store: `56 − entries in states 1–3`.
    /// At most this many arrivals are admitted.
    pub storage_free: u8,
}

impl Occupancy {
    /// Entries a Province stores (the frozen roster's 48 hosts and 8 more
    /// for musters and departures in flight).
    pub const STORAGE: u8 = 56;
    /// No pending musters, no departed entries, an empty store: the room
    /// is the caps alone (the kernel before I-43, bit for bit).
    pub const EMPTY: Occupancy = Occupancy {
        pending: [0; FACTION_LIMIT as usize],
        storage_free: Occupancy::STORAGE,
    };
}

impl Default for Occupancy {
    fn default() -> Self {
        Occupancy::EMPTY
    }
}

/// Everything one clash reads.
#[derive(Clone, Copy, Debug)]
pub struct ClashInput<'a> {
    pub province: ProvinceCoord,
    pub bell: u32,
    /// The bell seed `S(b, r)` of the province's region (drand round after
    /// the region's reveal close, §8.5).
    pub seed: Seed,
    pub terrain: &'a ProvinceTerrain,
    /// The roster frozen at the start of `bell`.
    pub residents: &'a [Fighter],
    pub garrisons: &'a [Garrison],
    /// Revealed arrivals of `bell` (unrevealed ones were routed and are
    /// not here).
    pub arrivals: &'a [Fighter],
    pub relations: Relations,
    /// The storage room (I-43); [`Occupancy::EMPTY`] where nothing is
    /// pending or departed.
    pub occupancy: Occupancy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClashError {
    TooManyResidents,
    TooManyArrivals,
    TooManyGarrisons,
    BadTile(u64),
    /// A faction id above [`NEUTRAL`] (CL-06).
    BadFaction(u64),
    DuplicateId(u64),
    /// Two garrisons on one tile.
    SharedTile(u8),
    /// An arrival's stance comes from its revealed commitment.
    ArrivalWithoutStance(u64),
    /// A host or garrison above `MAX_HOST_TROOPS` (CL-01).
    TroopsAboveCap(u64),
    /// A host above `STAMINA_CAP` (CL-01).
    StaminaAboveCap(u64),
    /// A doctrine multiplier outside `[BPS_ONE, COMBAT_MAX_BPS]` (CL-01,
    /// CL-14).
    BadMultiplier(u64),
    /// A retreat ratio above [`RETREAT_MAX_BPS`] (CL-01, I-27).
    BadRetreat(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Fate {
    /// Holds (or shares) the field on `tile`; arrivals join the roster
    /// from the next bell.
    Stays {
        tile: u8,
    },
    /// Lost the field and fell back to an adjacent friendly hex.
    Withdrew {
        tile: u8,
    },
    /// No room, or lost the field with nowhere to fall back: home, no loss.
    Bounced,
    /// Its `retreat_ratio` fired: home without fighting, no loss.
    Retreated,
    Destroyed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FighterResult {
    pub id: u64,
    pub arrival: bool,
    pub troops: MilliTroops,
    pub stamina: u16,
    pub fate: Fate,
    /// Fought this bell: may not depart before [`ready_bell_after`].
    pub engaged: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct GarrisonResult {
    pub id: u64,
    pub troops: MilliTroops,
    /// Hosts hostile to the owner hold the holding's hex (siege input).
    /// Scouts never count.
    pub attackers_hold: bool,
    /// Bit f set: faction f, hostile to the owner, holds the hex with a
    /// host of its own (a siege advances only while its declarer does).
    pub holders: u8,
    /// A host of the owner or a non-hostile faction stands on the hex
    /// (scouts never count).
    pub defender_present: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ClashOutcome {
    pub province: ProvinceCoord,
    pub bell: u32,
    /// Sorted by id.
    pub fighters: Vec<FighterResult>,
    /// Sorted by id.
    pub garrisons: Vec<GarrisonResult>,
    pub engagements: u32,
}

impl ClashOutcome {
    /// Hash of the whole outcome (event chains, property tests).
    pub fn digest(&self) -> Digest32 {
        let bytes = borsh::to_vec(self).unwrap_or_default();
        sha256(&[b"frontier/clash-outcome", &bytes])
    }

    pub fn fighter(&self, id: u64) -> Option<&FighterResult> {
        self.fighters
            .binary_search_by_key(&id, |f| f.id)
            .ok()
            .map(|i| &self.fighters[i])
    }
}

/// First bell a host that fought at `bell` may depart.
pub const fn ready_bell_after(bell: u32) -> u32 {
    bell + 1 + BATTLE_COOLDOWN_BELLS
}

/// The clash's own seed: `sha256("frontier/clash" ‖ S ‖ P ‖ Q ‖ b)`.
pub fn clash_seed(seed: &Seed, p: ProvinceCoord, bell: u32) -> Seed {
    sha256(&[
        b"frontier/clash",
        seed,
        &p.p.to_le_bytes(),
        &p.q.to_le_bytes(),
        &bell.to_le_bytes(),
    ])
}

// ------------------------------------------------------------ internals

#[derive(Clone, Copy, PartialEq, Eq)]
enum St {
    In,
    Out(Fate),
}

struct Unit {
    id: u64,
    city: bool,
    faction: u8,
    unit: Option<UnitType>,
    troops: MilliTroops,
    stamina: u16,
    tile: u8,
    posture: Posture,
    arrival: bool,
    retreat_bps: Option<Bps>,
    dealt_bps: Bps,
    walls: bool,
    tie: u64,
    st: St,
}

impl Unit {
    fn strength(&self) -> u64 {
        match self.unit {
            Some(u) => strength(u, self.troops),
            None => city_strength(self.troops),
        }
    }
    fn combatant(&self) -> Combatant {
        match self.unit {
            Some(unit) => Combatant::Army {
                unit,
                troops: self.troops,
            },
            None => Combatant::City {
                defense: self.troops,
            },
        }
    }
    /// A unit with attack or defence: a garrison, or a host of a fighting
    /// unit type (not a Scout or Settler). Only these contest a hex, rank
    /// for the field and hold or defend a holding's hex (CL-10).
    fn holds(&self) -> bool {
        self.unit.is_none_or(|u| !u.is_civilian())
    }
    fn fights(&self) -> bool {
        self.troops > 0 && self.holds()
    }
    fn key(&self) -> [u8; 9] {
        let mut k = [0u8; 9];
        k[0] = self.city as u8;
        k[1..].copy_from_slice(&self.id.to_le_bytes());
        k
    }
    /// Mass order: more troops first, then the lower tie key, then id (the
    /// host-only reference; the on-chain body sorts by [`order_key`]).
    #[cfg(any(
        test,
        feature = "std",
        not(any(target_os = "solana", target_arch = "wasm32"))
    ))]
    fn order(&self) -> (core::cmp::Reverse<MilliTroops>, u64, u64) {
        (core::cmp::Reverse(self.troops), self.tie, self.id)
    }
}

fn check_fighter(f: &Fighter) -> Result<(), ClashError> {
    use super::doctrine::bounds::COMBAT_MAX_BPS;
    if f.tile as usize >= PROVINCE_TILES {
        return Err(ClashError::BadTile(f.id));
    }
    if !valid_faction(f.faction, true) {
        return Err(ClashError::BadFaction(f.id));
    }
    if f.troops > MAX_HOST_TROOPS {
        return Err(ClashError::TroopsAboveCap(f.id));
    }
    if f.stamina > STAMINA_CAP {
        return Err(ClashError::StaminaAboveCap(f.id));
    }
    if !(BPS_ONE..=COMBAT_MAX_BPS).contains(&f.dealt_bps) {
        return Err(ClashError::BadMultiplier(f.id));
    }
    if f.retreat_bps.is_some_and(|r| r > RETREAT_MAX_BPS as Bps) {
        return Err(ClashError::BadRetreat(f.id));
    }
    Ok(())
}

fn validate(inp: &ClashInput) -> Result<(), ClashError> {
    if inp.residents.len() > PROVINCE_HOST_CAP {
        return Err(ClashError::TooManyResidents);
    }
    if inp.arrivals.len() > MAX_ARRIVALS {
        return Err(ClashError::TooManyArrivals);
    }
    if inp.garrisons.len() > MAX_GARRISONS {
        return Err(ClashError::TooManyGarrisons);
    }
    let mut ids: Vec<u64> = Vec::with_capacity(inp.residents.len() + inp.arrivals.len());
    for f in inp.residents.iter().chain(inp.arrivals.iter()) {
        check_fighter(f)?;
        ids.push(f.id);
    }
    for a in inp.arrivals {
        if a.posture == Posture::Disarray {
            return Err(ClashError::ArrivalWithoutStance(a.id));
        }
    }
    sort_by_key3(&mut ids, |&x| (x, 0, 0));
    if let Some(w) = ids.windows(2).find(|w| w[0] == w[1]) {
        return Err(ClashError::DuplicateId(w[0]));
    }
    let mut tiles: Vec<u8> = Vec::new();
    let mut gids: Vec<u64> = Vec::new();
    for g in inp.garrisons {
        if g.tile as usize >= PROVINCE_TILES {
            return Err(ClashError::BadTile(g.id));
        }
        if !valid_faction(g.faction, true) {
            return Err(ClashError::BadFaction(g.id));
        }
        if g.troops > MAX_HOST_TROOPS {
            return Err(ClashError::TroopsAboveCap(g.id));
        }
        if tiles.contains(&g.tile) {
            return Err(ClashError::SharedTile(g.tile));
        }
        if gids.contains(&g.id) {
            return Err(ClashError::DuplicateId(g.id));
        }
        tiles.push(g.tile);
        gids.push(g.id);
    }
    Ok(())
}

/// Sides among `factions`: the connected groups of factions that are not
/// hostile to each other. Returns the side (its smallest faction) of each.
fn side_of(factions: &[u8], rel: Relations) -> [u8; FACTION_LIMIT as usize] {
    let mut side: [u8; FACTION_LIMIT as usize] = core::array::from_fn(|f| f as u8);
    // Union by smallest member, repeated to a fixed point (≤ 8 factions).
    loop {
        let mut changed = false;
        for &a in factions {
            for &b in factions {
                if a != b && !rel.hostile(a, b) {
                    let m = side[a as usize].min(side[b as usize]);
                    for s in [a, b] {
                        if side[s as usize] != m {
                            side[s as usize] = m;
                            changed = true;
                        }
                    }
                }
            }
        }
        if !changed {
            return side;
        }
    }
}

/// Hex fair share: indices of `cands` (sorted by mass order) admitted
/// into `cap` slots. Each side is guaranteed `min(its hosts, ⌊cap /
/// sides⌋)`, its heaviest first; the rest go by mass.
fn fair_share(units: &[Unit], cands: &[usize], cap: usize, rel: Relations) -> Vec<usize> {
    if cands.len() <= cap {
        return cands.to_vec();
    }
    let mut factions: Vec<u8> = cands.iter().map(|&i| units[i].faction).collect();
    sort_by_key3(&mut factions, |&f| (f as u64, 0, 0));
    factions.dedup();
    let side = side_of(&factions, rel);
    let mut sides: Vec<u8> = factions.iter().map(|&f| side[f as usize]).collect();
    sort_by_key3(&mut sides, |&f| (f as u64, 0, 0));
    sides.dedup();
    let g = cap / sides.len();
    let mut taken = vec_of(false, cands.len());
    let mut n = 0;
    for s in &sides {
        let mut k = 0;
        for (j, &i) in cands.iter().enumerate() {
            if k == g {
                break;
            }
            if side[units[i].faction as usize] == *s {
                taken[j] = true;
                k += 1;
                n += 1;
            }
        }
    }
    for t in taken.iter_mut() {
        if n == cap {
            break;
        }
        if !*t {
            *t = true;
            n += 1;
        }
    }
    cands
        .iter()
        .zip(taken)
        .filter(|(_, t)| *t)
        .map(|(i, _)| *i)
        .collect()
}

fn vec_of<T: Clone>(v: T, n: usize) -> Vec<T> {
    let mut x = Vec::with_capacity(n);
    x.resize(n, v);
    x
}

/// Step 2b on one hex: `cands` are its hosts still in, sorted by mass
/// order, `garrison` the holding on it. Bounces the hosts the fair share
/// leaves out; returns whether it bounced any.
fn share_hex(units: &mut [Unit], cands: &[usize], garrison: Option<usize>, rel: Relations) -> bool {
    if cands.len() <= OWNER_HEX_SLOTS {
        return false;
    }
    let admitted = match garrison {
        Some(g) => {
            let owner = units[g].faction;
            let others: Vec<usize> = cands
                .iter()
                .copied()
                .filter(|&i| rel.hostile(units[i].faction, owner))
                .collect();
            let mut a = fair_share(units, &others, OWNER_HEX_SLOTS, rel);
            let room = HEX_HOST_CAP - a.len();
            // The owner's side: its own hosts first, then its friends,
            // each by mass.
            let mut own: Vec<usize> = cands
                .iter()
                .copied()
                .filter(|&i| !rel.hostile(units[i].faction, owner))
                .collect();
            sort_by_key3(&mut own, |&i| {
                let (a, b, c) = order_key(&units[i]);
                ((((units[i].faction != owner) as u64) << 32) | a, b, c)
            });
            a.extend(own.into_iter().take(room));
            a
        }
        None => fair_share(units, cands, HEX_HOST_CAP, rel),
    };
    let mut any = false;
    for &i in cands {
        if !admitted.contains(&i) {
            units[i].st = St::Out(Fate::Bounced);
            any = true;
        }
    }
    any
}

/// Step 2a: admit the arrivals marked `capped`, in mass order (`order`),
/// while the room allows — fewer than `PROVINCE_HOST_CAP` hosts and
/// `FACTION_RESIDENT_CAP` of the arrival's faction, counting the pending
/// musters and every host still in, and fewer than
/// `occupancy.storage_free` arrivals in (I-43).
fn admit_by_room(
    units: &mut [Unit],
    n_hosts: usize,
    occ: &Occupancy,
    order: &[usize],
    capped: &mut [bool],
) {
    let (mut per_faction, mut total, mut stored) = room_counts(units, n_hosts, occ);
    for &i in order {
        if !capped[i] {
            continue;
        }
        let f = units[i].faction as usize;
        if total < PROVINCE_HOST_CAP
            && per_faction[f] < FACTION_RESIDENT_CAP
            && stored < occ.storage_free as usize
        {
            units[i].st = St::In;
            capped[i] = false;
            per_faction[f] += 1;
            total += 1;
            stored += 1;
        }
    }
}

/// The room's counts: hosts per faction and in all (pending musters and
/// every host still in), and arrivals in.
fn room_counts(
    units: &[Unit],
    n_hosts: usize,
    occ: &Occupancy,
) -> ([usize; FACTION_LIMIT as usize], usize, usize) {
    let mut per_faction = [0usize; FACTION_LIMIT as usize];
    let mut total = 0usize;
    for (f, &m) in occ.pending.iter().enumerate() {
        per_faction[f] = m as usize;
        total += m as usize;
    }
    let mut stored = 0usize;
    for u in units[..n_hosts].iter().filter(|u| u.st == St::In) {
        per_faction[u.faction as usize] += 1;
        total += 1;
        stored += u.arrival as usize;
    }
    (per_faction, total, stored)
}

/// Step 2c, the recount (CL-10), run once the hex fair share has bounced a
/// host: the room's counts are rebuilt from the hosts still in, and the
/// arrivals the room refused are re-admitted in mass order where both the
/// room (as in 2a) and their hex have a free slot — fewer than
/// `HEX_HOST_CAP` hosts, and on a holding's hex, for an arrival hostile to
/// the owner, fewer than `OWNER_HEX_SLOTS` hosts hostile to it. Exactly
/// the arrivals the hex fair share would admit without bouncing anyone, so
/// a re-admitted arrival never displaces a host already admitted and one
/// pass reaches stable counts. Returns the tiles that gained a host.
fn readmit_free_slots(
    units: &mut [Unit],
    n_hosts: usize,
    occ: &Occupancy,
    order: &[usize],
    capped: &mut [bool],
    rel: Relations,
) -> u64 {
    const NONE: u8 = 0xff;
    let (mut per_faction, mut total, mut stored) = room_counts(units, n_hosts, occ);
    let mut owner = [NONE; PROVINCE_TILES];
    for g in &units[n_hosts..] {
        owner[g.tile as usize] = g.faction;
    }
    let mut hosts = [0u8; PROVINCE_TILES];
    let mut hostile = [0u8; PROVINCE_TILES];
    for u in units[..n_hosts].iter().filter(|u| u.st == St::In) {
        let t = u.tile as usize;
        hosts[t] += 1;
        if owner[t] != NONE && rel.hostile(u.faction, owner[t]) {
            hostile[t] += 1;
        }
    }
    let mut gained = 0u64;
    for &i in order {
        if !capped[i] {
            continue;
        }
        let (f, t) = (units[i].faction as usize, units[i].tile as usize);
        let h = owner[t] != NONE && rel.hostile(units[i].faction, owner[t]);
        if total < PROVINCE_HOST_CAP
            && per_faction[f] < FACTION_RESIDENT_CAP
            && stored < occ.storage_free as usize
            && (hosts[t] as usize) < HEX_HOST_CAP
            && (!h || (hostile[t] as usize) < OWNER_HEX_SLOTS)
        {
            units[i].st = St::In;
            capped[i] = false;
            per_faction[f] += 1;
            total += 1;
            stored += 1;
            hosts[t] += 1;
            hostile[t] += h as u8;
            gained |= 1 << t;
        }
    }
    gained
}

/// Whether the clash of `inp.bell` is quiet: no arrivals, and resolving
/// it would change nothing (no hostile combatants share a hex, no hex is
/// over its fair share). Independent of the seed, so a program can skip a
/// quiet bell before its seed exists; skipping it gives exactly the state
/// resolving it would.
pub fn is_quiet(rules: &Ruleset, inp: &ClashInput) -> Result<bool, ClashError> {
    if !inp.arrivals.is_empty() {
        return Ok(false);
    }
    let probe = ClashInput {
        seed: [0u8; 32],
        ..*inp
    };
    let o = resolve_clash(rules, &probe)?;
    let same = o.engagements == 0
        && o.fighters.iter().all(|f| {
            inp.residents.iter().any(|r| {
                r.id == f.id
                    && f.fate == Fate::Stays { tile: r.tile }
                    && f.troops == r.troops
                    && f.stamina == r.stamina
                    && !f.engaged
            })
        })
        && o.garrisons.iter().all(|g| {
            inp.garrisons
                .iter()
                .any(|x| x.id == g.id && x.troops == g.troops)
        });
    Ok(same)
}

/// Hosts in id order, then garrisons in id order (both bodies).
fn build_units(inp: &ClashInput, cs: &Seed) -> (Vec<Unit>, usize) {
    let mut units: Vec<Unit> =
        Vec::with_capacity(inp.residents.len() + inp.arrivals.len() + inp.garrisons.len());
    let mut hosts: Vec<(&Fighter, bool)> = inp
        .residents
        .iter()
        .map(|f| (f, false))
        .chain(inp.arrivals.iter().map(|f| (f, true)))
        .collect();
    sort_by_key3(&mut hosts, |h| (h.0.id, 0, 0));
    for (f, arrival) in hosts {
        units.push(Unit {
            id: f.id,
            city: false,
            faction: f.faction,
            unit: Some(f.unit),
            troops: f.troops,
            stamina: f.stamina,
            tile: f.tile,
            posture: f.posture,
            arrival,
            retreat_bps: if arrival { f.retreat_bps } else { None },
            dealt_bps: f.dealt_bps,
            walls: false,
            tie: tie_key(cs, f.id),
            st: St::In,
        });
    }
    let n_hosts = units.len();
    let mut gs: Vec<&Garrison> = inp.garrisons.iter().collect();
    sort_by_key3(&mut gs, |g| (g.id, 0, 0));
    for g in gs {
        units.push(Unit {
            id: g.id,
            city: true,
            faction: g.faction,
            unit: None,
            troops: g.troops,
            stamina: 0,
            tile: g.tile,
            posture: g.posture,
            arrival: false,
            retreat_bps: None,
            dealt_bps: BPS_ONE,
            walls: g.walls,
            tie: 0,
            st: St::In,
        });
    }
    (units, n_hosts)
}

/// Step 2a's first pass: every arrival still in (not retreated) starts
/// refused and is admitted by room in mass order. Returns the mass order
/// and the refused marks.
fn first_admission(
    units: &mut [Unit],
    n_hosts: usize,
    occ: &Occupancy,
) -> (Vec<usize>, [bool; MAX_UNITS]) {
    let mut order: Vec<usize> = (0..n_hosts)
        .filter(|&i| units[i].arrival && units[i].st == St::In)
        .collect();
    sort_by_key3(&mut order, |&i| order_key(&units[i]));
    let mut capped = [false; MAX_UNITS];
    for &i in &order {
        units[i].st = St::Out(Fate::Bounced);
        capped[i] = true;
    }
    admit_by_room(units, n_hosts, occ, &order, &mut capped);
    (order, capped)
}

/// Engagement damage of one half: `raw` scaled by the stance table, the
/// dealer's doctrine multiplier, its number of engagements on the hex and
/// the half weight. Cannot overflow (CL-14 asserts above).
fn scale(raw: MilliTroops, from: &Unit, to: &Unit, n: u64, half: u64) -> u64 {
    raw as u64 * damage_bps(from.posture, to.posture) as u64 / BPS_ONE as u64
        * from.dealt_bps as u64
        / BPS_ONE as u64
        / n.max(1)
        / half
}

// ------------------------------------------------------------ the reference body

/// The clash, step by step: the original (pre-Phase A) body with the M1
/// rules (I-43 room, CL-10 recount and scouts). Kept on host builds as the
/// oracle `resolve_clash` is proved identical to
/// (`tests/frontier_clash_equiv.rs`); never linked on chain.
#[cfg(any(
    test,
    feature = "std",
    not(any(target_os = "solana", target_arch = "wasm32"))
))]
pub fn resolve_clash_ref(rules: &Ruleset, inp: &ClashInput) -> Result<ClashOutcome, ClashError> {
    validate(inp)?;
    let cs = clash_seed(&inp.seed, inp.province, inp.bell);
    let rel = inp.relations;
    let (mut units, n_hosts) = build_units(inp, &cs);
    let garrison_on = |units: &[Unit], tile: u8| -> Option<usize> {
        (n_hosts..units.len()).find(|&g| units[g].tile == tile)
    };

    // 1. Retreat orders against the frozen roster.
    for a in 0..n_hosts {
        let Some(r) = units[a].retreat_bps else {
            continue;
        };
        let (tile, fa) = (units[a].tile, units[a].faction);
        let defending: u64 = units
            .iter()
            .filter(|u| !u.arrival && u.tile == tile && rel.hostile(u.faction, fa))
            .map(|u| u.strength())
            .sum();
        let mine = units[a].strength();
        if defending as u128 * BPS_ONE as u128 > r as u128 * mine as u128 {
            units[a].st = St::Out(Fate::Retreated);
        }
    }

    // 2a. Province caps and the storage room: arrivals by mass order.
    let (order, mut capped) = first_admission(&mut units, n_hosts, &inp.occupancy);

    // 2b. Hex fair share; 2c. the cap recount (CL-10).
    let tiles_in = |units: &[Unit]| -> Vec<u8> {
        let mut t: Vec<u8> = units[..n_hosts]
            .iter()
            .filter(|u| u.st == St::In)
            .map(|u| u.tile)
            .collect();
        t.sort_unstable();
        t.dedup();
        t
    };
    let mut bounced = false;
    for t in tiles_in(&units) {
        let mut cands: Vec<usize> = (0..n_hosts)
            .filter(|&i| units[i].st == St::In && units[i].tile == t)
            .collect();
        cands.sort_by_key(|&i| units[i].order());
        let g = garrison_on(&units, t);
        bounced |= share_hex(&mut units, &cands, g, rel);
    }
    if bounced {
        readmit_free_slots(
            &mut units,
            n_hosts,
            &inp.occupancy,
            &order,
            &mut capped,
            rel,
        );
    }
    let tiles = tiles_in(&units);

    // 3. Engagements from pre-clash counts.
    let n = units.len();
    let mut dmg = vec![0u64; n];
    let mut engaged = vec![false; n];
    // per (tile, faction): damage dealt and taken
    let mut dealt: Vec<(u8, u8, u64)> = Vec::new();
    let mut taken: Vec<(u8, u8, u64)> = Vec::new();
    let bump = |v: &mut Vec<(u8, u8, u64)>, t: u8, f: u8, d: u64| match v
        .iter_mut()
        .find(|x| x.0 == t && x.1 == f)
    {
        Some(x) => x.2 += d,
        None => v.push((t, f, d)),
    };
    let mut engagements = 0u32;
    for &t in &tiles {
        let part: Vec<usize> = (0..n)
            .filter(|&i| units[i].st == St::In && units[i].tile == t && units[i].fights())
            .collect();
        let rough = inp
            .terrain
            .terrain
            .get(t as usize)
            .is_some_and(|x| x.info().defense_bps < BPS_ONE);
        // Engagements each combatant is in on this hex.
        let count = |i: usize| -> u64 {
            part.iter()
                .filter(|&&j| rel.hostile(units[i].faction, units[j].faction))
                .count() as u64
        };
        for (x, &i) in part.iter().enumerate() {
            for &j in &part[x + 1..] {
                if !rel.hostile(units[i].faction, units[j].faction) {
                    continue;
                }
                // Roles: a garrison always defends; an arrival attacks a
                // resident; otherwise both ways at half weight.
                let (ui, uj) = (&units[i], &units[j]);
                let pairs: &[(usize, usize, u64)] = if uj.city {
                    &[(i, j, 1)]
                } else if ui.city {
                    &[(j, i, 1)]
                } else if ui.arrival != uj.arrival {
                    if ui.arrival {
                        &[(i, j, 1)]
                    } else {
                        &[(j, i, 1)]
                    }
                } else {
                    &[(i, j, 2), (j, i, 2)]
                };
                for &(ai, di, half) in pairs {
                    let (a, d) = (&units[ai], &units[di]);
                    let sit = Situation {
                        defender_on_rough_terrain: rough && !d.arrival,
                        city_walls: d.city && d.walls,
                        attacker_exhausted: a.stamina < ENGAGE_STAMINA,
                        ..Situation::default()
                    };
                    let mut id = [0u8; 18];
                    id[..9].copy_from_slice(&a.key());
                    id[9..].copy_from_slice(&d.key());
                    let eid = rand(&cs, b"eng", &id) as u32;
                    let va = variance(rules, &cs, eid, 0);
                    let vd = variance(rules, &cs, eid, 1);
                    let (to_def, to_att) =
                        resolve_engagement(rules, a.combatant(), d.combatant(), sit, va, vd);
                    let x_def = scale(to_def, a, d, count(ai), half);
                    let x_att = scale(to_att, d, a, count(di), half);
                    let (tile, fa, fd) = (t, a.faction, d.faction);
                    dmg[di] += x_def;
                    dmg[ai] += x_att;
                    engaged[ai] = true;
                    engaged[di] = true;
                    bump(&mut dealt, tile, fa, x_def);
                    bump(&mut taken, tile, fd, x_def);
                    bump(&mut dealt, tile, fd, x_att);
                    bump(&mut taken, tile, fa, x_att);
                    engagements += 1;
                }
            }
        }
    }

    // 4. Apply all damage at once; 6. stamina with the damage-ratio refund.
    for i in 0..n {
        if units[i].st != St::In {
            continue;
        }
        let u = &mut units[i];
        u.troops = (u.troops as u64).saturating_sub(dmg[i]) as MilliTroops;
        if !u.city {
            if engaged[i] {
                let get = |v: &Vec<(u8, u8, u64)>| {
                    v.iter()
                        .find(|x| x.0 == u.tile && x.1 == u.faction)
                        .map_or(0, |x| x.2)
                };
                let (d, tk) = (get(&dealt), get(&taken));
                let refund = refunded(d, tk);
                if !refund {
                    u.stamina = u.stamina.saturating_sub(ENGAGE_STAMINA);
                }
            }
            if u.troops < DESTROYED_BELOW {
                u.troops = 0;
                u.st = St::Out(Fate::Destroyed);
            }
        }
    }

    // 5. One side holds the field (only units that hold count, CL-10).
    let mut withdrawing: Vec<usize> = Vec::new();
    for &t in &tiles {
        let here: Vec<usize> = (0..n)
            .filter(|&i| {
                units[i].tile == t
                    && units[i].st == St::In
                    && (!units[i].city || units[i].troops > 0)
            })
            .collect();
        let mut factions: Vec<u8> = here
            .iter()
            .filter(|&&i| units[i].holds())
            .map(|&i| units[i].faction)
            .collect();
        factions.sort_unstable();
        factions.dedup();
        let contested = factions
            .iter()
            .any(|&a| factions.iter().any(|&b| rel.hostile(a, b)));
        if !contested {
            continue;
        }
        // (strength desc, defender first, seeded faction tie)
        let mut ranked: Vec<(core::cmp::Reverse<u64>, bool, u64, u8)> = factions
            .iter()
            .map(|&f| {
                let s: u64 = here
                    .iter()
                    .filter(|&&i| units[i].faction == f && units[i].holds())
                    .map(|&i| units[i].strength())
                    .sum();
                let defender = here
                    .iter()
                    .any(|&i| units[i].faction == f && units[i].holds() && !units[i].arrival);
                (
                    core::cmp::Reverse(s),
                    !defender,
                    rand_id(&cs, b"field", f as u64),
                    f,
                )
            })
            .collect();
        ranked.sort();
        let mut kept: Vec<u8> = Vec::new();
        for (_, _, _, f) in ranked {
            if kept.iter().all(|&k| !rel.hostile(k, f)) {
                kept.push(f);
            }
        }
        for &i in &here {
            if units[i].city {
                continue;
            }
            let f = units[i].faction;
            // A faction that ranked withdraws unless kept; one that only
            // has scouts here withdraws if it is hostile to the holders.
            let out = if factions.contains(&f) {
                !kept.contains(&f)
            } else {
                kept.iter().any(|&k| rel.hostile(k, f))
            };
            if out {
                withdrawing.push(i);
            }
        }
    }
    withdrawing.sort_by_key(|&i| units[i].order());
    // Mark all losers out first so they do not count as occupants.
    for &i in &withdrawing {
        units[i].st = St::Out(Fate::Bounced);
    }
    let k = inp.province.wedge().unwrap_or(0);
    for &i in &withdrawing {
        if units[i].arrival {
            continue;
        }
        let (f, from) = (units[i].faction, units[i].tile);
        let Some(o) = tile_offset(from) else { continue };
        for nb in Hex::ORIGIN.neighbors_in(k) {
            let Some(nt) = tile_index(Hex::new(o.q + nb.q, o.r + nb.r)) else {
                continue;
            };
            if !inp.terrain.passable(nt) {
                continue;
            }
            let occ: Vec<usize> = (0..n)
                .filter(|&j| {
                    units[j].tile == nt
                        && (matches!(units[j].st, St::In | St::Out(Fate::Withdrew { .. })))
                        && (!units[j].city || units[j].troops > 0)
                })
                .collect();
            let hosts_there = occ.iter().filter(|&&j| !units[j].city).count();
            let friendly = occ.iter().any(|&j| units[j].faction == f);
            let hostile = occ.iter().any(|&j| rel.hostile(units[j].faction, f));
            if friendly && !hostile && hosts_there < HEX_HOST_CAP {
                units[i].tile = nt;
                units[i].st = St::Out(Fate::Withdrew { tile: nt });
                break;
            }
        }
    }

    // Results.
    let mut fighters = Vec::with_capacity(n_hosts);
    for (i, u) in units[..n_hosts].iter().enumerate() {
        let fate = match u.st {
            St::In => Fate::Stays { tile: u.tile },
            St::Out(f) => f,
        };
        fighters.push(FighterResult {
            id: u.id,
            arrival: u.arrival,
            troops: u.troops,
            stamina: u.stamina,
            fate,
            engaged: engaged[i],
        });
    }
    let mut garrisons = Vec::with_capacity(n - n_hosts);
    for g in &units[n_hosts..] {
        let stay = |u: &Unit| {
            !u.city
                && u.holds()
                && u.tile == g.tile
                && matches!(u.st, St::In | St::Out(Fate::Withdrew { .. }))
        };
        let attackers_hold = units
            .iter()
            .any(|u| stay(u) && rel.hostile(u.faction, g.faction));
        let holders = units
            .iter()
            .filter(|u| stay(u) && rel.hostile(u.faction, g.faction) && u.faction < 8)
            .fold(0u8, |m, u| m | (1 << u.faction));
        let defender_present = units
            .iter()
            .any(|u| stay(u) && !rel.hostile(u.faction, g.faction));
        garrisons.push(GarrisonResult {
            id: g.id,
            troops: g.troops,
            attackers_hold,
            holders,
            defender_present,
        });
    }
    Ok(ClashOutcome {
        province: inp.province,
        bell: inp.bell,
        fighters,
        garrisons,
        engagements,
    })
}

// ------------------------------------------------------------ the Phase A body
//
// Same rules, same outcome digest as `resolve_clash_ref`: only the
// bookkeeping changes (M1 lab `m1/lab/clash-opt/phaseA.patch`).
// * units are bucketed by tile once (tiles do not change before step 5),
//   so no step scans every unit per tile;
// * dealt/taken per (tile, faction) are per-tile arrays, and the stamina
//   refund is decided when the tile's engagements are done;
// * the field step's withdraw search reads an occupancy table (host count
//   and faction mask per tile) updated as hosts withdraw, and a const
//   neighbour table, instead of re-scanning every unit per neighbour;
// * the field tie key `rand_id(cs, "field", f)` does not depend on the
//   tile, so it is hashed once per faction;
// * garrison results read a per-tile mask of staying hosts.
// Argument order of every `Relations::hostile` call is kept, so the result
// is identical even for an asymmetric `peaceful` matrix.

const NO_TILE: u8 = 0xff;
const NO_UNIT: u8 = 0xff;
const MAX_UNITS: usize = PROVINCE_HOST_CAP + MAX_ARRIVALS + MAX_GARRISONS;
const _: () = assert!(MAX_UNITS < NO_UNIT as usize && PROVINCE_TILES < NO_TILE as usize);

const fn c_offset(idx: u8) -> (i32, i32) {
    let r = super::geometry::PROVINCE_RADIUS;
    let mut i = idx as i32;
    let mut q = -r;
    while q <= r {
        let r_min = if -r > -q - r { -r } else { -q - r };
        let r_max = if r < -q + r { r } else { -q + r };
        let n = r_max - r_min + 1;
        if i < n {
            return (q, r_min + i);
        }
        i -= n;
        q += 1;
    }
    (i32::MAX, i32::MAX)
}

const fn c_index(q: i32, rr: i32) -> u8 {
    let r = super::geometry::PROVINCE_RADIUS;
    let s = -q - rr;
    let aq = if q < 0 { -q } else { q };
    let ar = if rr < 0 { -rr } else { rr };
    let as_ = if s < 0 { -s } else { s };
    if (aq + ar + as_) / 2 > r {
        return NO_TILE;
    }
    let mut base = 0;
    let mut x = -r;
    while x < q {
        let r_min = if -r > -x - r { -r } else { -x - r };
        let r_max = if r < -x + r { r } else { -x + r };
        base += r_max - r_min + 1;
        x += 1;
    }
    let r_min = if -r > -q - r { -r } else { -q - r };
    (base + rr - r_min) as u8
}

/// `NB0[t][j]`: the tile in direction `DIRECTIONS[j]` of tile `t`, or
/// `NO_TILE`. `Hex::neighbors_in(k)[j]` is direction `(j + 6 − k) mod 6`.
const NB0: [[u8; 6]; PROVINCE_TILES] = {
    let mut t = [[NO_TILE; 6]; PROVINCE_TILES];
    let mut i = 0;
    while i < PROVINCE_TILES {
        let (q, r) = c_offset(i as u8);
        let mut j = 0;
        while j < 6 {
            let (dq, dr) = crate::hex::DIRECTIONS[j];
            t[i][j] = c_index(q + dq, r + dr);
            j += 1;
        }
        i += 1;
    }
    t
};

/// Resolve the clash of `inp.province` at `inp.bell` (Phase A body).
pub fn resolve_clash(rules: &Ruleset, inp: &ClashInput) -> Result<ClashOutcome, ClashError> {
    validate(inp)?;
    let cs = clash_seed(&inp.seed, inp.province, inp.bell);
    let rel = inp.relations;
    // hostile_to[f]: bit g set iff rel.hostile(g, f) (argument order kept).
    let mut hostile_to = [0u8; FACTION_LIMIT as usize];
    for f in 0..FACTION_LIMIT {
        for g in 0..FACTION_LIMIT {
            if rel.hostile(g, f) {
                hostile_to[f as usize] |= 1 << g;
            }
        }
    }

    let (mut units, n_hosts) = build_units(inp, &cs);
    let n = units.len();
    // Tile buckets, each in increasing unit index (singly linked, built
    // backwards). Valid until step 5 moves withdrawing hosts.
    let mut first = [NO_UNIT; PROVINCE_TILES];
    let mut next = [NO_UNIT; MAX_UNITS];
    for i in (0..n).rev() {
        let t = units[i].tile as usize;
        next[i] = first[t];
        first[t] = i as u8;
    }
    macro_rules! bucket {
        ($t:expr, $i:ident, $body:block) => {{
            let mut __c = first[$t as usize];
            while __c != NO_UNIT {
                let $i = __c as usize;
                $body
                __c = next[$i];
            }
        }};
    }
    crate::probe::probe("K1 validate+units");

    // 1. Retreat orders against the frozen roster.
    for a in 0..n_hosts {
        let Some(r) = units[a].retreat_bps else {
            continue;
        };
        let (tile, fa) = (units[a].tile, units[a].faction);
        let mut defending: u64 = 0;
        bucket!(tile, i, {
            let u = &units[i];
            if !u.arrival && rel.hostile(u.faction, fa) {
                defending += u.strength();
            }
        });
        let mine = units[a].strength();
        if defending as u128 * BPS_ONE as u128 > r as u128 * mine as u128 {
            units[a].st = St::Out(Fate::Retreated);
        }
    }

    // 2a. Province caps and the storage room: arrivals by mass order.
    let (order, mut capped) = first_admission(&mut units, n_hosts, &inp.occupancy);
    crate::probe::probe("K2 retreat+caps");

    // 2b. Hex fair share; 2c. the cap recount (CL-10).
    let mut on_tile = [false; PROVINCE_TILES];
    for u in units[..n_hosts].iter().filter(|u| u.st == St::In) {
        on_tile[u.tile as usize] = true;
    }
    let mut cands: Vec<usize> = Vec::with_capacity(MAX_UNITS);
    let mut bounced = false;
    for t in 0..PROVINCE_TILES as u8 {
        if !on_tile[t as usize] {
            continue;
        }
        cands.clear();
        let mut garrison: Option<usize> = None;
        bucket!(t, i, {
            if i < n_hosts {
                if units[i].st == St::In {
                    cands.push(i);
                }
            } else if garrison.is_none() {
                garrison = Some(i);
            }
        });
        if cands.len() <= OWNER_HEX_SLOTS {
            continue;
        }
        sort_by_key3(&mut cands, |&i| order_key(&units[i]));
        bounced |= share_hex(&mut units, &cands, garrison, rel);
    }
    if bounced {
        let gained = readmit_free_slots(
            &mut units,
            n_hosts,
            &inp.occupancy,
            &order,
            &mut capped,
            rel,
        );
        for (t, on) in on_tile.iter_mut().enumerate() {
            *on |= gained & (1 << t) != 0;
        }
    }
    let tiles: Vec<u8> = (0..PROVINCE_TILES as u8)
        .filter(|&t| on_tile[t as usize])
        .collect();
    crate::probe::probe("K3 fair share");

    // 3. Engagements from pre-clash counts.
    let mut dmg = [0u64; MAX_UNITS];
    let mut engaged = [false; MAX_UNITS];
    let mut refund = [false; MAX_UNITS];
    let mut engagements = 0u32;
    let mut part: Vec<usize> = Vec::with_capacity(MAX_UNITS);
    let mut cnt: Vec<u64> = Vec::with_capacity(MAX_UNITS);
    for &t in &tiles {
        part.clear();
        bucket!(t, i, {
            if units[i].st == St::In && units[i].fights() {
                part.push(i);
            }
        });
        cnt.clear();
        for &i in &part {
            let c = part
                .iter()
                .filter(|&&j| rel.hostile(units[i].faction, units[j].faction))
                .count() as u64;
            cnt.push(c);
        }
        let rough = inp
            .terrain
            .terrain
            .get(t as usize)
            .is_some_and(|x| x.info().defense_bps < BPS_ONE);
        // Damage dealt and taken per faction on this hex.
        let mut dealt = [0u64; FACTION_LIMIT as usize];
        let mut taken = [0u64; FACTION_LIMIT as usize];
        for (x, &i) in part.iter().enumerate() {
            for (y, &j) in part.iter().enumerate().skip(x + 1) {
                if !rel.hostile(units[i].faction, units[j].faction) {
                    continue;
                }
                let (ui, uj) = (&units[i], &units[j]);
                // (attacker, defender, attacker's position, defender's
                // position, half)
                let pairs: &[(usize, usize, usize, usize, u64)] = if uj.city {
                    &[(i, j, x, y, 1)]
                } else if ui.city {
                    &[(j, i, y, x, 1)]
                } else if ui.arrival != uj.arrival {
                    if ui.arrival {
                        &[(i, j, x, y, 1)]
                    } else {
                        &[(j, i, y, x, 1)]
                    }
                } else {
                    &[(i, j, x, y, 2), (j, i, y, x, 2)]
                };
                for &(ai, di, ax, dx, half) in pairs {
                    let (a, d) = (&units[ai], &units[di]);
                    let sit = Situation {
                        defender_on_rough_terrain: rough && !d.arrival,
                        city_walls: d.city && d.walls,
                        attacker_exhausted: a.stamina < ENGAGE_STAMINA,
                        ..Situation::default()
                    };
                    let mut id = [0u8; 18];
                    id[..9].copy_from_slice(&a.key());
                    id[9..].copy_from_slice(&d.key());
                    let eid = rand(&cs, b"eng", &id) as u32;
                    let va = variance(rules, &cs, eid, 0);
                    let vd = variance(rules, &cs, eid, 1);
                    let (to_def, to_att) =
                        resolve_engagement(rules, a.combatant(), d.combatant(), sit, va, vd);
                    let x_def = scale(to_def, a, d, cnt[ax], half);
                    let x_att = scale(to_att, d, a, cnt[dx], half);
                    let (fa, fd) = (a.faction as usize, d.faction as usize);
                    dmg[di] += x_def;
                    dmg[ai] += x_att;
                    engaged[ai] = true;
                    engaged[di] = true;
                    dealt[fa] += x_def;
                    taken[fd] += x_def;
                    dealt[fd] += x_att;
                    taken[fa] += x_att;
                    engagements += 1;
                }
            }
        }
        for &i in &part {
            let f = units[i].faction as usize;
            let (d, tk) = (dealt[f], taken[f]);
            refund[i] = refunded(d, tk);
        }
    }
    crate::probe::probe("K4 engagements");

    // 4. Apply all damage at once; 6. stamina with the damage-ratio refund.
    for i in 0..n {
        if units[i].st != St::In {
            continue;
        }
        let u = &mut units[i];
        u.troops = (u.troops as u64).saturating_sub(dmg[i]) as MilliTroops;
        if !u.city {
            if engaged[i] && !refund[i] {
                u.stamina = u.stamina.saturating_sub(ENGAGE_STAMINA);
            }
            if u.troops < DESTROYED_BELOW {
                u.troops = 0;
                u.st = St::Out(Fate::Destroyed);
            }
        }
    }
    crate::probe::probe("K5 apply+stamina");

    // 5. One side holds the field (only units that hold count, CL-10).
    let mut field_key: [Option<u64>; FACTION_LIMIT as usize] = [None; FACTION_LIMIT as usize];
    let mut withdrawing: Vec<usize> = Vec::new();
    let mut here: Vec<usize> = Vec::with_capacity(MAX_UNITS);
    for &t in &tiles {
        here.clear();
        // Factions that hold the field here (garrisons with troops and
        // fighting hosts).
        let mut hmask = 0u8;
        bucket!(t, i, {
            let u = &units[i];
            if u.st == St::In && (!u.city || u.troops > 0) {
                here.push(i);
                if u.holds() {
                    hmask |= 1 << u.faction;
                }
            }
        });
        // contested: some a, b holding with hostile(a, b)
        let mut contested = false;
        for a in 0..FACTION_LIMIT {
            if hmask & (1 << a) != 0 && hmask & hostile_to[a as usize] != 0 {
                // hostile_to[a] holds g with hostile(g, a); scanning every a
                // covers every ordered pair.
                contested = true;
                break;
            }
        }
        if !contested {
            continue;
        }
        let mut ranked: Vec<(core::cmp::Reverse<u64>, bool, u64, u8)> = Vec::with_capacity(8);
        for f in 0..FACTION_LIMIT {
            if hmask & (1 << f) == 0 {
                continue;
            }
            let mut s: u64 = 0;
            let mut defender = false;
            for &i in &here {
                if units[i].faction == f && units[i].holds() {
                    s += units[i].strength();
                    defender |= !units[i].arrival;
                }
            }
            let key =
                *field_key[f as usize].get_or_insert_with(|| rand_id(&cs, b"field", f as u64));
            ranked.push((core::cmp::Reverse(s), !defender, key, f));
        }
        // (Reverse(s), !defender, key, f), pushed in f order: f is the
        // position, the last tie.
        sort_by_key3(&mut ranked, |r| (u64::MAX - r.0 .0, r.1 as u64, r.2));
        let mut kept: u8 = 0;
        for (_, _, _, f) in ranked {
            let mut ok = true;
            for k in 0..FACTION_LIMIT {
                if kept & (1 << k) != 0 && rel.hostile(k, f) {
                    ok = false;
                    break;
                }
            }
            if ok {
                kept |= 1 << f;
            }
        }
        for &i in &here {
            let u = &units[i];
            if u.city {
                continue;
            }
            let f = u.faction as usize;
            let out = if hmask & (1 << f) != 0 {
                kept & (1 << f) == 0
            } else {
                kept & hostile_to[f] != 0
            };
            if out {
                withdrawing.push(i);
            }
        }
    }
    sort_by_key3(&mut withdrawing, |&i| order_key(&units[i]));
    for &i in &withdrawing {
        units[i].st = St::Out(Fate::Bounced);
    }
    if !withdrawing.is_empty() {
        // Occupancy after the losers left: hosts per tile and the factions
        // present (hosts, and garrisons that still have troops).
        let mut hosts_at = [0u8; PROVINCE_TILES];
        let mut mask_at = [0u8; PROVINCE_TILES];
        for u in units.iter() {
            if matches!(u.st, St::In | St::Out(Fate::Withdrew { .. })) && (!u.city || u.troops > 0)
            {
                mask_at[u.tile as usize] |= 1 << u.faction;
                if !u.city {
                    hosts_at[u.tile as usize] += 1;
                }
            }
        }
        let k = (inp.province.wedge().unwrap_or(0) % 6) as usize;
        for &i in &withdrawing {
            if units[i].arrival {
                continue;
            }
            let (f, from) = (units[i].faction, units[i].tile as usize);
            for j in 0..6 {
                let nt = NB0[from][(j + 6 - k) % 6];
                if nt == NO_TILE || !inp.terrain.passable(nt) {
                    continue;
                }
                let m = mask_at[nt as usize];
                let friendly = m & (1 << f) != 0;
                let hostile = m & hostile_to[f as usize] != 0;
                if friendly && !hostile && (hosts_at[nt as usize] as usize) < HEX_HOST_CAP {
                    units[i].tile = nt;
                    units[i].st = St::Out(Fate::Withdrew { tile: nt });
                    hosts_at[nt as usize] += 1;
                    mask_at[nt as usize] |= 1 << f;
                    break;
                }
            }
        }
    }

    // Results.
    let mut fighters = Vec::with_capacity(n_hosts);
    for (i, u) in units[..n_hosts].iter().enumerate() {
        let fate = match u.st {
            St::In => Fate::Stays { tile: u.tile },
            St::Out(f) => f,
        };
        fighters.push(FighterResult {
            id: u.id,
            arrival: u.arrival,
            troops: u.troops,
            stamina: u.stamina,
            fate,
            engaged: engaged[i],
        });
    }
    let mut staying = [0u8; PROVINCE_TILES];
    for u in units.iter() {
        if !u.city && u.holds() && matches!(u.st, St::In | St::Out(Fate::Withdrew { .. })) {
            staying[u.tile as usize] |= 1 << u.faction;
        }
    }
    let mut garrisons = Vec::with_capacity(n - n_hosts);
    for g in &units[n_hosts..] {
        let s = staying[g.tile as usize];
        let h = hostile_to[g.faction as usize];
        garrisons.push(GarrisonResult {
            id: g.id,
            troops: g.troops,
            attackers_hold: s & h != 0,
            holders: s & h,
            defender_present: s & !h != 0,
        });
    }
    crate::probe::probe("K6 field");
    Ok(ClashOutcome {
        province: inp.province,
        bell: inp.bell,
        fighters,
        garrisons,
        engagements,
    })
}

// ------------------------------------------------------------ arrival quotas at Reveal

/// ArrivalSlots per (province, bell, faction) (design §6.2).
pub const FACTION_ARRIVAL_SLOTS: usize = 4;

/// What one ArrivalSlot records about a revealed arrival.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SlotEntry {
    pub host_id: u64,
    /// The citizen who owns the host (one arrival per citizen per province
    /// per bell).
    pub citizen: u64,
    pub troops: MilliTroops,
}

/// A faction's ArrivalSlots for one (province, bell).
pub type FactionSlots = [Option<SlotEntry>; FACTION_ARRIVAL_SLOTS];

/// Tie key of an arrival for the quota: known at Reveal (the bell seed is
/// not yet), the same whoever computes it and whenever:
/// `sha256("frontier/slot" ‖ P ‖ Q ‖ b ‖ host_id)`.
pub fn slot_key(p: ProvinceCoord, bell: u32, host_id: u64) -> u64 {
    let seed = sha256(&[
        b"frontier/slot",
        &p.p.to_le_bytes(),
        &p.q.to_le_bytes(),
        &bell.to_le_bytes(),
    ]);
    tie_key(&seed, host_id)
}

/// Quota rank: more troops first, then the lower slot key, then the lower
/// host id. A strict total order on distinct hosts.
fn slot_rank(
    p: ProvinceCoord,
    bell: u32,
    e: &SlotEntry,
) -> (core::cmp::Reverse<MilliTroops>, u64, u64) {
    (
        core::cmp::Reverse(e.troops),
        slot_key(p, bell, e.host_id),
        e.host_id,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotDecision {
    /// Written into the free slot `slot`.
    Fill { slot: u8 },
    /// Written over slot `slot`; the displaced arrival bounces home with no
    /// loss.
    Displace { slot: u8, displaced: SlotEntry },
    /// Not admitted: the arrival bounces home with no loss.
    Refuse(SlotRefusal),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotRefusal {
    /// This host already holds a slot.
    AlreadyIn,
    /// Its citizen already holds a slot with a higher-ranked host.
    CitizenHasLarger,
    /// Every slot holds a higher-ranked arrival.
    Full,
}

/// Reveal-time quota (design §6.2, A3): where a revealed arrival goes in
/// its faction's four ArrivalSlots. One arrival per citizen: a citizen's
/// second arrival competes only with the citizen's own slot. Otherwise a
/// free slot is filled, or the lowest-ranked arrival is displaced if the
/// new one ranks higher. Each Reveal writes at most one slot.
///
/// Whatever order the reveals land in, the final set is the four
/// highest-ranked arrivals among each citizen's highest-ranked arrival
/// ([`quota_set`]; property test `arrival_slots_are_the_four_largest_*`).
pub fn admit_arrival(
    p: ProvinceCoord,
    bell: u32,
    slots: &FactionSlots,
    x: SlotEntry,
) -> SlotDecision {
    let rank = |e: &SlotEntry| slot_rank(p, bell, e);
    if slots.iter().flatten().any(|e| e.host_id == x.host_id) {
        return SlotDecision::Refuse(SlotRefusal::AlreadyIn);
    }
    if let Some((i, e)) = slots
        .iter()
        .enumerate()
        .find_map(|(i, e)| e.filter(|e| e.citizen == x.citizen).map(|e| (i, e)))
    {
        return if rank(&x) < rank(&e) {
            SlotDecision::Displace {
                slot: i as u8,
                displaced: e,
            }
        } else {
            SlotDecision::Refuse(SlotRefusal::CitizenHasLarger)
        };
    }
    if let Some(i) = slots.iter().position(Option::is_none) {
        return SlotDecision::Fill { slot: i as u8 };
    }
    let (i, low) = slots
        .iter()
        .enumerate()
        .filter_map(|(i, e)| e.map(|e| (i, e)))
        .max_by_key(|(_, e)| rank(e))
        .expect("full");
    if rank(&x) < rank(&low) {
        SlotDecision::Displace {
            slot: i as u8,
            displaced: low,
        }
    } else {
        SlotDecision::Refuse(SlotRefusal::Full)
    }
}

/// Apply a decision to the slots (what the Reveal instruction writes).
pub fn apply_slot(slots: &mut FactionSlots, x: SlotEntry, d: SlotDecision) {
    match d {
        SlotDecision::Fill { slot } | SlotDecision::Displace { slot, .. } => {
            slots[slot as usize] = Some(x)
        }
        SlotDecision::Refuse(_) => {}
    }
}

/// The quota's order-free definition: each citizen's highest-ranked
/// arrival, then the four highest-ranked of those, sorted by host id.
pub fn quota_set(p: ProvinceCoord, bell: u32, arrivals: &[SlotEntry]) -> Vec<SlotEntry> {
    let rank = |e: &SlotEntry| slot_rank(p, bell, e);
    let mut best: Vec<SlotEntry> = Vec::new();
    for a in arrivals {
        match best.iter_mut().find(|e| e.citizen == a.citizen) {
            Some(e) if e.host_id == a.host_id => {}
            Some(e) => {
                if rank(a) < rank(e) {
                    *e = *a;
                }
            }
            None => best.push(*a),
        }
    }
    sort_by_key3(&mut best, |e| {
        (
            (u32::MAX - e.troops) as u64,
            slot_key(p, bell, e.host_id),
            e.host_id,
        )
    });
    best.truncate(FACTION_ARRIVAL_SLOTS);
    sort_by_key3(&mut best, |e| (e.host_id, 0, 0));
    best
}

// ------------------------------------------------------------ reveal window and seed round

/// Reveal window after the bell's beacon is anchored for the region:
/// reveals close at `A(b, r) + 600 s` (design §6.2, §8.4).
pub const REVEAL_WINDOW_SECS: i64 = 600;
/// Seed-round margin M (M0 report §4.2 item 6): the bell seed is the first
/// round at least this long after the reveal close. `round_at` rounds down
/// and beacons appear 0.8–2.0 s after their round time [measured,
/// S-TLOCK], so without a margin the seed could be public before reveals
/// close. Review against measured Solana clock skew.
pub const SEED_MARGIN_SECS: i64 = 60;
const _: () = assert!(SEED_MARGIN_SECS >= 60, "M0 review: M ≥ 60 s");

/// A drand network's clock (one network per season, owner decision O1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BeaconClock {
    /// Unix time of round 1.
    pub genesis: i64,
    /// Seconds between rounds.
    pub period: i64,
}

/// drand quicknet (League of Entropy): round 1 at 1692803367, every 3 s.
pub const QUICKNET: BeaconClock = BeaconClock {
    genesis: 1_692_803_367,
    period: 3,
};

impl BeaconClock {
    /// Scheduled time of `round` (≥ 1).
    pub const fn round_time(&self, round: u64) -> i64 {
        self.genesis + (round.saturating_sub(1) as i64) * self.period
    }

    /// The first round scheduled at or after `ts` (1 if `ts` is before
    /// genesis).
    pub const fn first_round_from(&self, ts: i64) -> u64 {
        if ts <= self.genesis {
            return 1;
        }
        let d = ts - self.genesis;
        (d / self.period + (d % self.period != 0) as i64) as u64 + 1
    }
}

/// Reveal close of a bell whose beacon was anchored at `anchor_ts`.
pub const fn reveal_close(anchor_ts: i64) -> i64 {
    anchor_ts + REVEAL_WINDOW_SECS
}

/// The bell seed's round `S(b, r)`: the first round scheduled at or after
/// the reveal close plus the margin, so `round_time(S) ≥ close + M`.
pub const fn seed_round(clock: &BeaconClock, anchor_ts: i64) -> u64 {
    clock.first_round_from(reveal_close(anchor_ts) + SEED_MARGIN_SECS)
}

/// Whether a Reveal is still accepted: before the reveal close by the
/// chain's clock, and while no round at or after the seed round is on
/// chain (`latest_round`: the highest round any anchor or cache of the
/// season has verified). Either condition alone closes the window, so a
/// fast Clock and an early beacon both refuse late reveals.
pub const fn reveal_open(clock: &BeaconClock, now: i64, anchor_ts: i64, latest_round: u64) -> bool {
    now < reveal_close(anchor_ts) && latest_round < seed_round(clock, anchor_ts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// W5-A: the shared sort is the core stable sort (every length up to
    /// 200, many equal keys, so stability is exercised on every merge; keys
    /// in `hi` and in `lo`).
    #[test]
    fn sort_keys_equal_the_core_stable_sort() {
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        for n in 0..200usize {
            for m in [1u64, 3, 7, 1_000, u64::MAX] {
                let keys: Vec<(u64, u64)> = (0..n)
                    .map(|_| {
                        x ^= x << 13;
                        x ^= x >> 7;
                        x ^= x << 17;
                        let a = x % m;
                        x ^= x << 13;
                        x ^= x >> 7;
                        x ^= x << 17;
                        (a, x % m)
                    })
                    .collect();
                let mut want: Vec<usize> = (0..n).collect();
                want.sort_by_key(|&i| keys[i]);
                let mut got: Vec<usize> = (0..n).collect();
                sort_by_key3(&mut got, |&i| (keys[i].0, 0, keys[i].1));
                assert_eq!(got, want, "n {n} m {m}");
                let mut want: Vec<usize> = (0..n).collect();
                want.sort_by_key(|&i| keys[i].1);
                let mut got: Vec<usize> = (0..n).collect();
                sort_by_key3(&mut got, |&i| (0, keys[i].1, 0));
                assert_eq!(got, want, "lo only: n {n} m {m}");
            }
        }
    }

    /// Rule 6 at its edges: the `refund-strict` mutant (`>=` → `>`) and the
    /// `d > 0` guard are both killed here (W1-A notes: no generated fill
    /// hit a ratio of exactly 10).
    #[test]
    fn the_refund_starts_at_exactly_ten_times() {
        assert!(refunded(10, 1));
        assert!(refunded(10_000, 1_000));
        assert!(!refunded(9_999, 1_000));
        assert!(refunded(1, 0));
        assert!(!refunded(0, 0));
        assert!(!refunded(0, 1));
        assert!(refunded(u64::MAX, u64::MAX / 10));
        assert!(!refunded(u64::MAX - 1, u64::MAX));
    }
}
