//! Sieges, vigils, protection, auto-reinforce and raids (design §6.3,
//! §6.6, owner decision D9).
//!
//! * A siege is declared publicly (the siege horn) and needs
//!   **36 + walls/50 bells** of progress. Progress rises by 1 per
//!   **resolved** bell in which the besiegers hold the holding's hex and no
//!   defending host is present, **except during the owner's vigil**: an
//!   8-hour daily window during which progress pauses. So a siege needs at
//!   least 6 hours outside the defender's vigil.
//! * Whether a bell falls in the vigil is decided by the bell's scheduled
//!   start time, never by when it was resolved, and each bell is counted at
//!   most once: siege progress does not depend on keeper lag (design §8.4).
//! * Protection: Shield (48 h, 72 h after day 7), Frontier protection (7
//!   days after the Shield, holdings founded after day 2 can be besieged
//!   only by factions with a holding within 2 provinces), heartlands (no
//!   sieges without War or March hostility), Seats never.
//! * Completion occupies a first holding (never transferred) and captures
//!   holdings 2–3 and Free Cities.

use super::geometry::{is_heartland, march_of, ProvinceCoord};
use super::holding::{Holding, RESOURCES};
use crate::fixed::{Bps, Milli, MilliTroops, BPS_ONE};
use alloc::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};

pub const HOUR: i64 = 3_600;
pub const DAY: i64 = 86_400;
pub const SIEGE_BASE_BELLS: u32 = 36;
/// One more bell per this many wall points.
pub const WALL_POINTS_PER_BELL: u32 = 50;
pub const VIGIL_SECS: i64 = 8 * HOUR;
/// The vigil can change once a week, with 24 hours' notice.
pub const VIGIL_CHANGE_INTERVAL: i64 = 7 * DAY;
pub const VIGIL_NOTICE: i64 = 24 * HOUR;
/// A declared siege lapses if its besiegers never hold the hex within this
/// many bells of the horn [design].
pub const SIEGE_START_WINDOW_BELLS: u32 = 72;
/// Laurels a siege stakes (paid to the defender if it fails).
pub const SIEGE_STAKE_LAURELS: u32 = 5;
pub const FRONTIER_PROTECTION_SECS: i64 = 7 * DAY;
/// Holdings founded after this season day get Frontier protection.
pub const FRONTIER_PROTECTION_AFTER_DAY: u32 = 2;
pub const FRONTIER_PROTECTION_RANGE: u32 = 2;
/// Auto-reinforce sends at most this share of a donor's garrison.
pub const AUTO_REINFORCE_MAX_BPS: Bps = 2_500;
/// A raid loots at most this share of each stock …
pub const RAID_MAX_BPS: Bps = 1_000;
/// … at most once per this long per holding.
pub const RAID_INTERVAL: i64 = 6 * HOUR;

/// Bells of progress a siege needs: `36 + walls/50`, plus a doctrine's
/// extra bells (e.g. Wardens of Stone in their heartland: +12).
pub const fn required_bells(walls: u32, extra: u32) -> u32 {
    SIEGE_BASE_BELLS + walls / WALL_POINTS_PER_BELL + extra
}

/// A holding's vigil: an 8-hour daily window (seconds after UTC midnight
/// it starts). The last three schedules are kept so any unresolved bell is
/// judged by the schedule in force at its scheduled start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Vigil {
    /// `(effective_from_ts, start_secs_of_day)`, oldest first.
    pub schedule: [(i64, u32); 3],
    /// When the last change was requested (`i64::MIN`: never).
    pub last_request: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VigilError {
    /// Changed less than a week ago.
    TooSoon {
        next_ts: i64,
    },
    BadStart,
}

impl Vigil {
    /// Chosen at join (default 00:00–08:00 in the player's time zone).
    pub fn new(start_secs: u32) -> Result<Vigil, VigilError> {
        if start_secs as i64 >= DAY {
            return Err(VigilError::BadStart);
        }
        Ok(Vigil {
            schedule: [(i64::MIN, start_secs); 3],
            last_request: i64::MIN,
        })
    }

    /// Window start (seconds of day) in force at `ts`.
    pub fn start_at(&self, ts: i64) -> u32 {
        let mut s = self.schedule[0].1;
        for (from, start) in self.schedule {
            if from <= ts {
                s = start;
            }
        }
        s
    }

