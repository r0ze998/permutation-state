//! Shared validity checks with typed reasons.
//!
//! The engine calls these to decide whether an order executes, and
//! `preview` calls the same functions to explain to players and agents why
//! an action is blocked. One predicate, two consumers: the UI can never
//! disagree with the rules (the Eternum lesson on logic drift).

use crate::buildings::Building;
use crate::fixed::MILLI;
use crate::hex::Hex;
use crate::params::Ruleset;
use crate::state::{CivId, Owner, QueueItem, Relation, WorldState};
use crate::tech::{Tech, TechSet};
use crate::units::{stats, UnitType};

/// Why an action cannot be taken. Clients localise by variant; the data
/// fields carry the numbers a message needs ("needs 3 more gold").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    // ownership / existence
    UnknownUnit,
    UnknownCity,
    UnknownCiv,
    NotYours,
    SameCiv,
    // founding (§4.2, §5.7)
    NotASettler,
    Impassable,
    ForeignTerritory,
    TooCloseToCity {
        distance: u32,
        min: u32,
    },
    TooCloseToCityState {
        distance: u32,
        min: u32,
    },
    InProtectedZone,
    /// Inside `civ`'s capital protection (§3.3); the zone shrinks past this
    /// hex at tick `until` (`u16::MAX` if it never does before protection ends).
    ProtectedCapital {
        civ: CivId,
        until: u16,
    },
    // production and research (§5.6, §6.2)
    AlreadyBuilt,
    AlreadyQueued,
    NeedsTech(Tech),
    StarGateInAnotherCity,
    NeedsPreviousStage,
    InvalidTroopCount,
    NeedsPop {
        need: u32,
        have: u32,
    },
    AlreadyResearched,
    // diplomacy (§10)
    NotAtPeace,
    InTruce {
        until: u16,
    },
    AlreadyAtWar,
    NotAtWar,
    UnderNap,
    Allied,
    NoProposal,
    BondTooSmall {
        min: u32,
    },
    NotEnoughGold {
        need: u32,
        have: u32,
    },
    AllianceFull {
        cap: u32,
    },
    AlreadyInAlliance,
    AllianceLeaving,
    // combat (§8)
    CivilianCannotAttack,
    OutOfRange {
        distance: u32,
        range: u32,
    },
    TargetProtected,
    TargetNotHostile,
    TargetGone,
    // markets (§11)
    Frozen,
    NothingToSell,
    OverCap {
        cap: u32,
    },
    // standing rules (§13)
    /// A unit rule sent to a city or a city rule sent to a unit.
    WrongStandingTarget,
    /// Radius or ratio outside its allowed range.
    OutOfBounds {
        min: u32,
        max: u32,
    },
    // offices and V5 fixes
    /// The office's whole batch was rejected at resolution (budget, office, adopted proposal).
    BatchRejected,
    /// The order belongs to another office (V5 §5.1).
    WrongOffice,
    /// War needs the general's or the steward's consent (V5 §5.6).
    NeedsConsent,
    /// One purchase per city per tick (v0.2 C3).
    AlreadyPurchased,
    /// Gold cannot buy a Star Gate stage (v0.2 C3).
    CannotBuyStarGate,
    NothingQueued,
    NotEnoughInfluence,
    /// Market: no trades with oneself or an enemy (v0.2 C8).
    NoCounterparty,
    /// Market: treasury spending above the threshold needs a second officer (V5 §7.5).
    NeedsSpendConsent,
    /// Market: not enough USDC in the treasury.
    NotEnoughUsdc,
    /// Another civ's city, a Free City or a city-state: a unit enters it only
    /// by capturing it (§7.3).
    ForeignCity,
}

