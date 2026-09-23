//! Combat (§8). Damage is computed from pre-combat counts for every
//! engagement in a tick, then applied simultaneously (§15 phase 5).

use crate::fixed::{Bps, MilliTroops, BPS_ONE};
use crate::params::Ruleset;
use crate::rng::{rand, Seed};
use crate::units::{counters, stats, UnitClass, UnitType};

/// `F[n] = round(n^0.2 × 1000)` for n = 1..=64 (§8.5). Index 0 is unused.
pub const F: [u32; 65] = [
    0, 1000, 1149, 1246, 1320, 1380, 1431, 1476, 1516, 1552, 1585, 1615, 1644, 1670, 1695, 1719,
    1741, 1762, 1783, 1802, 1821, 1838, 1856, 1872, 1888, 1904, 1919, 1933, 1947, 1961, 1974, 1987,
    2000, 2012, 2024, 2036, 2048, 2059, 2070, 2081, 2091, 2102, 2112, 2122, 2132, 2141, 2151, 2160,
    2169, 2178, 2187, 2195, 2204, 2212, 2221, 2229, 2237, 2245, 2253, 2260, 2268, 2275, 2283, 2290,
    2297,
];

/// Modifiers in the fixed application order of §8.2.
pub mod modifier {
    use crate::fixed::Bps;
    pub const COUNTER: Bps = 15_000;
    pub const RANGED_ATTACK: Bps = 8_000;
    pub const MELEE_VS_RANGED_RETALIATION: Bps = 5_000;
    pub const DEFENDER_TERRAIN: Bps = 8_000;
    pub const DEFENDER_FORTIFIED: Bps = 8_700;
    pub const ATTACKER_EXHAUSTED: Bps = 8_500;
    pub const FROM_RIVER: Bps = 8_500;
    pub const WALLS: Bps = 6_667;
    pub const WALLS_ENGINEERING: Bps = 5_000;
    pub const CITY_RETALIATION: Bps = 5_000;
}

/// Core formula (§8.1). `n_a`, `n_b` in milli-troops; `mods` already in §8.2
/// order; `variance_bps` in [9000, 11000]. Returns milli-troops of damage to B.
pub fn damage(
    rules: &Ruleset,
    n_a: MilliTroops,
    str_a: u32,
    n_b: MilliTroops,
    str_b: u32,
    mods: &[Bps],
    variance_bps: Bps,
) -> MilliTroops {
    if n_a == 0 || str_a == 0 || str_b == 0 {
        return 0;
    }
    let n = ((n_a as u64 + n_b as u64) / 1000).clamp(1, 64) as usize;
    let mut x: u64 =
        n_a as u64 * str_a as u64 * rules.combat_k_bps as u64 / BPS_ONE as u64 / str_b as u64;
    x = x * 1000 / F[n] as u64;
    for m in mods {
        x = x * *m as u64 / BPS_ONE as u64;
    }
    x = x * variance_bps as u64 / BPS_ONE as u64;
    x.min(u32::MAX as u64) as MilliTroops
}

/// `v = 9000 + rand(seed, "var", engagement_id ‖ side) mod 2001` (§8.1).
pub fn variance(rules: &Ruleset, seed: &Seed, engagement_id: u32, side: u8) -> Bps {
    let mut id = [0u8; 5];
    id[..4].copy_from_slice(&engagement_id.to_le_bytes());
    id[4] = side;
    rules.variance_min_bps + (rand(seed, b"var", &id) % rules.variance_span as u64) as Bps
}

/// What a side brings to an engagement.
#[derive(Clone, Copy, Debug)]
pub enum Combatant {
    Army {
        unit: UnitType,
        troops: MilliTroops,
    },
    /// Virtual city defence: strength 10, never counters (§8.3).
    City {
        defense: MilliTroops,
    },
}

impl Combatant {
    fn troops(self) -> MilliTroops {
        match self {
            Combatant::Army { troops, .. } => troops,
            Combatant::City { defense } => defense,
        }
    }
    fn strength(self) -> u32 {
        match self {
            Combatant::Army { unit, .. } => stats(unit).strength,
            Combatant::City { .. } => 10,
        }
    }
    fn unit(self) -> Option<UnitType> {
        match self {
            Combatant::Army { unit, .. } => Some(unit),
            Combatant::City { .. } => None,
        }
    }
}

