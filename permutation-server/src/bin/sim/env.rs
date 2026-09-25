//! The `SIM_*` environment variables, read once (see the list in `main.rs`).

use permutation_rules::{Preset, Ruleset};
use permutation_server::bots::{Persona, PERSONAS};
use std::str::FromStr;

pub struct Env {
    /// `SIM_SET`, applied by [`Env::rules`].
    set: String,
    /// `SIM_PERSONA`, when it names a persona.
    pub persona: Option<Persona>,
    pub treasury: u64,
    pub rotate: Option<u8>,
    pub ai: usize,
    pub home_aware: bool,
    pub bounty: u64,
    /// `SIM_DEBUG="seed:tick"`.
    pub debug: Option<(u32, u16)>,
    pub from: u32,
    pub skips: bool,
    pub equiv: bool,
    pub equiv_k: u8,
    pub equiv_debug: bool,
}

/// A variable parsed as `T`; `None` when unset or unparsable.
fn env_opt<T: FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

/// A variable parsed as `T`, or `default`.
fn env_num<T: FromStr>(name: &str, default: T) -> T {
    env_opt(name).unwrap_or(default)
}

/// Whether a variable is set (to any value).
fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok()
}

impl Env {
    pub fn read() -> Env {
        let persona = std::env::var("SIM_PERSONA")
            .ok()
            .and_then(|name| PERSONAS.iter().copied().find(|p| p.name() == name));
        let debug = std::env::var("SIM_DEBUG").ok().and_then(|d| {
            let (a, b) = d.split_once(':')?;
            Some((a.parse::<u32>().ok()?, b.parse::<u16>().ok()?))
        });
        Env {
            set: std::env::var("SIM_SET").unwrap_or_default(),
            persona,
            treasury: env_num("SIM_TREASURY", 0),
            rotate: env_opt("SIM_ROTATE"),
            ai: env_num("SIM_AI", 1),
            home_aware: env_flag("SIM_HOMEAWARE"),
            bounty: env_num("SIM_BOUNTY", 5_000_000),
            debug,
            from: env_num("SIM_FROM", 0),
            skips: env_flag("SIM_SKIPS"),
            equiv: env_flag("SIM_EQUIV"),
            equiv_k: env_num("SIM_EQUIV_K", 1),
            equiv_debug: env_flag("SIM_EQUIV_DEBUG"),
        }
    }

    /// Blitz ruleset with the `SIM_SET` calibration overrides, e.g.
    /// `SIM_SET="science_techs=4,9,13;prosperity_pop=12,22,32,42,55"`.
    /// Panics on an unknown key or a malformed value.
    pub fn rules(&self) -> Ruleset {
        let mut r = Ruleset::new(Preset::Blitz);
        for part in self.set.split(';').filter(|p| !p.trim().is_empty()) {
            let (k, v) = part.split_once('=').expect("key=values");
            let v: Vec<u32> = v
                .split(',')
                .map(|x| x.trim().parse().expect("number"))
                .collect();
            let set = |dst: &mut [u32]| dst.copy_from_slice(&v[..dst.len()]);
            match k.trim() {
                "hegemony_tiles" => set(&mut r.hegemony_tiles),
                "hegemony_cities" => set(&mut r.hegemony_cities),
                "prosperity_pop" => set(&mut r.prosperity_pop),
                "prosperity_wealth" => set(&mut r.prosperity_wealth),
                "science_techs" => set(&mut r.science_techs),
                "concord_trade" => set(&mut r.concord_trade),
                "concord_partners" => set(&mut r.concord_partners),
                "concord_suzerains" => set(&mut r.concord_suzerains),
                "tier_points" => set(&mut r.tier_points),
                "conquest_min_pop" => r.conquest_min_pop = v[0],
                "peace_pact_min_war" => r.peace_pact_min_war = v[0] as u16,
                "eureka_cost_bps" => r.eureka_cost_bps = v[0],
                "crisis_targets" => r.crisis_targets = v[0] as u8,
                "crisis_pop" => r.crisis_pop = v[0],
                "crisis_loyalty" => r.crisis_loyalty = v[0] as i32,
                "crisis_interval" => r.crisis_interval = v[0] as u16,
                "dark_age_share_bps" => r.dark_age_share_bps = v[0],
                "dark_age_research_bps" => r.dark_age_research_bps = v[0],
                "dark_age_budget" => r.dark_age_budget = v[0] as u16,
                "stalemate_floor_bps" => r.stalemate_floor_bps = v[0],
                "tech_cost_per_city_bps" => r.tech_cost_per_city_bps = v[0],
                "suzerain_lock_ticks" => r.suzerain_lock_ticks = v[0] as u16,
                "ai_home_tick" => r.ai_home_tick = v[0] as u16,
                "bounty_pact_window" => r.bounty_pact_window = v[0] as u16,
                other => panic!("unknown SIM_SET key {other}"),
            }
        }
        r
    }
}
