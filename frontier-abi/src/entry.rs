//! Province entry (48 B) ↔ kernel `Host` codec (§5.3 Entry, I-55).
//!
//! The program, the verifier, the herald and the WASM client read and write
//! `Province.entries[56]` through this one codec, so the byte form of a
//! host and its pending change is defined once.
//!
//! | entry | kernel |
//! |---|---|
//! | `id` | `Host.id` |
//! | — | `Host.owner` = [`holding_key_of_host`]`(id)` (the issuing holding and its generation) |
//! | `faction`, `unit` | `Host.faction`, `Host.unit` (`UnitType` in declaration order) |
//! | `troops` (MilliTroops) | `Host.troops` |
//! | `stamina_value`, `stamina_bell` | `Host.stamina` |
//! | `ready_bell` | `Host.ready_bell` |
//! | `pend_bell`, `pend_op`, `op_*` | `Host.pending` (ops 1–4) |
//! | `tile`, `state`, `dealt_bps`, `from_bell` | entry only |
//!
//! Pending ops 5 (`Leave`, Dissolve) and 6 (`Forfeit`, bad seal, I-44) are
//! program-level: [`Entry::to_host`] gives `pending = None` for them and the
//! program consults [`Entry::op`] (a host with any op is `HostBusy`).
//!
//! Op arguments: `Spend {cost}` → `op_b = cost`; `Absorb {from}` /
//! `AbsorbedInto {into}` → `op_ref` = the other host's sequence number (a
//! merge is within one holding, so the rest of the id is this entry's);
//! `Split {troops, of, new_id}` → `op_troops` bits 0..25 = `troops`,
//! `op_ref` = `new_id`'s sequence number, `of` bits 0..24 in `op_a | op_b << 8`
//! and bits 24..31 in `op_troops` bits 25..32 (M1 has no split instruction;
//! the encoding is reserved and round-trips).

use crate::addr::holding_key_of_host;
use crate::bytes::{rd_u16, rd_u32, rd_u64, rd_u8, wr_u16, wr_u32, wr_u64, wr_u8};
use crate::layout::province::entry as E;
use permutation_rules::fixed::MilliTroops;
use permutation_rules::frontier::host::{Host, Pending, PendingOp, Stamina};
use permutation_rules::units::UnitType;

/// Why an entry cannot be decoded or encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryError {
    /// Fewer than 48 bytes.
    Short,
    BadState(u8),
    BadOp(u8),
    BadUnit(u8),
    /// A merge partner or split id from another holding.
    ForeignHost(u64),
    /// A split whose numbers do not fit the reserved encoding.
    SplitRange,
}

/// The eight unit types in declaration (= borsh) order.
pub const UNITS: [UnitType; 8] = [
    UnitType::Spearman,
    UnitType::Archer,
    UnitType::Horseman,
    UnitType::Pikeman,
    UnitType::Crossbowman,
    UnitType::Knight,
    UnitType::Scout,
    UnitType::Settler,
];

pub fn unit_from_u8(u: u8) -> Option<UnitType> {
    UNITS.get(u as usize).copied()
}

pub fn unit_to_u8(u: UnitType) -> u8 {
    UNITS.iter().position(|x| *x == u).unwrap_or(0) as u8
}

/// An entry's pending change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryOp {
    None,
    Spend {
        cost: u16,
    },
    Split {
        troops: MilliTroops,
        of: MilliTroops,
        new_id: u64,
    },
    Absorb {
        from: u64,
    },
    AbsorbedInto {
        into: u64,
    },
    /// Dissolve: troops back to the reserve after the clash.
    Leave,
    /// Bad seal: troops lost at the settle (I-44).
    Forfeit,
}

impl EntryOp {
    pub const fn code(&self) -> u8 {
        match self {
            EntryOp::None => E::OP_NONE,
            EntryOp::Spend { .. } => E::OP_SPEND,
            EntryOp::Split { .. } => E::OP_SPLIT,
            EntryOp::Absorb { .. } => E::OP_ABSORB,
            EntryOp::AbsorbedInto { .. } => E::OP_ABSORBED_INTO,
            EntryOp::Leave => E::OP_LEAVE,
            EntryOp::Forfeit => E::OP_FORFEIT,
        }
    }
}

/// A decoded Province entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    pub faction: u8,
    pub unit: u8,
    pub tile: u8,
    /// 0 free, 1 roster, 2 muster-pending, 3 departed.
    pub state: u8,
    pub troops: MilliTroops,
    pub stamina_value: u16,
    pub dealt_bps: u16,
    pub stamina_bell: u32,
    pub ready_bell: u32,
    pub from_bell: u32,
    /// The bell the pending change was issued in (0 when none).
    pub pend_bell: u32,
    pub op: EntryOp,
}