/// Names of `Blocked` codes, index = `Blocked::code()`, for clients.
pub const BLOCKED_NAMES: [&str; 53] = [
    "UnknownUnit",
    "UnknownCity",
    "UnknownCiv",
    "NotYours",
    "SameCiv",
    "NotASettler",
    "Impassable",
    "ForeignTerritory",
    "TooCloseToCity",
    "TooCloseToCityState",
    "InProtectedZone",
    "ProtectedCapital",
    "AlreadyBuilt",
    "AlreadyQueued",
    "NeedsTech",
    "StarGateInAnotherCity",
    "NeedsPreviousStage",
    "InvalidTroopCount",
    "NeedsPop",
    "AlreadyResearched",
    "NotAtPeace",
    "InTruce",
    "AlreadyAtWar",
    "NotAtWar",
    "UnderNap",
    "Allied",
    "NoProposal",
    "BondTooSmall",
    "NotEnoughGold",
    "AllianceFull",
    "AlreadyInAlliance",
    "AllianceLeaving",
    "CivilianCannotAttack",
    "OutOfRange",
    "TargetProtected",
    "TargetNotHostile",
    "TargetGone",
    "Frozen",
    "NothingToSell",
    "OverCap",
    "WrongStandingTarget",
    "OutOfBounds",
    "BatchRejected",
    "WrongOffice",
    "NeedsConsent",
    "AlreadyPurchased",
    "CannotBuyStarGate",
    "NothingQueued",
    "NotEnoughInfluence",
    "NoCounterparty",
    "NeedsSpendConsent",
    "NotEnoughUsdc",
    "ForeignCity",
];

impl Blocked {
    /// Stable numeric code (the variant's position), recorded in `Skip`.
    pub const fn code(&self) -> u8 {
        use Blocked::*;
        match self {
            UnknownUnit => 0,
            UnknownCity => 1,
            UnknownCiv => 2,
            NotYours => 3,
            SameCiv => 4,
            NotASettler => 5,
            Impassable => 6,
            ForeignTerritory => 7,
            TooCloseToCity { .. } => 8,
            TooCloseToCityState { .. } => 9,
            InProtectedZone => 10,
            ProtectedCapital { .. } => 11,
            AlreadyBuilt => 12,
            AlreadyQueued => 13,
            NeedsTech(_) => 14,
            StarGateInAnotherCity => 15,
            NeedsPreviousStage => 16,
            InvalidTroopCount => 17,
            NeedsPop { .. } => 18,
            AlreadyResearched => 19,
            NotAtPeace => 20,
            InTruce { .. } => 21,
            AlreadyAtWar => 22,
            NotAtWar => 23,
            UnderNap => 24,
            Allied => 25,
            NoProposal => 26,
            BondTooSmall { .. } => 27,
            NotEnoughGold { .. } => 28,
            AllianceFull { .. } => 29,
            AlreadyInAlliance => 30,
            AllianceLeaving => 31,
            CivilianCannotAttack => 32,
            OutOfRange { .. } => 33,
            TargetProtected => 34,
            TargetNotHostile => 35,
            TargetGone => 36,
            Frozen => 37,
            NothingToSell => 38,
            OverCap { .. } => 39,
            WrongStandingTarget => 40,
            OutOfBounds { .. } => 41,
            BatchRejected => 42,
            WrongOffice => 43,
            NeedsConsent => 44,
            AlreadyPurchased => 45,
            CannotBuyStarGate => 46,
            NothingQueued => 47,
            NotEnoughInfluence => 48,
            NoCounterparty => 49,
            NeedsSpendConsent => 50,
            NotEnoughUsdc => 51,
            ForeignCity => 52,
        }
    }
}

/// `FoundCity` (§4.2, §5.7). Returns the site on success.
pub fn found_city(
    state: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    settler: u32,
) -> Result<Hex, Blocked> {
    let u = state
        .units
        .get(settler as usize)
        .filter(|u| u.alive)
        .ok_or(Blocked::UnknownUnit)?;
    if u.owner != Owner::Civ(civ) {
        return Err(Blocked::NotYours);
    }
    if u.unit_type != UnitType::Settler {
        return Err(Blocked::NotASettler);
    }
    found_site(state, rules, civ, u.hex).map(|_| u.hex)
}

