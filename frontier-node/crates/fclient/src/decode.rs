//! Account decoders over the §5.3 layouts. Every decoder checks the magic
//! and the minimum size and returns typed fields; the byte offsets live in
//! [`crate::abi::layout`]. Readers never trust a decoded convenience field
//! for anything they sign: they re-derive keys with [`crate::addr`].

use solana_address::Address;

use crate::abi::{layout as l, magic, size};

/// Why an account cannot be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Shorter than the layout.
    Short { want: usize, got: usize },
    /// Magic does not match the expected kind.
    Magic { want: [u8; 8], got: [u8; 8] },
}

/// Little-endian reads at fixed offsets.
#[derive(Clone, Copy)]
pub struct Bytes<'a>(pub &'a [u8]);

impl<'a> Bytes<'a> {
    pub fn u8(&self, o: usize) -> u8 {
        self.0[o]
    }
    pub fn u16(&self, o: usize) -> u16 {
        u16::from_le_bytes(self.0[o..o + 2].try_into().expect("2"))
    }
    pub fn i16(&self, o: usize) -> i16 {
        i16::from_le_bytes(self.0[o..o + 2].try_into().expect("2"))
    }
    pub fn u32(&self, o: usize) -> u32 {
        u32::from_le_bytes(self.0[o..o + 4].try_into().expect("4"))
    }
    pub fn i32(&self, o: usize) -> i32 {
        i32::from_le_bytes(self.0[o..o + 4].try_into().expect("4"))
    }
    pub fn u64(&self, o: usize) -> u64 {
        u64::from_le_bytes(self.0[o..o + 8].try_into().expect("8"))
    }
    pub fn i64(&self, o: usize) -> i64 {
        i64::from_le_bytes(self.0[o..o + 8].try_into().expect("8"))
    }
    pub fn arr<const N: usize>(&self, o: usize) -> [u8; N] {
        self.0[o..o + N].try_into().expect("N")
    }
    pub fn key(&self, o: usize) -> Address {
        Address::new_from_array(self.arr::<32>(o))
    }
    pub fn slice(&self, o: usize, n: usize) -> &'a [u8] {
        &self.0[o..o + n]
    }
}

fn check<'a>(d: &'a [u8], want_magic: &[u8; 8], want_len: usize) -> Result<Bytes<'a>, DecodeError> {
    if d.len() < want_len {
        return Err(DecodeError::Short {
            want: want_len,
            got: d.len(),
        });
    }
    let got: [u8; 8] = d[..8].try_into().expect("8");
    if &got != want_magic {
        return Err(DecodeError::Magic {
            want: *want_magic,
            got,
        });
    }
    Ok(Bytes(d))
}

/// An account is *absent* iff System-owned with no data (lamports ignored,
/// §4.1: pre-funded = absent).
pub fn is_absent(owner: &Address, data: &[u8]) -> bool {
    *owner == crate::addr::system_program() && data.is_empty()
}

/// The chained header H.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub magic: [u8; 8],
    pub season_id: u64,
    pub layout_version: u16,
    pub event_seq: u64,
    pub event_head: [u8; 32],
}

fn header(b: Bytes) -> Header {
    Header {
        magic: b.arr(l::h::MAGIC),
        season_id: b.u64(l::h::SEASON_ID),
        layout_version: b.u16(l::h::LAYOUT_VERSION),
        event_seq: b.u64(l::h::EVENT_SEQ),
        event_head: b.arr(l::h::EVENT_HEAD),
    }
}