    /// Whether `ts` falls in the vigil window in force at `ts`.
    pub fn covers(&self, ts: i64) -> bool {
        let start = self.start_at(ts) as i64;
        let tod = ts.rem_euclid(DAY);
        (tod - start).rem_euclid(DAY) < VIGIL_SECS
    }

    /// Move the window, effective 24 hours from `now`; at most once a week.
    /// Returns when the new window takes effect.
    pub fn request_change(&mut self, now: i64, start_secs: u32) -> Result<i64, VigilError> {
        if start_secs as i64 >= DAY {
            return Err(VigilError::BadStart);
        }
        if self.last_request != i64::MIN && now < self.last_request + VIGIL_CHANGE_INTERVAL {
            return Err(VigilError::TooSoon {
                next_ts: self.last_request + VIGIL_CHANGE_INTERVAL,
            });
        }
        let from = now + VIGIL_NOTICE;
        self.schedule = [self.schedule[1], self.schedule[2], (from, start_secs)];
        self.last_request = now;
        Ok(from)
    }
}

/// What kind of holding is attacked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum HoldingKind {
    /// A citizen's first holding: occupied, never taken.
    First,
    /// A citizen's second or third holding: capturable.
    Other,
    /// Neutral: released holdings and barbarian towns; always attackable.
    FreeCity,
    /// A faction Seat (ring 1): never capturable.
    Seat,
}

/// Diplomatic state of the attacker's faction toward the owner's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Relation {
    Rivalry,
    War,
    Peace,
    Nap,
    Alliance,
}

