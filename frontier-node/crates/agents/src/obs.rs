//! What a bot sees: the herald's files (M1 contract §8.4, §9.2, §9.3), the
//! same read path people use, decoded with `fclient::decode`. The herald is
//! never a trust root: every account arrives as bytes and is decoded here;
//! the herald's own convenience fields are used for pacing only (the clock
//! and quotas), never for anything a bot signs.
//!
//! | view | herald file |
//! |---|---|
//! | [`SeasonView`] | `GET /h/season` |
//! | [`MeView`] | `GET /h/me/{wallet}` |
//! | [`ProvinceView`] | `GET /h/province/{P},{Q}/{bell|latest}` (§9.2 envelope) |
//! | [`Overview`] | `GET /h/overview/{ring}/{bell|latest}.bin` (§9.3) |
//! | [`BellView`] | `GET /h/bell/{bell}/region/{r}` |
//!
//! [`Observation`] gathers them for one decision.

use std::collections::BTreeMap;

use base64::Engine;
use fclient::decode::{
    ArrivalDay, ArrivalSlot, BellAnchor, Citizen, ClashInputs, Holding, Province, Season,
};
use fclient::Address;
use serde_json::Value;

/// Why a herald file does not read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObsError(pub String);

impl std::fmt::Display for ObsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "herald: {}", self.0)
    }
}
impl std::error::Error for ObsError {}

fn bad<T>(m: impl Into<String>) -> Result<T, ObsError> {
    Err(ObsError(m.into()))
}

fn field<'a>(v: &'a Value, k: &str) -> Result<&'a Value, ObsError> {
    v.get(k).ok_or_else(|| ObsError(format!("missing `{k}`")))
}

/// A JSON integer, or a decimal string (u64 fields travel as strings).
fn int(v: &Value, k: &str) -> Result<i128, ObsError> {
    let x = field(v, k)?;
    if let Some(n) = x.as_i64() {
        return Ok(n as i128);
    }
    if let Some(n) = x.as_u64() {
        return Ok(n as i128);
    }
    if let Some(s) = x.as_str() {
        return s
            .parse::<i128>()
            .map_err(|_| ObsError(format!("`{k}` is not an integer")));
    }
    bad(format!("`{k}` is not an integer"))
}

fn opt_int(v: &Value, k: &str) -> Option<i128> {
    v.get(k).and_then(|_| int(v, k).ok())
}

fn string<'a>(v: &'a Value, k: &str) -> Result<&'a str, ObsError> {
    field(v, k)?
        .as_str()
        .ok_or_else(|| ObsError(format!("`{k}` is not a string")))
}

pub fn b64(s: &str) -> Result<Vec<u8>, ObsError> {
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|_| ObsError("bad base64".into()))
}

pub fn b64_encode(b: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(b)
}

fn hex32(s: &str) -> Result<[u8; 32], ObsError> {
    let v = hex::decode(s).map_err(|_| ObsError("bad hex".into()))?;
    v.try_into()
        .map_err(|_| ObsError("want 32 bytes of hex".into()))
}

pub fn key(s: &str) -> Result<Address, ObsError> {
    s.parse::<Address>()
        .map_err(|_| ObsError(format!("bad base58 key {s:?}")))
}

fn dec<T, E: std::fmt::Debug>(what: &str, r: Result<T, E>) -> Result<T, ObsError> {
    r.map_err(|e| ObsError(format!("{what}: {e:?}")))
}

// ------------------------------------------------------------------ season

/// `GET /h/season`.
#[derive(Clone, Debug)]
pub struct SeasonView {
    pub program: Address,
    pub season_id: u64,
    pub genesis_ts: i64,
    pub bell_secs: u32,
    pub w: u32,
    pub delta: u32,
    pub drand_pk: [u8; 96],
    pub drand_period: u32,
    pub drand_genesis: i64,
    pub r_max: u16,
    pub rings: Vec<(u16, [u8; 32])>,
    pub tip_priority_milli: u32,
    pub reveal_cu_limit: u32,
    pub march_fee: u64,
    pub seal_bond: u64,
    pub latest_slot: u64,
    pub latest_unix: i64,
    /// The Season account, decoded (absent in a thin file).
    pub season: Option<Season>,
}