/// Situational facts that select §8.2 modifiers.
#[derive(Clone, Copy, Debug, Default)]
pub struct Situation {
    /// Attacker is ranged and attacking from range 1 or 2.
    pub ranged_attack: bool,
    pub defender_on_rough_terrain: bool,
    pub defender_fortified: bool,
    pub attacker_exhausted: bool,
    pub attacker_on_river: bool,
    pub city_walls: bool,
    pub engineering: bool,
}

/// Damage both ways for one engagement: `(to_defender, to_attacker)`.
pub fn resolve_engagement(
    rules: &Ruleset,
    attacker: Combatant,
    defender: Combatant,
    sit: Situation,
    v_attacker: Bps,
    v_defender: Bps,
) -> (MilliTroops, MilliTroops) {
    use modifier::*;
    let mut a_mods: [Bps; 8] = [0; 8];
    let mut na = 0;
    let push = |mods: &mut [Bps; 8], n: &mut usize, m: Bps| {
        mods[*n] = m;
        *n += 1;
    };
    // Attacker's damage, §8.2 order.
    if let (Some(a), Some(d)) = (attacker.unit(), defender.unit()) {
        if counters(a, d) {
            push(&mut a_mods, &mut na, COUNTER);
        }
    }
    if sit.ranged_attack {
        push(&mut a_mods, &mut na, RANGED_ATTACK);
    }
    if sit.defender_on_rough_terrain {
        push(&mut a_mods, &mut na, DEFENDER_TERRAIN);
    }
    if sit.defender_fortified {
        push(&mut a_mods, &mut na, DEFENDER_FORTIFIED);
    }
    if sit.attacker_exhausted {
        push(&mut a_mods, &mut na, ATTACKER_EXHAUSTED);
    }
    if sit.attacker_on_river {
        push(&mut a_mods, &mut na, FROM_RIVER);
    }
    if matches!(defender, Combatant::City { .. }) && sit.city_walls {
        push(
            &mut a_mods,
            &mut na,
            if sit.engineering {
                WALLS_ENGINEERING
            } else {
                WALLS
            },
        );
    }
    let to_def = damage(
        rules,
        attacker.troops(),
        attacker.strength(),
        defender.troops(),
        defender.strength(),
        &a_mods[..na],
        v_attacker,
    );

    // Defender's retaliation. Ranged attacks receive none (§8.2 #2).
    if sit.ranged_attack {
        return (to_def, 0);
    }
    let mut d_mods: [Bps; 8] = [0; 8];
    let mut nd = 0;
    if let (Some(d), Some(a)) = (defender.unit(), attacker.unit()) {
        if counters(d, a) {
            push(&mut d_mods, &mut nd, COUNTER);
        }
        if stats(d).class == UnitClass::Ranged {
            push(&mut d_mods, &mut nd, MELEE_VS_RANGED_RETALIATION);
        }
    }
    if matches!(defender, Combatant::City { .. }) {
        push(&mut d_mods, &mut nd, CITY_RETALIATION);
    }
    let to_att = damage(
        rules,
        defender.troops(),
        defender.strength(),
        attacker.troops(),
        attacker.strength(),
        &d_mods[..nd],
        v_defender,
    );
    (to_def, to_att)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Preset;

    #[test]
    fn f_table_matches_integer_fifth_root() {
        // F[n]^5 ≈ n × 1000^5; check the rounding is to the nearest integer.
        for n in 1u64..=64 {
            let f = F[n as usize] as u64;
            let target = n * 1_000_000_000_000_000;
            let below = (f * 2 - 1).pow(5); // (f - 0.5)^5 × 32
            let above = (f * 2 + 1).pow(5); // (f + 0.5)^5 × 32
            assert!(below <= target * 32 && target * 32 <= above, "n={n} F={f}");
        }
    }

    #[test]
    fn damage_is_zero_without_attackers() {
        let r = Ruleset::new(Preset::Blitz);
        assert_eq!(damage(&r, 0, 10, 10_000, 10, &[], 10_000), 0);
    }
}