impl Entry {
    /// A free entry (all zero).
    pub const FREE: Entry = Entry {
        id: 0,
        faction: 0,
        unit: 0,
        tile: 0,
        state: E::STATE_FREE,
        troops: 0,
        stamina_value: 0,
        dealt_bps: 0,
        stamina_bell: 0,
        ready_bell: 0,
        from_bell: 0,
        pend_bell: 0,
        op: EntryOp::None,
    };

    /// Decodes `d[0..48]`.
    pub fn read(d: &[u8]) -> Result<Entry, EntryError> {
        if d.len() < E::SIZE {
            return Err(EntryError::Short);
        }
        let s = EntryError::Short;
        let id = rd_u64(d, E::ID).ok_or(s)?;
        let state = rd_u8(d, E::STATE).ok_or(s)?;
        if state > E::STATE_DEPARTED {
            return Err(EntryError::BadState(state));
        }
        let unit = rd_u8(d, E::UNIT).ok_or(s)?;
        if unit_from_u8(unit).is_none() {
            return Err(EntryError::BadUnit(unit));
        }
        let code = rd_u8(d, E::PEND_OP).ok_or(s)?;
        let a = rd_u8(d, E::OP_A).ok_or(s)? as u32;
        let b = rd_u16(d, E::OP_B).ok_or(s)?;
        let t = rd_u32(d, E::OP_TROOPS).ok_or(s)?;
        let r = rd_u32(d, E::OP_REF).ok_or(s)?;
        let key = holding_key_of_host(id);
        let op = match code {
            E::OP_NONE => EntryOp::None,
            E::OP_SPEND => EntryOp::Spend { cost: b },
            E::OP_SPLIT => EntryOp::Split {
                troops: t & SPLIT_TROOPS_MASK,
                of: a | (b as u32) << 8 | (t >> 25) << 24,
                new_id: key | r as u64,
            },
            E::OP_ABSORB => EntryOp::Absorb {
                from: key | r as u64,
            },
            E::OP_ABSORBED_INTO => EntryOp::AbsorbedInto {
                into: key | r as u64,
            },
            E::OP_LEAVE => EntryOp::Leave,
            E::OP_FORFEIT => EntryOp::Forfeit,
            x => return Err(EntryError::BadOp(x)),
        };
        Ok(Entry {
            id,
            faction: rd_u8(d, E::FACTION).ok_or(s)?,
            unit,
            tile: rd_u8(d, E::TILE).ok_or(s)?,
            state,
            troops: rd_u32(d, E::TROOPS).ok_or(s)?,
            stamina_value: rd_u16(d, E::STAMINA_VALUE).ok_or(s)?,
            dealt_bps: rd_u16(d, E::DEALT_BPS).ok_or(s)?,
            stamina_bell: rd_u32(d, E::STAMINA_BELL).ok_or(s)?,
            ready_bell: rd_u32(d, E::READY_BELL).ok_or(s)?,
            from_bell: rd_u32(d, E::FROM_BELL).ok_or(s)?,
            pend_bell: rd_u32(d, E::PEND_BELL).ok_or(s)?,
            op,
        })
    }

    /// Encodes into `d[0..48]`.
    pub fn write(&self, d: &mut [u8]) -> Result<(), EntryError> {
        if d.len() < E::SIZE {
            return Err(EntryError::Short);
        }
        let key = holding_key_of_host(self.id);
        let same = |other: u64| {
            if holding_key_of_host(other) == key {
                Ok(other as u32)
            } else {
                Err(EntryError::ForeignHost(other))
            }
        };
        let (a, b, t, r): (u8, u16, u32, u32) = match self.op {
            EntryOp::None | EntryOp::Leave | EntryOp::Forfeit => (0, 0, 0, 0),
            EntryOp::Spend { cost } => (0, cost, 0, 0),
            EntryOp::Split { troops, of, new_id } => {
                if troops > SPLIT_TROOPS_MASK || of >= 1 << 31 {
                    return Err(EntryError::SplitRange);
                }
                (
                    of as u8,
                    (of >> 8) as u16,
                    troops | (of >> 24) << 25,
                    same(new_id)?,
                )
            }
            EntryOp::Absorb { from } => (0, 0, 0, same(from)?),
            EntryOp::AbsorbedInto { into } => (0, 0, 0, same(into)?),
        };
        let ok = wr_u64(d, E::ID, self.id)
            & wr_u8(d, E::FACTION, self.faction)
            & wr_u8(d, E::UNIT, self.unit)
            & wr_u8(d, E::TILE, self.tile)
            & wr_u8(d, E::STATE, self.state)
            & wr_u32(d, E::TROOPS, self.troops)
            & wr_u16(d, E::STAMINA_VALUE, self.stamina_value)
            & wr_u16(d, E::DEALT_BPS, self.dealt_bps)
            & wr_u32(d, E::STAMINA_BELL, self.stamina_bell)
            & wr_u32(d, E::READY_BELL, self.ready_bell)
            & wr_u32(d, E::FROM_BELL, self.from_bell)
            & wr_u32(d, E::PEND_BELL, self.pend_bell)
            & wr_u8(d, E::PEND_OP, self.op.code())
            & wr_u8(d, E::OP_A, a)
            & wr_u16(d, E::OP_B, b)
            & wr_u32(d, E::OP_TROOPS, t)
            & wr_u32(d, E::OP_REF, r);
        if ok {
            Ok(())
        } else {
            Err(EntryError::Short)
        }
    }

