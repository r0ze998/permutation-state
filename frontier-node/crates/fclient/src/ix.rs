//! One builder per instruction (M1 contract §5.5–§5.12). Account order is
//! the contract's; data is `tag ‖ fields`, little-endian, fixed width, no
//! borsh. The builders recompute every canonical address from its key, so a
//! caller can never pass a forged one by mistake (the program re-checks).
//!
//! `P` = player prologue `[actor s] [payer s,w] [season r] [citizen w]`;
//! `K` = keeper prologue `[fee_payer s,w] [season r]` (§5.6).

use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};

use crate::abi::tag;
use crate::addr::{self, Addresses};

/// Hash-to-curve hints of one beacon round: two maps × {branch u8,
/// inv_tv1, y, inv_den (48 B BE each)} = 290 B (SP-V2 `Hint::encode`).
pub const HINTS_LEN: usize = 290;

/// Little-endian data writer.
#[derive(Default, Clone, Debug)]
pub struct Data(pub Vec<u8>);

impl Data {
    pub fn new(tag: u8) -> Data {
        Data(vec![tag])
    }
    pub fn u8(mut self, v: u8) -> Self {
        self.0.push(v);
        self
    }
    pub fn u16(mut self, v: u16) -> Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn i16(mut self, v: i16) -> Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u32(mut self, v: u32) -> Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u64(mut self, v: u64) -> Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn i64(mut self, v: i64) -> Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn bytes(mut self, v: &[u8]) -> Self {
        self.0.extend_from_slice(v);
        self
    }
    pub fn key(self, k: &Address) -> Self {
        self.bytes(k.as_ref())
    }
}

pub fn w(k: Address) -> AccountMeta {
    AccountMeta::new(k, false)
}
pub fn r(k: Address) -> AccountMeta {
    AccountMeta::new_readonly(k, false)
}
pub fn ws(k: Address) -> AccountMeta {
    AccountMeta::new(k, true)
}
pub fn rs(k: Address) -> AccountMeta {
    AccountMeta::new_readonly(k, true)
}

fn build(a: &Addresses, metas: Vec<AccountMeta>, data: Data) -> Instruction {
    Instruction {
        program_id: a.program,
        accounts: metas,
        data: data.0,
    }
}

/// A beacon round as instructions carry it: round, compressed signature and
/// the hints for its H(round).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeaconArg {
    pub round: u64,
    pub sig48: [u8; 48],
    pub hints: Vec<u8>,
}

impl BeaconArg {
    fn put(&self, d: Data) -> Data {
        assert_eq!(self.hints.len(), HINTS_LEN, "hints are 2 × 145 B");
        d.u64(self.round).bytes(&self.sig48).bytes(&self.hints)
    }
}

/// Player prologue (§5.6): `[actor s] [payer s,w] [season r] [citizen w]`.
pub struct Player {
    pub actor: Address,
    pub payer: Address,
    /// The player's wallet (the Citizen address is derived from it).
    pub wallet: Address,
}

impl Player {
    /// Duplicate keys (actor = payer when self-funded) are merged by the
    /// message compiler, which keeps the strongest privileges.
    fn metas(&self, a: &Addresses) -> Vec<AccountMeta> {
        vec![
            rs(self.actor),
            ws(self.payer),
            r(a.season),
            w(a.citizen(&self.wallet)),
        ]
    }
}

// ================================================================ lifecycle

/// 0x08 AnnounceSeason: `[authority s,w] [season w (PDA)] [program r] [programdata r] [system]`.
pub fn announce_season(
    a: &Addresses,
    authority: Address,
    params_hash: [u8; 32],
    t_create_min: i64,
    bond: u64,
) -> Instruction {
    build(
        a,
        vec![
            ws(authority),
            w(a.season),
            r(a.program),
            r(a.programdata()),
            r(addr::system_program()),
        ],
        Data::new(tag::ANNOUNCE_SEASON)
            .u64(a.season_id)
            .bytes(&params_hash)
            .i64(t_create_min)
            .u64(bond),
    )
}

/// 0x01 CreateSeason: `[authority s,w] [season w] [frontier w] [pfund × 6 w] [dpool w] [system]`.
/// `season_params` is the fixed `SeasonParams` layout of `frontier-abi`
/// (224 B, `frontier_abi::presets::SEASON_PARAMS_LEN`) and `payout_params`
/// the borsh `PayoutParams` (≤ 128 B).
pub fn create_season(
    a: &Addresses,
    authority: Address,
    season_params: &[u8],
    payout_params: &[u8],
) -> Instruction {
    let mut m = vec![ws(authority), w(a.season), w(a.frontier())];
    m.extend(a.province_funds().map(w));
    m.push(w(a.defence_pool()));
    m.push(r(addr::system_program()));
    build(
        a,
        m,
        Data::new(tag::CREATE_SEASON)
            .bytes(season_params)
            .bytes(payout_params),
    )
}

/// 0x09 InitBeaconLogs: `[authority s,w] [season] [blog × 16 w] [system]`.
pub fn init_beacon_logs(a: &Addresses, authority: Address) -> Instruction {
    let mut m = vec![ws(authority), r(a.season)];
    m.extend((0..crate::abi::REGIONS).map(|rg| w(a.beacon_log(rg))));
    m.push(r(addr::system_program()));
    build(a, m, Data::new(tag::INIT_BEACON_LOGS))
}

/// 0x02 InitShards(f): `[authority s,w] [season] [js × 8 w] [system]`.
pub fn init_shards(a: &Addresses, authority: Address, faction: u8) -> Instruction {
    let mut m = vec![ws(authority), r(a.season)];
    m.extend((0..crate::abi::SHARDS_PER_FACTION).map(|s| w(a.join_shard(faction, s))));
    m.push(r(addr::system_program()));
    build(a, m, Data::new(tag::INIT_SHARDS).u8(faction))
}

/// 0x03 ConsumeGenesisSeed: K with the season writable.
pub fn consume_genesis_seed(a: &Addresses, fee_payer: Address, b: &BeaconArg) -> Instruction {
    build(
        a,
        vec![ws(fee_payer), w(a.season)],
        b.put(Data::new(tag::CONSUME_GENESIS_SEED)),
    )
}

/// 0x04 EndSeason: `[any s] [season w]`.
pub fn end_season(a: &Addresses, any: Address) -> Instruction {
    build(a, vec![ws(any), w(a.season)], Data::new(tag::END_SEASON))
}

/// 0x06 AbortSeason: `[any s] [season w] [authority w] [incinerator w]`.
pub fn abort_season(a: &Addresses, any: Address, authority: Address) -> Instruction {
    let m = vec![rs(any), w(a.season), w(authority), w(addr::incinerator())];
    build(a, m, Data::new(tag::ABORT_SEASON))
}

/// 0x05 CloseSeason(part): `[authority s,w] [season w] [frontier w] [pfund × 6 w] [dpool w] [js × n w] [blog × n w]`.
pub fn close_season(
    a: &Addresses,
    authority: Address,
    part: u8,
    shards: &[(u8, u8)],
    logs: &[u8],
) -> Instruction {
    let mut m = vec![ws(authority), w(a.season), w(a.frontier())];
    m.extend(a.province_funds().map(w));
    m.push(w(a.defence_pool()));
    m.extend(shards.iter().map(|&(f, s)| w(a.join_shard(f, s))));
    m.extend(logs.iter().map(|&rg| w(a.beacon_log(rg))));
    build(a, m, Data::new(tag::CLOSE_SEASON).u8(part))
}

