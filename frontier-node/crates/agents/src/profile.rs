//! Player archetypes and how they play, copied field for field from
//! `frontier-sim/src/model.rs` (I-36; the simulator is normative). The
//! test `tests/profile_equality.rs` compiles the simulator's file next to
//! this one and compares every field of every archetype, so a change on
//! either side fails the build until both agree.
//!
//! [`Mix`] and [`roster`] deal a population the way the simulator's
//! `Sim::make_agents` does (the human archetype shares, the scripted-bot
//! share, stratified factions, the day-0 join spike then a uniform spread),
//! with two M1 changes, both listed in the W3-E notes: the join spread is
//! over the season's own join days (the simulator's 21 of 28 are more than a
//! 7-day local season has), and no Shades (M3).

use crate::persona::Persona;
use crate::rng::Rng;

/// Player archetypes (design §2.7 and the M0 brief).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Arch {
    /// Pays and founds a holding, then never plays (passive or Sybil wallet).
    Idle,
    /// 15 minutes, 3 days a week.
    Casual,
    /// 30 minutes every day.
    Daily,
    /// 1–2 hours a day, good decisions.
    Skilled,
    /// ~4 hours a day, best decisions (the "whale, one wallet" of §2.7).
    VerySkilled,
    /// Scripted wallet at the public SDK's default strategy: acts every
    /// hour within the action bucket, never withholds a reveal.
    Bot,
}

pub const ARCHS: [Arch; 6] = [
    Arch::Idle,
    Arch::Casual,
    Arch::Daily,
    Arch::Skilled,
    Arch::VerySkilled,
    Arch::Bot,
];

impl Arch {
    pub fn name(self) -> &'static str {
        match self {
            Arch::Idle => "idle wallet",
            Arch::Casual => "casual 3x/week",
            Arch::Daily => "daily 30 min",
            Arch::Skilled => "skilled 1-2 h",
            Arch::VerySkilled => "very skilled 4 h",
            Arch::Bot => "scripted bot",
        }
    }
    pub fn idx(self) -> usize {
        self as usize
    }
    /// A short machine name (reports, CLI).
    pub fn key(self) -> &'static str {
        match self {
            Arch::Idle => "idle",
            Arch::Casual => "casual",
            Arch::Daily => "daily",
            Arch::Skilled => "skilled",
            Arch::VerySkilled => "very_skilled",
            Arch::Bot => "bot",
        }
    }
}

/// How an archetype plays. [sim]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    /// Probability of playing on a given day.
    pub day_p: f64,
    /// Sessions on a day played.
    pub sessions: u32,
    /// Actions per session (the per-wallet bucket is 30/h, burst 60).
    pub actions: u32,
    /// Decision quality 0..1: estimate noise, stance play, build order.
    pub q: f64,
    /// Probability of looking for a fight in a session.
    pub aggression: f64,
    /// Share of surplus goods pledged to the Engine and research.
    pub pledge: f64,
    /// Probability of taking the term's Mandate when online.
    pub mandate: f64,
    /// Probability of checking a building's payback and saving for the
    /// next tier before buying (economic care; the SDK does it always).
    pub thrift: f64,
    /// Probability that a committed posture or a march goes unrevealed
    /// (tlock auto-reveal is the default, so this is small).
    pub withhold: f64,
}

pub fn profile(a: Arch) -> Profile {
    match a {
        Arch::Idle => Profile {
            day_p: 0.0,
            sessions: 0,
            actions: 0,
            q: 0.0,
            aggression: 0.0,
            pledge: 0.0,
            mandate: 0.0,
            thrift: 0.0,
            withhold: 0.0,
        },
        Arch::Casual => Profile {
            day_p: 3.0 / 7.0,
            sessions: 1,
            actions: 15,
            q: 0.3,
            aggression: 0.15,
            pledge: 0.10,
            mandate: 0.2,
            thrift: 0.4,
            withhold: 0.02,
        },
        Arch::Daily => Profile {
            day_p: 1.0,
            sessions: 1,
            actions: 30,
            q: 0.5,
            aggression: 0.3,
            pledge: 0.15,
            mandate: 0.5,
            thrift: 0.6,
            withhold: 0.01,
        },
        Arch::Skilled => Profile {
            day_p: 1.0,
            sessions: 2,
            actions: 45,
            q: 0.75,
            aggression: 0.5,
            pledge: 0.20,
            mandate: 0.8,
            thrift: 0.85,
            withhold: 0.005,
        },
        Arch::VerySkilled => Profile {
            day_p: 1.0,
            sessions: 4,
            actions: 60,
            q: 0.9,
            aggression: 0.6,
            pledge: 0.20,
            mandate: 0.95,
            thrift: 0.95,
            withhold: 0.0,
        },
        Arch::Bot => Profile {
            day_p: 1.0,
            sessions: 24,
            actions: 30,
            q: 0.6,
            aggression: 0.5,
            pledge: 0.20,
            mandate: 1.0,
            thrift: 1.0,
            withhold: 0.0,
        },
    }
}

/// Bells per game day (kernel `travel::BELLS_PER_DAY`).
pub const BELLS_PER_DAY: u32 = permutation_rules::frontier::travel::BELLS_PER_DAY;