/// Whether `hex` is a legal city site for `civ` (independent of any settler).
pub fn found_site(
    state: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    hex: Hex,
) -> Result<(), Blocked> {
    let tile = state.map.tile(hex).ok_or(Blocked::Impassable)?;
    if !tile.terrain.is_passable() {
        return Err(Blocked::Impassable);
    }
    let foreign = tile
        .owner_city
        .and_then(|c| state.cities.get(c as usize))
        .is_some_and(|c| c.alive && c.owner != Some(civ));
    if foreign {
        return Err(Blocked::ForeignTerritory);
    }
    let min = rules.city_min_distance as u32;
    if let Some(d) = state
        .cities
        .iter()
        .filter(|c| c.alive)
        .map(|c| c.hex.distance(hex))
        .min()
    {
        if d < min {
            return Err(Blocked::TooCloseToCity { distance: d, min });
        }
    }
    if let Some(d) = state.city_states.iter().map(|c| c.hex.distance(hex)).min() {
        if d < min {
            return Err(Blocked::TooCloseToCityState { distance: d, min });
        }
    }
    if crate::battle::is_protected(state, rules, civ, hex) {
        return Err(Blocked::InProtectedZone);
    }
    Ok(())
}

/// One production item in a city queue (§5.6, §7.1).
pub fn queue_item(
    state: &WorldState,
    civ: CivId,
    city: u32,
    item: &QueueItem,
) -> Result<(), Blocked> {
    let c = state
        .cities
        .get(city as usize)
        .filter(|c| c.alive)
        .ok_or(Blocked::UnknownCity)?;
    if c.owner != Some(civ) {
        return Err(Blocked::NotYours);
    }
    let techs = state.civs[civ as usize].techs;
    match *item {
        QueueItem::Building(b) => {
            if c.buildings.has(b) {
                return Err(Blocked::AlreadyBuilt);
            }
            if let Some(t) = crate::buildings::info(b).tech.filter(|t| !techs.has(*t)) {
                return Err(Blocked::NeedsTech(t));
            }
            if b.is_star_gate() {
                // One Star Gate city per civilization, stages in order (§5.6).
                if state
                    .living_cities_of(civ)
                    .any(|o| o.id != city && o.buildings.star_gate_stages() > 0)
                {
                    return Err(Blocked::StarGateInAnotherCity);
                }
                let prev_ok = match b {
                    Building::StarGate2 => c.buildings.has(Building::StarGate1),
                    Building::StarGate3 => c.buildings.has(Building::StarGate2),
                    _ => true,
                };
                if !prev_ok {
                    return Err(Blocked::NeedsPreviousStage);
                }
            }
            Ok(())
        }
        QueueItem::Troops { unit, n } => {
            if unit.is_civilian() || !(1..=20).contains(&n) {
                return Err(Blocked::InvalidTroopCount);
            }
            match stats(unit).tech.filter(|t| !techs.has(*t)) {
                Some(t) => Err(Blocked::NeedsTech(t)),
                None => Ok(()),
            }
        }
        QueueItem::Scout | QueueItem::Settler => Ok(()),
    }
}

/// Researching `tech` given what is already held or planned (§6.2).
pub fn research(planned: TechSet, tech: Tech) -> Result<(), Blocked> {
    if planned.has(tech) {
        return Err(Blocked::AlreadyResearched);
    }
    match crate::tech::info(tech)
        .prereqs
        .iter()
        .flatten()
        .find(|p| !planned.has(**p))
    {
        Some(p) => Err(Blocked::NeedsTech(*p)),
        None => Ok(()),
    }
}

fn pair(state: &WorldState, a: CivId, b: CivId) -> Result<(), Blocked> {
    if (b as usize) >= state.civs.len() {
        return Err(Blocked::UnknownCiv);
    }
    if a == b {
        return Err(Blocked::SameCiv);
    }
    Ok(())
}

