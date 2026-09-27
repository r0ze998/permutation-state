//! Instruction data (M1 contract §5.5–§5.12): byte 0 is the tag, then the
//! listed fields, little-endian, fixed width, no borsh (the one exception
//! is CreateSeason's trailing `PayoutParams`, borsh by contract).
//!
//! Every fixed-size instruction is a struct with `LEN` (tag included),
//! `encode`, `to_bytes` and `decode`; `decode` refuses a wrong tag or a
//! wrong length with `BadData`. Troop counts in instruction data
//! (`Train.n`, `Muster.troops`, `Garrison.delta`) are **whole troops** (the
//! unit of `Holding.reserve`); the program converts to the kernel's
//! `MilliTroops` (× 1,000) where it writes entries and garrisons.

use crate::bytes::{Cursor, Writer};
use crate::error::FrontierError;
use crate::tags::Ix;

/// Compressed quicknet signature (G1).
pub const SIG48_LEN: usize = 48;
/// One SSWU hint (SP-V2 `quick::HINT_LEN`: branch u8, 1/tv1, y', 1/den,
/// 48-B big-endian field elements).
pub const HINT_LEN: usize = 1 + 3 * 48;
/// Both hints, `hint(u0) ‖ hint(u1)`.
pub const HINTS_LEN: usize = 2 * HINT_LEN;
/// Sealed march (tlock envelope: 96-B compressed G2 `U`, 32-B `V`, 37-B `W`).
pub const SEAL_LEN: usize = 165;
/// March plaintext (`seal::pack`).
pub const PLAIN_LEN: usize = 37;
/// Most sites on a FileTicket.
pub const MAX_TICKET_SITES: usize = 3;
/// Most bells per ArchiveAnchors.
pub const MAX_ARCHIVE_BELLS: usize = 8;
/// Largest `SeasonParams` + `PayoutParams` in CreateSeason (§5.7).
pub const MAX_PAYOUT_PARAMS: usize = 128;