/// The population and its join schedule (the simulator's `Config`
/// defaults: `human_mix`, `bot_share`, `day0_share`).
#[derive(Clone, Debug, PartialEq)]
pub struct Mix {
    /// Human archetype shares: idle, casual, daily, skilled, very skilled.
    pub human_mix: [f64; 5],
    /// Share of all wallets that are scripted bots.
    pub bot_share: f64,
    /// Share of wallets joining on day 0; the rest join uniformly on days
    /// `1..=last_join_day`.
    pub day0_share: f64,
    /// The simulator spreads late joins over days 1..=21 of 28; a local
    /// season spreads them over its own join days.
    pub last_join_day: u32,
    /// Adversarial personas: agents per persona (`None`: the default rule
    /// of [`Mix::personas_for`]; `Some(0)`: off).
    pub persona_count: Option<u32>,
}

impl Mix {
    /// The simulator's default mix (`frontier-sim/src/config.rs`).
    pub const SIM_DEFAULT: Mix = Mix {
        human_mix: [0.15, 0.45, 0.30, 0.09, 0.01],
        bot_share: 0.05,
        day0_share: 0.6,
        last_join_day: 21,
        persona_count: None,
    };

    /// The simulator's mix with the join spread over a `days`-day season's
    /// join days (the last day admits no late joiner: `days − 1`, at least 1).
    pub fn for_season_days(days: u32) -> Mix {
        Mix {
            last_join_day: days.saturating_sub(1).clamp(1, 21),
            ..Mix::SIM_DEFAULT
        }
    }

    /// Agents per persona for `n` agents: §8.6 allows each persona at most
    /// 1% of the bots, so `n / 100` is the ceiling; the default is half of
    /// it (at least 1 once `n ≥ 100`): 5 each at 1,000 bots, 1 each at 100.
    pub fn personas_for(&self, n: usize) -> u32 {
        let cap = (n / 100) as u32;
        match self.persona_count {
            Some(c) => c.min(cap),
            None => (n as u32 / 200).clamp(1, cap.max(1)).min(cap),
        }
    }
}

/// One agent of a roster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentSpec {
    pub index: u32,
    pub arch: Arch,
    pub faction: u8,
    pub join_day: u32,
    pub join_bell: u32,
    pub persona: Option<Persona>,
}

impl AgentSpec {
    pub fn profile(&self) -> Profile {
        profile(self.arch)
    }
}

/// Deals `n` agents for `seed` the way `Sim::make_agents` does: archetypes
/// by the mix (rounded cumulatively, the last human share takes the rest),
/// then the bots; a shuffled order; factions stratified (every faction the
/// same archetype mix: the k-th agent of an archetype joins faction
/// `k mod 6`); join day 0 with `day0_share`, else uniform over
/// `1..=last_join_day`; join bell uniform in the day. Personas are then
/// given to `personas_for(n)` agents each, drawn from a separate stream
/// among the non-idle agents, and join in the first 12 bells of day 0 so a
/// one-day run observes every persona.
///
/// The rules are the simulator's, the draws are not bit-identical to it:
/// `make_agents` interleaves its stake and fee draws in the same stream.
pub fn roster(n: usize, seed: u64, mix: &Mix) -> Vec<AgentSpec> {
    let mut rng = Rng::new(seed);
    let n_bots = (n as f64 * mix.bot_share).round() as usize;
    let humans = n.saturating_sub(n_bots);
    let mut archs: Vec<Arch> = Vec::with_capacity(n);
    let mix_sum: f64 = mix.human_mix.iter().sum();
    let mut acc = 0.0;
    let mut given = 0usize;
    for (i, share) in mix.human_mix.iter().enumerate() {
        acc += share / mix_sum;
        let upto = if i == 4 {
            humans
        } else {
            ((acc * humans as f64).round() as usize).min(humans)
        };
        for _ in given..upto {
            archs.push(ARCHS[i]);
        }
        given = upto.max(given);
    }
    archs.extend(std::iter::repeat_n(Arch::Bot, n_bots));
    let mut order: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = rng.below(i as u64 + 1) as usize;
        order.swap(i, j);
    }
    let mut by_arch = [0u64; 6];
    let mut out: Vec<Option<AgentSpec>> = vec![None; n];
    let last = mix.last_join_day.max(1);
    for &i in &order {
        let arch = archs[i];
        let c = &mut by_arch[arch.idx()];
        let faction = (*c % 6) as u8;
        *c += 1;
        let join_day = if rng.chance(mix.day0_share) {
            0
        } else {
            1 + rng.below(last as u64) as u32
        };
        let join_bell = join_day * BELLS_PER_DAY + rng.below(BELLS_PER_DAY as u64) as u32;
        out[i] = Some(AgentSpec {
            index: i as u32,
            arch,
            faction,
            join_day,
            join_bell,
            persona: None,
        });
    }
    let mut out: Vec<AgentSpec> = out.into_iter().map(|a| a.expect("dealt")).collect();
    let per = mix.personas_for(n);
    if per > 0 {
        let mut pr = Rng::fork(seed, 0x9E25_0BA5);
        let mut pool: Vec<usize> = (0..n).filter(|&i| out[i].arch != Arch::Idle).collect();
        for p in Persona::ALL {
            for _ in 0..per {
                if pool.is_empty() {
                    break;
                }
                let k = pr.below(pool.len() as u64) as usize;
                let i = pool.swap_remove(k);
                let a = &mut out[i];
                a.persona = Some(p);
                a.join_day = 0;
                a.join_bell = pr.below(12) as u32;
            }
        }
    }
    out
}

/// Archetype counts of a roster (`ARCHS` order).
pub fn arch_counts(r: &[AgentSpec]) -> [usize; 6] {
    let mut c = [0; 6];
    for a in r {
        c[a.arch.idx()] += 1;
    }
    c
}