impl SeasonView {
    pub fn from_json(v: &Value) -> Result<SeasonView, ObsError> {
        if int(v, "v")? != 1 {
            return bad("season: v != 1");
        }
        let d = field(v, "drand")?;
        let pk = hex::decode(string(d, "publicKey")?).map_err(|_| ObsError("drand pk".into()))?;
        let drand_pk: [u8; 96] = pk
            .try_into()
            .map_err(|_| ObsError("drand publicKey is not 96 bytes (G2)".into()))?;
        let season = match v.get("bytes_b64").and_then(|x| x.as_str()) {
            Some(s) => Some(dec("season bytes", Season::decode(&b64(s)?))?),
            None => None,
        };
        let season_id = int(v, "season")? as u64;
        if let Some(s) = &season {
            if s.h.season_id != season_id {
                return bad("season bytes are another season's");
            }
        }
        let mut rings = vec![];
        if let Some(rs) = v.get("rings").and_then(|x| x.as_array()) {
            for r in rs {
                rings.push((int(r, "d")? as u16, hex32(string(r, "seed")?)?));
            }
        }
        Ok(SeasonView {
            program: key(string(v, "programId")?)?,
            season_id,
            genesis_ts: int(v, "genesisTs")? as i64,
            bell_secs: int(v, "bellSecs")? as u32,
            w: int(v, "W")? as u32,
            delta: opt_int(v, "delta").unwrap_or(0) as u32,
            drand_pk,
            drand_period: int(d, "period")? as u32,
            drand_genesis: int(d, "genesis")? as i64,
            r_max: opt_int(v, "rMax").unwrap_or(0) as u16,
            rings,
            tip_priority_milli: opt_int(v, "tipPriorityMilli").unwrap_or(0) as u32,
            reveal_cu_limit: opt_int(v, "revealCuLimit").unwrap_or(0) as u32,
            march_fee: opt_int(v, "marchFee").unwrap_or(0) as u64,
            seal_bond: opt_int(v, "sealBond").unwrap_or(0) as u64,
            latest_slot: opt_int(v, "latestSlot").unwrap_or(0) as u64,
            latest_unix: int(v, "latestUnix")? as i64,
            season,
        })
    }

    /// The bell of game time `t` (before genesis: 0).
    pub fn bell_at(&self, t: i64) -> u32 {
        fclient::clock::bell_at(self.genesis_ts, t).unwrap_or(0)
    }

    /// `tlock_round(b)`: the round a march arriving at `b` is sealed to.
    pub fn tlock_round(&self, b: u32) -> u64 {
        fclient::clock::Drand {
            genesis: self.drand_genesis,
            period: self.drand_period,
        }
        .tlock_round(self.genesis_ts, b)
    }

    /// The Depart minimum tip (§5.11 check 5) from the Season account;
    /// without it, from the herald's `tipPriorityMilli` and `revealCuLimit`.
    pub fn tip_min(&self, reveal_loaded_limit: u32) -> u64 {
        let (p, cu, l) = match &self.season {
            Some(s) => (
                s.min_reveal_priority_milli,
                s.reveal_cu_limit,
                if s.reveal_loaded_limit > 0 {
                    s.reveal_loaded_limit
                } else {
                    reveal_loaded_limit
                },
            ),
            None => (
                self.tip_priority_milli,
                self.reveal_cu_limit,
                reveal_loaded_limit,
            ),
        };
        fclient::fees::min_tip_lamports(p, cu, l)
    }

    /// The three tip presets the relay sponsors (§8.3): `tip_min`,
    /// `⌈1.5 × tip_min⌉`, `2 × tip_min`.
    pub fn tip_presets(&self, reveal_loaded_limit: u32) -> [u64; 3] {
        let t = self.tip_min(reveal_loaded_limit);
        [t, (3 * t).div_ceil(2), 2 * t]
    }

