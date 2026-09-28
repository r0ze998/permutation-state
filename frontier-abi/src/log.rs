//! PS2 log records (M1 contract §6): the verifier's and the herald's input.
//!
//! One `sol_log_data(&[b"PS2", body])` per record. `body` =
//!
//! | part | bytes | content |
//! |---|---|---|
//! | head | 6 | `ver u8 = 1` ‖ `kind u8` ‖ `bell u32` (`u32::MAX` before genesis) |
//! | key | per kind, fixed | the entity key (§4.1 raw form, coordinates i32) |
//! | payload | per kind, fixed | the fields of [`SPECS`], in order, LE |
//! | tail | `1 + 41 n` | `n u8`, then n × `{entity_kind u8, seq u64, head [32]}` |
//!
//! For each chained entity E the record touches:
//! `E.seq += 1; E.head = sha256(E.head ‖ le64(E.seq) ‖ body_without_tail)`
//! ([`next_head`]), and the tail carries the new `(seq, head)`.
//!
//! **Tail order (pinned here):** ascending `entity_kind`; entities of one
//! kind in the order the instruction lists their accounts (§5). A verifier
//! can therefore map tail entries to the transaction's writable chained
//! accounts without any other context; [`chains_of`] gives, per record
//! kind, the entities the record is expected to chain and derives their
//! addresses where the key and payload determine them.

use crate::addr::{self, split_host_id, AddrCtx};
use crate::bytes::{rd_arr, rd_i32, rd_u64, rd_u8, Cursor, Writer};
use crate::layout::AccountKind;
use permutation_rules::hash::sha256;

/// The `sol_log_data` prefix field.
pub const PREFIX: &[u8; 3] = b"PS2";
/// Body format version.
pub const VERSION: u8 = 1;
pub const HEAD_LEN: usize = 6;
/// One tail link: `entity_kind u8, seq u64, head [32]`.
pub const LINK_LEN: usize = 41;
/// Most chained entities one record touches (SETTLE with a displacement).
pub const MAX_LINKS: usize = 12;

/// Most Holdings one GatherClash part lists (and may stamp, v1.7).
pub const GATHER_HOLDINGS_MAX: usize = 10;
/// `bell` before genesis.
pub const NO_BELL: u32 = u32::MAX;
/// Soft ceiling on `body_without_tail` (§6); DEPART, CLASH and
/// SEASON_CREATED are the listed exceptions.
pub const SOFT_BODY_MAX: usize = 128;

/// Chained entity kinds of the tail (§6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum EntityKind {
    Season = 1,
    Frontier = 2,
    JoinShard = 3,
    Citizen = 4,
    Holding = 5,
    Province = 6,
    ClashInputs = 7,
}

impl EntityKind {
    pub fn from_u8(v: u8) -> Option<EntityKind> {
        use EntityKind::*;
        [
            Season,
            Frontier,
            JoinShard,
            Citizen,
            Holding,
            Province,
            ClashInputs,
        ]
        .into_iter()
        .find(|k| *k as u8 == v)
    }
    pub const fn account_kind(self) -> AccountKind {
        match self {
            EntityKind::Season => AccountKind::Season,
            EntityKind::Frontier => AccountKind::Frontier,
            EntityKind::JoinShard => AccountKind::JoinShard,
            EntityKind::Citizen => AccountKind::Citizen,
            EntityKind::Holding => AccountKind::Holding,
            EntityKind::Province => AccountKind::Province,
            EntityKind::ClashInputs => AccountKind::ClashInputs,
        }
    }
    pub fn of_account(k: AccountKind) -> Option<EntityKind> {
        Some(match k {
            AccountKind::Season => EntityKind::Season,
            AccountKind::Frontier => EntityKind::Frontier,
            AccountKind::JoinShard => EntityKind::JoinShard,
            AccountKind::Citizen => EntityKind::Citizen,
            AccountKind::Holding => EntityKind::Holding,
            AccountKind::Province => EntityKind::Province,
            AccountKind::ClashInputs => EntityKind::ClashInputs,
            _ => return None,
        })
    }
}

macro_rules! kinds {
    ($( $name:ident = $code:literal; key [$( $kf:literal : $kn:expr ),*]; payload [$( $pf:literal : $pn:expr ),*]; )*) => {
        /// Record kinds (§6).
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[allow(non_camel_case_types)]
        #[repr(u8)]
        pub enum Kind { $( $name = $code, )* }

        /// Key and payload fields of every kind: `(kind, name, key, payload)`.
        pub const SPECS: &[KindSpec] = &[ $( KindSpec {
            kind: Kind::$name,
            name: stringify!($name),
            key: &[ $( ($kf, $kn), )* ],
            payload: &[ $( ($pf, $pn), )* ],
        }, )* ];

        impl Kind {
            pub const fn from_u8(v: u8) -> Option<Kind> {
                match v { $( $code => Some(Kind::$name), )* _ => None }
            }
        }
    };
}