/// `DeclareWar` (§10.2): only from Peace, never during a truce.
pub fn declare_war(state: &WorldState, civ: CivId, target: CivId) -> Result<(), Blocked> {
    pair(state, civ, target)?;
    match state.relation(civ, target) {
        Relation::Peace => {}
        Relation::War { .. } => return Err(Blocked::AlreadyAtWar),
        Relation::Nap { .. } => return Err(Blocked::UnderNap),
        Relation::Alliance { .. } => return Err(Blocked::Allied),
    }
    let until = state.truce_until[state.pair_index(civ, target)];
    if state.tick < until {
        return Err(Blocked::InTruce { until });
    }
    Ok(())
}

/// `ProposePeace` (§10.2): only while at war and no peace is already scheduled.
pub fn propose_peace(state: &WorldState, civ: CivId, target: CivId) -> Result<(), Blocked> {
    pair(state, civ, target)?;
    match state.relation(civ, target) {
        Relation::War { peace_at: None, .. } => Ok(()),
        _ => Err(Blocked::NotAtWar),
    }
}

/// `ProposeNap` (§10.3).
pub fn propose_nap(
    state: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    target: CivId,
    bond: u32,
) -> Result<(), Blocked> {
    pair(state, civ, target)?;
    if bond < rules.nap_min_bond {
        return Err(Blocked::BondTooSmall {
            min: rules.nap_min_bond,
        });
    }
    if !matches!(state.relation(civ, target), Relation::Peace) {
        return Err(Blocked::NotAtPeace);
    }
    let have = (state.civs[civ as usize].gold / 1000).max(0) as u32;
    if have < bond {
        return Err(Blocked::NotEnoughGold { need: bond, have });
    }
    Ok(())
}

/// `ProposeAlliance` (§10.4).
pub fn propose_alliance(
    state: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    target: CivId,
) -> Result<(), Blocked> {
    pair(state, civ, target)?;
    if !matches!(
        state.relation(civ, target),
        Relation::Peace | Relation::Nap { .. }
    ) {
        return Err(Blocked::NotAtPeace);
    }
    let _ = rules;
    Ok(())
}

/// `AcceptAlliance` (§10.4, §10.6): the joiner takes the proposer's whole group.
pub fn join_alliance(
    state: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    from: CivId,
) -> Result<(), Blocked> {
    pair(state, civ, from)?;
    if crate::diplomacy::alliance_group(state, civ).len() > 1 {
        return Err(Blocked::AlreadyInAlliance);
    }
    let group = crate::diplomacy::alliance_group(state, from);
    let cap = rules.alliance_cap(state.civs.len());
    if group.len() >= cap {
        return Err(Blocked::AllianceFull { cap: cap as u32 });
    }
    if group.iter().any(|m| {
        !matches!(
            state.relation(civ, *m),
            Relation::Peace | Relation::Nap { .. }
        )
    }) {
        return Err(Blocked::NotAtPeace);
    }
    let leaving = (0..state.civs.len() as u16).any(|o| {
        o != from
            && matches!(
                state.relation(from, o),
                Relation::Alliance {
                    leaving_at: Some(_)
                }
            )
    });
    if leaving {
        return Err(Blocked::AllianceLeaving);
    }
    Ok(())
}

/// `AcceptPeace` (§10.2), apart from the pending proposal: still at war with
/// no peace already agreed. A civ naming itself is `UnknownCiv` here, as the
/// engine has always recorded it.
pub fn accept_peace(state: &WorldState, civ: CivId, from: CivId) -> Result<(), Blocked> {
    if !crate::diplomacy::valid_pair(state, civ, from) {
        return Err(Blocked::UnknownCiv);
    }
    match state.relation(civ, from) {
        Relation::War { peace_at: None, .. } => Ok(()),
        _ => Err(Blocked::NotAtWar),
    }
}