    /// Rings opened so far (from the Season's view of the herald: the
    /// `rings` list; the Frontier account is not in `/h/season`).
    pub fn rings_opened(&self) -> u16 {
        self.rings.iter().map(|r| r.0 + 1).max().unwrap_or(0)
    }

    pub fn join_close_bell(&self) -> u32 {
        self.season
            .as_ref()
            .map(|s| s.join_close_bell)
            .unwrap_or(u32::MAX)
    }

    pub fn end_bell(&self) -> u32 {
        self.season.as_ref().map(|s| s.end_bell).unwrap_or(u32::MAX)
    }
}

// ------------------------------------------------------------------ me

/// `GET /h/me/{wallet}`: the Citizen and its (≤ 3) Holdings.
#[derive(Clone, Debug)]
pub struct MeView {
    pub wallet: Address,
    pub citizen: Option<(Address, Citizen)>,
    pub holdings: Vec<(Address, Holding)>,
    /// The relay quota the herald reports (pacing only).
    pub quota_left: Option<u32>,
}

impl MeView {
    pub fn from_json(v: &Value) -> Result<MeView, ObsError> {
        if int(v, "v")? != 1 {
            return bad("me: v != 1");
        }
        let wallet = key(string(v, "wallet")?)?;
        let citizen = match v.get("citizen") {
            Some(c) if !c.is_null() => {
                let a = key(string(c, "address")?)?;
                let bytes = b64(string(c, "bytes_b64")?)?;
                let cz = dec("citizen", Citizen::decode(&bytes))?;
                if cz.wallet != wallet {
                    return bad("me: the Citizen is another wallet's");
                }
                Some((a, cz))
            }
            _ => None,
        };
        let mut holdings = vec![];
        if let Some(hs) = v.get("holdings").and_then(|x| x.as_array()) {
            for h in hs {
                // The herald lists a holding the Citizen names but the fold
                // has not captured (or closed) as null: skip it (wave-3
                // review, W3-E), never fail the whole view.
                if h.is_null() {
                    continue;
                }
                let a = key(string(h, "address")?)?;
                let bytes = b64(string(h, "bytes_b64")?)?;
                holdings.push((a, dec("holding", Holding::decode(&bytes))?));
            }
        }
        let quota_left = v
            .get("quota")
            .and_then(|q| opt_int(q, "left"))
            .map(|x| x.clamp(0, u32::MAX as i128) as u32);
        Ok(MeView {
            wallet,
            citizen,
            holdings,
            quota_left,
        })
    }

    /// An empty view (a wallet the herald has not seen: not joined).
    pub fn empty(wallet: Address) -> MeView {
        MeView {
            wallet,
            citizen: None,
            holdings: vec![],
            quota_left: None,
        }
    }
}

// ------------------------------------------------------------------ province

/// `GET /h/province/{P},{Q}/{bell|latest}` (§9.2).
#[derive(Clone, Debug)]
pub struct ProvinceView {
    pub bell: u32,
    pub slot: u64,
    pub province: Province,
    pub slots: Vec<ArrivalSlot>,
    pub day: Option<ArrivalDay>,
    pub inputs: Option<ClashInputs>,
}

impl ProvinceView {
    pub fn from_json(v: &Value) -> Result<ProvinceView, ObsError> {
        if int(v, "v")? != 1 {
            return bad("province: v != 1");
        }
        let province = dec("province", Province::decode(&b64(string(v, "bytes")?)?))?;
        let want = format!("pv:{},{}", province.p, province.q);
        if string(v, "key")? != want {
            return bad(format!("province key is not {want}"));
        }
        let mut slots = vec![];
        if let Some(ss) = v.get("slots").and_then(|x| x.as_array()) {
            for s in ss {
                slots.push(dec(
                    "slot",
                    ArrivalSlot::decode(&b64(string(s, "bytes")?)?),
                )?);
            }
        }
        let day = match v.get("day") {
            Some(d) if !d.is_null() => Some(dec(
                "arrival day",
                ArrivalDay::decode(&b64(string(d, "bytes")?)?),
            )?),
            _ => None,
        };
        let inputs = match v.get("inputs") {
            Some(d) if !d.is_null() => Some(dec(
                "inputs",
                ClashInputs::decode(&b64(string(d, "bytes")?)?),
            )?),
            _ => None,
        };
        Ok(ProvinceView {
            bell: int(v, "bell")? as u32,
            slot: opt_int(v, "slot").unwrap_or(0) as u64,
            province,
            slots,
            day,
            inputs,
        })
    }