/// Fixed-width wire values.
pub trait Wire: Sized {
    const N: usize;
    fn put(&self, w: &mut Writer<'_>);
    fn get(c: &mut Cursor<'_>) -> Option<Self>;
}

macro_rules! wire_int {
    ($($t:ident),*) => {$(
        impl Wire for $t {
            const N: usize = core::mem::size_of::<$t>();
            fn put(&self, w: &mut Writer<'_>) { w.$t(*self); }
            fn get(c: &mut Cursor<'_>) -> Option<Self> { c.$t() }
        }
    )*};
}
wire_int!(u8, u16, u32, u64, i16, i32, i64);

impl<const M: usize> Wire for [u8; M] {
    const N: usize = M;
    fn put(&self, w: &mut Writer<'_>) {
        w.bytes(self);
    }
    fn get(c: &mut Cursor<'_>) -> Option<Self> {
        c.arr()
    }
}

/// The tag of instruction data (`BadData` for an unknown or reserved tag).
pub fn tag_of(data: &[u8]) -> Result<Ix, FrontierError> {
    data.first()
        .and_then(|t| Ix::from_tag(*t))
        .ok_or(FrontierError::BadData)
}

macro_rules! ix_data {
    ($( $(#[$m:meta])* $name:ident { $( $(#[$fm:meta])* $f:ident : $t:ty ),* $(,)? } )*) => {$(
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name { $( $(#[$fm])* pub $f: $t, )* }

        impl $name {
            pub const IX: Ix = Ix::$name;
            /// Data length with the tag.
            pub const LEN: usize = 1 $( + <$t as Wire>::N )*;
            /// Field names and widths, for the vector writer.
            pub const WIRE: &'static [(&'static str, usize)] = &[ $( (stringify!($f), <$t as Wire>::N), )* ];

            #[allow(unused_mut)]
            pub fn encode(&self, out: &mut [u8]) -> Option<usize> {
                let mut w = Writer::new(out);
                w.u8(Self::IX.tag());
                $( Wire::put(&self.$f, &mut w); )*
                w.finish()
            }

            pub fn to_bytes(&self) -> [u8; Self::LEN] {
                let mut b = [0u8; Self::LEN];
                // `b` has exactly `LEN` bytes, so encoding cannot run out.
                let _ = self.encode(&mut b);
                b
            }

            #[allow(unused_mut, unused_variables)]
            pub fn decode(d: &[u8]) -> Result<Self, FrontierError> {
                if d.len() != Self::LEN || d[0] != Self::IX.tag() {
                    return Err(FrontierError::BadData);
                }
                let mut c = Cursor::new(&d[1..]);
                Ok($name { $( $f: <$t as Wire>::get(&mut c).ok_or(FrontierError::BadData)?, )* })
            }
        }
    )*};
}

ix_data! {
    /// 0x08 (§5.7): only the program's upgrade authority (I-51).
    AnnounceSeason { id: u64, params_hash: [u8; 32], t_create_min: i64, bond: u64 }
    /// 0x09.
    InitBeaconLogs {}
    /// 0x02: `faction ≤ 5`.
    InitShards { faction: u8 }
    /// 0x03.
    ConsumeGenesisSeed { round: u64, sig48: [u8; 48], hints: [u8; HINTS_LEN] }
    /// 0x04.
    EndSeason {}
    /// 0x05 (repeatable in parts).
    CloseSeason { part: u8 }
    /// 0x06.
    AbortSeason {}
    /// 0x07: `600 ≤ window ≤ 1,800`, `from_bell ≥ now_bell + 144`.
    SetWindowSchedule { window: u32, from_bell: u32 }
    /// 0x10.
    PostAnchor { region: u8, bell: u32, round: u64, sig48: [u8; 48], hints: [u8; HINTS_LEN], beneficiary: [u8; 32] }
    /// 0x11: `mask` names the regions whose anchors follow, in region order.
    PostAnchorMulti { bell: u32, round: u64, sig48: [u8; 48], hints: [u8; HINTS_LEN], mask: u16, beneficiary: [u8; 32] }
    /// 0x12.
    PostSeed { region: u8, bell: u32, nonce: u8, round: u64, sig48: [u8; 48], hints: [u8; HINTS_LEN], beneficiary: [u8; 32] }
    /// 0x13.
    PostBeacon { region: u8, round: u64, sig48: [u8; 48], hints: [u8; HINTS_LEN] }
    /// 0x15.
    CloseSeedCache { bell: u32, region: u8, nonce: u8 }
    /// 0x20.
    OpenRing { d: u16 }
    /// 0x21.
    ConsumeRingSeed { d: u16, round: u64, sig48: [u8; 48], hints: [u8; HINTS_LEN] }
    /// 0x22.
    OpenProvince { p: i16, q: i16 }
    /// 0x23.
    FoldOccupancy { part: u8 }
    /// 0x24.
    CloseProvince { p: i16, q: i16 }
    /// 0x30: the session pubkey is data, not a signer (I-40).
    Join { faction: u8, session: [u8; 32], session_expiry: i64 }
    /// 0x31 (actor = wallet only).
    SetSession { session: [u8; 32], expiry: i64 }
    /// 0x32.
    SetVigil { start_min: u16 }
    /// 0x34.
    SettleTicket { k: u8 }
    /// 0x35.
    ReleaseDormant {}
    /// 0x36.
    CloseHolding {}
    /// 0x37.
    CloseCitizen {}
    /// 0x40.
    Harvest {}
    /// 0x41.
    Build { item: u8 }
    /// 0x42: `n` whole troops, trained at once (I-56).
    Train { unit: u8, n: u32 }
    /// 0x43: `troops` whole troops from the reserve.
    Muster { unit: u8, troops: u32, tile: u8 }
    /// 0x44.
    Dissolve { host_id: u64 }
    /// 0x45: `delta` whole troops (positive: reserve → garrison).
    Garrison { delta: i64 }
    /// 0x46: `n` ∈ {1, 2}; `tiles[1]` is ignored (and must be 0xFF) when `n == 1`.
    Explore { host_id: u64, n: u8, tiles: [u8; 2] }
    /// 0x47.
    SettleExplore {}
    /// 0x48.
    DisbandStranded { entry: u8 }
    /// 0x50 (219 B with the tag).
    Depart { host_id: u64, commit: [u8; 32], seal: [u8; SEAL_LEN], arrive_bell: u32, tip: u64, transit_slot: u8 }
    /// 0x51 (136 B with the tag).
    Reveal { transit_slot: u8, target_i: u8, plain: [u8; PLAIN_LEN], salt: [u8; 32], ct_hash: [u8; 32], beneficiary: [u8; 32] }
    /// 0x52.
    SettleDeparture { transit_slot: u8 }
    /// 0x54 (231 B with the tag).
    SettleTransit { transit_slot: u8, commit: [u8; 32], seal: [u8; SEAL_LEN], beneficiary: [u8; 32] }
    /// 0x55.
    SweepPoolOwed {}
    /// 0x60.
    GatherClash { bell: u32, start: u8, n: u8, holdings_bitmap: u32, beneficiary: [u8; 32] }
    /// 0x61.
    ResolveFromInputs { bell: u32, beneficiary: [u8; 32] }
    /// 0x62 (feature `oracle`, tests only).
    ResolveClash { bell: u32, beneficiary: [u8; 32] }
    /// 0x63: `b0 == resolved_next`, `1 ≤ n ≤ 24`.
    SkipQuiet { b0: u32, n: u8 }
    /// 0x64.
    CloseClashInputs { p: i16, q: i16, bell: u32 }
    /// 0x65.
    CloseArrivalDay { p: i16, q: i16, day: u32 }
    /// 0x66.
    CloseArrivalSlot { p: i16, q: i16, bell: u32, faction: u8, i: u8 }
    /// 0x70.
    ClaimDefence { day: u32, n: u8 }
}

/// One FileTicket site `{P i16, Q i16, site u8}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TicketSite {
    pub p: i16,
    pub q: i16,
    pub site: u8,
}

/// 0x33 FileTicket: `n u8 (1–3), n × {P i16, Q i16, site u8}` (7–17 B).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileTicket {
    pub n: u8,
    pub sites: [TicketSite; MAX_TICKET_SITES],
}

impl FileTicket {
    pub const IX: Ix = Ix::FileTicket;
    pub const MAX_LEN: usize = 2 + 5 * MAX_TICKET_SITES;

    pub const fn data_len(&self) -> usize {
        2 + 5 * self.n as usize
    }

    pub fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if self.n == 0 || self.n as usize > MAX_TICKET_SITES {
            return None;
        }
        let mut w = Writer::new(out);
        w.u8(Self::IX.tag()).u8(self.n);
        for s in &self.sites[..self.n as usize] {
            w.i16(s.p).i16(s.q).u8(s.site);
        }
        w.finish()
    }

    pub fn decode(d: &[u8]) -> Result<Self, FrontierError> {
        let bad = FrontierError::BadData;
        if d.first() != Some(&Self::IX.tag()) {
            return Err(bad);
        }
        let mut c = Cursor::new(&d[1..]);
        let n = c.u8().ok_or(bad)?;
        if n == 0 || n as usize > MAX_TICKET_SITES || d.len() != 2 + 5 * n as usize {
            return Err(bad);
        }
        let mut sites = [TicketSite::default(); MAX_TICKET_SITES];
        for s in sites.iter_mut().take(n as usize) {
            *s = TicketSite {
                p: c.i16().ok_or(bad)?,
                q: c.i16().ok_or(bad)?,
                site: c.u8().ok_or(bad)?,
            };
        }
        Ok(FileTicket { n, sites })
    }
}

/// 0x14 ArchiveAnchors: `region u8, day u32, n u8 (1–8), bells [n] u32`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveAnchors {
    pub region: u8,
    pub day: u32,
    pub n: u8,
    pub bells: [u32; MAX_ARCHIVE_BELLS],
}

impl ArchiveAnchors {
    pub const IX: Ix = Ix::ArchiveAnchors;
    pub const MAX_LEN: usize = 7 + 4 * MAX_ARCHIVE_BELLS;

    pub const fn data_len(&self) -> usize {
        7 + 4 * self.n as usize
    }

    pub fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if self.n == 0 || self.n as usize > MAX_ARCHIVE_BELLS {
            return None;
        }
        let mut w = Writer::new(out);
        w.u8(Self::IX.tag())
            .u8(self.region)
            .u32(self.day)
            .u8(self.n);
        for b in &self.bells[..self.n as usize] {
            w.u32(*b);
        }
        w.finish()
    }

    pub fn decode(d: &[u8]) -> Result<Self, FrontierError> {
        let bad = FrontierError::BadData;
        if d.first() != Some(&Self::IX.tag()) {
            return Err(bad);
        }
        let mut c = Cursor::new(&d[1..]);
        let region = c.u8().ok_or(bad)?;
        let day = c.u32().ok_or(bad)?;
        let n = c.u8().ok_or(bad)?;
        if n == 0 || n as usize > MAX_ARCHIVE_BELLS || d.len() != 7 + 4 * n as usize {
            return Err(bad);
        }
        let mut bells = [0u32; MAX_ARCHIVE_BELLS];
        for b in bells.iter_mut().take(n as usize) {
            *b = c.u32().ok_or(bad)?;
        }
        Ok(ArchiveAnchors {
            region,
            day,
            n,
            bells,
        })
    }
}

/// 0x01 CreateSeason: `SeasonParams` (fixed, [`crate::presets::SEASON_PARAMS_LEN`])
/// ‖ `PayoutParams` (borsh, ≤ 128 B). `params_hash = sha256("PSF-PARAMS-v1"
/// ‖ data[1..])` must equal the announced hash ([`crate::presets::params_hash`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreateSeason<'a> {
    pub params: crate::presets::SeasonParams,
    /// Borsh `permutation_rules::frontier::payout::PayoutParams`.
    pub payout: &'a [u8],
}