/// Everything `may_besiege` needs, read from the target Holding, its
/// Province, FactionState and the attacker's Citizen.
#[derive(Clone, Copy, Debug)]
pub struct SiegeCheck {
    pub province: ProvinceCoord,
    pub kind: HoldingKind,
    pub owner_faction: u8,
    pub attacker_faction: u8,
    pub relation: Relation,
    /// A March hostility in force between the two factions at this March
    /// (counts as War for heartland rules, design §4.2).
    pub march_hostility: bool,
    /// A March truce of the owner's March toward the attacker's faction.
    pub march_truce: bool,
    pub founded_ts: i64,
    pub founded_day: u32,
    pub shield_until: i64,
    pub dormant: bool,
    /// The attacker's faction has a holding within 2 provinces.
    pub attacker_nearby: bool,
    pub now: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiegeRefusal {
    Seat,
    Friendly,
    Shielded,
    FrontierProtected,
    Heartland,
    Truce,
}

/// Whether the attacker may declare a siege now.
pub fn may_besiege(c: &SiegeCheck) -> Result<(), SiegeRefusal> {
    if c.kind == HoldingKind::Seat || c.province.is_seat() {
        return Err(SiegeRefusal::Seat);
    }
    if c.kind == HoldingKind::FreeCity {
        return Ok(());
    }
    if c.owner_faction == c.attacker_faction
        || matches!(
            c.relation,
            Relation::Peace | Relation::Nap | Relation::Alliance
        )
    {
        return Err(SiegeRefusal::Friendly);
    }
    if c.now < c.shield_until && !c.dormant {
        return Err(SiegeRefusal::Shielded);
    }
    if c.founded_day > FRONTIER_PROTECTION_AFTER_DAY
        && c.now < c.shield_until + FRONTIER_PROTECTION_SECS
        && !c.attacker_nearby
    {
        return Err(SiegeRefusal::FrontierProtected);
    }
    if c.march_truce {
        return Err(SiegeRefusal::Truce);
    }
    let war = c.relation == Relation::War || c.march_hostility;
    if is_heartland(c.province, c.owner_faction) && !war {
        return Err(SiegeRefusal::Heartland);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum SiegeStatus {
    Active,
    /// Progress reached the requirement: occupy or capture.
    Completed,
    /// The besiegers lost the hex (or never took it): the stake goes to
    /// the defender.
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Siege {
    pub attacker_faction: u8,
    pub declared_bell: u32,
    /// Last resolved bell counted (each bell once, in order).
    pub last_bell: u32,
    pub progress: u32,
    pub required: u32,
    /// The besiegers have held the hex at some counted bell.
    pub held: bool,
    pub status: SiegeStatus,
}

/// What the clash of one bell reported about the besieged hex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BellReport {
    /// Hosts hostile to the owner hold the holding's hex after the clash.
    pub attackers_hold: bool,
    /// A host of the owner (or an ally) stands on the hex.
    pub defender_present: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiegeError {
    /// Bells must be counted in order: this one is not `last_bell + 1`.
    OutOfOrder {
        expected: u32,
    },
    NotActive,
}

impl Siege {
    /// The horn at bell `b`: counting starts with the next bell.
    pub const fn declare(attacker_faction: u8, b: u32, walls: u32, extra: u32) -> Siege {
        Siege {
            attacker_faction,
            declared_bell: b,
            last_bell: b,
            progress: 0,
            required: required_bells(walls, extra),
            held: false,
            status: SiegeStatus::Active,
        }
    }

    /// Count resolved bell `b` (scheduled start `bell_start_ts`). Returns
    /// the status after it.
    pub fn advance(
        &mut self,
        b: u32,
        bell_start_ts: i64,
        report: BellReport,
        vigil: &Vigil,
    ) -> Result<SiegeStatus, SiegeError> {
        if self.status != SiegeStatus::Active {
            return Err(SiegeError::NotActive);
        }
        if b != self.last_bell + 1 {
            return Err(SiegeError::OutOfOrder {
                expected: self.last_bell + 1,
            });
        }
        self.last_bell = b;
        if !report.attackers_hold {
            // Before the besiegers first hold the hex the siege waits for
            // them (up to the start window); after, losing it fails it.
            if self.held || b > self.declared_bell + SIEGE_START_WINDOW_BELLS {
                self.status = SiegeStatus::Failed;
            }
            return Ok(self.status);
        }
        self.held = true;
        if report.defender_present || vigil.covers(bell_start_ts) {
            return Ok(self.status);
        }
        self.progress += 1;
        if self.progress >= self.required {
            self.status = SiegeStatus::Completed;
        }
        Ok(self.status)
    }
}

/// What a completed siege does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completion {
    /// First holding: the occupier takes 50% of its laurel emission share
    /// and 20% of its production as tribute while it keeps a host in the
    /// province; the owner can Liberate by winning a clash there.
    Occupy,
    /// Holdings 2–3 and Free Cities change hands.
    Capture,
}

pub const OCCUPY_LAUREL_BPS: Bps = 5_000;
pub const OCCUPY_TRIBUTE_BPS: Bps = 2_000;

pub const fn completion(kind: HoldingKind) -> Option<Completion> {
    match kind {
        HoldingKind::First => Some(Completion::Occupy),
        HoldingKind::Other | HoldingKind::FreeCity => Some(Completion::Capture),
        HoldingKind::Seat => None,
    }
}

/// A friendly holding with a standing auto-reinforce order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Donor {
    pub id: u64,
    pub province: ProvinceCoord,
    pub faction: u8,
    pub garrison: MilliTroops,
    /// Share of its garrison the order sends (capped at 25%).
    pub order_bps: Bps,
}

/// Auto-reinforce (design §6.3): donors of the same faction in the same
/// March as the besieged holding each send `garrison × min(order, 25%)`.
/// Returns `(donor id, troops sent)` sorted by donor id; the transfers take
/// effect at the next bell like any garrison change.
pub fn auto_reinforce(
    target_id: u64,
    target: ProvinceCoord,
    faction: u8,
    donors: &[Donor],
) -> Vec<(u64, MilliTroops)> {
    let m = march_of(target);
    let mut out: Vec<(u64, MilliTroops)> = donors
        .iter()
        .filter(|d| d.id != target_id && d.faction == faction && march_of(d.province) == m)
        .map(|d| {
            let bps = d.order_bps.min(AUTO_REINFORCE_MAX_BPS);
            (
                d.id,
                (d.garrison as u64 * bps as u64 / BPS_ONE as u64) as MilliTroops,
            )
        })
        .filter(|(_, t)| *t > 0)
        .collect();
    out.sort();
    out.dedup_by_key(|x| x.0);
    out
}

/// Raid loot: at most 10% of each stock, at most once per 6 hours per
/// holding (`last_raid` is the previous raid's time). Settles the holding
/// to `now` and takes the loot; returns what was taken.
pub fn raid(holding: &mut Holding, last_raid: Option<i64>, now: i64) -> Option<[Milli; RESOURCES]> {
    if last_raid.is_some_and(|t| now < t + RAID_INTERVAL) {
        return None;
    }
    let stock = holding.stock_at(now);
    let loot: [Milli; RESOURCES] =
        core::array::from_fn(|r| stock[r].max(0) * RAID_MAX_BPS as i64 / BPS_ONE as i64);
    holding.pay(now, &loot).ok()?;
    Some(loot)
}