    pub fn coord(&self) -> (i16, i16) {
        (self.province.p, self.province.q)
    }
}

// ------------------------------------------------------------------ overview

pub const OVERVIEW_MAGIC: &[u8; 8] = b"PSFOV1\0\0";
pub const OVERVIEW_HEADER: usize = 32;
pub const OVERVIEW_RECORD: usize = 24;

/// Site states of the overview: free, holding, camp, reserved/released.
pub mod site_state {
    pub const FREE: u8 = 0;
    pub const HOLDING: u8 = 1;
    pub const CAMP: u8 = 2;
    pub const RESERVED: u8 = 3;
}

/// One province of the overview (§9.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverviewRec {
    pub p: i16,
    pub q: i16,
    /// Per site: 0–5 faction, 6 neutral/camp, 7 none.
    pub owners: [u8; 12],
    pub sites: [u8; 12],
    pub hosts: [u8; 7],
    pub clash: bool,
    pub dormant: bool,
    pub opened: bool,
    pub resolved_next: u32,
}

impl OverviewRec {
    /// Sites that are free (a ticket may name them).
    pub fn free_sites(&self, site_count: u8) -> impl Iterator<Item = u8> + '_ {
        (0..site_count.min(12)).filter(|&s| self.sites[s as usize] == site_state::FREE)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overview {
    pub season: u64,
    pub ring: u16,
    pub bell: u32,
    pub slot: u64,
    pub provinces: Vec<OverviewRec>,
}

impl Overview {
    pub fn decode(b: &[u8]) -> Result<Overview, ObsError> {
        if b.len() < OVERVIEW_HEADER || &b[..8] != OVERVIEW_MAGIC {
            return bad("overview: magic");
        }
        let u16_at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
        let n = u16_at(18) as usize;
        if b.len() != OVERVIEW_HEADER + n * OVERVIEW_RECORD {
            return bad("overview: length");
        }
        let mut provinces: Vec<OverviewRec> = Vec::with_capacity(n);
        for i in 0..n {
            let o = OVERVIEW_HEADER + i * OVERVIEW_RECORD;
            let mut owners40 = 0u64;
            for j in (0..5).rev() {
                owners40 = (owners40 << 8) | b[o + 4 + j] as u64;
            }
            let sites24 = b[o + 9] as u32 | (b[o + 10] as u32) << 8 | (b[o + 11] as u32) << 16;
            let flags = b[o + 19];
            let rec = OverviewRec {
                p: i16::from_le_bytes([b[o], b[o + 1]]),
                q: i16::from_le_bytes([b[o + 2], b[o + 3]]),
                owners: core::array::from_fn(|s| ((owners40 >> (3 * s)) & 7) as u8),
                sites: core::array::from_fn(|s| ((sites24 >> (2 * s)) & 3) as u8),
                hosts: b[o + 12..o + 19].try_into().expect("7"),
                clash: flags & 1 != 0,
                dormant: flags & 2 != 0,
                opened: flags & 4 != 0,
                resolved_next: u32::from_le_bytes(b[o + 20..o + 24].try_into().expect("4")),
            };
            if let Some(prev) = provinces.last() {
                if (prev.p, prev.q) >= (rec.p, rec.q) {
                    return bad("overview: records not sorted by (P, Q)");
                }
            }
            provinces.push(rec);
        }
        Ok(Overview {
            season: u64::from_le_bytes(b[8..16].try_into().expect("8")),
            ring: u16_at(16),
            bell: u32::from_le_bytes(b[20..24].try_into().expect("4")),
            slot: u64::from_le_bytes(b[24..32].try_into().expect("8")),
            provinces,
        })
    }