/// Field widths of one record kind.
#[derive(Clone, Copy, Debug)]
pub struct KindSpec {
    pub kind: Kind,
    pub name: &'static str,
    pub key: &'static [(&'static str, usize)],
    pub payload: &'static [(&'static str, usize)],
}

impl KindSpec {
    pub fn key_len(&self) -> usize {
        self.key.iter().map(|f| f.1).sum()
    }
    pub fn payload_len(&self) -> usize {
        self.payload.iter().map(|f| f.1).sum()
    }
    /// `body_without_tail` length.
    pub fn body_len(&self) -> usize {
        HEAD_LEN + self.key_len() + self.payload_len()
    }
}

kinds! {
    ANNOUNCE = 1; key ["season_id": 8]; payload ["params_hash": 32, "t_create_min": 8, "bond": 8];
    SEASON_CREATED = 2; key ["season_id": 8]; payload ["params_hash": 32, "genesis_round": 8, "genesis_ts": 8, "ruleset_hash": 32, "quicknet_pk_hash": 32, "reveal_window": 4, "seed_margin": 4, "r_max": 2, "program_version": 2];
    GENESIS_SEED = 3; key ["season_id": 8]; payload ["round": 8, "seed": 32];
    RING_OPEN = 4; key ["d": 2]; payload ["t_open": 8, "round": 8, "seed": 32];
    RING_SEED = 5; key ["d": 2]; payload ["round": 8, "seed": 32];
    PROVINCE_OPEN = 6; key ["p": 4, "q": 4]; payload ["ring": 2, "wedge": 1, "region": 1, "terrain_digest": 32, "site_count": 1, "camp_tile": 1, "camp_troops": 4, "reserved": 1];
    FOLD = 7; key ["part": 1]; payload ["occupied": 4, "wedge_occupied": 24, "open_sites": 4, "wedge_open": 24, "provinces_opened": 4];
    SEASON_STATUS = 8; key ["season_id": 8]; payload ["old": 1, "new": 1, "bond_outcome": 1];
    WINDOW = 9; key ["season_id": 8]; payload ["window_next": 4, "window_from_bell": 4];
    JOIN = 10; key ["citizen_tag15": 15]; payload ["wallet": 32, "faction": 1, "shard": 1, "session": 32, "expiry": 8];
    SESSION = 11; key ["citizen_tag15": 15]; payload ["session": 32, "expiry": 8];
    VIGIL = 12; key ["citizen_tag15": 15]; payload ["start_min": 2, "from_ts": 8];
    TICKET = 13; key ["citizen_tag15": 15]; payload ["ticket_bell": 4, "n": 1, "sites": 15, "escrow": 8, "funder": 32];
    SETTLE = 14; key ["p": 4, "q": 4, "site": 1]; payload ["outcome": 1, "citizen_tag": 8, "score": 8, "displaced_tag": 8, "gen": 1, "final_ts": 8, "ticket_bell": 4];
    RELEASE = 15; key ["p": 4, "q": 4, "site": 1]; payload ["citizen_tag": 8];
    HOLDING_FINAL = 16; key ["p": 4, "q": 4, "site": 1]; payload ["final_ts": 8];
    HARVEST = 20; key ["p": 4, "q": 4, "site": 1]; payload ["stores_digest": 32];
    BUILD = 21; key ["p": 4, "q": 4, "site": 1]; payload ["item": 1, "cost_digest": 32, "done_at": 8];
    TRAIN = 22; key ["p": 4, "q": 4, "site": 1]; payload ["unit": 1, "n": 4, "done_at": 8];
    MUSTER = 23; key ["host_id": 8]; payload ["unit": 1, "troops": 4, "tile": 1, "entry": 1];
    DISSOLVE = 24; key ["host_id": 8]; payload ["pending_bell": 4, "delta": 8];
    GARRISON = 25; key ["p": 4, "q": 4, "site": 1]; payload ["pending_bell": 4, "delta": 8];
    EXPLORE = 26; key ["host_id": 8]; payload ["p": 4, "q": 4, "n": 1, "tiles": 2];
    EXPLORE_RESULT = 27; key ["host_id": 8]; payload ["works_per_tile": 8, "works": 4, "floor_used": 1];
    STRANDED = 28; key ["host_id": 8]; payload ["troops_lost": 4];
    DEPART = 30; key ["host_id": 8]; payload ["origin_p": 4, "origin_q": 4, "origin_tile": 1, "depart_bell": 4, "arrive_bell": 4, "dep_mass": 4, "march_stamina": 2, "tip": 8, "seal_root": 32, "commit": 32, "seal": 165];
    REVEAL = 31; key ["p": 4, "q": 4, "arrive": 4, "faction": 1, "i": 1]; payload ["host_id": 8, "tile": 1, "stance": 1, "retreat": 2, "displace": 1, "displaced_host": 8, "beneficiary": 32, "ev_slot": 8, "ev_price": 8, "ev_limit": 4, "arrivalday_created": 1];
    DEPARTURE_SETTLED = 32; key ["host_id": 8]; payload ["troops_after": 4, "stamina_after": 2, "destroyed": 1];
    TRANSIT_SETTLED = 34; key ["host_id": 8]; payload ["outcome": 1, "seal_code": 1, "troops": 4, "tip_to": 8, "tip": 8, "fee_to": 8, "fee": 8, "bond_to": 8, "bond": 8, "reward_to": 8, "reward": 8, "pool_owed_delta": 8, "slot_kept": 1];
    GATHER = 40; key ["p": 4, "q": 4, "bell": 4]; payload ["start": 1, "n": 1, "arrivals_mask": 4, "no_arrivals": 1];
    CLASH = 41; key ["p": 4, "q": 4, "bell": 4]; payload ["outcome_digest": 32, "input_digest": 32, "engagements": 4, "fates": 9];
    SKIP = 42; key ["p": 4, "q": 4]; payload ["b0": 4, "n": 1, "quiet_digest": 32];
    CAMP = 43; key ["p": 4, "q": 4]; payload ["tile": 1, "troops": 4, "day": 4];
    ANCHOR = 50; key ["bell": 4, "region": 1]; payload ["round": 8, "a": 8, "slot": 8, "beneficiary": 32];
    SEED = 51; key ["bell": 4, "region": 1, "nonce": 1]; payload ["round": 8, "seed": 32, "a": 8];
    BEACON = 52; key ["region": 1]; payload ["round": 8];
    ARCHIVE = 53; key ["region": 1, "day": 4]; payload ["bell": 4, "a_off": 4, "seed": 32];
    DIVERT = 60; key ["recipient": 32]; payload ["amount": 8, "reason": 1];
    DEFENCE_CLAIM = 61; key ["beneficiary": 32]; payload ["day": 4, "slots": 1, "amount": 8, "partial": 1];
    POOL_SWEEP = 62; key ["p": 4, "q": 4, "site": 1]; payload ["amount": 8];
    CLOSE = 70; key ["account_kind": 1, "key": 15]; payload ["final_seq": 8, "final_head": 32, "recipient": 32, "lamports": 8];
}

