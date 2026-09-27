//! I-36: `frontier-agents` copies the simulator's archetypes; this test
//! compiles the simulator's own files (`frontier-sim/src/{model,rng,
//! config}.rs`) next to the copy and compares them field for field, draw
//! for draw and share for share. The simulator is normative: when it
//! changes, this test fails until the copy follows.

#[allow(dead_code, unused_imports, clippy::all)]
#[path = "../../../../frontier-sim/src/model.rs"]
mod model;

#[allow(dead_code, unused_imports, clippy::all)]
#[path = "../../../../frontier-sim/src/rng.rs"]
mod sim_rng;

#[allow(dead_code, unused_imports, clippy::all)]
#[path = "../../../../frontier-sim/src/config.rs"]
mod config;

use frontier_agents::profile::{self, Arch, Mix, ARCHS};
use frontier_agents::rng::Rng;

fn sim_arch(a: Arch) -> model::Arch {
    match a {
        Arch::Idle => model::Arch::Idle,
        Arch::Casual => model::Arch::Casual,
        Arch::Daily => model::Arch::Daily,
        Arch::Skilled => model::Arch::Skilled,
        Arch::VerySkilled => model::Arch::VerySkilled,
        Arch::Bot => model::Arch::Bot,
    }
}

#[test]
fn archetypes_equal_the_simulators() {
    assert_eq!(ARCHS.len(), model::ARCHS.len());
    for (i, a) in ARCHS.iter().enumerate() {
        let s = sim_arch(*a);
        assert_eq!(model::ARCHS[i], s, "order");
        assert_eq!(a.idx(), s.idx());
        assert_eq!(a.name(), s.name());
        let (p, q) = (profile::profile(*a), model::profile(s));
        // Field for field (a destructuring pattern: a new field on either
        // side is a compile error here).
        let model::Profile {
            day_p,
            sessions,
            actions,
            q: quality,
            aggression,
            pledge,
            mandate,
            thrift,
            withhold,
        } = q;
        let profile::Profile {
            day_p: d2,
            sessions: s2,
            actions: a2,
            q: q2,
            aggression: g2,
            pledge: p2,
            mandate: m2,
            thrift: t2,
            withhold: w2,
        } = p;
        assert_eq!(
            (
                day_p.to_bits(),
                sessions,
                actions,
                quality.to_bits(),
                aggression.to_bits()
            ),
            (d2.to_bits(), s2, a2, q2.to_bits(), g2.to_bits()),
            "{a:?}"
        );
        assert_eq!(
            (
                pledge.to_bits(),
                mandate.to_bits(),
                thrift.to_bits(),
                withhold.to_bits()
            ),
            (p2.to_bits(), m2.to_bits(), t2.to_bits(), w2.to_bits()),
            "{a:?}"
        );
    }
}

#[test]
fn the_generator_draws_as_the_simulators() {
    for seed in [0u64, 1, 30_001, u64::MAX] {
        let (mut a, mut b) = (Rng::new(seed), sim_rng::Rng::new(seed));
        let (mut fa, mut fb) = (Rng::fork(seed, 77), sim_rng::Rng::fork(seed, 77));
        for _ in 0..1_000 {
            assert_eq!(a.next_u64(), b.next_u64());
            assert_eq!(a.below(21), b.below(21));
            assert_eq!(a.chance(0.6), b.chance(0.6));
            assert_eq!(fa.f64().to_bits(), fb.f64().to_bits());
        }
    }
}

#[test]
fn the_default_mix_is_the_simulators() {
    let c = config::Config::default();
    let m = Mix::SIM_DEFAULT;
    assert_eq!(m.human_mix, c.human_mix);
    assert_eq!(m.bot_share, c.bot_share);
    assert_eq!(m.day0_share, c.day0_share);
    // The simulator spreads late joins over days 1..=21.
    assert_eq!(m.last_join_day, 21);
}