    /// The inverse of [`Overview::decode`] (fixtures, the herald twin test).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(OVERVIEW_HEADER + self.provinces.len() * OVERVIEW_RECORD);
        out.extend_from_slice(OVERVIEW_MAGIC);
        out.extend_from_slice(&self.season.to_le_bytes());
        out.extend_from_slice(&self.ring.to_le_bytes());
        out.extend_from_slice(&(self.provinces.len() as u16).to_le_bytes());
        out.extend_from_slice(&self.bell.to_le_bytes());
        out.extend_from_slice(&self.slot.to_le_bytes());
        for r in &self.provinces {
            out.extend_from_slice(&r.p.to_le_bytes());
            out.extend_from_slice(&r.q.to_le_bytes());
            let mut owners40 = 0u64;
            for (s, &f) in r.owners.iter().enumerate() {
                owners40 |= ((f & 7) as u64) << (3 * s);
            }
            out.extend_from_slice(&owners40.to_le_bytes()[..5]);
            let mut sites24 = 0u32;
            for (s, &st) in r.sites.iter().enumerate() {
                sites24 |= ((st & 3) as u32) << (2 * s);
            }
            out.extend_from_slice(&sites24.to_le_bytes()[..3]);
            out.extend_from_slice(&r.hosts);
            out.push(r.clash as u8 | (r.dormant as u8) << 1 | (r.opened as u8) << 2);
            out.extend_from_slice(&r.resolved_next.to_le_bytes());
        }
        out
    }
}

// ------------------------------------------------------------------ bell

/// A seed cache as `/h/bell` lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheRef {
    pub nonce: u8,
    pub round: u64,
    pub seed: [u8; 32],
    /// The cache account is still on chain (the herald keeps closed
    /// caches listed with `present: false`); absent in older files = true.
    pub present: bool,
}

/// `GET /h/bell/{bell}/region/{r}`: THE anchor, `S`, the seed caches and
/// the archive state.
#[derive(Clone, Debug)]
pub struct BellView {
    pub bell: u32,
    pub region: u8,
    pub anchor: Option<BellAnchor>,
    pub s: Option<u64>,
    pub caches: Vec<CacheRef>,
    pub tombstoned: bool,
    pub archived: bool,
}

impl BellView {
    pub fn from_json(v: &Value) -> Result<BellView, ObsError> {
        if int(v, "v")? != 1 {
            return bad("bell: v != 1");
        }
        let anchor = match v.get("anchor") {
            Some(a) if !a.is_null() => Some(dec(
                "anchor",
                BellAnchor::decode(&b64(string(a, "bytes_b64")?)?),
            )?),
            _ => None,
        };
        let mut caches = vec![];
        if let Some(cs) = v.get("caches").and_then(|x| x.as_array()) {
            for c in cs {
                caches.push(CacheRef {
                    nonce: int(c, "nonce")? as u8,
                    round: int(c, "round")? as u64,
                    seed: hex32(string(c, "seed")?)?,
                    present: c.get("present").and_then(|x| x.as_bool()).unwrap_or(true),
                });
            }
        }
        Ok(BellView {
            bell: int(v, "bell")? as u32,
            region: int(v, "region")? as u8,
            anchor,
            s: opt_int(v, "S").map(|x| x as u64),
            caches,
            tombstoned: v
                .get("tombstoned")
                .and_then(|x| x.as_bool())
                .unwrap_or(false),
            archived: v.get("archived").and_then(|x| x.as_bool()).unwrap_or(false),
        })
    }

