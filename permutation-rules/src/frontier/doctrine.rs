//! Faction doctrines (design §4.1, D17; owner decision O5, 2026-09-27).
//!
//! Each of the six factions has one **doctrine**: a tech bias, a unit
//! variant and a civic power. Doctrines are asymmetric, but **no doctrine
//! multiplies a scored fact** (O5). The faction index (§5.6) scores:
//!
//! | Path | Scored facts |
//! |---|---|
//! | Dominion | March-control bells, captures |
//! | Prosperity | production |
//! | Knowledge | techs researched, science pledged |
//! | Concord | Engine contribution, treaty-days kept |
//!
//! Per-capita indices of large factions vary by only ~1.4% between seasons
//! [sim], so any multiplier on one of these facts (the draft table's +10%
//! food, +10% ore, +20% science, ×1.2 Knowledge weight, cheaper settlers,
//! doubled Frontier Grant) wins almost every season: Lumen won 99.5% of 600
//! simulated seasons with the draft table [sim, M0 §E1]. So [`Doctrine`]
//! has **no field** that scales production, science, a path weight, Engine
//! pledges, settler cost or grants. A doctrine changes only how facts are
//! earned: combat (a drilled stance, the arrival bell), time (marches,
//! caravans, the war horn), costs that are not facts (walls, troop upkeep),
//! logistics (supply) and siege rules.
//!
//! The magnitudes are tuned in the host simulator (`frontier-sim
//! doctrines`) so that every doctrine wins 16.7% ± 2 points of seasons,
//! and a reduced harness gates the table in CI (`frontier-sim` test
//! `doctrine_balance_gate`). Parts marked *not simulated* have no
//! mechanism in the simulator yet (Bourse, caravans, Waystones, research
//! tree, War, heartland sieges); they are kept small and must be re-checked
//! when their mechanism lands.

use super::siege::required_bells;
use super::stance::{Posture, Stance};
use crate::fixed::{Bps, MilliTroops, BPS_ONE};
use crate::units::UnitType;