impl Kind {
    pub fn spec(self) -> &'static KindSpec {
        // Every Kind has exactly one entry in SPECS (checked by a test).
        SPECS.iter().find(|s| s.kind == self).unwrap_or(&SPECS[0])
    }
    pub fn name(self) -> &'static str {
        self.spec().name
    }
}

/// SETTLE outcomes.
pub mod settle_outcome {
    pub const FRESH: u8 = 0;
    pub const DISPLACE: u8 = 1;
    pub const TAKEN: u8 = 2;
    pub const EXPIRED: u8 = 3;
}

/// TRANSIT_SETTLED outcomes (1–5 as the ClashInputs fates).
pub mod transit_outcome {
    pub const STAYS: u8 = 1;
    pub const WITHDREW: u8 = 2;
    pub const BOUNCED: u8 = 3;
    pub const RETREATED: u8 = 4;
    pub const DESTROYED: u8 = 5;
    pub const BOUNCED_UNRANKED: u8 = 6;
    pub const ROUTED: u8 = 7;
    pub const BAD_SEAL: u8 = 8;
}

/// SettleTransit seal codes (§5.3 SealVerdict note, I-44).
pub mod seal_code {
    pub const VALID: u8 = 0;
    pub const FO_FAILED: u8 = 1;
    pub const BAD_POINT: u8 = 2;
    pub const WRONG_ROUND: u8 = 3;
    pub const COMMIT_MISMATCH: u8 = 4;
    pub const BAD_PLAINTEXT: u8 = 5;
}

/// SEASON_STATUS bond outcomes.
pub mod bond_outcome {
    pub const NONE: u8 = 0;
    pub const RETURNED: u8 = 1;
    pub const BURNED: u8 = 2;
}

/// DIVERT reasons.
pub mod divert_reason {
    pub const TIP: u8 = 1;
    pub const MARCH_FEE: u8 = 2;
    pub const BOND: u8 = 3;
    pub const REWARD: u8 = 4;
    pub const RENT_REFUND: u8 = 5;
    pub const ESCROW_REFUND: u8 = 6;
}

/// One tail link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link {
    pub entity: EntityKind,
    pub seq: u64,
    pub head: [u8; 32],
}

/// `head' = sha256(prev_head ‖ le64(seq) ‖ body_without_tail)`.
pub fn next_head(prev_head: &[u8; 32], seq: u64, body_without_tail: &[u8]) -> [u8; 32] {
    sha256(&[prev_head, &seq.to_le_bytes(), body_without_tail])
}