    /// The seed source a settle names: a cache of THE anchor, else the
    /// archive once archived, else not ready.
    pub fn seed_source(&self) -> Option<fclient::ix::SeedSource> {
        // Archived: the archive entry (caches close after the archive;
        // wave-3 review, W3-E).
        if self.archived && self.s.is_some() {
            return Some(fclient::ix::SeedSource::Archive);
        }
        // A cache still present whose round is THE anchor's S.
        if self.anchor.is_some() {
            if let Some(c) = self
                .caches
                .iter()
                .find(|c| c.present && self.s.is_none_or(|s| c.round == s))
            {
                return Some(fclient::ix::SeedSource::Cache { nonce: c.nonce });
            }
        }
        None
    }

    /// The reveal close `A + W` (None while no anchor).
    pub fn close(&self, w: u32) -> Option<i64> {
        self.anchor
            .as_ref()
            .map(|a| fclient::clock::reveal_close(a.a, w))
    }
}

// ------------------------------------------------------------------ observation

/// Everything one decision reads. Built by the runner from herald files at
/// game time `now` (the herald's `latestUnix` extrapolated by the game
/// clock); the policy is a pure function of it (and the bot's seed and
/// memory).
#[derive(Clone, Debug)]
pub struct Observation {
    pub now: i64,
    pub season: SeasonView,
    pub me: MeView,
    pub provinces: BTreeMap<(i16, i16), ProvinceView>,
    /// Immutable per-bell envelopes `/h/province/{P},{Q}/{bell}` of the
    /// arrival bells of due marches (wave-3 review, W3-E: SettleTransit's
    /// slot and resolver are the arrival bell's, not the latest bell's).
    pub province_bells: BTreeMap<((i16, i16), u32), ProvinceView>,
    pub overviews: Vec<Overview>,
    pub bells: BTreeMap<(u32, u8), BellView>,
}

impl Observation {
    pub fn bell(&self) -> u32 {
        self.season.bell_at(self.now)
    }

    pub fn province(&self, p: i16, q: i16) -> Option<&Province> {
        self.provinces.get(&(p, q)).map(|v| &v.province)
    }

    /// The envelope of `(P, Q)` as of `bell`: the per-bell file, or the
    /// latest envelope when it is that bell's.
    pub fn province_at(&self, pq: (i16, i16), bell: u32) -> Option<&ProvinceView> {
        self.province_bells
            .get(&(pq, bell))
            .or_else(|| self.provinces.get(&pq).filter(|v| v.bell == bell))
    }

    /// Every overview record, newest ring file first per ring.
    pub fn overview_recs(&self) -> impl Iterator<Item = &OverviewRec> {
        self.overviews.iter().flat_map(|o| o.provinces.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bell_view(archived: bool, present: bool) -> BellView {
        BellView {
            bell: 3,
            region: 1,
            anchor: Some({
                let mut d = vec![0u8; fclient::abi::size::BELL_ANCHOR];
                d[..8].copy_from_slice(fclient::abi::magic::BELL_ANCHOR);
                BellAnchor::decode(&d).unwrap()
            }),
            s: Some(77),
            caches: vec![
                CacheRef {
                    nonce: 4,
                    round: 70,
                    seed: [0; 32],
                    present: true,
                },
                CacheRef {
                    nonce: 9,
                    round: 77,
                    seed: [0; 32],
                    present,
                },
            ],
            tombstoned: false,
            archived,
        }
    }

    /// Wave-3 review (W3-E): a present cache of round S; the archive once
    /// archived; a closed cache never.
    #[test]
    fn seed_source_is_a_present_cache_of_s_or_the_archive() {
        use fclient::ix::SeedSource;
        assert_eq!(
            bell_view(false, true).seed_source(),
            Some(SeedSource::Cache { nonce: 9 })
        );
        assert_eq!(bell_view(false, false).seed_source(), None);
        assert_eq!(
            bell_view(true, true).seed_source(),
            Some(SeedSource::Archive)
        );
    }

    #[test]
    fn a_null_holding_is_skipped() {
        let w = Address::new_from_array([3; 32]);
        let v = serde_json::json!({"v": 1, "wallet": w.to_string(), "citizen": null,
            "holdings": [null], "quota": null});
        let m = MeView::from_json(&v).unwrap();
        assert!(m.holdings.is_empty());
    }
}