/// CloseSeason's float parts (v1.8, W5-A; W5-A R5): part 8 RingSeeds →
/// stored `payer`, 9 AnchorArchives → `rent_to`, 10 DefenceClaims →
/// `beneficiary`. The ten fixed accounts of every part, then each
/// `(target, recipient)` pair in the repeat group (1–24 pairs; parts 9 and
/// 10 only on the Closed tombstone or an Aborted season). Each archive pair
/// adds 6,272 B to the loaded-data need of `L(CloseSeason)` beyond two
/// (W5-A §1.1). Same accounts as the svm builder `ix::season::close_float`.
pub fn close_season_float(
    a: &Addresses,
    authority: Address,
    part: u8,
    pairs: &[(Address, Address)],
) -> Instruction {
    let mut ix = close_season(a, authority, part, &[], &[]);
    for &(t, r) in pairs {
        ix.accounts.push(w(t));
        ix.accounts.push(w(r));
    }
    ix
}

/// 0x07 SetWindowSchedule: `[authority s] [season w]`.
pub fn set_window_schedule(
    a: &Addresses,
    authority: Address,
    window: u32,
    from_bell: u32,
) -> Instruction {
    build(
        a,
        vec![ws(authority), w(a.season)],
        Data::new(tag::SET_WINDOW_SCHEDULE)
            .u32(window)
            .u32(from_bell),
    )
}

// ================================================================ beacons

/// 0x10 PostAnchor: K + `[anchor w] [archive r] [ix sysvar] [system]`.
pub fn post_anchor(
    a: &Addresses,
    fee_payer: Address,
    region: u8,
    bell: u32,
    b: &BeaconArg,
    beneficiary: &Address,
) -> Instruction {
    let m = vec![
        ws(fee_payer),
        r(a.season),
        w(a.anchor(bell, region)),
        r(a.archive(region, addr::archive_part(bell))),
        r(addr::instructions_sysvar()),
        r(addr::system_program()),
    ];
    build(
        a,
        m,
        b.put(Data::new(tag::POST_ANCHOR).u8(region).u32(bell))
            .key(beneficiary),
    )
}

/// 0x11 PostAnchorMulti: K + `[anchor × k w] [archive × k r] [ix sysvar] [system]`;
/// the regions are the set bits of `mask`, in ascending order.
pub fn post_anchor_multi(
    a: &Addresses,
    fee_payer: Address,
    bell: u32,
    b: &BeaconArg,
    mask: u16,
    beneficiary: &Address,
) -> Instruction {
    let regions: Vec<u8> = (0..16u8).filter(|rg| mask & (1 << rg) != 0).collect();
    let mut m = vec![ws(fee_payer), r(a.season)];
    m.extend(regions.iter().map(|&rg| w(a.anchor(bell, rg))));
    m.extend(
        regions
            .iter()
            .map(|&rg| r(a.archive(rg, addr::archive_part(bell)))),
    );
    m.push(r(addr::instructions_sysvar()));
    m.push(r(addr::system_program()));
    let d = Data::new(tag::POST_ANCHOR_MULTI).u32(bell);
    build(a, m, b.put(d).u16(mask).key(beneficiary))
}

/// 0x12 PostSeed: K + `[anchor r] [cache w] [ix sysvar] [system]`.
pub fn post_seed(
    a: &Addresses,
    fee_payer: Address,
    region: u8,
    bell: u32,
    nonce: u8,
    b: &BeaconArg,
    beneficiary: &Address,
) -> Instruction {
    let m = vec![
        ws(fee_payer),
        r(a.season),
        r(a.anchor(bell, region)),
        w(a.seed_cache(bell, region, nonce)),
        r(addr::instructions_sysvar()),
        r(addr::system_program()),
    ];
    build(
        a,
        m,
        b.put(Data::new(tag::POST_SEED).u8(region).u32(bell).u8(nonce))
            .key(beneficiary),
    )
}

/// 0x13 PostBeacon: K + `[beaconlog w]`.
pub fn post_beacon(a: &Addresses, fee_payer: Address, region: u8, b: &BeaconArg) -> Instruction {
    build(
        a,
        vec![ws(fee_payer), r(a.season), w(a.beacon_log(region))],
        b.put(Data::new(tag::POST_BEACON).u8(region)),
    )
}

/// One bell of an ArchiveAnchors batch: THE anchor, the cache that gives
/// its seed, and the anchor's `rent_to` (refund recipient).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveItem {
    pub bell: u32,
    pub cache_nonce: u8,
    pub anchor_rent_to: Address,
}

/// 0x14 ArchiveAnchors: `[payer s,w] [season] [archive w] [system] ([anchor w] [cache r] [anchor_beneficiary w]) × ≤ 8`;
/// `part` = [`addr::archive_part`] of the bells (v1.3, half-day archives).
pub fn archive_anchors(
    a: &Addresses,
    payer: Address,
    region: u8,
    part: u32,
    items: &[ArchiveItem],
) -> Instruction {
    assert!(items.len() <= 8, "≤ 8 bells per ArchiveAnchors");
    let mut m = vec![
        ws(payer),
        r(a.season),
        w(a.archive(region, part)),
        r(addr::system_program()),
    ];
    let mut d = Data::new(tag::ARCHIVE_ANCHORS)
        .u8(region)
        .u32(part)
        .u8(items.len() as u8);
    for it in items {
        m.push(w(a.anchor(it.bell, region)));
        m.push(r(a.seed_cache(it.bell, region, it.cache_nonce)));
        m.push(w(it.anchor_rent_to));
        d = d.u32(it.bell);
    }
    build(a, m, d)
}

/// 0x15 CloseSeedCache: `[any s] [season] [cache w] [archive r] [rent_to w]`.
pub fn close_seed_cache(
    a: &Addresses,
    any: Address,
    bell: u32,
    region: u8,
    nonce: u8,
    rent_to: Address,
) -> Instruction {
    let m = vec![
        rs(any),
        r(a.season),
        w(a.seed_cache(bell, region, nonce)),
        r(a.archive(region, addr::archive_part(bell))),
        w(rent_to),
    ];
    build(
        a,
        m,
        Data::new(tag::CLOSE_SEED_CACHE)
            .u32(bell)
            .u8(region)
            .u8(nonce),
    )
}

// ================================================================ map

/// 0x20 OpenRing(d): `[payer s,w] [season] [frontier w] [ringseed w] [pfund × 6 r] [system]`.
pub fn open_ring(a: &Addresses, payer: Address, d: u16) -> Instruction {
    let mut m = vec![ws(payer), r(a.season), w(a.frontier()), w(a.ring_seed(d))];
    m.extend(a.province_funds().map(r));
    m.push(r(addr::system_program()));
    build(a, m, Data::new(tag::OPEN_RING).u16(d))
}

/// 0x21 ConsumeRingSeed(d): K + `[ringseed w]`.
pub fn consume_ring_seed(a: &Addresses, fee_payer: Address, d: u16, b: &BeaconArg) -> Instruction {
    build(
        a,
        vec![ws(fee_payer), r(a.season), w(a.ring_seed(d))],
        b.put(Data::new(tag::CONSUME_RING_SEED).u16(d)),
    )
}

/// The wedge of a province outside the Concord (the Concord uses wedge 0's fund).
pub fn wedge_of(p: i32, q: i32) -> u8 {
    permutation_rules::frontier::geometry::ProvinceCoord::new(p, q)
        .wedge()
        .unwrap_or(0)
}