/// Advances one entity's chain: returns the link to put in the tail
/// (`seq + 1` and the new head).
pub fn advance(
    entity: EntityKind,
    seq: u64,
    head: &[u8; 32],
    body_without_tail: &[u8],
) -> Option<Link> {
    let seq = seq.checked_add(1)?;
    Some(Link {
        entity,
        seq,
        head: next_head(head, seq, body_without_tail),
    })
}

/// Writes `body_without_tail` (head, key, payload) into `out`; checks the
/// key and payload widths of `kind`.
pub fn write_body(
    kind: Kind,
    bell: u32,
    key: &[u8],
    payload: &[u8],
    out: &mut [u8],
) -> Option<usize> {
    let spec = kind.spec();
    if key.len() != spec.key_len() || payload.len() != spec.payload_len() {
        return None;
    }
    let mut w = Writer::new(out);
    w.u8(VERSION)
        .u8(kind as u8)
        .u32(bell)
        .bytes(key)
        .bytes(payload);
    w.finish()
}

/// Appends the tail to a body written by [`write_body`]; the links must be
/// in tail order (ascending entity kind).
pub fn write_tail(links: &[Link], out: &mut [u8], at: usize) -> Option<usize> {
    if links.len() > MAX_LINKS || links.windows(2).any(|w| w[0].entity > w[1].entity) {
        return None;
    }
    let mut w = Writer::new(out.get_mut(at..)?);
    w.u8(links.len() as u8);
    for l in links {
        w.u8(l.entity as u8).u64(l.seq).bytes(&l.head);
    }
    Some(at + w.finish()?)
}

/// A decoded record (borrowing the body).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record<'a> {
    pub kind: Kind,
    pub bell: u32,
    pub key: &'a [u8],
    pub payload: &'a [u8],
    /// `head ‖ key ‖ payload`, the bytes each chain hashes.
    pub body_without_tail: &'a [u8],
    pub links: [Option<Link>; MAX_LINKS],
    pub n_links: usize,
}

/// Why a body does not decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogError {
    Short,
    Version(u8),
    UnknownKind(u8),
    UnknownEntity(u8),
    TooManyLinks(u8),
    /// Trailing bytes after the tail.
    Trailing,
    /// Tail not in ascending entity-kind order.
    TailOrder,
}

/// Decodes a PS2 `body` (without the `"PS2"` field).
pub fn decode(body: &[u8]) -> Result<Record<'_>, LogError> {
    let mut c = Cursor::new(body);
    let ver = c.u8().ok_or(LogError::Short)?;
    if ver != VERSION {
        return Err(LogError::Version(ver));
    }
    let k = c.u8().ok_or(LogError::Short)?;
    let kind = Kind::from_u8(k).ok_or(LogError::UnknownKind(k))?;
    let bell = c.u32().ok_or(LogError::Short)?;
    let spec = kind.spec();
    let key = c.take(spec.key_len()).ok_or(LogError::Short)?;
    let payload = c.take(spec.payload_len()).ok_or(LogError::Short)?;
    let bwt = &body[..c.pos()];
    let n = c.u8().ok_or(LogError::Short)?;
    if n as usize > MAX_LINKS {
        return Err(LogError::TooManyLinks(n));
    }
    let mut links = [None; MAX_LINKS];
    for slot in links.iter_mut().take(n as usize) {
        let e = c.u8().ok_or(LogError::Short)?;
        let entity = EntityKind::from_u8(e).ok_or(LogError::UnknownEntity(e))?;
        let seq = c.u64().ok_or(LogError::Short)?;
        let head = c.arr::<32>().ok_or(LogError::Short)?;
        *slot = Some(Link { entity, seq, head });
    }
    if !c.done() {
        return Err(LogError::Trailing);
    }
    let order_ok = links
        .iter()
        .take(n as usize)
        .flatten()
        .zip(
            links
                .iter()
                .skip(1)
                .take((n as usize).saturating_sub(1))
                .flatten(),
        )
        .all(|(a, b)| a.entity <= b.entity);
    if !order_ok {
        return Err(LogError::TailOrder);
    }
    Ok(Record {
        kind,
        bell,
        key,
        payload,
        body_without_tail: bwt,
        links,
        n_links: n as usize,
    })
}

/// Offset and width of a named key or payload field of `kind`
/// (`in_payload` selects the part).
pub fn field(kind: Kind, name: &str, in_payload: bool) -> Option<(usize, usize)> {
    let s = kind.spec();
    let list = if in_payload { s.payload } else { s.key };
    let mut off = 0;
    for (n, w) in list {
        if *n == name {
            return Some((off, *w));
        }
        off += w;
    }
    None
}