    /// The kernel host of this entry (ops 5–6 give `pending = None`; see the
    /// module note).
    pub fn to_host(&self) -> Result<Host, EntryError> {
        let unit = unit_from_u8(self.unit).ok_or(EntryError::BadUnit(self.unit))?;
        let op = match self.op {
            EntryOp::Spend { cost } => Some(PendingOp::Spend { cost }),
            EntryOp::Split { troops, of, new_id } => Some(PendingOp::Split { troops, of, new_id }),
            EntryOp::Absorb { from } => Some(PendingOp::Absorb { from }),
            EntryOp::AbsorbedInto { into } => Some(PendingOp::AbsorbedInto { into }),
            EntryOp::None | EntryOp::Leave | EntryOp::Forfeit => None,
        };
        Ok(Host {
            id: self.id,
            owner: holding_key_of_host(self.id),
            faction: self.faction,
            unit,
            troops: self.troops,
            stamina: Stamina {
                value: self.stamina_value,
                bell: self.stamina_bell,
            },
            ready_bell: self.ready_bell,
            pending: op.map(|op| Pending {
                bell: self.pend_bell,
                op,
            }),
        })
    }

    /// Writes a kernel host's values back into this entry (tile, state,
    /// `dealt_bps` and `from_bell` are kept). A program-level op (Leave,
    /// Forfeit) is kept unless the kernel now has a pending change.
    pub fn set_host(&mut self, h: &Host) {
        self.id = h.id;
        self.faction = h.faction;
        self.unit = unit_to_u8(h.unit);
        self.troops = h.troops;
        self.stamina_value = h.stamina.value;
        self.stamina_bell = h.stamina.bell;
        self.ready_bell = h.ready_bell;
        match h.pending {
            Some(p) => {
                self.pend_bell = p.bell;
                self.op = match p.op {
                    PendingOp::Spend { cost } => EntryOp::Spend { cost },
                    PendingOp::Split { troops, of, new_id } => {
                        EntryOp::Split { troops, of, new_id }
                    }
                    PendingOp::Absorb { from } => EntryOp::Absorb { from },
                    PendingOp::AbsorbedInto { into } => EntryOp::AbsorbedInto { into },
                };
            }
            None => {
                if !matches!(self.op, EntryOp::Leave | EntryOp::Forfeit) {
                    self.op = EntryOp::None;
                    self.pend_bell = 0;
                }
            }
        }
    }

    /// A new entry for a mustered host (state 2, joins the roster at
    /// `from_bell`).
    pub fn from_host(h: &Host, tile: u8, state: u8, dealt_bps: u16, from_bell: u32) -> Entry {
        let mut e = Entry {
            tile,
            state,
            dealt_bps,
            from_bell,
            ..Entry::FREE
        };
        e.set_host(h);
        e
    }

    /// Whether any change (kernel or program-level) is pending.
    pub const fn busy(&self) -> bool {
        !matches!(self.op, EntryOp::None)
    }
}

/// `op_troops` bits holding a split's troops (25 bits ≥ 30,000,000 milli).
pub const SPLIT_TROOPS_MASK: u32 = (1 << 25) - 1;

/// Reads entry `i` of a Province account.
pub fn read_entry(province: &[u8], i: usize) -> Result<Entry, EntryError> {
    if i >= crate::layout::province::province::ENTRIES_N {
        return Err(EntryError::Short);
    }
    let off = crate::layout::province::province::entry(i);
    Entry::read(province.get(off..off + E::SIZE).ok_or(EntryError::Short)?)
}