/// The ring of a province.
pub fn ring_of(p: i32, q: i32) -> u16 {
    permutation_rules::frontier::geometry::ProvinceCoord::new(p, q).ring() as u16
}

/// Beacon region of a province.
pub fn region_of(p: i32, q: i32) -> u8 {
    permutation_rules::frontier::geometry::region_of(
        permutation_rules::frontier::geometry::ProvinceCoord::new(p, q),
    )
}

/// 0x22 OpenProvince(P, Q): `[payer s,w] [season] [ringseed w] [pfund(w) w] [province w] [system]`.
pub fn open_province(a: &Addresses, payer: Address, p: i16, q: i16) -> Instruction {
    let (pi, qi) = (p as i32, q as i32);
    let m = vec![
        ws(payer),
        r(a.season),
        w(a.ring_seed(ring_of(pi, qi))),
        w(a.province_fund(wedge_of(pi, qi))),
        w(a.province(pi, qi)),
        r(addr::system_program()),
    ];
    build(a, m, Data::new(tag::OPEN_PROVINCE).i16(p).i16(q))
}

/// 0x23 FoldOccupancy(part), three parts per bell (contract v1.2 §5.9):
/// `[payer s] [season] [frontier w]` then part 0 `[js × 24 r]` (factions
/// 0–2), part 1 `[js × 24 r]` (factions 3–5), part 2 `[pfund × 6 r]`.
/// v1.1's part 1 (24 shards + 6 funds) was 1,288 B, over the packet.
pub fn fold_occupancy(a: &Addresses, payer: Address, part: u8) -> Instruction {
    let mut m = vec![ws(payer), r(a.season), w(a.frontier())];
    match part {
        0 | 1 => {
            let factions = if part == 0 { 0..3u8 } else { 3..6u8 };
            for f in factions {
                m.extend((0..crate::abi::SHARDS_PER_FACTION).map(|s| r(a.join_shard(f, s))));
            }
        }
        _ => m.extend(a.province_funds().map(r)),
    }
    build(a, m, Data::new(tag::FOLD_OCCUPANCY).u8(part))
}

/// 0x24 CloseProvince: `[any s] [season] [province w] [pfund(w) w]`.
pub fn close_province(a: &Addresses, any: Address, p: i16, q: i16) -> Instruction {
    let (pi, qi) = (p as i32, q as i32);
    let m = vec![
        rs(any),
        r(a.season),
        w(a.province(pi, qi)),
        w(a.province_fund(wedge_of(pi, qi))),
    ];
    build(a, m, Data::new(tag::CLOSE_PROVINCE).i16(p).i16(q))
}

// ================================================================ citizens and land

/// 0x30 Join: `[wallet s] [payer s,w] [season] [frontier r] [citizen w] [joinshard w] [system] [join_gate s?]`.
pub fn join(
    a: &Addresses,
    wallet: Address,
    payer: Address,
    faction: u8,
    session: &Address,
    session_expiry: i64,
    join_gate: Option<Address>,
) -> Instruction {
    let shard = addr::join_shard_of(&wallet.to_bytes());
    let mut m = vec![
        rs(wallet),
        ws(payer),
        r(a.season),
        r(a.frontier()),
        w(a.citizen(&wallet)),
        w(a.join_shard(faction, shard)),
        r(addr::system_program()),
    ];
    if let Some(g) = join_gate {
        m.push(rs(g));
    }
    build(
        a,
        m,
        Data::new(tag::JOIN)
            .u8(faction)
            .key(session)
            .i64(session_expiry),
    )
}

/// 0x31 SetSession: P (actor = the wallet).
pub fn set_session(a: &Addresses, p: &Player, session: &Address, expiry: i64) -> Instruction {
    build(
        a,
        p.metas(a),
        Data::new(tag::SET_SESSION).key(session).i64(expiry),
    )
}

/// 0x32 SetVigil: P.
pub fn set_vigil(a: &Addresses, p: &Player, start_min: u16) -> Instruction {
    build(a, p.metas(a), Data::new(tag::SET_VIGIL).u16(start_min))
}

/// A site of a ticket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Site {
    pub p: i16,
    pub q: i16,
    pub site: u8,
}

/// The distinct provinces of a ticket's sites, in first-seen order.
pub fn ticket_provinces(sites: &[Site]) -> Vec<(i16, i16)> {
    let mut v: Vec<(i16, i16)> = vec![];
    for s in sites {
        if !v.contains(&(s.p, s.q)) {
            v.push((s.p, s.q));
        }
    }
    v
}

/// 0x33 FileTicket: P + `[frontier r] [province × m w] [system]`.
pub fn file_ticket(a: &Addresses, p: &Player, sites: &[Site]) -> Instruction {
    assert!((1..=3).contains(&sites.len()), "1–3 sites");
    let mut m = p.metas(a);
    m.push(r(a.frontier()));
    m.extend(
        ticket_provinces(sites)
            .into_iter()
            .map(|(pp, qq)| w(a.province(pp as i32, qq as i32))),
    );
    m.push(r(addr::system_program()));
    let mut d = Data::new(tag::FILE_TICKET).u8(sites.len() as u8);
    for s in sites {
        d = d.i16(s.p).i16(s.q).u8(s.site);
    }
    build(a, m, d)
}

/// Where a seed comes from: a SeedCache of THE anchor, or the archive
/// entry once the bell is archived (§5.8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedSource {
    Cache { nonce: u8 },
    Archive,
}

/// The `[seedcache|archive r] [anchor|archive r]` pair of a (bell, region).
pub fn seed_pair(a: &Addresses, bell: u32, region: u8, src: SeedSource) -> [AccountMeta; 2] {
    match src {
        SeedSource::Cache { nonce } => [
            r(a.seed_cache(bell, region, nonce)),
            r(a.anchor(bell, region)),
        ],
        SeedSource::Archive => {
            let ar = a.archive(region, addr::archive_part(bell));
            [r(ar), r(ar)]
        }
    }
}

/// The displaced holder, when SettleTicket displaces a provisional holding.
#[derive(Clone, Debug)]
pub struct Displaced {
    pub rent_payer: Address,
    pub wallet: Address,
    pub faction: u8,
}

/// 0x34 SettleTicket(k): `[payer s,w] [season] [citizen w] [holding w] [province w] [joinshard w] [seedcache r] [anchor|archive r]
/// [other ticket provinces w × ≤ 2] [displaced_rent_payer w?] [displaced_citizen w?] [displaced_joinshard w?] [system]`.
#[allow(clippy::too_many_arguments)]
pub fn settle_ticket(
    a: &Addresses,
    payer: Address,
    wallet: &Address,
    faction: u8,
    k: u8,
    site: Site,
    ticket_bell: u32,
    all_sites: &[Site],
    src: SeedSource,
    displaced: Option<&Displaced>,
) -> Instruction {
    let (pi, qi) = (site.p as i32, site.q as i32);
    let shard = addr::join_shard_of(&wallet.to_bytes());
    let mut m = vec![
        ws(payer),
        r(a.season),
        w(a.citizen(wallet)),
        w(a.holding(pi, qi, site.site)),
        w(a.province(pi, qi)),
        w(a.join_shard(faction, shard)),
    ];
    m.extend(seed_pair(a, ticket_bell, region_of(pi, qi), src));
    for (pp, qq) in ticket_provinces(all_sites) {
        if (pp, qq) != (site.p, site.q) {
            m.push(w(a.province(pp as i32, qq as i32)));
        }
    }
    if let Some(dp) = displaced {
        let dshard = addr::join_shard_of(&dp.wallet.to_bytes());
        m.push(w(dp.rent_payer));
        m.push(w(a.citizen(&dp.wallet)));
        m.push(w(a.join_shard(dp.faction, dshard)));
    }
    m.push(r(addr::system_program()));
    build(a, m, Data::new(tag::SETTLE_TICKET).u8(k))
}