/// Doctrine of faction `f` (faction ids 0..6 are doctrines A..F).
pub const fn of_faction(f: u8) -> Option<&'static Doctrine> {
    if (f as usize) < DOCTRINES.len() {
        Some(&DOCTRINES[f as usize])
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Doctrine {
    /// "A Wardens of Stone" … "F Iron".
    pub name: &'static str,

    // ---- unit variant
    /// Base unit of the doctrine's host variant.
    pub unit: UnitType,
    /// The stance the variant is drilled in, and the damage it deals (bps)
    /// when it fights in that stance.
    pub drill: Option<(Stance, Bps)>,
    /// Road building speed (bps; Engineers build roads ×2). *Not simulated
    /// directly*; the simulator's proxy is `travel_bps`.
    pub road_build_bps: Bps,

    // ---- tech bias
    /// Damage dealt on the bell a host arrives (bps).
    pub arrival_bps: Bps,
    /// March time (bps, after cavalry and roads).
    pub travel_bps: Bps,
    /// Troop upkeep (bps).
    pub upkeep_bps: Bps,
    /// Wall build cost (bps).
    pub wall_cost_bps: Bps,
    /// Supply attrition out of supply range (bps of the 1% a bell).
    pub attrition_bps: Bps,
    /// Caravan speed (bps of travel time). *Not simulated.*
    pub caravan_time_bps: Bps,
    /// Bourse fee (bps of volume; 0 = the default fee). *Not simulated.*
    pub bourse_fee_bps: Bps,

    // ---- civic power
    /// Extra bells a siege needs against its heartland holdings.
    /// *Not simulated* (heartland sieges need War).
    pub heartland_siege_extra_bells: u32,
    /// Its war horn, in bells (default `clash::WAR_HORN_BELLS`). *Not
    /// simulated* (no War decrees in the simulator).
    pub war_horn_bells: u32,
    /// Free Cities it captures keep their walls.
    pub keeps_walls_on_capture: bool,
    /// Extra Waystones per City. *Not simulated.*
    pub extra_waystones: u8,
    /// Supply range in provinces beyond the default 3.
    pub supply_range_bonus: u8,
    /// Explore also reveals the adjacent tiles. *Not simulated.*
    pub survey_explore: bool,
}

/// No doctrine: every knob neutral.
pub const NEUTRAL: Doctrine = Doctrine {
    name: "none",
    unit: UnitType::Spearman,
    drill: None,
    road_build_bps: BPS_ONE,
    arrival_bps: BPS_ONE,
    travel_bps: BPS_ONE,
    upkeep_bps: BPS_ONE,
    wall_cost_bps: BPS_ONE,
    attrition_bps: BPS_ONE,
    caravan_time_bps: BPS_ONE,
    bourse_fee_bps: 0,
    heartland_siege_extra_bells: 0,
    war_horn_bells: super::clash::WAR_HORN_BELLS,
    keeps_walls_on_capture: false,
    extra_waystones: 0,
    supply_range_bonus: 0,
    survey_explore: false,
};

/// The Season 1 doctrine table (rules v10), faction 0..6 = A..F.
///
/// Tuned in `frontier-sim` (see `scratchpad/frontier/m0b/sim/DOCTRINES.md`)
/// from the M0 proposal (the draft table minus its multipliers on scored
/// facts). Changes from the design §4.1 draft:
/// * A: unchanged (Pikes drilled in Brace, walls −10%, heartland +12).
/// * B: unchanged (light cavalry, caravans, Bourse fee, Waystone).
/// * C: the arrival-bell bonus and the Assault drill are smaller.
/// * D: "+10% food, cheaper Hamlets" → Foraging (half supply attrition);
///   "Frontier Grant doubled" → Long supply lines (+1 province of supply).
/// * E: "+1 research focus slot" → Cartography (marches take ×0.75 the
///   time; the simulator's proxy for the Engineers' roads, as in M0);
///   "science pledges ×1.2 toward Knowledge" → survey charters (Explore
///   reveals the adjacent tiles).
/// * F: "ore +10%" dropped. Its heavy cavalry is fielded from the Horseman
///   line, not the tier-2 Knight: with Knights, Iron sat 0.17–0.30% of
///   index below the mean in every tuning run whatever its upkeep (×0.5)
///   or muster discount (×0.25), and pulled its neighbours off balance
///   [sim]; from the Horseman line all six doctrines land in the band.
pub const DOCTRINES: [Doctrine; 6] = [
    Doctrine {
        name: "A Wardens of Stone",
        unit: UnitType::Spearman,
        drill: Some((Stance::Brace, 11_000)),
        wall_cost_bps: 9_000,
        heartland_siege_extra_bells: 12,
        ..NEUTRAL
    },
    Doctrine {
        name: "B Tide",
        unit: UnitType::Horseman,
        caravan_time_bps: 8_000,
        bourse_fee_bps: 100,
        extra_waystones: 1,
        ..NEUTRAL
    },
    Doctrine {
        name: "C Ember",
        unit: UnitType::Spearman,
        drill: Some((Stance::Assault, 10_500)),
        arrival_bps: 10_500,
        war_horn_bells: 24,
        ..NEUTRAL
    },
    Doctrine {
        name: "D Verdant",
        unit: UnitType::Spearman,
        drill: Some((Stance::Flank, 10_500)),
        attrition_bps: 5_000,
        supply_range_bonus: 1,
        ..NEUTRAL
    },
    Doctrine {
        name: "E Lumen",
        unit: UnitType::Spearman,
        road_build_bps: 20_000,
        travel_bps: 7_500,
        survey_explore: true,
        ..NEUTRAL
    },
    Doctrine {
        name: "F Iron",
        unit: UnitType::Horseman,
        upkeep_bps: 9_000,
        keeps_walls_on_capture: true,
        ..NEUTRAL
    },
];

/// Bounds every doctrine knob must stay inside: a doctrine is a flavour,
/// not a power level. Checked by [`Doctrine::validate`] and in CI.
pub mod bounds {
    use crate::fixed::Bps;
    /// Combat multipliers (drill, arrival): at most +15%.
    pub const COMBAT_MAX_BPS: Bps = 11_500;
    /// Time and cost multipliers: between ×0.5 and ×1.
    pub const DISCOUNT_MIN_BPS: Bps = 5_000;
    /// Heartland siege extra bells.
    pub const SIEGE_EXTRA_MAX: u32 = 24;
    /// War horn: at least 24 bells.
    pub const WAR_HORN_MIN: u32 = 24;
    pub const SUPPLY_BONUS_MAX: u8 = 1;
    pub const WAYSTONES_MAX: u8 = 1;
    /// Bourse fee: at most 2%.
    pub const BOURSE_FEE_MAX_BPS: Bps = 200;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoctrineError {
    /// A combat multiplier outside `[1, bounds::COMBAT_MAX_BPS]`.
    Combat,
    /// A time or cost multiplier outside `[bounds::DISCOUNT_MIN_BPS, 1]`
    /// (road building: `[1, 2]`).
    Discount,
    /// A civic knob outside its bound.
    Civic,
    /// The unit variant is not a fighting host unit.
    Unit,
    /// A drill in Hold (Hold is the public default, never a drill).
    HoldDrill,
    /// The table does not give every doctrine a tech bias, a unit variant
    /// and a civic power, or two doctrines are identical.
    Shape,
}

impl Doctrine {
    /// Damage multiplier (bps) for `Fighter::dealt_bps`: the drill applies
    /// when the host fights in its drilled stance (not in Disarray), the
    /// arrival bonus on the bell it arrives.
    pub const fn dealt_bps(&self, posture: Posture, arrival: bool) -> Bps {
        let mut m = BPS_ONE as u64;
        if let (Some((s, bps)), Posture::Stance(p)) = (self.drill, posture) {
            if s as u8 == p as u8 {
                m = m * bps as u64 / BPS_ONE as u64;
            }
        }
        if arrival {
            m = m * self.arrival_bps as u64 / BPS_ONE as u64;
        }
        m as Bps
    }

    /// March time in seconds after the doctrine's travel bias.
    pub const fn travel_secs(&self, secs: u64) -> u64 {
        secs * self.travel_bps as u64 / BPS_ONE as u64
    }

    /// Troop upkeep after the doctrine's upkeep bias.
    pub const fn upkeep(&self, per_hour: i64) -> i64 {
        per_hour * self.upkeep_bps as i64 / BPS_ONE as i64
    }

    /// Wall cost after the doctrine's masonry bias.
    pub const fn wall_cost(&self, cost: i64) -> i64 {
        cost * self.wall_cost_bps as i64 / BPS_ONE as i64
    }

    /// Supply range in provinces (`base` is the default, 3).
    pub const fn supply_range(&self, base: u32) -> u32 {
        base + self.supply_range_bonus as u32
    }

    /// Supply attrition for `bells` out of supply: the host kernel's
    /// `host::supply_attrition` result, of which the doctrine keeps back
    /// `1 − attrition_bps` of the loss.
    pub const fn attrition(&self, troops: MilliTroops, after_kernel: MilliTroops) -> MilliTroops {
        let lost = troops.saturating_sub(after_kernel) as u64;
        let lost = lost * self.attrition_bps as u64 / BPS_ONE as u64;
        troops - lost as MilliTroops
    }

    /// Bells a siege needs against a holding of this doctrine's faction
    /// (`heartland`: the holding is in its heartland).
    pub const fn siege_bells(&self, walls: u32, heartland: bool) -> u32 {
        required_bells(
            walls,
            if heartland {
                self.heartland_siege_extra_bells
            } else {
                0
            },
        )
    }

    fn has_tech_bias(&self) -> bool {
        self.arrival_bps != BPS_ONE
            || self.travel_bps != BPS_ONE
            || self.upkeep_bps != BPS_ONE
            || self.wall_cost_bps != BPS_ONE
            || self.attrition_bps != BPS_ONE
            || self.caravan_time_bps != BPS_ONE
            || self.bourse_fee_bps != 0
    }

    fn has_unit_variant(&self) -> bool {
        self.unit != UnitType::Spearman || self.drill.is_some() || self.road_build_bps != BPS_ONE
    }

    fn has_civic_power(&self) -> bool {
        self.heartland_siege_extra_bells != 0
            || self.war_horn_bells != super::clash::WAR_HORN_BELLS
            || self.keeps_walls_on_capture
            || self.extra_waystones != 0
            || self.supply_range_bonus != 0
            || self.survey_explore
    }

    /// Every knob inside [`bounds`].
    pub fn validate(&self) -> Result<(), DoctrineError> {
        use bounds::*;
        let combat_ok = |b: Bps| (BPS_ONE..=COMBAT_MAX_BPS).contains(&b);
        let discount_ok = |b: Bps| (DISCOUNT_MIN_BPS..=BPS_ONE).contains(&b);
        if let Some((s, b)) = self.drill {
            if s == Stance::Hold {
                return Err(DoctrineError::HoldDrill);
            }
            if !combat_ok(b) {
                return Err(DoctrineError::Combat);
            }
        }
        if !combat_ok(self.arrival_bps) {
            return Err(DoctrineError::Combat);
        }
        if ![
            self.travel_bps,
            self.upkeep_bps,
            self.wall_cost_bps,
            self.attrition_bps,
            self.caravan_time_bps,
        ]
        .into_iter()
        .all(discount_ok)
            || !(BPS_ONE..=2 * BPS_ONE).contains(&self.road_build_bps)
        {
            return Err(DoctrineError::Discount);
        }
        if self.heartland_siege_extra_bells > SIEGE_EXTRA_MAX
            || self.war_horn_bells < WAR_HORN_MIN
            || self.war_horn_bells > super::clash::WAR_HORN_BELLS
            || self.supply_range_bonus > SUPPLY_BONUS_MAX
            || self.extra_waystones > WAYSTONES_MAX
            || self.bourse_fee_bps > BOURSE_FEE_MAX_BPS
        {
            return Err(DoctrineError::Civic);
        }
        if !super::host::host_unit(self.unit) {
            return Err(DoctrineError::Unit);
        }
        Ok(())
    }
}

/// The whole table: every doctrine valid, each with a tech bias, a unit
/// variant and a civic power, no two alike.
pub fn validate_table(t: &[Doctrine; 6]) -> Result<(), DoctrineError> {
    for (i, d) in t.iter().enumerate() {
        d.validate()?;
        if !(d.has_tech_bias() && d.has_unit_variant() && d.has_civic_power()) {
            return Err(DoctrineError::Shape);
        }
        if t[..i].iter().any(|e| e == d || e.name == d.name) {
            return Err(DoctrineError::Shape);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_season_table_is_valid() {
        assert_eq!(validate_table(&DOCTRINES), Ok(()));
        assert_eq!(NEUTRAL.validate(), Ok(()));
        for f in 0..6u8 {
            assert!(of_faction(f).is_some());
        }
        assert!(of_faction(6).is_none());
    }
}