impl<'a> CreateSeason<'a> {
    pub const IX: Ix = Ix::CreateSeason;
    pub const MAX_LEN: usize = 1 + crate::presets::SEASON_PARAMS_LEN + MAX_PAYOUT_PARAMS;

    pub fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if self.payout.len() > MAX_PAYOUT_PARAMS {
            return None;
        }
        let mut w = Writer::new(out);
        w.u8(Self::IX.tag())
            .bytes(&self.params.to_bytes())
            .bytes(self.payout);
        w.finish()
    }

    pub fn decode(d: &'a [u8]) -> Result<Self, FrontierError> {
        let bad = FrontierError::BadData;
        let p = crate::presets::SEASON_PARAMS_LEN;
        if d.first() != Some(&Self::IX.tag()) || d.len() < 1 + p || d.len() > Self::MAX_LEN {
            return Err(bad);
        }
        let params = crate::presets::SeasonParams::from_bytes(&d[1..1 + p]).ok_or(bad)?;
        Ok(CreateSeason {
            params,
            payout: &d[1 + p..],
        })
    }
}

/// Byte widths of the variable instructions, for budgets and vectors:
/// `(ix, min data length, max data length)` with the tag.
pub fn data_len_range(ix: Ix) -> (usize, usize) {
    match ix {
        Ix::FileTicket => (7, FileTicket::MAX_LEN),
        Ix::ArchiveAnchors => (11, ArchiveAnchors::MAX_LEN),
        Ix::CreateSeason => (1 + crate::presets::SEASON_PARAMS_LEN, CreateSeason::MAX_LEN),
        _ => {
            let n = fixed_len(ix).unwrap_or(0);
            (n, n)
        }
    }
}