/// A Holding's identity plus its owner's wallet and faction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoldingRef {
    pub p: i16,
    pub q: i16,
    pub site: u8,
}

impl HoldingRef {
    pub fn address(&self, a: &Addresses) -> Address {
        a.holding(self.p as i32, self.q as i32, self.site)
    }
    pub fn province(&self, a: &Addresses) -> Address {
        a.province(self.p as i32, self.q as i32)
    }
}

/// 0x35 ReleaseDormant: `[any s] [season] [holding w] [province w] [citizen w] [joinshard w] [rent_payer w] [dpool w]`.
pub fn release_dormant(
    a: &Addresses,
    any: Address,
    h: HoldingRef,
    wallet: &Address,
    faction: u8,
    rent_payer: Address,
) -> Instruction {
    let shard = addr::join_shard_of(&wallet.to_bytes());
    let m = vec![
        rs(any),
        r(a.season),
        w(h.address(a)),
        w(h.province(a)),
        w(a.citizen(wallet)),
        w(a.join_shard(faction, shard)),
        w(rent_payer),
        w(a.defence_pool()),
    ];
    build(a, m, Data::new(tag::RELEASE_DORMANT))
}

/// 0x36 CloseHolding: `[any s] [season] [holding w] [rent_payer w] [dpool w]`.
pub fn close_holding(
    a: &Addresses,
    any: Address,
    h: HoldingRef,
    rent_payer: Address,
) -> Instruction {
    let m = vec![
        rs(any),
        r(a.season),
        w(h.address(a)),
        w(rent_payer),
        w(a.defence_pool()),
    ];
    build(a, m, Data::new(tag::CLOSE_HOLDING))
}

/// 0x37 CloseCitizen: `[any s] [season] [citizen w] [rent_payer w] ([ticket_funder w])`.
pub fn close_citizen(
    a: &Addresses,
    any: Address,
    wallet: &Address,
    rent_payer: Address,
    ticket_funder: Option<Address>,
) -> Instruction {
    let mut m = vec![rs(any), r(a.season), w(a.citizen(wallet)), w(rent_payer)];
    if let Some(f) = ticket_funder {
        m.push(w(f));
    }
    build(a, m, Data::new(tag::CLOSE_CITIZEN))
}

// ================================================================ holdings

fn resident(a: &Addresses, p: &Player, h: HoldingRef, province: bool) -> Vec<AccountMeta> {
    let mut m = p.metas(a);
    m.push(w(h.address(a)));
    if province {
        m.push(w(h.province(a)));
    }
    m
}

/// 0x40 Harvest: P + `[holding w]`.
pub fn harvest(a: &Addresses, p: &Player, h: HoldingRef) -> Instruction {
    build(a, resident(a, p, h, false), Data::new(tag::HARVEST))
}

/// 0x41 Build(item): P + `[holding w] [province w?]` (the province for walls).
pub fn build_item(a: &Addresses, p: &Player, h: HoldingRef, item: u8, walls: bool) -> Instruction {
    build(a, resident(a, p, h, walls), Data::new(tag::BUILD).u8(item))
}

/// 0x42 Train(unit, n): P + `[holding w]`.
pub fn train(a: &Addresses, p: &Player, h: HoldingRef, unit: u8, n: u32) -> Instruction {
    build(
        a,
        resident(a, p, h, false),
        Data::new(tag::TRAIN).u8(unit).u32(n),
    )
}

/// 0x43 Muster: P + `[holding w] [province w]`.
pub fn muster(
    a: &Addresses,
    p: &Player,
    h: HoldingRef,
    unit: u8,
    troops: u32,
    tile: u8,
) -> Instruction {
    build(
        a,
        resident(a, p, h, true),
        Data::new(tag::MUSTER).u8(unit).u32(troops).u8(tile),
    )
}

/// 0x44 Dissolve(host): P + `[holding w] [province w]`.
pub fn dissolve(a: &Addresses, p: &Player, h: HoldingRef, host_id: u64) -> Instruction {
    build(
        a,
        resident(a, p, h, true),
        Data::new(tag::DISSOLVE).u64(host_id),
    )
}

/// 0x45 Garrison(delta): P + `[holding w] [province w]`.
pub fn garrison(a: &Addresses, p: &Player, h: HoldingRef, delta: i64) -> Instruction {
    build(
        a,
        resident(a, p, h, true),
        Data::new(tag::GARRISON).i64(delta),
    )
}

/// 0x46 Explore: P + `[holding w] [province w]` — `province` is where the
/// host stands (`(hp, hq)`), not necessarily the holding's.
pub fn explore(
    a: &Addresses,
    p: &Player,
    h: HoldingRef,
    host_at: (i16, i16),
    host_id: u64,
    tiles: &[u8],
) -> Instruction {
    assert!((1..=2).contains(&tiles.len()), "1–2 tiles");
    let mut m = p.metas(a);
    m.push(w(h.address(a)));
    m.push(w(a.province(host_at.0 as i32, host_at.1 as i32)));
    let t1 = tiles.get(1).copied().unwrap_or(0);
    build(
        a,
        m,
        Data::new(tag::EXPLORE)
            .u64(host_id)
            .u8(tiles.len() as u8)
            .u8(tiles[0])
            .u8(t1),
    )
}

/// 0x47 SettleExplore: `[payer s] [season] [holding w] [citizen w] [seedcache|archive r] [anchor|archive r]`.
/// `bell`/`region` are the explore record's bell and the explored province's region.
pub fn settle_explore(
    a: &Addresses,
    payer: Address,
    h: HoldingRef,
    wallet: &Address,
    bell: u32,
    region: u8,
    src: SeedSource,
) -> Instruction {
    let mut m = vec![
        ws(payer),
        r(a.season),
        w(h.address(a)),
        w(a.citizen(wallet)),
    ];
    m.extend(seed_pair(a, bell, region, src));
    build(a, m, Data::new(tag::SETTLE_EXPLORE))
}

/// 0x48 DisbandStranded(entry): `[any s] [season] [province w] [holding r]`.
pub fn disband_stranded(
    a: &Addresses,
    any: Address,
    p: i16,
    q: i16,
    entry: u8,
    host_id: u64,
) -> Instruction {
    let holding = a.holding_of_host(host_id).expect("valid host id");
    let m = vec![
        rs(any),
        r(a.season),
        w(a.province(p as i32, q as i32)),
        r(holding),
    ];
    build(a, m, Data::new(tag::DISBAND_STRANDED).u8(entry))
}

// ================================================================ marches

/// Depart's data (§5.11): 219 B with the tag.
#[derive(Clone, Debug)]
pub struct DepartArgs {
    pub host_id: u64,
    pub commit: [u8; 32],
    pub seal: [u8; 165],
    pub arrive_bell: u32,
    pub tip: u64,
    pub transit_slot: u8,
}