/// Packs 24 fates (3 bits each, position k at bits `3k..3k+3` of a 72-bit
/// little-endian integer) for CLASH.
pub fn pack_fates(f: &[u8; 24]) -> [u8; 9] {
    let mut out = [0u8; 9];
    for (k, v) in f.iter().enumerate() {
        for b in 0..3 {
            if (v >> b) & 1 == 1 {
                let bit = 3 * k + b;
                out[bit / 8] |= 1 << (bit % 8);
            }
        }
    }
    out
}

pub fn unpack_fates(p: &[u8; 9]) -> [u8; 24] {
    let mut f = [0u8; 24];
    for (k, v) in f.iter_mut().enumerate() {
        for b in 0..3 {
            let bit = 3 * k + b;
            if (p[bit / 8] >> (bit % 8)) & 1 == 1 {
                *v |= 1 << b;
            }
        }
    }
    f
}

// ------------------------------------------------------------ chains

/// Who a chained citizen is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CitizenRef {
    /// The seed tag (derivable address).
    Tag15([u8; 15]),
    /// The quota tag (first 8 bytes of the address; resolved through JOIN).
    Tag8(u64),
    /// The owner of a holding (resolved through SETTLE).
    OfHolding { p: i32, q: i32, site: u8 },
}

/// An entity a record chains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityRef {
    Season,
    Frontier,
    JoinShard {
        faction: u8,
        shard: u8,
    },
    /// The citizen's JoinShard (from its JOIN record).
    JoinShardOf(CitizenRef),
    Citizen(CitizenRef),
    Holding {
        p: i32,
        q: i32,
        site: u8,
    },
    Province {
        p: i32,
        q: i32,
    },
    ClashInputs {
        p: i32,
        q: i32,
        bell: u32,
    },
    /// Not determined by key and payload: the transaction's writable
    /// account of this entity kind (tail order).
    InTx,
}

impl EntityRef {
    /// The address, where key and payload determine it.
    pub fn address(&self, ctx: &AddrCtx) -> Option<[u8; 32]> {
        Some(match *self {
            EntityRef::Season => ctx.season,
            EntityRef::Frontier => ctx.frontier(),
            EntityRef::JoinShard { faction, shard } => ctx.join_shard(faction, shard),
            EntityRef::Citizen(CitizenRef::Tag15(t)) => ctx.citizen_by_tag15(&t),
            EntityRef::Holding { p, q, site } => ctx.holding(p, q, site),
            EntityRef::Province { p, q } => ctx.province(p, q),
            EntityRef::ClashInputs { p, q, bell } => ctx.clash_inputs(p, q, bell),
            _ => return None,
        })
    }
}

/// One expected chain link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainExpect {
    pub entity: EntityKind,
    pub who: EntityRef,
    /// Present only in some cases the payload does not decide.
    pub optional: bool,
}

/// The expected chain links of one record, in tail order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chains {
    pub items: [Option<ChainExpect>; MAX_LINKS],
    pub len: usize,
}