/// Reads the chained header of any H account (no magic check).
pub fn chained_header(d: &[u8]) -> Option<Header> {
    (d.len() >= l::h::LEN).then(|| header(Bytes(d)))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Season {
    pub h: Header,
    pub status: u8,
    pub bump: u8,
    pub regions: u8,
    pub genesis_ring: u8,
    pub r_max: u16,
    pub postures_enabled: u8,
    pub authority: Address,
    pub ruleset_hash: [u8; 32],
    pub rules_version: u16,
    pub program_version: u16,
    pub bell_secs: u32,
    pub genesis_ts: i64,
    pub created_ts: i64,
    pub join_close_bell: u32,
    pub end_bell: u32,
    pub drand_genesis: i64,
    pub drand_period: u32,
    pub network: u8,
    pub quicknet_pk_hash: [u8; 32],
    pub reveal_window: u32,
    pub seed_margin: u32,
    pub window_next: u32,
    pub window_from_bell: u32,
    pub genesis_round: u64,
    pub genesis_seed: [u8; 32],
    pub archive_after: u32,
    pub min_lead: u8,
    pub max_lead: u8,
    pub transit_slots: u8,
    pub march_fee: u64,
    pub seal_bond: u64,
    pub min_reveal_priority_milli: u32,
    pub reveal_cu_limit: u32,
    pub bucket_rate_per_h: u16,
    pub bucket_burst: u16,
    pub defence_cap_milli: u32,
    pub lateness_slots: u8,
    pub theta_early_bps: u16,
    pub theta_late_bps: u16,
    pub theta_switch_secs: u32,
    pub reserve_bps: u16,
    pub extra_free_bps: u16,
    pub clash_close_grace: u32,
    pub camp_regrow_bells: u32,
    pub params_hash: [u8; 32],
    pub t_create_min: i64,
    pub announced_ts: i64,
    pub creation_bond: u64,
    pub payout_params_hash: [u8; 32],
    pub dormant_after_secs: u32,
    pub release_after_secs: u32,
    pub pfund_initial: u64,
    pub dpool_initial: u64,
    pub reveal_loaded_limit: u32,
    pub join_gate: Address,
}

impl Season {
    pub fn decode(d: &[u8]) -> Result<Season, DecodeError> {
        use l::season as s;
        let b = check(d, magic::SEASON, s::END)?;
        Ok(Season {
            h: header(b),
            status: b.u8(s::STATUS),
            bump: b.u8(s::BUMP),
            regions: b.u8(s::REGIONS),
            genesis_ring: b.u8(s::GENESIS_RING),
            r_max: b.u16(s::R_MAX),
            postures_enabled: b.u8(s::POSTURES_ENABLED),
            authority: b.key(s::AUTHORITY),
            ruleset_hash: b.arr(s::RULESET_HASH),
            rules_version: b.u16(s::RULES_VERSION),
            program_version: b.u16(s::PROGRAM_VERSION),
            bell_secs: b.u32(s::BELL_SECS),
            genesis_ts: b.i64(s::GENESIS_TS),
            created_ts: b.i64(s::CREATED_TS),
            join_close_bell: b.u32(s::JOIN_CLOSE_BELL),
            end_bell: b.u32(s::END_BELL),
            drand_genesis: b.i64(s::DRAND_GENESIS),
            drand_period: b.u32(s::DRAND_PERIOD),
            network: b.u8(s::NETWORK),
            quicknet_pk_hash: b.arr(s::QUICKNET_PK_HASH),
            reveal_window: b.u32(s::REVEAL_WINDOW),
            seed_margin: b.u32(s::SEED_MARGIN),
            window_next: b.u32(s::WINDOW_NEXT),
            window_from_bell: b.u32(s::WINDOW_FROM_BELL),
            genesis_round: b.u64(s::GENESIS_ROUND),
            genesis_seed: b.arr(s::GENESIS_SEED),
            archive_after: b.u32(s::ARCHIVE_AFTER),
            min_lead: b.u8(s::MIN_LEAD),
            max_lead: b.u8(s::MAX_LEAD),
            transit_slots: b.u8(s::TRANSIT_SLOTS),
            march_fee: b.u64(s::MARCH_FEE),
            seal_bond: b.u64(s::SEAL_BOND),
            min_reveal_priority_milli: b.u32(s::MIN_REVEAL_PRIORITY_MILLI),
            reveal_cu_limit: b.u32(s::REVEAL_CU_LIMIT),
            bucket_rate_per_h: b.u16(s::BUCKET_RATE_PER_H),
            bucket_burst: b.u16(s::BUCKET_BURST),
            defence_cap_milli: b.u32(s::DEFENCE_CAP_MILLI),
            lateness_slots: b.u8(s::LATENESS_SLOTS),
            theta_early_bps: b.u16(s::THETA_EARLY_BPS),
            theta_late_bps: b.u16(s::THETA_LATE_BPS),
            theta_switch_secs: b.u32(s::THETA_SWITCH_SECS),
            reserve_bps: b.u16(s::RESERVE_BPS),
            extra_free_bps: b.u16(s::EXTRA_FREE_BPS),
            clash_close_grace: b.u32(s::CLASH_CLOSE_GRACE),
            camp_regrow_bells: b.u32(s::CAMP_REGROW_BELLS),
            params_hash: b.arr(s::PARAMS_HASH),
            t_create_min: b.i64(s::T_CREATE_MIN),
            announced_ts: b.i64(s::ANNOUNCED_TS),
            creation_bond: b.u64(s::CREATION_BOND),
            payout_params_hash: b.arr(s::PAYOUT_PARAMS_HASH),
            dormant_after_secs: b.u32(s::DORMANT_AFTER_SECS),
            release_after_secs: b.u32(s::RELEASE_AFTER_SECS),
            pfund_initial: b.u64(s::PFUND_INITIAL),
            dpool_initial: b.u64(s::DPOOL_INITIAL),
            reveal_loaded_limit: b.u32(s::REVEAL_LOADED_LIMIT),
            join_gate: b.key(s::JOIN_GATE),
        })
    }

    /// Effective status at `now` (§5.3: Seeded with `now ≥ genesis_ts` is Running).
    pub fn effective_status(&self, now: i64) -> u8 {
        if self.status == crate::abi::status::SEEDED && now >= self.genesis_ts {
            crate::abi::status::RUNNING
        } else {
            self.status
        }
    }

    /// `W(b)` (§5.1).
    pub fn window(&self, bell: u32) -> u32 {
        if self.window_from_bell != u32::MAX && bell >= self.window_from_bell {
            self.window_next
        } else {
            self.reveal_window
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frontier {
    pub h: Header,
    pub rings_opened: u16,
    pub last_ring_open_bell: u32,
    pub last_ring_open_ts: i64,
    pub fold_bell: u32,
    pub fold_part: u8,
    pub open_sites: u32,
    pub occupied_sites: u32,
    pub provinces_opened: u32,
    pub wedge_open: [u32; 6],
    pub wedge_occupied: [u32; 6],
    pub acc_occupied: u32,
    pub acc_wedge: [u32; 6],
}

fn u32x6(b: Bytes, o: usize) -> [u32; 6] {
    core::array::from_fn(|i| b.u32(o + 4 * i))
}

impl Frontier {
    pub fn decode(d: &[u8]) -> Result<Frontier, DecodeError> {
        use l::frontier as f;
        let b = check(d, magic::FRONTIER, size::FRONTIER)?;
        Ok(Frontier {
            h: header(b),
            rings_opened: b.u16(f::RINGS_OPENED),
            last_ring_open_bell: b.u32(f::LAST_RING_OPEN_BELL),
            last_ring_open_ts: b.i64(f::LAST_RING_OPEN_TS),
            fold_bell: b.u32(f::FOLD_BELL),
            fold_part: b.u8(f::FOLD_PART),
            open_sites: b.u32(f::OPEN_SITES),
            occupied_sites: b.u32(f::OCCUPIED_SITES),
            provinces_opened: b.u32(f::PROVINCES_OPENED),
            wedge_open: u32x6(b, f::WEDGE_OPEN),
            wedge_occupied: u32x6(b, f::WEDGE_OCCUPIED),
            acc_occupied: b.u32(f::ACC_OCCUPIED),
            acc_wedge: u32x6(b, f::ACC_WEDGE),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RingSeed {
    pub season_id: u64,
    pub d: u16,
    pub status: u8,
    pub opened_bell: u32,
    pub t_open: i64,
    pub round: u64,
    pub seed: [u8; 32],
    pub provinces_created: u16,
    pub payer: Address,
}

impl RingSeed {
    pub fn decode(d: &[u8]) -> Result<RingSeed, DecodeError> {
        use l::ring_seed as r;
        let b = check(d, magic::RING_SEED, size::RING_SEED)?;
        Ok(RingSeed {
            season_id: b.u64(l::sh::SEASON_ID),
            d: b.u16(r::D),
            status: b.u8(r::STATUS),
            opened_bell: b.u32(r::OPENED_BELL),
            t_open: b.i64(r::T_OPEN),
            round: b.u64(r::ROUND),
            seed: b.arr(r::SEED),
            provinces_created: b.u16(r::PROVINCES_CREATED),
            payer: b.key(r::PAYER),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvinceFund {
    pub season_id: u64,
    pub wedge: u8,
    pub provinces_opened: u32,
    pub funded_total: u64,
    pub spent_total: u64,
    pub open_sites: u32,
    pub provinces_funded: u32,
}

impl ProvinceFund {
    pub fn decode(d: &[u8]) -> Result<ProvinceFund, DecodeError> {
        use l::province_fund as p;
        let b = check(d, magic::PROVINCE_FUND, size::PROVINCE_FUND)?;
        Ok(ProvinceFund {
            season_id: b.u64(l::sh::SEASON_ID),
            wedge: b.u8(p::WEDGE),
            provinces_opened: b.u32(p::PROVINCES_OPENED),
            funded_total: b.u64(p::FUNDED_TOTAL),
            spent_total: b.u64(p::SPENT_TOTAL),
            open_sites: b.u32(p::OPEN_SITES),
            provinces_funded: b.u32(p::PROVINCES_FUNDED),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinShard {
    pub h: Header,
    pub faction: u8,
    pub shard: u8,
    pub members: u32,
    pub holdings: u32,
    pub final_holdings: u32,
    pub holdings_by_wedge: [u32; 6],
    pub released: u32,
}

impl JoinShard {
    pub fn decode(d: &[u8]) -> Result<JoinShard, DecodeError> {
        use l::join_shard as j;
        let b = check(d, magic::JOIN_SHARD, size::JOIN_SHARD)?;
        Ok(JoinShard {
            h: header(b),
            faction: b.u8(j::FACTION),
            shard: b.u8(j::SHARD),
            members: b.u32(j::MEMBERS),
            holdings: b.u32(j::HOLDINGS),
            final_holdings: b.u32(j::FINAL_HOLDINGS),
            holdings_by_wedge: u32x6(b, j::HOLDINGS_BY_WEDGE),
            released: b.u32(j::RELEASED),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeaconLog {
    pub season_id: u64,
    pub region: u8,
    pub latest_round: u64,
    pub posted_ts: i64,
    pub posted_slot: u64,
    pub sig48: [u8; 48],
    pub beneficiary: Address,
}

impl BeaconLog {
    pub fn decode(d: &[u8]) -> Result<BeaconLog, DecodeError> {
        use l::beacon_log as g;
        let b = check(d, magic::BEACON_LOG, size::BEACON_LOG)?;
        Ok(BeaconLog {
            season_id: b.u64(l::sh::SEASON_ID),
            region: b.u8(g::REGION),
            latest_round: b.u64(g::LATEST_ROUND),
            posted_ts: b.i64(g::POSTED_TS),
            posted_slot: b.u64(g::POSTED_SLOT),
            sig48: b.arr(g::SIG48),
            beneficiary: b.key(g::BENEFICIARY),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefencePool {
    pub season_id: u64,
    pub paid_total: u64,
    pub diverted_total: u64,
    pub per_bell_region_cap: u64,
    pub per_keeper_day_cap: u64,
    pub claims: u64,
}

impl DefencePool {
    pub fn decode(d: &[u8]) -> Result<DefencePool, DecodeError> {
        use l::defence_pool as p;
        let b = check(d, magic::DEFENCE_POOL, size::DEFENCE_POOL)?;
        Ok(DefencePool {
            season_id: b.u64(l::sh::SEASON_ID),
            paid_total: b.u64(p::PAID_TOTAL),
            diverted_total: b.u64(p::DIVERTED_TOTAL),
            per_bell_region_cap: b.u64(p::PER_BELL_REGION_CAP),
            per_keeper_day_cap: b.u64(p::PER_KEEPER_DAY_CAP),
            claims: b.u64(p::CLAIMS),
        })
    }
}

/// `{P, Q, site, gen}` (Citizen holdings) or `{P, Q, site}` (ticket sites).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SiteRef {
    pub p: i16,
    pub q: i16,
    pub site: u8,
    pub gen: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Citizen {
    pub h: Header,
    pub wallet: Address,
    pub session: Address,
    pub session_expiry: i64,
    pub faction: u8,
    pub flags: u8,
    pub holdings_n: u8,
    pub explores_floor_left: u8,
    pub join_bell: u32,
    pub join_shard: u8,
    pub vigil_start_min: u16,
    pub vigil_next_min: u16,
    pub vigil_from_ts: i64,
    pub bucket_milli: u32,
    pub bucket_t: u32,
    pub holding: [SiteRef; 3],
    pub office_terms_used: u8,
    pub ticket_bell: u32,
    pub ticket_sites: [SiteRef; 3],
    pub ticket_next: u8,
    pub citizen_tag: u64,
    pub last_action_ts: i64,
    pub works: u64,
    pub explores: u32,
    pub arrivals: u32,
    pub rent_payer: Address,
    pub ticket_escrow: u64,
    pub ticket_funder: Address,
}

impl Citizen {
    pub fn decode(d: &[u8]) -> Result<Citizen, DecodeError> {
        use l::citizen as c;
        let b = check(d, magic::CITIZEN, size::CITIZEN)?;
        let holding = core::array::from_fn(|i| {
            let o = c::HOLDING + i * c::HOLDING_STRIDE;
            SiteRef {
                p: b.i16(o),
                q: b.i16(o + 2),
                site: b.u8(o + 4),
                gen: b.u8(o + 5),
            }
        });
        let ticket_sites = core::array::from_fn(|i| {
            let o = c::TICKET_SITES + i * c::TICKET_SITE_STRIDE;
            SiteRef {
                p: b.i16(o),
                q: b.i16(o + 2),
                site: b.u8(o + 4),
                gen: 0,
            }
        });
        Ok(Citizen {
            h: header(b),
            wallet: b.key(c::WALLET),
            session: b.key(c::SESSION),
            session_expiry: b.i64(c::SESSION_EXPIRY),
            faction: b.u8(c::FACTION),
            flags: b.u8(c::FLAGS),
            holdings_n: b.u8(c::HOLDINGS_N),
            explores_floor_left: b.u8(c::EXPLORES_FLOOR_LEFT),
            join_bell: b.u32(c::JOIN_BELL),
            join_shard: b.u8(c::JOIN_SHARD),
            vigil_start_min: b.u16(c::VIGIL_START_MIN),
            vigil_next_min: b.u16(c::VIGIL_NEXT_MIN),
            vigil_from_ts: b.i64(c::VIGIL_FROM_TS),
            bucket_milli: b.u32(c::BUCKET_MILLI),
            bucket_t: b.u32(c::BUCKET_T),
            holding,
            office_terms_used: b.u8(c::OFFICE_TERMS_USED),
            ticket_bell: b.u32(c::TICKET_BELL),
            ticket_sites,
            ticket_next: b.u8(c::TICKET_NEXT),
            citizen_tag: b.u64(c::CITIZEN_TAG),
            last_action_ts: b.i64(c::LAST_ACTION_TS),
            works: b.u64(c::WORKS),
            explores: b.u32(c::EXPLORES),
            arrivals: b.u32(c::ARRIVALS),
            rent_payer: b.key(c::RENT_PAYER),
            ticket_escrow: b.u64(c::TICKET_ESCROW),
            ticket_funder: b.key(c::TICKET_FUNDER),
        })
    }
}

/// Transit record (96 B).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transit {
    pub state: u8,
    pub unit: u8,
    pub faction: u8,
    pub origin_tile: u8,
    pub origin_p: i16,
    pub origin_q: i16,
    pub host_id: u64,
    pub depart_bell: u32,
    pub arrive_bell: u32,
    pub depart_ts: i64,
    pub dep_mass: u32,
    pub march_stamina: u16,
    pub dealt_bps: u16,
    pub troops_after: u32,
    pub stamina_after: u16,
    pub ready_bell_off: u16,
    pub seal_root: [u8; 32],
    pub tip: u64,
    pub flags: u8,
}

impl Transit {
    pub fn decode(r: &[u8]) -> Transit {
        use l::transit as t;
        let b = Bytes(r);
        Transit {
            state: b.u8(t::STATE),
            unit: b.u8(t::UNIT),
            faction: b.u8(t::FACTION),
            origin_tile: b.u8(t::ORIGIN_TILE),
            origin_p: b.i16(t::ORIGIN_P),
            origin_q: b.i16(t::ORIGIN_Q),
            host_id: b.u64(t::HOST_ID),
            depart_bell: b.u32(t::DEPART_BELL),
            arrive_bell: b.u32(t::ARRIVE_BELL),
            depart_ts: b.i64(t::DEPART_TS),
            dep_mass: b.u32(t::DEP_MASS),
            march_stamina: b.u16(t::MARCH_STAMINA),
            dealt_bps: b.u16(t::DEALT_BPS),
            troops_after: b.u32(t::TROOPS_AFTER),
            stamina_after: b.u16(t::STAMINA_AFTER),
            ready_bell_off: b.u16(t::READY_BELL_OFF),
            seal_root: b.arr(t::SEAL_ROOT),
            tip: b.u64(t::TIP),
            flags: b.u8(t::FLAGS),
        }
    }
}

/// Accrual {value, rate, cap, t0, frac}.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Accrual {
    pub value: i64,
    pub rate: i64,
    pub cap: i64,
    pub t0: i64,
    pub frac: i64,
}

/// Queue item {done_at, kind, arg, delta}.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueItem {
    pub done_at: i64,
    pub kind: u8,
    pub arg: u8,
    pub delta: i64,
}

/// Explore record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExploreRec {
    pub bell: u32,
    pub p: i16,
    pub q: i16,
    pub tiles: [u8; 2],
    pub host: u64,
    pub state: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holding {
    pub h: Header,
    pub p: i16,
    pub q: i16,
    pub site: u8,
    pub gen: u8,
    pub tile: u8,
    pub state: u8,
    pub owner_citizen: Address,
    pub ticket_score: u64,
    pub faction: u8,
    pub order: u8,
    pub tier: u8,
    pub flags: u8,
    pub ticket_bell: u32,
    pub founded_ts: i64,
    pub founded_day: u32,
    pub host_seq: u32,
    pub last_owner_action: i64,
    pub shield_until: i64,
    pub stores: [Accrual; 8],
    pub production: [i64; 8],
    pub upkeep: [i64; 8],
    pub queue: [QueueItem; 4],
    pub walls: u32,
    pub walls_committed_before: i64,
    pub food_shortfall: i64,
    pub reserve: [u32; 8],
    pub delegate: Address,
    pub transit: [Transit; 4],
    pub explore: ExploreRec,
    pub escrow: u64,
    pub rent_payer: Address,
    pub final_ts: i64,
    pub pool_owed: u64,
}

impl Holding {
    pub fn decode(d: &[u8]) -> Result<Holding, DecodeError> {
        use l::holding as h;
        let b = check(d, magic::HOLDING, size::HOLDING)?;
        let stores = core::array::from_fn(|i| {
            let o = h::STORES + i * h::ACCRUAL_STRIDE;
            Accrual {
                value: b.i64(o),
                rate: b.i64(o + 8),
                cap: b.i64(o + 16),
                t0: b.i64(o + 24),
                frac: b.i64(o + 32),
            }
        });
        let queue = core::array::from_fn(|i| {
            let o = h::QUEUE + i * h::QUEUE_STRIDE;
            QueueItem {
                done_at: b.i64(o),
                kind: b.u8(o + 8),
                arg: b.u8(o + 9),
                delta: b.i64(o + 16),
            }
        });
        let transit = core::array::from_fn(|i| {
            let o = h::TRANSIT + i * h::TRANSIT_STRIDE;
            Transit::decode(b.slice(o, h::TRANSIT_STRIDE))
        });
        let e = h::EXPLORE;
        Ok(Holding {
            h: header(b),
            p: b.i16(h::P),
            q: b.i16(h::Q),
            site: b.u8(h::SITE),
            gen: b.u8(h::GEN),
            tile: b.u8(h::TILE),
            state: b.u8(h::STATE),
            owner_citizen: b.key(h::OWNER_CITIZEN),
            ticket_score: b.u64(h::TICKET_SCORE),
            faction: b.u8(h::FACTION),
            order: b.u8(h::ORDER),
            tier: b.u8(h::TIER),
            flags: b.u8(h::FLAGS),
            ticket_bell: b.u32(h::TICKET_BELL),
            founded_ts: b.i64(h::FOUNDED_TS),
            founded_day: b.u32(h::FOUNDED_DAY),
            host_seq: b.u32(h::HOST_SEQ),
            last_owner_action: b.i64(h::LAST_OWNER_ACTION),
            shield_until: b.i64(h::SHIELD_UNTIL),
            stores,
            production: core::array::from_fn(|i| b.i64(h::PRODUCTION + 8 * i)),
            upkeep: core::array::from_fn(|i| b.i64(h::UPKEEP + 8 * i)),
            queue,
            walls: b.u32(h::WALLS),
            walls_committed_before: b.i64(h::WALLS_COMMITTED_BEFORE),
            food_shortfall: b.i64(h::FOOD_SHORTFALL),
            reserve: core::array::from_fn(|i| b.u32(h::RESERVE + 4 * i)),
            delegate: b.key(h::DELEGATE),
            transit,
            explore: ExploreRec {
                bell: b.u32(e + l::explore::BELL),
                p: b.i16(e + l::explore::P),
                q: b.i16(e + l::explore::Q),
                tiles: b.arr(e + l::explore::TILES),
                host: b.u64(e + l::explore::HOST),
                state: b.u8(e + l::explore::STATE),
            },
            escrow: b.u64(h::ESCROW),
            rent_payer: b.key(h::RENT_PAYER),
            final_ts: b.i64(h::FINAL_TS),
            pool_owed: b.u64(h::POOL_OWED),
        })
    }

    /// The transit record that carries `host_id` in states 1–3 (I-44).
    pub fn transit_of(&self, host_id: u64) -> Option<(usize, &Transit)> {
        self.transit
            .iter()
            .enumerate()
            .find(|(_, t)| (1..=3).contains(&t.state) && t.host_id == host_id)
    }
}

/// Province entry (48 B).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    pub faction: u8,
    pub unit: u8,
    pub tile: u8,
    pub state: u8,
    pub troops: u32,
    pub stamina_value: u16,
    pub dealt_bps: u16,
    pub stamina_bell: u32,
    pub ready_bell: u32,
    pub from_bell: u32,
    pub pend_bell: u32,
    pub pend_op: u8,
    pub op_a: u8,
    pub op_b: u16,
    pub op_troops: u32,
    pub op_ref: u32,
}

impl Entry {
    pub fn decode(r: &[u8]) -> Entry {
        use l::entry as e;
        let b = Bytes(r);
        Entry {
            id: b.u64(e::ID),
            faction: b.u8(e::FACTION),
            unit: b.u8(e::UNIT),
            tile: b.u8(e::TILE),
            state: b.u8(e::STATE),
            troops: b.u32(e::TROOPS),
            stamina_value: b.u16(e::STAMINA_VALUE),
            dealt_bps: b.u16(e::DEALT_BPS),
            stamina_bell: b.u32(e::STAMINA_BELL),
            ready_bell: b.u32(e::READY_BELL),
            from_bell: b.u32(e::FROM_BELL),
            pend_bell: b.u32(e::PEND_BELL),
            pend_op: b.u8(e::PEND_OP),
            op_a: b.u8(e::OP_A),
            op_b: b.u16(e::OP_B),
            op_troops: b.u32(e::OP_TROOPS),
            op_ref: b.u32(e::OP_REF),
        }
    }
}

/// Site mirror (64 B).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SiteMirror {
    pub state: u8,
    pub faction: u8,
    pub order: u8,
    pub tier: u8,
    pub gen: u8,
    pub garrison: u32,
    pub pend0_bell: u32,
    pub pend0_delta: i64,
    pub pend1_bell: u32,
    pub pend1_delta: i64,
    pub walls_committed: u32,
    pub wall_item0: (u32, u32),
    pub wall_item1: (u32, u32),
    pub shield_until_bell: u32,
}

impl SiteMirror {
    pub fn decode(r: &[u8]) -> SiteMirror {
        use l::site as s;
        let b = Bytes(r);
        SiteMirror {
            state: b.u8(s::STATE),
            faction: b.u8(s::FACTION),
            order: b.u8(s::ORDER),
            tier: b.u8(s::TIER),
            gen: b.u8(s::GEN),
            garrison: b.u32(s::GARRISON),
            pend0_bell: b.u32(s::PEND0_BELL),
            pend0_delta: b.i64(s::PEND0_DELTA),
            pend1_bell: b.u32(s::PEND1_BELL),
            pend1_delta: b.i64(s::PEND1_DELTA),
            walls_committed: b.u32(s::WALLS_COMMITTED),
            wall_item0: (b.u32(s::WALL_ITEM0), b.u32(s::WALL_ITEM0 + 4)),
            wall_item1: (b.u32(s::WALL_ITEM1), b.u32(s::WALL_ITEM1 + 4)),
            shield_until_bell: b.u32(s::SHIELD_UNTIL_BELL),
        }
    }
}

/// Camp {tile, state, troops, next_check_day, gen}.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Camp {
    pub tile: u8,
    pub state: u8,
    pub troops: u32,
    pub next_check_day: u32,
    pub gen: u32,
}

/// Ticket cohort {bell, filed, settled}.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cohort {
    pub bell: u32,
    pub filed: u16,
    pub settled: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Province {
    pub h: Header,
    pub p: i16,
    pub q: i16,
    pub ring: u16,
    pub wedge: u8,
    pub region: u8,
    pub resolved_next: u32,
    pub opened_bell: u32,
    pub relations: u64,
    pub last_outcome_digest: [u8; 32],
    pub n_entries: u8,
    pub n_sites_used: u8,
    pub quiet_ok: u8,
    pub roster_epoch: u32,
    pub terrain: [u8; 61],
    pub resource: [u8; 61],
    pub sites: [u8; 12],
    pub site_count: u8,
    pub passable_mask: u64,
    pub rough_mask: u64,
    pub road_mask: u64,
    pub explored_mask: u64,
    pub site_mirror: [SiteMirror; 12],
    pub entries: Vec<Entry>,
    pub last_resolve: [u8; 32],
    pub camp: Camp,
    pub cohorts: [Cohort; 8],
}

impl Province {
    pub fn decode(d: &[u8]) -> Result<Province, DecodeError> {
        use l::province as p;
        let b = check(d, magic::PROVINCE, size::PROVINCE)?;
        Ok(Province {
            h: header(b),
            p: b.i16(p::P),
            q: b.i16(p::Q),
            ring: b.u16(p::RING),
            wedge: b.u8(p::WEDGE),
            region: b.u8(p::REGION),
            resolved_next: b.u32(p::RESOLVED_NEXT),
            opened_bell: b.u32(p::OPENED_BELL),
            relations: b.u64(p::RELATIONS),
            last_outcome_digest: b.arr(p::LAST_OUTCOME_DIGEST),
            n_entries: b.u8(p::N_ENTRIES),
            n_sites_used: b.u8(p::N_SITES_USED),
            quiet_ok: b.u8(p::QUIET_OK),
            roster_epoch: b.u32(p::ROSTER_EPOCH),
            terrain: b.arr(p::TERRAIN),
            resource: b.arr(p::RESOURCE),
            sites: b.arr(p::SITES),
            site_count: b.u8(p::SITE_COUNT),
            passable_mask: b.u64(p::PASSABLE_MASK),
            rough_mask: b.u64(p::ROUGH_MASK),
            road_mask: b.u64(p::ROAD_MASK),
            explored_mask: b.u64(p::EXPLORED_MASK),
            site_mirror: core::array::from_fn(|i| {
                SiteMirror::decode(b.slice(p::SITE_MIRROR + i * p::SITE_MIRROR_STRIDE, 64))
            }),
            entries: (0..crate::abi::ENTRIES)
                .map(|i| Entry::decode(b.slice(p::ENTRIES + i * p::ENTRY_STRIDE, 48)))
                .collect(),
            last_resolve: b.arr(p::LAST_RESOLVE),
            camp: Camp {
                tile: b.u8(p::CAMP),
                state: b.u8(p::CAMP + 1),
                troops: b.u32(p::CAMP + 4),
                next_check_day: b.u32(p::CAMP + 8),
                gen: b.u32(p::CAMP + 12),
            },
            cohorts: core::array::from_fn(|i| {
                let o = p::TICKET_COHORTS + i * p::COHORT_STRIDE;
                Cohort {
                    bell: b.u32(o),
                    filed: b.u16(o + 4),
                    settled: b.u16(o + 6),
                }
            }),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrivalSlot {
    pub season_id: u64,
    pub p: i16,
    pub q: i16,
    pub bell: u32,
    pub faction: u8,
    pub i: u8,
    pub unit: u8,
    pub stance: u8,
    pub tile: u8,
    pub flags: u8,
    pub retreat_bps: u16,
    pub host_id: u64,
    pub citizen_tag: u64,
    pub dep_mass: u32,
    pub dealt_bps: u16,
    pub beneficiary: Address,
    pub rent_to: Address,
    pub ev_slot: u64,
    pub ev_price: u64,
    pub ev_limit: u32,
    pub ev_loaded: u32,
    pub claimed: u8,
}

impl ArrivalSlot {
    pub fn decode(d: &[u8]) -> Result<ArrivalSlot, DecodeError> {
        use l::arrival_slot as s;
        let b = check(d, magic::ARRIVAL_SLOT, size::ARRIVAL_SLOT)?;
        Ok(ArrivalSlot {
            season_id: b.u64(l::sh::SEASON_ID),
            p: b.i16(s::P),
            q: b.i16(s::Q),
            bell: b.u32(s::BELL),
            faction: b.u8(s::FACTION),
            i: b.u8(s::I),
            unit: b.u8(s::UNIT),
            stance: b.u8(s::STANCE),
            tile: b.u8(s::TILE),
            flags: b.u8(s::FLAGS),
            retreat_bps: b.u16(s::RETREAT_BPS),
            host_id: b.u64(s::HOST_ID),
            citizen_tag: b.u64(s::CITIZEN_TAG),
            dep_mass: b.u32(s::DEP_MASS),
            dealt_bps: b.u16(s::DEALT_BPS),
            beneficiary: b.key(s::BENEFICIARY),
            rent_to: b.key(s::RENT_TO),
            ev_slot: b.u64(s::EV_SLOT),
            ev_price: b.u64(s::EV_PRICE),
            ev_limit: b.u32(s::EV_LIMIT),
            ev_loaded: b.u32(s::EV_LOADED),
            claimed: b.u8(s::CLAIMED),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrivalDay {
    pub season_id: u64,
    pub p: i16,
    pub q: i16,
    pub day: u32,
    pub bits: [u8; 18],
    pub rent_to: Address,
}

impl ArrivalDay {
    pub fn decode(d: &[u8]) -> Result<ArrivalDay, DecodeError> {
        use l::arrival_day as a;
        let b = check(d, magic::ARRIVAL_DAY, size::ARRIVAL_DAY)?;
        Ok(ArrivalDay {
            season_id: b.u64(l::sh::SEASON_ID),
            p: b.i16(a::P),
            q: b.i16(a::Q),
            day: b.u32(a::DAY),
            bits: b.arr(a::BITS),
            rent_to: b.key(a::RENT_TO),
        })
    }
    /// Bit `bell mod 144`: an ArrivalSlot of `(P, Q, bell)` was created.
    pub fn has(&self, bell: u32) -> bool {
        let k = (bell % crate::abi::BELLS_PER_DAY) as usize;
        self.bits[k / 8] & (1 << (k % 8)) != 0
    }
}

/// Arrival record (40 B) inside ClashInputs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArrivalRec {
    pub host_id: u64,
    pub citizen_tag: u64,
    pub dep_mass: u32,
    pub troops: u32,
    pub stamina: u16,
    pub retreat: u16,
    pub dealt: u16,
    pub faction: u8,
    pub unit: u8,
    pub tile: u8,
    pub stance: u8,
    pub present: u8,
    pub fate: u8,
    pub troops_after: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClashInputs {
    pub h: Header,
    pub p: i16,
    pub q: i16,
    pub bell: u32,
    pub arrivals_mask: u32,
    pub posture_mask: u64,
    pub flags: u8,
    pub n_present: u8,
    pub settled_mask: u32,
    pub arrivals: [ArrivalRec; 24],
    pub resolver: Address,
    pub ev_slot: u64,
    pub ev_price: u64,
    pub ev_limit: u32,
    pub resolved_ts: u32,
    pub rent_to: Address,
}

impl ClashInputs {
    pub fn decode(d: &[u8]) -> Result<ClashInputs, DecodeError> {
        use l::clash_inputs as c;
        let b = check(d, magic::CLASH_INPUTS, size::CLASH_INPUTS)?;
        let arrivals = core::array::from_fn(|i| {
            use l::arrival as a;
            let o = c::ARRIVALS + i * c::ARRIVAL_STRIDE;
            ArrivalRec {
                host_id: b.u64(o + a::HOST_ID),
                citizen_tag: b.u64(o + a::CITIZEN_TAG),
                dep_mass: b.u32(o + a::DEP_MASS),
                troops: b.u32(o + a::TROOPS),
                stamina: b.u16(o + a::STAMINA),
                retreat: b.u16(o + a::RETREAT),
                dealt: b.u16(o + a::DEALT),
                faction: b.u8(o + a::FACTION),
                unit: b.u8(o + a::UNIT),
                tile: b.u8(o + a::TILE),
                stance: b.u8(o + a::STANCE),
                present: b.u8(o + a::PRESENT),
                fate: b.u8(o + a::FATE),
                troops_after: b.u32(o + a::TROOPS_AFTER),
            }
        });
        Ok(ClashInputs {
            h: header(b),
            p: b.i16(c::P),
            q: b.i16(c::Q),
            bell: b.u32(c::BELL),
            arrivals_mask: b.u32(c::ARRIVALS_MASK),
            posture_mask: b.u64(c::POSTURE_MASK),
            flags: b.u8(c::FLAGS),
            n_present: b.u8(c::N_PRESENT),
            settled_mask: b.u32(c::SETTLED_MASK),
            arrivals,
            resolver: b.key(c::RESOLVER),
            ev_slot: b.u64(c::EV_SLOT),
            ev_price: b.u64(c::EV_PRICE),
            ev_limit: b.u32(c::EV_LIMIT),
            resolved_ts: b.u32(c::RESOLVED_TS),
            rent_to: b.key(c::RENT_TO),
        })
    }
    pub fn resolved(&self) -> bool {
        self.flags & 2 != 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BellAnchor {
    pub season_id: u64,
    pub bell: u32,
    pub region: u8,
    pub net: u8,
    pub round: u64,
    pub a: i64,
    pub slot: u64,
    pub sig48: [u8; 48],
    pub rent_to: Address,
    pub ev_price: u64,
    pub ev_limit: u32,
}

impl BellAnchor {
    pub fn decode(d: &[u8]) -> Result<BellAnchor, DecodeError> {
        use l::bell_anchor as a;
        let b = check(d, magic::BELL_ANCHOR, size::BELL_ANCHOR)?;
        Ok(BellAnchor {
            season_id: b.u64(l::sh::SEASON_ID),
            bell: b.u32(a::BELL),
            region: b.u8(a::REGION),
            net: b.u8(a::NET),
            round: b.u64(a::ROUND),
            a: b.i64(a::A),
            slot: b.u64(a::SLOT),
            sig48: b.arr(a::SIG48),
            rent_to: b.key(a::RENT_TO),
            ev_price: b.u64(a::EV_PRICE),
            ev_limit: b.u32(a::EV_LIMIT),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeedCache {
    pub season_id: u64,
    pub bell: u32,
    pub region: u8,
    pub nonce: u8,
    pub round: u64,
    pub seed: [u8; 32],
    pub anchor_key: Address,
    pub a: i64,
    pub slot: u64,
    pub rent_to: Address,
}

impl SeedCache {
    pub fn decode(d: &[u8]) -> Result<SeedCache, DecodeError> {
        use l::seed_cache as s;
        let b = check(d, magic::SEED_CACHE, size::SEED_CACHE)?;
        Ok(SeedCache {
            season_id: b.u64(l::sh::SEASON_ID),
            bell: b.u32(s::BELL),
            region: b.u8(s::REGION),
            nonce: b.u8(s::NONCE),
            round: b.u64(s::ROUND),
            seed: b.arr(s::SEED),
            anchor_key: b.key(s::ANCHOR_KEY),
            a: b.i64(s::A),
            slot: b.u64(s::SLOT),
            rent_to: b.key(s::RENT_TO),
        })
    }
}

/// AnchorArchive entry {a_off, seed, sig}.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub a_off: u32,
    pub seed: [u8; 32],
    pub sig: [u8; 48],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnchorArchive {
    pub season_id: u64,
    pub region: u8,
    /// The half day it covers (`bell / 72`, v1.3).
    pub part: u32,
    pub tombstone: [u8; 9],
    pub archived: [u8; 9],
    pub entries: Vec<ArchiveEntry>,
    pub rent_to: Address,
}

impl AnchorArchive {
    pub fn decode(d: &[u8]) -> Result<AnchorArchive, DecodeError> {
        use l::anchor_archive as a;
        let b = check(d, magic::ANCHOR_ARCHIVE, size::ANCHOR_ARCHIVE)?;
        Ok(AnchorArchive {
            season_id: b.u64(l::sh::SEASON_ID),
            region: b.u8(a::REGION),
            part: b.u32(a::PART),
            tombstone: b.arr(a::TOMBSTONE),
            archived: b.arr(a::ARCHIVED),
            entries: (0..a::ENTRIES_N)
                .map(|i| {
                    let o = a::ENTRIES + i * a::ENTRY_STRIDE;
                    ArchiveEntry {
                        a_off: b.u32(o),
                        seed: b.arr(o + 4),
                        sig: b.arr(o + 36),
                    }
                })
                .collect(),
            rent_to: b.key(a::RENT_TO),
        })
    }
    fn bit(bits: &[u8; 9], bell: u32) -> bool {
        let k = bell as usize % l::anchor_archive::ENTRIES_N;
        bits[k / 8] & (1 << (k % 8)) != 0
    }
    pub fn tombstoned(&self, bell: u32) -> bool {
        Self::bit(&self.tombstone, bell)
    }
    pub fn is_archived(&self, bell: u32) -> bool {
        Self::bit(&self.archived, bell)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefenceClaim {
    pub season_id: u64,
    pub beneficiary: Address,
    pub day: u32,
    pub claimed: u64,
    pub count: u32,
}

impl DefenceClaim {
    pub fn decode(d: &[u8]) -> Result<DefenceClaim, DecodeError> {
        use l::defence_claim as c;
        let b = check(d, magic::DEFENCE_CLAIM, size::DEFENCE_CLAIM)?;
        Ok(DefenceClaim {
            season_id: b.u64(l::sh::SEASON_ID),
            beneficiary: b.key(c::BENEFICIARY),
            day: b.u32(c::DAY),
            claimed: b.u64(c::CLAIMED),
            count: b.u32(c::COUNT),
        })
    }
}

/// Any decoded program account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnyAccount {
    Season(Box<Season>),
    Frontier(Frontier),
    RingSeed(RingSeed),
    ProvinceFund(ProvinceFund),
    JoinShard(JoinShard),
    BeaconLog(BeaconLog),
    DefencePool(DefencePool),
    Citizen(Box<Citizen>),
    Holding(Box<Holding>),
    Province(Box<Province>),
    ArrivalSlot(ArrivalSlot),
    ArrivalDay(ArrivalDay),
    ClashInputs(Box<ClashInputs>),
    BellAnchor(BellAnchor),
    SeedCache(SeedCache),
    AnchorArchive(Box<AnchorArchive>),
    DefenceClaim(DefenceClaim),
}

/// Decodes by magic.
pub fn any(d: &[u8]) -> Option<Result<AnyAccount, DecodeError>> {
    let m: [u8; 8] = d.get(..8)?.try_into().ok()?;
    Some(match &m {
        x if x == magic::SEASON => Season::decode(d).map(|v| AnyAccount::Season(Box::new(v))),
        x if x == magic::FRONTIER => Frontier::decode(d).map(AnyAccount::Frontier),
        x if x == magic::RING_SEED => RingSeed::decode(d).map(AnyAccount::RingSeed),
        x if x == magic::PROVINCE_FUND => ProvinceFund::decode(d).map(AnyAccount::ProvinceFund),
        x if x == magic::JOIN_SHARD => JoinShard::decode(d).map(AnyAccount::JoinShard),
        x if x == magic::BEACON_LOG => BeaconLog::decode(d).map(AnyAccount::BeaconLog),
        x if x == magic::DEFENCE_POOL => DefencePool::decode(d).map(AnyAccount::DefencePool),
        x if x == magic::CITIZEN => Citizen::decode(d).map(|v| AnyAccount::Citizen(Box::new(v))),
        x if x == magic::HOLDING => Holding::decode(d).map(|v| AnyAccount::Holding(Box::new(v))),
        x if x == magic::PROVINCE => Province::decode(d).map(|v| AnyAccount::Province(Box::new(v))),
        x if x == magic::ARRIVAL_SLOT => ArrivalSlot::decode(d).map(AnyAccount::ArrivalSlot),
        x if x == magic::ARRIVAL_DAY => ArrivalDay::decode(d).map(AnyAccount::ArrivalDay),
        x if x == magic::CLASH_INPUTS => {
            ClashInputs::decode(d).map(|v| AnyAccount::ClashInputs(Box::new(v)))
        }
        x if x == magic::BELL_ANCHOR => BellAnchor::decode(d).map(AnyAccount::BellAnchor),
        x if x == magic::SEED_CACHE => SeedCache::decode(d).map(AnyAccount::SeedCache),
        x if x == magic::ANCHOR_ARCHIVE => {
            AnchorArchive::decode(d).map(|v| AnyAccount::AnchorArchive(Box::new(v)))
        }
        x if x == magic::DEFENCE_CLAIM => DefenceClaim::decode(d).map(AnyAccount::DefenceClaim),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(d: &mut [u8], o: usize, v: &[u8]) {
        d[o..o + v.len()].copy_from_slice(v);
    }

    #[test]
    fn decodes_a_holding_and_its_transits() {
        let mut d = vec![0u8; size::HOLDING];
        put(&mut d, 0, magic::HOLDING);
        put(&mut d, 8, &7u64.to_le_bytes());
        put(&mut d, l::holding::P, &(-3i16).to_le_bytes());
        put(&mut d, l::holding::STATE, &[2]);
        let t1 = l::holding::TRANSIT + l::holding::TRANSIT_STRIDE;
        put(&mut d, t1 + l::transit::STATE, &[1]);
        put(&mut d, t1 + l::transit::HOST_ID, &42u64.to_le_bytes());
        put(&mut d, t1 + l::transit::TIP, &14_441u64.to_le_bytes());
        put(&mut d, l::holding::POOL_OWED, &9u64.to_le_bytes());
        let h = Holding::decode(&d).unwrap();
        assert_eq!((h.h.season_id, h.p, h.state, h.pool_owed), (7, -3, 2, 9));
        let (i, t) = h.transit_of(42).unwrap();
        assert_eq!((i, t.tip), (1, 14_441));
        assert!(matches!(any(&d), Some(Ok(AnyAccount::Holding(_)))));
    }

    #[test]
    fn refuses_wrong_magic_and_short_data() {
        let mut d = vec![0u8; size::BELL_ANCHOR];
        put(&mut d, 0, magic::SEED_CACHE);
        assert!(matches!(
            BellAnchor::decode(&d),
            Err(DecodeError::Magic { .. })
        ));
        assert!(matches!(
            BellAnchor::decode(&d[..20]),
            Err(DecodeError::Short { .. })
        ));
    }

    #[test]
    fn arrival_day_bits() {
        let mut d = vec![0u8; size::ARRIVAL_DAY];
        put(&mut d, 0, magic::ARRIVAL_DAY);
        d[l::arrival_day::BITS + 1] = 0b0000_0100; // bit 10
        let a = ArrivalDay::decode(&d).unwrap();
        assert!(a.has(144 * 3 + 10));
        assert!(!a.has(11));
    }
}