/// 0x50 Depart: P + `[holding w] [province w] [system]`.
pub fn depart(
    a: &Addresses,
    p: &Player,
    h: HoldingRef,
    host_province: (i16, i16),
    args: &DepartArgs,
) -> Instruction {
    let mut m = p.metas(a);
    m.push(w(h.address(a)));
    m.push(w(a.province(host_province.0 as i32, host_province.1 as i32)));
    m.push(r(addr::system_program()));
    let d = Data::new(tag::DEPART)
        .u64(args.host_id)
        .bytes(&args.commit)
        .bytes(&args.seal)
        .u32(args.arrive_bell)
        .u64(args.tip)
        .u8(args.transit_slot);
    build(a, m, d)
}

/// Reveal's inputs (§5.11).
#[derive(Clone, Debug)]
pub struct RevealArgs {
    pub holding: HoldingRef,
    pub transit_slot: u8,
    pub target_i: u8,
    pub plain: [u8; 37],
    pub salt: [u8; 32],
    pub ct_hash: [u8; 32],
    pub beneficiary: Address,
    /// Destination province and arrival bell (from the plaintext).
    pub dest: (i32, i32),
    pub arrive: u32,
    pub faction: u8,
    /// Whether this Reveal must create or write the ArrivalDay (its bit clear).
    pub day_writable: bool,
    /// Path provinces other than the destination, 0–3, first-entered order.
    pub path_provinces: Vec<(i32, i32)>,
}

/// 0x51 Reveal (class W): `[fee_payer s,w] [season r] [holding r] [anchor r] [archive r] [beaconlog r] [inputs r] [dest_province r]
/// [arrivalday r|w] [slot0..slot3 (the target writable)] [path provinces 0–3] [ix sysvar r] [system r]`.
pub fn reveal(a: &Addresses, fee_payer: Address, x: &RevealArgs) -> Instruction {
    assert!(
        x.path_provinces.len() <= 3,
        "≤ 3 path provinces besides the destination"
    );
    let (p, q) = x.dest;
    let rg = region_of(p, q);
    let mut m = vec![
        ws(fee_payer),
        r(a.season),
        r(x.holding.address(a)),
        r(a.anchor(x.arrive, rg)),
        r(a.archive(rg, addr::archive_part(x.arrive))),
        r(a.beacon_log(rg)),
        r(a.clash_inputs(p, q, x.arrive)),
        r(a.province(p, q)),
    ];
    let day = a.arrival_day(p, q, addr::day_of(x.arrive));
    m.push(if x.day_writable { w(day) } else { r(day) });
    for i in 0..4u8 {
        let k = a.arrival_slot(p, q, x.arrive, x.faction, i);
        m.push(if i == x.target_i { w(k) } else { r(k) });
    }
    m.extend(
        x.path_provinces
            .iter()
            .map(|&(pp, qq)| r(a.province(pp, qq))),
    );
    m.push(r(addr::instructions_sysvar()));
    m.push(r(addr::system_program()));
    let d = Data::new(tag::REVEAL)
        .u8(x.transit_slot)
        .u8(x.target_i)
        .bytes(&x.plain)
        .bytes(&x.salt)
        .bytes(&x.ct_hash)
        .key(&x.beneficiary);
    build(a, m, d)
}

/// 0x52 SettleDeparture: `[payer s] [season] [origin province w] [holding w]`.
pub fn settle_departure(
    a: &Addresses,
    payer: Address,
    origin: (i16, i16),
    h: HoldingRef,
    transit_slot: u8,
) -> Instruction {
    let m = vec![
        ws(payer),
        r(a.season),
        w(a.province(origin.0 as i32, origin.1 as i32)),
        w(h.address(a)),
    ];
    build(a, m, Data::new(tag::SETTLE_DEPARTURE).u8(transit_slot))
}

/// `transit_slot` of SettleDeparture that asks for the **return settle**
/// (v1.5 §21): W4-A's choice (its `proc/clash.rs` `RETURN_SLOT`, to be
/// recorded in v1.6). Same accounts as SettleDeparture; every state-3
/// `Leave` entry of that Holding in that Province is freed and
/// `reserve[unit] += troops / 1,000` credited to the Holding when it is live
/// with the host's generation (else the troops are lost); nothing to
/// return is `AlreadyDone`.
pub const RETURN_SLOT: u8 = 0xFF;

/// The return settle: SettleDeparture `[payer s] [season] [province w]
/// [holding w]` with `transit_slot = RETURN_SLOT`.
pub fn settle_return(
    a: &Addresses,
    payer: Address,
    province: (i16, i16),
    h: HoldingRef,
) -> Instruction {
    settle_departure(a, payer, province, h, RETURN_SLOT)
}

/// SettleTransit's inputs (§5.11).
#[derive(Clone, Debug)]
pub struct SettleTransitArgs {
    pub holding: HoldingRef,
    pub transit_slot: u8,
    pub commit: [u8; 32],
    pub seal: [u8; 165],
    pub beneficiary: Address,
    pub dest: (i32, i32),
    pub arrive: u32,
    pub faction: u8,
    /// The slot index the arrival holds (or the canonical absent index 0).
    pub slot_i: u8,
    pub home: (i32, i32),
    /// THE anchor present (false: its region-day archive).
    pub anchor_present: bool,
    pub slot_beneficiary: Address,
    pub resolver: Address,
    pub holding_rent_payer: Address,
    /// v1.7 (I-56): the Holding's owner Citizen, when this host earns the
    /// camp's Works (the lowest `camp_mask` position, fate Stays:
    /// [`camp_winner`]); `None` otherwise.
    pub camp_citizen: Option<Address>,
}

/// Whether the arrival at `(faction, i)` of resolved inputs with this
/// `camp_mask` (ClashInputs offset 76) earns the camp's Works when its fate
/// is Stays (v1.7: the lowest set position, one winner per camp).
pub fn camp_winner(camp_mask: u32, faction: u8, i: u8, fate: u8) -> bool {
    fate == 1 && camp_mask != 0 && camp_mask.trailing_zeros() == faction as u32 * 4 + i as u32
}

/// 0x54 SettleTransit: `[payer s,w] [season] [holding w] [dest province w] [inputs w] [slot w] [home province w] [anchor|archive r]
/// [slot_beneficiary w] [resolver w] [holding_rent_payer w] [settle_beneficiary w] [system] ([citizen w])`.
pub fn settle_transit(a: &Addresses, payer: Address, x: &SettleTransitArgs) -> Instruction {
    let (p, q) = x.dest;
    let rg = region_of(p, q);
    let anchor = if x.anchor_present {
        a.anchor(x.arrive, rg)
    } else {
        a.archive(rg, addr::archive_part(x.arrive))
    };
    let mut m = vec![
        ws(payer),
        r(a.season),
        w(x.holding.address(a)),
        w(a.province(p, q)),
        w(a.clash_inputs(p, q, x.arrive)),
        w(a.arrival_slot(p, q, x.arrive, x.faction, x.slot_i)),
        w(a.province(x.home.0, x.home.1)),
        r(anchor),
        w(x.slot_beneficiary),
        w(x.resolver),
        w(x.holding_rent_payer),
        w(x.beneficiary),
        r(addr::system_program()),
    ];
    if let Some(c) = x.camp_citizen {
        m.push(w(c));
    }
    let d = Data::new(tag::SETTLE_TRANSIT)
        .u8(x.transit_slot)
        .bytes(&x.commit)
        .bytes(&x.seal)
        .key(&x.beneficiary);
    build(a, m, d)
}