/// Writes entry `i` of a Province account.
pub fn write_entry(province: &mut [u8], i: usize, e: &Entry) -> Result<(), EntryError> {
    if i >= crate::layout::province::province::ENTRIES_N {
        return Err(EntryError::Short);
    }
    let off = crate::layout::province::province::entry(i);
    e.write(
        province
            .get_mut(off..off + E::SIZE)
            .ok_or(EntryError::Short)?,
    )
}

/// Index of the entry holding `host_id` in states 1–3.
pub fn find_entry(province: &[u8], host_id: u64) -> Option<usize> {
    (0..crate::layout::province::province::ENTRIES_N).find(|&i| {
        let off = crate::layout::province::province::entry(i);
        rd_u64(province, off + E::ID) == Some(host_id)
            && matches!(rd_u8(province, off + E::STATE), Some(1..=3))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::host_id;
    use permutation_rules::frontier::host::MAX_HOST_TROOPS;

    fn sample() -> Host {
        let id = host_id(-3, 5, 7, 2, 41).unwrap();
        Host::muster(
            id,
            holding_key_of_host(id),
            4,
            UnitType::Knight,
            12_345_678,
            900,
        )
        .unwrap()
    }

    #[test]
    fn host_round_trips_with_every_op() {
        let h0 = sample();
        let key = holding_key_of_host(h0.id);
        let ops = [
            None,
            Some(PendingOp::Spend { cost: 120 }),
            Some(PendingOp::Split {
                troops: MAX_HOST_TROOPS,
                of: MAX_HOST_TROOPS,
                new_id: key | 99,
            }),
            Some(PendingOp::Split {
                troops: 100_000,
                of: (1 << 31) - 1,
                new_id: key | u32::MAX as u64,
            }),
            Some(PendingOp::Absorb { from: key | 7 }),
            Some(PendingOp::AbsorbedInto { into: key }),
        ];
        for op in ops {
            let mut h = h0;
            h.stamina = Stamina {
                value: 77,
                bell: 1_234,
            };
            h.ready_bell = 1_236;
            h.pending = op.map(|op| Pending { bell: 1_235, op });
            let e = Entry::from_host(&h, 30, E::STATE_ROSTER, 11_000, 901);
            let mut b = [0u8; 48];
            e.write(&mut b).unwrap();
            let back = Entry::read(&b).unwrap();
            assert_eq!(back, e);
            assert_eq!(back.to_host().unwrap(), h);
        }
    }

    #[test]
    fn program_ops_and_refusals() {
        let h = sample();
        let mut e = Entry::from_host(&h, 3, E::STATE_ROSTER, 10_000, 1);
        e.op = EntryOp::Forfeit;
        e.pend_bell = 5;
        let mut b = [0u8; 48];
        e.write(&mut b).unwrap();
        let back = Entry::read(&b).unwrap();
        assert_eq!(back.op, EntryOp::Forfeit);
        assert!(back.busy());
        assert_eq!(back.to_host().unwrap().pending, None);
        // a kernel op-free host keeps the program op
        let mut kept = back;
        kept.set_host(&back.to_host().unwrap());
        assert_eq!(kept.op, EntryOp::Forfeit);

        // foreign merge partner
        let mut f = e;
        f.op = EntryOp::Absorb {
            from: host_id(0, 0, 0, 0, 1).unwrap(),
        };
        assert!(matches!(f.write(&mut b), Err(EntryError::ForeignHost(_))));
        // bad codes
        let mut raw = [0u8; 48];
        raw[E::PEND_OP] = 7;
        assert_eq!(Entry::read(&raw), Err(EntryError::BadOp(7)));
        raw[E::PEND_OP] = 0;
        raw[E::STATE] = 4;
        assert_eq!(Entry::read(&raw), Err(EntryError::BadState(4)));
        raw[E::STATE] = 0;
        raw[E::UNIT] = 8;
        assert_eq!(Entry::read(&raw), Err(EntryError::BadUnit(8)));
        assert_eq!(Entry::read(&raw[..47]), Err(EntryError::Short));
        assert_eq!(Entry::read(&[0u8; 48]), Ok(Entry::FREE));
    }

    #[test]
    fn province_helpers_find_entries() {
        let mut prov = [0u8; crate::layout::province::province::SIZE];
        let h = sample();
        let e = Entry::from_host(&h, 3, E::STATE_DEPARTED, 10_000, 1);
        write_entry(&mut prov, 55, &e).unwrap();
        assert_eq!(find_entry(&prov, h.id), Some(55));
        assert_eq!(read_entry(&prov, 55).unwrap(), e);
        assert!(read_entry(&prov, 56).is_err());
        assert_eq!(find_entry(&prov, h.id + 1), None);
    }
}