/// `AcceptNap` (§10.3), apart from the pending proposal and the escrow
/// ([`nap_bonds_payable`]): the offered bond and the relation.
pub fn accept_nap(
    state: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    from: CivId,
    bond: u32,
) -> Result<(), Blocked> {
    if !crate::diplomacy::valid_pair(state, civ, from) {
        return Err(Blocked::UnknownCiv);
    }
    if bond < rules.nap_min_bond {
        return Err(Blocked::BondTooSmall {
            min: rules.nap_min_bond,
        });
    }
    if !matches!(state.relation(civ, from), Relation::Peace) {
        return Err(Blocked::NotAtPeace);
    }
    Ok(())
}

/// Both NAP bonds are escrowed when the pact is accepted; if either side
/// cannot pay, it fails. The shortfall is reported as the accepter's gold.
pub fn nap_bonds_payable(
    state: &WorldState,
    civ: CivId,
    from: CivId,
    bond: u32,
    their_bond: u32,
) -> Result<(), Blocked> {
    let gold = |c: CivId| state.civs[c as usize].gold;
    if gold(civ) < bond as i64 * MILLI || gold(from) < their_bond as i64 * MILLI {
        return Err(Blocked::NotEnoughGold {
            need: bond,
            have: (gold(civ) / MILLI).max(0) as u32,
        });
    }
    Ok(())
}

// ------------------------------------------------------------------ movement

/// Whether `mover` may enter `hex` (terrain, protected capitals, borders;
/// §3.3, §7.3). `None` is a barbarian, which ignores borders.
pub fn enter(
    state: &WorldState,
    rules: &Ruleset,
    mover: Option<CivId>,
    hex: Hex,
) -> Result<(), Blocked> {
    let tile = state.map.tile(hex).ok_or(Blocked::Impassable)?;
    if !tile.terrain.is_passable() {
        return Err(Blocked::Impassable);
    }
    // A city tile is entered only by capture (§7.3): walking into an
    // ungarrisoned enemy city would leave an army inside it that the city's
    // eventual captor then shares the tile with.
    let foreign_city = state
        .cities
        .iter()
        .any(|c| c.alive && c.hex == hex && (mover.is_none() || c.owner != mover))
        || state
            .city_states
            .iter()
            .any(|cs| cs.captured_by.is_none() && cs.hex == hex);
    if foreign_city {
        return Err(Blocked::ForeignCity);
    }
    let radius = rules.protection_radius(state.tick) as u32;
    if radius > 0 {
        for civ in &state.civs {
            if Some(civ.id) == mover || civ.protection_lost {
                continue;
            }
            let Some(cap) = civ
                .capital
                .and_then(|id| state.cities.get(id as usize))
                .filter(|c| c.alive)
            else {
                continue;
            };
            let d = cap.hex.distance(hex);
            if d <= radius {
                let until = rules
                    .protection_schedule
                    .iter()
                    .find(|(from, r)| *from > state.tick && (*r as u32) < d)
                    .map_or(u16::MAX, |(from, _)| *from);
                return Err(Blocked::ProtectedCapital { civ: civ.id, until });
            }
        }
    }
    let Some(mover) = mover else { return Ok(()) };
    match tile
        .owner_city
        .and_then(|id| state.cities.get(id as usize))
        .and_then(|c| c.owner)
    {
        Some(owner)
            if owner != mover
                && !state.at_war(mover, owner)
                && !crate::tick::allied(state, mover, owner) =>
        {
            Err(Blocked::ForeignTerritory)
        }
        _ => Ok(()),
    }
    // TODO(§7.3): city-state territory for suzerains once city-states own tiles.
}

// ------------------------------------------------------------------ standing rules

/// Largest `AutoDefend` radius and `AutoPurchase` cap (§13).
pub const MAX_DEFEND_RADIUS: u32 = 3;
pub const MAX_AUTO_PURCHASE: u32 = 500;