/// 0x55 SweepPoolOwed: `[any s] [season] [holding w] [dpool w]`.
pub fn sweep_pool_owed(a: &Addresses, any: Address, h: HoldingRef) -> Instruction {
    build(
        a,
        vec![rs(any), r(a.season), w(h.address(a)), w(a.defence_pool())],
        Data::new(tag::SWEEP_POOL_OWED),
    )
}

// ================================================================ clashes

/// Where a gather or skip reads THE anchor: present, or the archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorSource {
    Anchor,
    Archive,
}

fn anchor_or_archive(a: &Addresses, bell: u32, region: u8, s: AnchorSource) -> Address {
    match s {
        AnchorSource::Anchor => a.anchor(bell, region),
        AnchorSource::Archive => a.archive(region, addr::archive_part(bell)),
    }
}

/// 0x60 GatherClash(part): K + `[province r] [anchor|archive r] [arrivalday r] [inputs w] [ix sysvar] [system] [slot_k r …] [holding_k r …]`.
/// `slots` are `(faction, i)` positions in `start..start+n` order; `holdings`
/// the owner Holdings of the present ones (`holdings_bitmap` marks which).
#[allow(clippy::too_many_arguments)]
pub fn gather_clash(
    a: &Addresses,
    fee_payer: Address,
    dest: (i32, i32),
    bell: u32,
    src: AnchorSource,
    start: u8,
    slots: &[(u8, u8)],
    holdings: &[Address],
    holdings_bitmap: u32,
    beneficiary: &Address,
) -> Instruction {
    let (p, q) = dest;
    let rg = region_of(p, q);
    let mut m = vec![
        ws(fee_payer),
        r(a.season),
        r(a.province(p, q)),
        r(anchor_or_archive(a, bell, rg, src)),
        r(a.arrival_day(p, q, addr::day_of(bell))),
        w(a.clash_inputs(p, q, bell)),
        r(addr::instructions_sysvar()),
        r(addr::system_program()),
    ];
    m.extend(
        slots
            .iter()
            .map(|&(f, i)| r(a.arrival_slot(p, q, bell, f, i))),
    );
    // v1.7: writable (the gathered transit's stamp, W4-B F1).
    m.extend(holdings.iter().map(|&h| w(h)));
    let d = Data::new(tag::GATHER_CLASH)
        .u32(bell)
        .u8(start)
        .u8(slots.len() as u8)
        .u32(holdings_bitmap)
        .key(beneficiary);
    build(a, m, d)
}

/// 0x61 ResolveFromInputs: K + `[province w] [inputs w] [seedcache|archive r] [anchor|archive r] [ix sysvar]`.
pub fn resolve_from_inputs(
    a: &Addresses,
    fee_payer: Address,
    dest: (i32, i32),
    bell: u32,
    src: SeedSource,
    beneficiary: &Address,
) -> Instruction {
    let (p, q) = dest;
    let mut m = vec![
        ws(fee_payer),
        r(a.season),
        w(a.province(p, q)),
        w(a.clash_inputs(p, q, bell)),
    ];
    m.extend(seed_pair(a, bell, region_of(p, q), src));
    m.push(r(addr::instructions_sysvar()));
    build(
        a,
        m,
        Data::new(tag::RESOLVE_FROM_INPUTS)
            .u32(bell)
            .key(beneficiary),
    )
}

/// 0x62 ResolveClash (`oracle` builds only), frontier-abi's shape:
/// `[fee_payer s,w] [season] [province w] [any × ≤ 60]`, data
/// `bell u32 ‖ beneficiary [32]` (integ-W1: the first builder passed raw
/// bytes and omitted the province).
pub fn resolve_clash_oracle(
    a: &Addresses,
    fee_payer: Address,
    dest: (i32, i32),
    bell: u32,
    beneficiary: &Address,
    accounts: Vec<AccountMeta>,
) -> Instruction {
    let (p, q) = dest;
    let mut m = vec![ws(fee_payer), r(a.season), w(a.province(p, q))];
    m.extend(accounts);
    build(
        a,
        m,
        Data::new(tag::RESOLVE_CLASH).u32(bell).key(beneficiary),
    )
}

/// 0x63 SkipQuiet(b0, n): `[payer s] [season] [province w] [arrivalday_0 r] [arrivalday_1 r] [anchor_or_archive × n r]`.
pub fn skip_quiet(
    a: &Addresses,
    payer: Address,
    dest: (i32, i32),
    b0: u32,
    n: u8,
    src: &[AnchorSource],
) -> Instruction {
    assert!((1..=24).contains(&n) && src.len() == n as usize);
    let (p, q) = dest;
    let rg = region_of(p, q);
    let d0 = addr::day_of(b0);
    let d1 = addr::day_of(b0 + n as u32 - 1);
    let mut m = vec![
        ws(payer),
        r(a.season),
        w(a.province(p, q)),
        r(a.arrival_day(p, q, d0)),
        r(a.arrival_day(p, q, if d1 != d0 { d1 } else { d0 + 1 })),
    ];
    for (k, s) in src.iter().enumerate() {
        m.push(r(anchor_or_archive(a, b0 + k as u32, rg, *s)));
    }
    build(a, m, Data::new(tag::SKIP_QUIET).u32(b0).u8(n))
}

/// 0x64 CloseClashInputs: `[any s] [season] [province r] [inputs w] [rent_to w]`.
pub fn close_clash_inputs(
    a: &Addresses,
    any: Address,
    p: i16,
    q: i16,
    bell: u32,
    rent_to: Address,
) -> Instruction {
    let (pi, qi) = (p as i32, q as i32);
    let m = vec![
        rs(any),
        r(a.season),
        r(a.province(pi, qi)),
        w(a.clash_inputs(pi, qi, bell)),
        w(rent_to),
    ];
    build(
        a,
        m,
        Data::new(tag::CLOSE_CLASH_INPUTS).i16(p).i16(q).u32(bell),
    )
}

/// 0x65 CloseArrivalDay: `[any s] [season] [province r] [day w] [rent_to w]`.
pub fn close_arrival_day(
    a: &Addresses,
    any: Address,
    p: i16,
    q: i16,
    day: u32,
    rent_to: Address,
) -> Instruction {
    let (pi, qi) = (p as i32, q as i32);
    let m = vec![
        rs(any),
        r(a.season),
        r(a.province(pi, qi)),
        w(a.arrival_day(pi, qi, day)),
        w(rent_to),
    ];
    build(
        a,
        m,
        Data::new(tag::CLOSE_ARRIVAL_DAY).i16(p).i16(q).u32(day),
    )
}

/// 0x66 CloseArrivalSlot: `[any s] [season] [slot w] [rent_to w] ([anchor r] for case a)`.
#[allow(clippy::too_many_arguments)]
pub fn close_arrival_slot(
    a: &Addresses,
    any: Address,
    p: i16,
    q: i16,
    bell: u32,
    faction: u8,
    i: u8,
    rent_to: Address,
    with_anchor: bool,
) -> Instruction {
    let (pi, qi) = (p as i32, q as i32);
    let mut m = vec![
        rs(any),
        r(a.season),
        w(a.arrival_slot(pi, qi, bell, faction, i)),
        w(rent_to),
    ];
    if with_anchor {
        m.push(r(a.anchor(bell, region_of(pi, qi))));
    }
    build(
        a,
        m,
        Data::new(tag::CLOSE_ARRIVAL_SLOT)
            .i16(p)
            .i16(q)
            .u32(bell)
            .u8(faction)
            .u8(i),
    )
}