/// `LEN` of each fixed-size instruction.
pub const fn fixed_len(ix: Ix) -> Option<usize> {
    Some(match ix {
        Ix::AnnounceSeason => AnnounceSeason::LEN,
        Ix::InitBeaconLogs => InitBeaconLogs::LEN,
        Ix::InitShards => InitShards::LEN,
        Ix::ConsumeGenesisSeed => ConsumeGenesisSeed::LEN,
        Ix::EndSeason => EndSeason::LEN,
        Ix::CloseSeason => CloseSeason::LEN,
        Ix::AbortSeason => AbortSeason::LEN,
        Ix::SetWindowSchedule => SetWindowSchedule::LEN,
        Ix::PostAnchor => PostAnchor::LEN,
        Ix::PostAnchorMulti => PostAnchorMulti::LEN,
        Ix::PostSeed => PostSeed::LEN,
        Ix::PostBeacon => PostBeacon::LEN,
        Ix::CloseSeedCache => CloseSeedCache::LEN,
        Ix::OpenRing => OpenRing::LEN,
        Ix::ConsumeRingSeed => ConsumeRingSeed::LEN,
        Ix::OpenProvince => OpenProvince::LEN,
        Ix::FoldOccupancy => FoldOccupancy::LEN,
        Ix::CloseProvince => CloseProvince::LEN,
        Ix::Join => Join::LEN,
        Ix::SetSession => SetSession::LEN,
        Ix::SetVigil => SetVigil::LEN,
        Ix::SettleTicket => SettleTicket::LEN,
        Ix::ReleaseDormant => ReleaseDormant::LEN,
        Ix::CloseHolding => CloseHolding::LEN,
        Ix::CloseCitizen => CloseCitizen::LEN,
        Ix::Harvest => Harvest::LEN,
        Ix::Build => Build::LEN,
        Ix::Train => Train::LEN,
        Ix::Muster => Muster::LEN,
        Ix::Dissolve => Dissolve::LEN,
        Ix::Garrison => Garrison::LEN,
        Ix::Explore => Explore::LEN,
        Ix::SettleExplore => SettleExplore::LEN,
        Ix::DisbandStranded => DisbandStranded::LEN,
        Ix::Depart => Depart::LEN,
        Ix::Reveal => Reveal::LEN,
        Ix::SettleDeparture => SettleDeparture::LEN,
        Ix::SettleTransit => SettleTransit::LEN,
        Ix::SweepPoolOwed => SweepPoolOwed::LEN,
        Ix::GatherClash => GatherClash::LEN,
        Ix::ResolveFromInputs => ResolveFromInputs::LEN,
        Ix::ResolveClash => ResolveClash::LEN,
        Ix::SkipQuiet => SkipQuiet::LEN,
        Ix::CloseClashInputs => CloseClashInputs::LEN,
        Ix::CloseArrivalDay => CloseArrivalDay::LEN,
        Ix::CloseArrivalSlot => CloseArrivalSlot::LEN,
        Ix::ClaimDefence => ClaimDefence::LEN,
        Ix::FileTicket | Ix::ArchiveAnchors | Ix::CreateSeason => return None,
    })
}