/// `SetStanding` (§13).
pub fn standing(
    state: &WorldState,
    civ: CivId,
    target: crate::orders::StandingTarget,
    rule: &crate::orders::StandingOrder,
) -> Result<(), Blocked> {
    use crate::orders::{StandingOrder as R, StandingTarget as T};
    match target {
        T::Unit(id) => {
            let u = state
                .units
                .get(id as usize)
                .filter(|u| u.alive)
                .ok_or(Blocked::UnknownUnit)?;
            if u.owner != Owner::Civ(civ) {
                return Err(Blocked::NotYours);
            }
            match rule {
                R::Clear => Ok(()),
                R::AutoDefend { radius } => {
                    if u.unit_type.is_civilian() {
                        return Err(Blocked::CivilianCannotAttack);
                    }
                    if *radius == 0 || *radius as u32 > MAX_DEFEND_RADIUS {
                        return Err(Blocked::OutOfBounds {
                            min: 1,
                            max: MAX_DEFEND_RADIUS,
                        });
                    }
                    Ok(())
                }
                R::Retreat { ratio_bps } => {
                    if u.unit_type.is_civilian() {
                        return Err(Blocked::CivilianCannotAttack);
                    }
                    if !(1_000..=100_000).contains(ratio_bps) {
                        return Err(Blocked::OutOfBounds {
                            min: 1_000,
                            max: 100_000,
                        });
                    }
                    Ok(())
                }
                R::Patrol { route } => {
                    if route.is_empty() || route.len() > crate::orders::MAX_PATROL {
                        return Err(Blocked::OutOfBounds {
                            min: 1,
                            max: crate::orders::MAX_PATROL as u32,
                        });
                    }
                    if route
                        .iter()
                        .any(|h| state.map.tile(*h).is_none_or(|t| !t.terrain.is_passable()))
                    {
                        return Err(Blocked::Impassable);
                    }
                    Ok(())
                }
                R::QueueRepeat { .. } | R::AutoPurchase { .. } => Err(Blocked::WrongStandingTarget),
            }
        }
        T::City(id) => {
            let c = state
                .cities
                .get(id as usize)
                .filter(|c| c.alive)
                .ok_or(Blocked::UnknownCity)?;
            if c.owner != Some(civ) {
                return Err(Blocked::NotYours);
            }
            match rule {
                R::Clear | R::QueueRepeat { .. } => Ok(()),
                R::AutoPurchase { max_gold } if *max_gold > MAX_AUTO_PURCHASE => {
                    Err(Blocked::OutOfBounds {
                        min: 0,
                        max: MAX_AUTO_PURCHASE,
                    })
                }
                R::AutoPurchase { .. } => Ok(()),
                _ => Err(Blocked::WrongStandingTarget),
            }
        }
    }
}

#[cfg(test)]
mod blocked_codes {
    use super::Blocked::{self, *};
    use super::BLOCKED_NAMES;
    use crate::tech::Tech;

    /// One of each variant. The exhaustive match below stops compiling when a
    /// variant is added, so this list, `code()` and `BLOCKED_NAMES` cannot drift.
    fn every_variant() -> [Blocked; BLOCKED_NAMES.len()] {
        let z = 0;
        [
            UnknownUnit,
            UnknownCity,
            UnknownCiv,
            NotYours,
            SameCiv,
            NotASettler,
            Impassable,
            ForeignTerritory,
            TooCloseToCity {
                distance: z,
                min: z,
            },
            TooCloseToCityState {
                distance: z,
                min: z,
            },
            InProtectedZone,
            ProtectedCapital { civ: 0, until: 0 },
            AlreadyBuilt,
            AlreadyQueued,
            NeedsTech(Tech::Agriculture),
            StarGateInAnotherCity,
            NeedsPreviousStage,
            InvalidTroopCount,
            NeedsPop { need: z, have: z },
            AlreadyResearched,
            NotAtPeace,
            InTruce { until: 0 },
            AlreadyAtWar,
            NotAtWar,
            UnderNap,
            Allied,
            NoProposal,
            BondTooSmall { min: z },
            NotEnoughGold { need: z, have: z },
            AllianceFull { cap: z },
            AlreadyInAlliance,
            AllianceLeaving,
            CivilianCannotAttack,
            OutOfRange {
                distance: z,
                range: z,
            },
            TargetProtected,
            TargetNotHostile,
            TargetGone,
            Frozen,
            NothingToSell,
            OverCap { cap: z },
            WrongStandingTarget,
            OutOfBounds { min: z, max: z },
            BatchRejected,
            WrongOffice,
            NeedsConsent,
            AlreadyPurchased,
            CannotBuyStarGate,
            NothingQueued,
            NotEnoughInfluence,
            NoCounterparty,
            NeedsSpendConsent,
            NotEnoughUsdc,
            ForeignCity,
        ]
    }