/// One ArrivalSlot claimed by ClaimDefence.
#[derive(Clone, Copy, Debug)]
pub struct ClaimSlot {
    pub p: i32,
    pub q: i32,
    pub bell: u32,
    pub faction: u8,
    pub i: u8,
}

/// 0x70 ClaimDefence: `[keeper s,w] [season] [dpool w] [claim w] [system] ([slot w] [anchor r]) × ≤ 6`.
pub fn claim_defence(a: &Addresses, keeper: Address, day: u32, slots: &[ClaimSlot]) -> Instruction {
    assert!(slots.len() <= 6, "≤ 6 slots per claim");
    let mut m = vec![
        ws(keeper),
        r(a.season),
        w(a.defence_pool()),
        w(a.defence_claim(&keeper, day)),
        r(addr::system_program()),
    ];
    for s in slots {
        m.push(w(a.arrival_slot(s.p, s.q, s.bell, s.faction, s.i)));
        m.push(r(a.anchor(s.bell, region_of(s.p, s.q))));
    }
    build(
        a,
        m,
        Data::new(tag::CLAIM_DEFENCE).u32(day).u8(slots.len() as u8),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// W5-A R5: the float close appends `[target w] [recipient w]` pairs to
    /// the ten fixed accounts of CloseSeason (the svm builder's shape).
    #[test]
    fn close_season_float_appends_writable_pairs() {
        let a = Addresses::new(Address::new_from_array([1; 32]), 7);
        let k = Address::new_from_array([2; 32]);
        let pairs: Vec<(Address, Address)> = (0..3u8)
            .map(|i| {
                (
                    Address::new_from_array([10 + i; 32]),
                    Address::new_from_array([20 + i; 32]),
                )
            })
            .collect();
        let base = close_season(&a, k, 9, &[], &[]);
        let ix = close_season_float(&a, k, 9, &pairs);
        assert_eq!(base.accounts.len(), 10);
        assert_eq!(ix.accounts.len(), 10 + 2 * pairs.len());
        assert_eq!(ix.accounts[..10], base.accounts[..]);
        for (n, (t, r)) in pairs.iter().enumerate() {
            let (mt, mr) = (&ix.accounts[10 + 2 * n], &ix.accounts[11 + 2 * n]);
            assert_eq!((mt.pubkey, mt.is_writable, mt.is_signer), (*t, true, false));
            assert_eq!((mr.pubkey, mr.is_writable, mr.is_signer), (*r, true, false));
        }
        assert_eq!(ix.data, vec![tag::CLOSE_SEASON, 9]);
    }

    fn addrs() -> Addresses {
        Addresses::new(Address::new_from_array([7; 32]), 1)
    }

    #[test]
    fn data_lengths_match_the_contract() {
        let a = addrs();
        let k = Address::new_from_array([9; 32]);
        let dep = DepartArgs {
            host_id: 1,
            commit: [0; 32],
            seal: [0; 165],
            arrive_bell: 3,
            tip: 14_441,
            transit_slot: 0,
        };
        let pl = Player {
            actor: k,
            payer: k,
            wallet: k,
        };
        let h = HoldingRef {
            p: 2,
            q: 0,
            site: 3,
        };
        assert_eq!(
            depart(&a, &pl, h, (2, 0), &dep).data.len(),
            219,
            "Depart 219 B with the tag"
        );
        let rv = RevealArgs {
            holding: h,
            transit_slot: 0,
            target_i: 2,
            plain: [0; 37],
            salt: [0; 32],
            ct_hash: [0; 32],
            beneficiary: k,
            dest: (2, 0),
            arrive: 10,
            faction: 1,
            day_writable: true,
            path_provinces: vec![],
        };
        let ix = reveal(&a, k, &rv);
        assert_eq!(ix.data.len(), 136, "Reveal 135 B + tag");
        assert_eq!(ix.accounts.len(), 15);
        assert!(
            ix.accounts[11].is_writable && !ix.accounts[9].is_writable,
            "only the target slot writable"
        );
        let st = SettleTransitArgs {
            holding: h,
            transit_slot: 0,
            commit: [0; 32],
            seal: [0; 165],
            beneficiary: k,
            dest: (2, 0),
            arrive: 10,
            faction: 1,
            slot_i: 0,
            home: (2, 0),
            anchor_present: true,
            slot_beneficiary: k,
            resolver: k,
            holding_rent_payer: k,
            camp_citizen: None,
        };
        assert_eq!(
            settle_transit(&a, k, &st).data.len(),
            231,
            "SettleTransit 231 B with the tag"
        );
        let b = BeaconArg {
            round: 1,
            sig48: [0; 48],
            hints: vec![0; HINTS_LEN],
        };
        // PostAnchor: tag + region + bell + round + sig48 + hints + beneficiary.
        assert_eq!(
            post_anchor(&a, k, 0, 0, &b, &k).data.len(),
            1 + 1 + 4 + 8 + 48 + HINTS_LEN + 32
        );
    }

    #[test]
    fn fold_parts_list_24_shards() {
        let a = addrs();
        let k = Address::new_from_array([9; 32]);
        assert_eq!(fold_occupancy(&a, k, 0).accounts.len(), 3 + 24);
        assert_eq!(fold_occupancy(&a, k, 1).accounts.len(), 3 + 24);
        assert_eq!(fold_occupancy(&a, k, 2).accounts.len(), 3 + 6);
    }

    /// Regions a PostAnchorMulti fits in one packet with the budget prefix
    /// (k anchors + k archives): measured below.
    const MULTI_FIT: u16 = 7;

    #[test]
    fn post_anchor_multi_packet_limit() {
        let a = addrs();
        let k = Address::new_from_array([9; 32]);
        let b = BeaconArg {
            round: 1,
            sig48: [0; 48],
            hints: vec![0; HINTS_LEN],
        };
        let bh = solana_hash::Hash::new_from_array([1; 32]);
        let size = |n: u16| {
            let ixn = post_anchor_multi(&a, k, 0, &b, (1u16 << n) - 1, &k);
            let prefix = crate::tx::TxBudget {
                cu_limit: 1,
                cu_price: 1,
                loaded_limit: 1,
                heap: None,
            }
            .instructions();
            let mut all = prefix;
            all.push(ixn);
            crate::tx::shape(&solana_message::Message::new_with_blockhash(
                &all,
                Some(&k),
                &bh,
            ))
            .bytes
        };
        let sizes: Vec<usize> = (1..=8).map(size).collect();
        eprintln!("PostAnchorMulti bytes for k = 1..8 regions: {sizes:?}");
        assert!(size(MULTI_FIT) <= crate::abi::PACKET);
        assert!(
            size(MULTI_FIT + 1) > crate::abi::PACKET,
            "8 regions do not fit: MULTI_MAX_REGIONS ≤ 7 with per-region archives"
        );
    }

    /// One builder per tag of §5.5 (50 incl. the oracle ResolveClash), each
    /// within the packet and lock limits when signed by its fee payer.
    #[test]
    fn every_tag_has_a_builder_that_fits_a_packet() {
        let a = addrs();
        let k = Address::new_from_array([9; 32]);
        let w2 = Address::new_from_array([8; 32]);
        let pl = Player {
            actor: w2,
            payer: k,
            wallet: w2,
        };
        let h = HoldingRef {
            p: 2,
            q: 0,
            site: 3,
        };
        let b = BeaconArg {
            round: 1,
            sig48: [0; 48],
            hints: vec![0; HINTS_LEN],
        };
        let s = |p, q, site| Site { p, q, site };
        let dep = DepartArgs {
            host_id: 1,
            commit: [0; 32],
            seal: [0; 165],
            arrive_bell: 3,
            tip: 14_441,
            transit_slot: 0,
        };
        let rv = RevealArgs {
            holding: h,
            transit_slot: 0,
            target_i: 3,
            plain: [0; 37],
            salt: [0; 32],
            ct_hash: [0; 32],
            beneficiary: k,
            dest: (3, 0),
            arrive: 10,
            faction: 1,
            day_writable: true,
            path_provinces: vec![(2, 0), (2, 1), (3, 1)],
        };
        let st = SettleTransitArgs {
            holding: h,
            transit_slot: 0,
            commit: [0; 32],
            seal: [0; 165],
            beneficiary: k,
            dest: (3, 0),
            arrive: 10,
            faction: 1,
            slot_i: 0,
            home: (2, 0),
            anchor_present: true,
            slot_beneficiary: k,
            resolver: k,
            holding_rent_payer: k,
            camp_citizen: None,
        };
        let items: Vec<ArchiveItem> = (0..8)
            .map(|i| ArchiveItem {
                bell: i,
                cache_nonce: 0,
                anchor_rent_to: k,
            })
            .collect();
        let disp = Displaced {
            rent_payer: k,
            wallet: w2,
            faction: 1,
        };
        let three = [s(2, 0, 3), s(2, 1, 4), s(3, 0, 5)];
        let slots: Vec<(u8, u8)> = (0..10).map(|i| (1, i % 4)).collect();
        let holds: Vec<Address> = (0..10)
            .map(|i| Address::new_from_array([i as u8 + 20; 32]))
            .collect();
        let claims: Vec<ClaimSlot> = (0..6)
            .map(|i| ClaimSlot {
                p: 3,
                q: 0,
                bell: 10,
                faction: 1,
                i: i % 4,
            })
            .collect();
        let shards: Vec<(u8, u8)> = (0..8).map(|i| (0, i)).collect();
        let all = vec![
            announce_season(&a, k, [1; 32], 5, 6),
            create_season(&a, k, &[0; 224], &[0; 128]),
            init_beacon_logs(&a, k),
            init_shards(&a, k, 5),
            consume_genesis_seed(&a, k, &b),
            end_season(&a, k),
            close_season(&a, k, 0, &shards, &[]),
            abort_season(&a, k, w2),
            set_window_schedule(&a, k, 900, 300),
            post_anchor(&a, k, 0, 0, &b, &k),
            post_anchor_multi(&a, k, 0, &b, (1 << MULTI_FIT) - 1, &k),
            post_seed(&a, k, 0, 0, 7, &b, &k),
            post_beacon(&a, k, 0, &b),
            archive_anchors(&a, k, 3, 0, &items),
            close_seed_cache(&a, k, 0, 3, 7, k),
            open_ring(&a, k, 2),
            consume_ring_seed(&a, k, 2, &b),
            open_province(&a, k, 2, 0),
            fold_occupancy(&a, k, 1),
            close_province(&a, k, 2, 0),
            join(&a, w2, k, 1, &k, 9, Some(k)),
            set_session(&a, &pl, &k, 9),
            set_vigil(&a, &pl, 60),
            file_ticket(&a, &pl, &three),
            settle_ticket(
                &a,
                k,
                &w2,
                1,
                0,
                three[0],
                5,
                &three,
                SeedSource::Cache { nonce: 1 },
                Some(&disp),
            ),
            release_dormant(&a, k, h, &w2, 1, k),
            close_holding(&a, k, h, k),
            close_citizen(&a, k, &w2, k, Some(k)),
            harvest(&a, &pl, h),
            build_item(&a, &pl, h, 7, true),
            train(&a, &pl, h, 1, 500),
            muster(&a, &pl, h, 1, 500, 30),
            dissolve(&a, &pl, h, 1),
            garrison(&a, &pl, h, -5),
            explore(&a, &pl, h, (2, 0), 1, &[1, 2]),
            settle_explore(&a, k, h, &w2, 5, 3, SeedSource::Archive),
            disband_stranded(&a, k, 2, 0, 3, crate::addr::host_id(2, 0, 3, 0, 1).unwrap()),
            depart(&a, &pl, h, (2, 0), &dep),
            reveal(&a, k, &rv),
            settle_departure(&a, k, (2, 0), h, 0),
            settle_transit(&a, k, &st),
            sweep_pool_owed(&a, k, h),
            gather_clash(
                &a,
                k,
                (3, 0),
                10,
                AnchorSource::Anchor,
                0,
                &slots,
                &holds,
                0x3FF,
                &k,
            ),
            resolve_from_inputs(&a, k, (3, 0), 10, SeedSource::Cache { nonce: 0 }, &k),
            resolve_clash_oracle(&a, k, (2, 0), 7, &k, vec![]),
            skip_quiet(&a, k, (3, 0), 140, 24, &[AnchorSource::Anchor; 24]),
            close_clash_inputs(&a, k, 3, 0, 10, k),
            close_arrival_day(&a, k, 3, 0, 0, k),
            close_arrival_slot(&a, k, 3, 0, 10, 1, 2, k, true),
            claim_defence(&a, k, 0, &claims),
        ];
        let mut tags: Vec<u8> = all.iter().map(|i| i.data[0]).collect();
        tags.sort();
        let mut want: Vec<u8> = crate::abi::INSTRUCTIONS.iter().map(|i| i.tag).collect();
        want.sort();
        assert_eq!(tags, want, "one builder per tag");
        let bh = solana_hash::Hash::new_from_array([1; 32]);
        let mut oversize = vec![];
        for ixn in &all {
            let msg = solana_message::Message::new_with_blockhash(
                &[
                    crate::tx::set_compute_unit_limit(1),
                    crate::tx::set_compute_unit_price(1),
                    crate::tx::set_loaded_accounts_data_size_limit(1),
                    ixn.clone(),
                ],
                Some(&k),
                &bh,
            );
            let sh = crate::tx::shape(&msg);
            let name = crate::abi::ix_info(ixn.data[0]).unwrap().name;
            // frontier-abi's table accepts the shape (integ-W1: data length
            // and account count inside the canonical bounds).
            let aix = frontier_abi::tags::Ix::from_tag(ixn.data[0]).expect("abi tag");
            let (dlo, dhi) = frontier_abi::ix::data_len_range(aix);
            assert!(
                (dlo..=dhi).contains(&ixn.data.len()),
                "{name}: data {} not in {dlo}..={dhi}",
                ixn.data.len()
            );
            let (alo, ahi) = frontier_abi::prologue::count_bounds(aix);
            assert!(
                (alo..=ahi).contains(&ixn.accounts.len()),
                "{name}: {} accounts not in {alo}..={ahi}",
                ixn.accounts.len()
            );
            assert!(
                sh.locks <= crate::abi::LOCK_LIMIT,
                "{name}: {} locks",
                sh.locks
            );
            if sh.bytes > crate::abi::PACKET {
                oversize.push((name, sh.bytes));
            }
        }
        // W1-F's contract finding (FoldOccupancy part 1 at 1,288 B) is
        // resolved by v1.2's three-part fold: every shape fits the packet.
        assert!(oversize.is_empty(), "{oversize:?}");
    }
}