/// Field names and widths of each fixed-size instruction (vectors).
pub const fn wire_of(ix: Ix) -> Option<&'static [(&'static str, usize)]> {
    Some(match ix {
        Ix::AnnounceSeason => AnnounceSeason::WIRE,
        Ix::InitBeaconLogs => InitBeaconLogs::WIRE,
        Ix::InitShards => InitShards::WIRE,
        Ix::ConsumeGenesisSeed => ConsumeGenesisSeed::WIRE,
        Ix::EndSeason => EndSeason::WIRE,
        Ix::CloseSeason => CloseSeason::WIRE,
        Ix::AbortSeason => AbortSeason::WIRE,
        Ix::SetWindowSchedule => SetWindowSchedule::WIRE,
        Ix::PostAnchor => PostAnchor::WIRE,
        Ix::PostAnchorMulti => PostAnchorMulti::WIRE,
        Ix::PostSeed => PostSeed::WIRE,
        Ix::PostBeacon => PostBeacon::WIRE,
        Ix::CloseSeedCache => CloseSeedCache::WIRE,
        Ix::OpenRing => OpenRing::WIRE,
        Ix::ConsumeRingSeed => ConsumeRingSeed::WIRE,
        Ix::OpenProvince => OpenProvince::WIRE,
        Ix::FoldOccupancy => FoldOccupancy::WIRE,
        Ix::CloseProvince => CloseProvince::WIRE,
        Ix::Join => Join::WIRE,
        Ix::SetSession => SetSession::WIRE,
        Ix::SetVigil => SetVigil::WIRE,
        Ix::SettleTicket => SettleTicket::WIRE,
        Ix::ReleaseDormant => ReleaseDormant::WIRE,
        Ix::CloseHolding => CloseHolding::WIRE,
        Ix::CloseCitizen => CloseCitizen::WIRE,
        Ix::Harvest => Harvest::WIRE,
        Ix::Build => Build::WIRE,
        Ix::Train => Train::WIRE,
        Ix::Muster => Muster::WIRE,
        Ix::Dissolve => Dissolve::WIRE,
        Ix::Garrison => Garrison::WIRE,
        Ix::Explore => Explore::WIRE,
        Ix::SettleExplore => SettleExplore::WIRE,
        Ix::DisbandStranded => DisbandStranded::WIRE,
        Ix::Depart => Depart::WIRE,
        Ix::Reveal => Reveal::WIRE,
        Ix::SettleDeparture => SettleDeparture::WIRE,
        Ix::SettleTransit => SettleTransit::WIRE,
        Ix::SweepPoolOwed => SweepPoolOwed::WIRE,
        Ix::GatherClash => GatherClash::WIRE,
        Ix::ResolveFromInputs => ResolveFromInputs::WIRE,
        Ix::ResolveClash => ResolveClash::WIRE,
        Ix::SkipQuiet => SkipQuiet::WIRE,
        Ix::CloseClashInputs => CloseClashInputs::WIRE,
        Ix::CloseArrivalDay => CloseArrivalDay::WIRE,
        Ix::CloseArrivalSlot => CloseArrivalSlot::WIRE,
        Ix::ClaimDefence => ClaimDefence::WIRE,
        Ix::FileTicket | Ix::ArchiveAnchors | Ix::CreateSeason => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_match_the_contract() {
        assert_eq!(Depart::LEN, 219);
        assert_eq!(Reveal::LEN, 136);
        assert_eq!(SettleTransit::LEN, 231);
        assert_eq!(HINTS_LEN, 290);
        for ix in Ix::ALL {
            let (lo, hi) = data_len_range(*ix);
            assert!(lo >= 1 && lo <= hi, "{}", ix.name());
        }
    }

    #[test]
    fn round_trips_and_refusals() {
        let d = Depart {
            host_id: 0x0123_4567_89AB_CDEF,
            commit: [7; 32],
            seal: [9; SEAL_LEN],
            arrive_bell: 77,
            tip: 14_441,
            transit_slot: 3,
        };
        let b = d.to_bytes();
        assert_eq!(b[0], 0x50);
        assert_eq!(Depart::decode(&b), Ok(d));
        assert_eq!(Depart::decode(&b[..218]), Err(FrontierError::BadData));
        let mut wrong = b;
        wrong[0] = 0x51;
        assert_eq!(Depart::decode(&wrong), Err(FrontierError::BadData));
        assert_eq!(tag_of(&b), Ok(Ix::Depart));
        assert_eq!(tag_of(&[0x53]), Err(FrontierError::BadData));

        let t = FileTicket {
            n: 2,
            sites: [
                TicketSite {
                    p: -3,
                    q: 5,
                    site: 11,
                },
                TicketSite {
                    p: 2,
                    q: 0,
                    site: 0,
                },
                TicketSite::default(),
            ],
        };
        let mut buf = [0u8; FileTicket::MAX_LEN];
        let n = t.encode(&mut buf).unwrap();
        assert_eq!(n, 12);
        assert_eq!(FileTicket::decode(&buf[..n]), Ok(t));
        assert!(FileTicket::decode(&buf[..n - 1]).is_err());

        let a = ArchiveAnchors {
            region: 3,
            day: 9,
            n: 8,
            bells: [1, 2, 3, 4, 5, 6, 7, 8],
        };
        let mut buf = [0u8; ArchiveAnchors::MAX_LEN];
        let n = a.encode(&mut buf).unwrap();
        assert_eq!(ArchiveAnchors::decode(&buf[..n]), Ok(a));
    }
}