    #[allow(dead_code)]
    fn exhaustive(b: Blocked) {
        match b {
            UnknownUnit
            | UnknownCity
            | UnknownCiv
            | NotYours
            | SameCiv
            | NotASettler
            | Impassable
            | ForeignTerritory
            | TooCloseToCity { .. }
            | TooCloseToCityState { .. }
            | InProtectedZone
            | ProtectedCapital { .. }
            | AlreadyBuilt
            | AlreadyQueued
            | NeedsTech(_)
            | StarGateInAnotherCity
            | NeedsPreviousStage
            | InvalidTroopCount
            | NeedsPop { .. }
            | AlreadyResearched
            | NotAtPeace
            | InTruce { .. }
            | AlreadyAtWar
            | NotAtWar
            | UnderNap
            | Allied
            | NoProposal
            | BondTooSmall { .. }
            | NotEnoughGold { .. }
            | AllianceFull { .. }
            | AlreadyInAlliance
            | AllianceLeaving
            | CivilianCannotAttack
            | OutOfRange { .. }
            | TargetProtected
            | TargetNotHostile
            | TargetGone
            | Frozen
            | NothingToSell
            | OverCap { .. }
            | WrongStandingTarget
            | OutOfBounds { .. }
            | BatchRejected
            | WrongOffice
            | NeedsConsent
            | AlreadyPurchased
            | CannotBuyStarGate
            | NothingQueued
            | NotEnoughInfluence
            | NoCounterparty
            | NeedsSpendConsent
            | NotEnoughUsdc
            | ForeignCity => {}
        }
    }

    #[test]
    fn codes_are_positions_and_names_match_variants() {
        for (i, b) in every_variant().iter().enumerate() {
            // Codes are recorded in hashed state (`Skip`): they are the variant
            // positions and must never be renumbered.
            assert_eq!(b.code() as usize, i, "{b:?}");
            let debug = alloc::format!("{b:?}");
            let name = debug.split(['(', ' ', '{']).next().unwrap();
            assert_eq!(BLOCKED_NAMES[i], name);
        }
    }
}

#[cfg(test)]
mod city_entry {
    use super::{enter, Blocked};
    use crate::genesis::{nation_entries, new_season};
    use crate::params::{Preset, Ruleset};

    /// A city tile is entered only by capture (§7.3): not by another civ, even
    /// at war and past protection, nor by anyone once it is a Free City.
    #[test]
    fn only_the_owner_enters_a_city_tile() {
        let rules = Ruleset::new(Preset::Blitz);
        let mut s = new_season(&rules, &[3; 32], &[4; 32], &nation_entries(6)).unwrap();
        s.tick = 61; // protection over
        let cap = s.civs[0].capital.unwrap() as usize;
        let hex = s.cities[cap].hex;
        assert_eq!(enter(&s, &rules, Some(0), hex), Ok(()));
        assert_eq!(enter(&s, &rules, Some(1), hex), Err(Blocked::ForeignCity));
        assert_eq!(enter(&s, &rules, None, hex), Err(Blocked::ForeignCity));
        s.cities[cap].owner = None; // Free City
        assert_eq!(enter(&s, &rules, Some(0), hex), Err(Blocked::ForeignCity));
        let cs = s.city_states[0].hex;
        assert_eq!(enter(&s, &rules, Some(0), cs), Err(Blocked::ForeignCity));
    }
}