impl Chains {
    const fn new() -> Self {
        Chains {
            items: [None; MAX_LINKS],
            len: 0,
        }
    }
    fn push(&mut self, entity: EntityKind, who: EntityRef, optional: bool) {
        if self.len < MAX_LINKS {
            self.items[self.len] = Some(ChainExpect {
                entity,
                who,
                optional,
            });
            self.len += 1;
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = &ChainExpect> {
        self.items.iter().take(self.len).flatten()
    }
    /// Fewest and most links the tail may carry.
    pub fn bounds(&self) -> (usize, usize) {
        let min = self.iter().filter(|c| !c.optional).count();
        (min, self.len)
    }
    fn sort(&mut self) {
        // stable insertion sort by entity kind (≤ 8 items)
        for i in 1..self.len {
            let mut j = i;
            while j > 0 {
                let (a, b) = (self.items[j - 1], self.items[j]);
                match (a, b) {
                    (Some(x), Some(y)) if x.entity > y.entity => {
                        self.items.swap(j - 1, j);
                        j -= 1;
                    }
                    _ => break,
                }
            }
        }
    }
}

fn pqs(key: &[u8]) -> Option<(i32, i32, u8)> {
    Some((rd_i32(key, 0)?, rd_i32(key, 4)?, rd_u8(key, 8)?))
}

/// The entities a record chains (§6 "Chains" column), in tail order.
///
/// Deviation noted in the W1-E notes: RING_SEED chains nothing (its
/// instruction, ConsumeRingSeed, writes only the short-header RingSeed, so
/// it cannot advance the Frontier's head as the §6 table says).
pub fn chains_of(kind: Kind, key: &[u8], payload: &[u8]) -> Option<Chains> {
    use EntityKind as E;
    use EntityRef as R;
    let mut c = Chains::new();
    let host = || -> Option<(i32, i32, u8)> {
        let h = split_host_id(rd_u64(key, 0)?)?;
        Some((h.province.p, h.province.q, h.site))
    };
    let pay = |name: &str| -> Option<&[u8]> {
        let (o, w) = field(kind, name, true)?;
        payload.get(o..o + w)
    };
    match kind {
        Kind::ANNOUNCE
        | Kind::SEASON_CREATED
        | Kind::GENESIS_SEED
        | Kind::SEASON_STATUS
        | Kind::WINDOW => c.push(E::Season, R::Season, false),
        Kind::RING_OPEN | Kind::FOLD => c.push(E::Frontier, R::Frontier, false),
        Kind::PROVINCE_OPEN | Kind::SKIP | Kind::CAMP => c.push(
            E::Province,
            R::Province {
                p: rd_i32(key, 0)?,
                q: rd_i32(key, 4)?,
            },
            false,
        ),
        Kind::JOIN => {
            let t: [u8; 15] = rd_arr(key, 0)?;
            let faction = pay("faction")?[0];
            let shard = pay("shard")?[0];
            c.push(E::JoinShard, R::JoinShard { faction, shard }, false);
            c.push(E::Citizen, R::Citizen(CitizenRef::Tag15(t)), false);
        }
        Kind::SESSION | Kind::VIGIL => c.push(
            E::Citizen,
            R::Citizen(CitizenRef::Tag15(rd_arr(key, 0)?)),
            false,
        ),
        Kind::TICKET => {
            c.push(
                E::Citizen,
                R::Citizen(CitizenRef::Tag15(rd_arr(key, 0)?)),
                false,
            );
            let n = pay("n")?[0] as usize;
            let sites = pay("sites")?;
            let mut seen: [(i32, i32); 3] = [(0, 0); 3];
            let mut m = 0;
            for i in 0..n.min(3) {
                let p = crate::bytes::rd_i16(sites, 5 * i)? as i32;
                let q = crate::bytes::rd_i16(sites, 5 * i + 2)? as i32;
                if !seen[..m].contains(&(p, q)) {
                    seen[m] = (p, q);
                    m += 1;
                    c.push(E::Province, R::Province { p, q }, false);
                }
            }
        }
        Kind::SETTLE => {
            let (p, q, site) = pqs(key)?;
            let outcome = pay("outcome")?[0];
            let me = CitizenRef::Tag8(rd_u64(pay("citizen_tag")?, 0)?);
            let won = matches!(outcome, settle_outcome::FRESH | settle_outcome::DISPLACE);
            if won {
                c.push(E::JoinShard, R::JoinShardOf(me), false);
            }
            c.push(E::Citizen, R::Citizen(me), false);
            if outcome == settle_outcome::DISPLACE {
                let d = CitizenRef::Tag8(rd_u64(pay("displaced_tag")?, 0)?);
                // v1.5 (§6, W3-A F4): absent when the displaced citizen
                // shares the displacer's JoinShard (one account, chained
                // once); the payload cannot tell, so the link is optional.
                c.push(E::JoinShard, R::JoinShardOf(d), true);
                c.push(E::Citizen, R::Citizen(d), false);
            }
            if won {
                c.push(E::Holding, R::Holding { p, q, site }, false);
            }
            if outcome != settle_outcome::TAKEN {
                c.push(E::Province, R::Province { p, q }, false);
            }
            // the ticket's other provinces when the ticket ends (0–2; an
            // exhausted `taken` ticket also writes this site's province)
            let others = if outcome == settle_outcome::TAKEN {
                3
            } else {
                2
            };
            for _ in 0..others {
                c.push(E::Province, R::InTx, true);
            }
        }
        Kind::RELEASE => {
            let (p, q, site) = pqs(key)?;
            let me = CitizenRef::OfHolding { p, q, site };
            c.push(E::JoinShard, R::JoinShardOf(me), false);
            c.push(E::Citizen, R::Citizen(me), false);
            c.push(E::Holding, R::Holding { p, q, site }, false);
            c.push(E::Province, R::Province { p, q }, false);
        }
        Kind::HOLDING_FINAL | Kind::HARVEST | Kind::TRAIN | Kind::BUILD | Kind::GARRISON => {
            let (p, q, site) = pqs(key)?;
            c.push(
                E::Citizen,
                R::Citizen(CitizenRef::OfHolding { p, q, site }),
                false,
            );
            c.push(E::Holding, R::Holding { p, q, site }, false);
            if kind == Kind::GARRISON {
                c.push(E::Province, R::Province { p, q }, false);
            }
            if kind == Kind::BUILD {
                c.push(E::Province, R::Province { p, q }, true); // walls only
            }
        }
        Kind::MUSTER | Kind::DISSOLVE | Kind::EXPLORE | Kind::DEPART => {
            let (hp, hq, site) = host()?;
            c.push(
                E::Citizen,
                R::Citizen(CitizenRef::OfHolding { p: hp, q: hq, site }),
                false,
            );
            c.push(E::Holding, R::Holding { p: hp, q: hq, site }, false);
            let prov = match kind {
                Kind::MUSTER => R::Province { p: hp, q: hq },
                Kind::EXPLORE => R::Province {
                    p: rd_i32(pay("p")?, 0)?,
                    q: rd_i32(pay("q")?, 0)?,
                },
                Kind::DEPART => R::Province {
                    p: rd_i32(pay("origin_p")?, 0)?,
                    q: rd_i32(pay("origin_q")?, 0)?,
                },
                _ => R::InTx, // Dissolve: where the host is
            };
            c.push(E::Province, prov, false);
        }
        Kind::EXPLORE_RESULT => {
            let (hp, hq, site) = host()?;
            c.push(
                E::Citizen,
                R::Citizen(CitizenRef::OfHolding { p: hp, q: hq, site }),
                false,
            );
            c.push(E::Holding, R::Holding { p: hp, q: hq, site }, false);
        }
        Kind::STRANDED => c.push(E::Province, R::InTx, false),
        Kind::DEPARTURE_SETTLED => {
            let (hp, hq, site) = host()?;
            c.push(E::Holding, R::Holding { p: hp, q: hq, site }, false);
            c.push(E::Province, R::InTx, false);
        }
        Kind::TRANSIT_SETTLED => {
            let (hp, hq, site) = host()?;
            // v1.7: the owner's Citizen when the host earned the camp's
            // Works (I-56).
            c.push(
                E::Citizen,
                R::Citizen(CitizenRef::OfHolding { p: hp, q: hq, site }),
                true,
            );
            c.push(E::Holding, R::Holding { p: hp, q: hq, site }, false);
            c.push(E::Province, R::InTx, true);
            c.push(E::Province, R::InTx, true);
            c.push(E::ClashInputs, R::InTx, true);
        }
        Kind::GATHER => {
            c.push(
                E::ClashInputs,
                R::ClashInputs {
                    p: rd_i32(key, 0)?,
                    q: rd_i32(key, 4)?,
                    bell: crate::bytes::rd_u32(key, 8)?,
                },
                false,
            );
            // v1.7 (W4-B F1): each Holding whose transit the gather
            // stamped, in account order (≤ 10 per part).
            for _ in 0..GATHER_HOLDINGS_MAX {
                c.push(E::Holding, R::InTx, true);
            }
        }
        Kind::CLASH => {
            let (p, q, bell) = (
                rd_i32(key, 0)?,
                rd_i32(key, 4)?,
                crate::bytes::rd_u32(key, 8)?,
            );
            c.push(E::Province, R::Province { p, q }, false);
            c.push(E::ClashInputs, R::ClashInputs { p, q, bell }, false);
        }
        Kind::POOL_SWEEP => {
            let (p, q, site) = pqs(key)?;
            c.push(E::Holding, R::Holding { p, q, site }, false);
        }
        Kind::CLOSE => {
            // Every read is checked: a short key is `None`, never a panic
            // (integ-W1 review; `chains_of` is public no_std API).
            let ak = AccountKind::from_u8(rd_u8(key, 0)?)?;
            if let Some(e) = EntityKind::of_account(ak) {
                let k = key.get(1..)?;
                let who = match e {
                    E::Season => R::Season,
                    E::Frontier => R::Frontier,
                    E::JoinShard => R::JoinShard {
                        faction: rd_u8(k, 0)?,
                        shard: rd_u8(k, 1)?,
                    },
                    E::Citizen => R::Citizen(CitizenRef::Tag15(rd_arr(k, 0)?)),
                    E::Holding => R::Holding {
                        p: rd_i32(k, 0)?,
                        q: rd_i32(k, 4)?,
                        site: rd_u8(k, 8)?,
                    },
                    E::Province => R::Province {
                        p: rd_i32(k, 0)?,
                        q: rd_i32(k, 4)?,
                    },
                    E::ClashInputs => R::ClashInputs {
                        p: rd_i32(k, 0)?,
                        q: rd_i32(k, 4)?,
                        bell: crate::bytes::rd_u32(k, 8)?,
                    },
                };
                c.push(e, who, false);
            }
        }
        Kind::RING_SEED
        | Kind::REVEAL
        | Kind::ANCHOR
        | Kind::SEED
        | Kind::BEACON
        | Kind::ARCHIVE
        | Kind::DIVERT
        | Kind::DEFENCE_CLAIM => {}
    }
    c.sort();
    Some(c)
}

/// The CLOSE key: account kind ‖ the raw seed key zero-padded to 15 bytes
/// (the Season: its id, LE, in the first 8).
pub fn close_key(kind: AccountKind, raw: &[u8]) -> Option<[u8; 16]> {
    if raw.len() > addr::MAX_RAW {
        return None;
    }
    let mut k = [0u8; 16];
    k[0] = kind as u8;
    k[1..1 + raw.len()].copy_from_slice(raw);
    Some(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_cover_every_kind_once_and_bodies_fit() {
        for s in SPECS {
            assert_eq!(Kind::from_u8(s.kind as u8), Some(s.kind));
            assert_eq!(SPECS.iter().filter(|t| t.kind == s.kind).count(), 1);
            let exception = matches!(s.kind, Kind::DEPART | Kind::CLASH | Kind::SEASON_CREATED);
            assert!(
                exception || s.body_len() <= SOFT_BODY_MAX,
                "{} is {} B",
                s.name,
                s.body_len()
            );
        }
        assert_eq!(Kind::DEPART.spec().payload_len(), 260);
        assert_eq!(Kind::from_u8(33), None, "33 is reserved (BAD_SEAL removed)");
    }

    #[test]
    fn encode_decode_and_heads() {
        let mut buf = [0u8; 512];
        let key = 42u64.to_le_bytes();
        let payload = [0u8; 7];
        let n = write_body(Kind::DEPARTURE_SETTLED, 17, &key, &payload, &mut buf).unwrap();
        let bwt = buf[..n].to_vec();
        let l1 = advance(EntityKind::Holding, 0, &[0; 32], &bwt).unwrap();
        let l2 = advance(EntityKind::Province, 9, &[1; 32], &bwt).unwrap();
        let end = write_tail(&[l1, l2], &mut buf, n).unwrap();
        let r = decode(&buf[..end]).unwrap();
        assert_eq!(r.kind, Kind::DEPARTURE_SETTLED);
        assert_eq!(r.bell, 17);
        assert_eq!(r.body_without_tail, &bwt[..]);
        assert_eq!(r.n_links, 2);
        assert_eq!(r.links[0], Some(l1));
        assert_eq!(l1.seq, 1);
        assert_eq!(l2.head, next_head(&[1; 32], 10, &bwt));
        assert!(
            write_tail(&[l2, l1], &mut buf, n).is_none(),
            "tail order enforced"
        );
        assert_eq!(decode(&buf[..end - 1]), Err(LogError::Short));
        let mut t = buf[..end].to_vec();
        t.push(0);
        assert_eq!(decode(&t), Err(LogError::Trailing));
    }

    #[test]
    fn fates_pack_round_trip() {
        let mut f = [0u8; 24];
        for (k, v) in f.iter_mut().enumerate() {
            *v = (k % 6) as u8;
        }
        assert_eq!(unpack_fates(&pack_fates(&f)), f);
    }

    /// v1.5 §6 (W3-A F4): a DISPLACE whose two citizens share a JoinShard
    /// carries one JoinShard link; the bounds admit it and the two-shard
    /// tail.
    #[test]
    fn settle_displace_admits_one_shared_join_shard() {
        let spec = SPECS.iter().find(|s| s.kind == Kind::SETTLE).unwrap();
        let key = std::vec![0u8; spec.key_len()];
        let mut pl = std::vec![0u8; spec.payload_len()];
        let (o, _) = field(Kind::SETTLE, "outcome", true).unwrap();
        pl[o] = settle_outcome::DISPLACE;
        let c = chains_of(Kind::SETTLE, &key, &pl).unwrap();
        // JoinShard (me), [JoinShard (displaced)], Citizen × 2, Holding,
        // Province, [Province × 2]
        assert_eq!(c.bounds(), (5, 8));
        let js: std::vec::Vec<_> = c
            .iter()
            .filter(|x| x.entity == EntityKind::JoinShard)
            .map(|x| x.optional)
            .collect();
        assert_eq!(js, [false, true]);
        pl[o] = settle_outcome::FRESH;
        assert_eq!(chains_of(Kind::SETTLE, &key, &pl).unwrap().bounds(), (4, 6));
    }

    #[test]
    fn chains_follow_the_table() {
        let key15 = [3u8; 15];
        let mut payload = [0u8; 74];
        payload[32] = 4; // faction
        payload[33] = 6; // shard
        let c = chains_of(Kind::JOIN, &key15, &payload).unwrap();
        let v: std::vec::Vec<_> = c.iter().map(|x| x.entity).collect();
        assert_eq!(v, [EntityKind::JoinShard, EntityKind::Citizen]);
        assert_eq!(c.bounds(), (2, 2));
        let c = chains_of(Kind::RING_SEED, &[0, 0], &[0; 40]).unwrap();
        assert_eq!(c.len, 0);
        // every kind yields chains for a zero key/payload of the right size
        for s in SPECS {
            let key = std::vec![0u8; s.key_len()];
            let pl = std::vec![0u8; s.payload_len()];
            let c = chains_of(s.kind, &key, &pl);
            if s.kind == Kind::CLOSE {
                continue; // account kind 0 is invalid
            }
            let c = c.unwrap_or_else(|| panic!("{}", s.name));
            let ents: std::vec::Vec<_> = c.iter().map(|x| x.entity).collect();
            let mut sorted = ents.clone();
            sorted.sort();
            assert_eq!(ents, sorted, "{} tail order", s.name);
        }
    }
}
