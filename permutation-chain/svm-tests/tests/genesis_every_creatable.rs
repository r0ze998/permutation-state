//! WP13: every season CreateSeason accepts can finish its genesis. Only
//! `state::creatable` pairs (Blitz, 2..=6 nations) are created; each one's
//! genesis reaches Seating in steps that fit a transaction, whoever picks
//! `work`; StartSeason refuses a season that is not creatable.
//! `GENESIS_SEEDS=n` sets the world seeds per pair (default 8; the design's
//! sweep is 32).

use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::ix::CreateArgs;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use solana_signer::Signer;

/// A regression budget for one GenesisStep (the measured maximum is under
/// 1.0M CU; the client requests 1.4M).
const STEP_CU: u64 = 1_150_000;
/// Steps to Seating (measured maximum 66 at WP13's sweep).
const MAX_STEPS: usize = 120;

fn seeds() -> u8 {
    std::env::var("GENESIS_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8)
}

/// Created from `a` (the season `id` of `a`), with a member per nation.
fn created(c: &mut Chain, a: &CreateArgs) -> SeasonFx {
    let mut s = SeasonFx::bare(
        c,
        Params {
            id: a.season_id,
            preset: a.preset,
            nations: a.nations,
            ..Params::default()
        },
    );
    let admin = s.admin.insecure_clone();
    let a = CreateArgs {
        crank: s.crank.pubkey().to_bytes(),
        ..a.clone()
    };
    c.send(
        vec![s.create_ix_from(&admin.pubkey(), &a, vec![])],
        &[&admin],
    )
    .expect("CreateSeason");
    s.alloc_all(c);
    for civ in 0..a.nations as u16 {
        s.register(c, civ, 0);
    }
    s
}

#[test]
fn create_season_accepts_exactly_the_creatable_pairs() {
    let mut c = Chain::new();
    let mut id = 100;
    for preset in 0..=2u8 {
        for nations in 1..=7u8 {
            id += 1;
            let s = SeasonFx::bare(
                &mut c,
                Params {
                    id,
                    ..Params::default()
                },
            );
            let admin = s.admin.insecure_clone();
            let a = CreateArgs {
                preset,
                nations,
                ..s.create_args()
            };
            let r = c.send(
                vec![s.create_ix_from(&admin.pubkey(), &a, vec![])],
                &[&admin],
            );
            if creatable(preset, nations) {
                let l = r.unwrap_or_else(|f| panic!("({preset}, {nations}): {f:#?}"));
                println!("CreateSeason ({preset}, {nations}): {} CU", l.cu);
                assert_eq!(s.season(&c).nations, nations);
            } else {
                assert_err(r, E::InvalidParams);
            }
        }
    }
}

#[test]
fn genesis_finishes_for_every_creatable_pair() {
    let mut worst = (0u64, 0usize);
    for nations in 2..=6u8 {
        assert!(creatable(PRESET_BLITZ, nations));
        for seed in 0..seeds() {
            let mut c = Chain::new();
            c.advance(seed as i64);
            let id = 1000 + nations as u64 * 100 + seed as u64;
            let bare = SeasonFx::bare(&mut c, Params::default());
            let a = CreateArgs {
                season_id: id,
                nations,
                world_seed: [seed; 32],
                ..bare.create_args()
            };
            let s = created(&mut c, &a);
            let crank = s.crank.insecure_clone();
            c.send(vec![s.start_ix(&crank.pubkey())], &[&crank])
                .expect("StartSeason");
            s.seed(&mut c);
            let mut steps = 0;
            while s.season(&c).status == SeasonStatus::Genesis {
                let l = c
                    .send(vec![s.genesis_step_ix(50)], &[&crank])
                    .unwrap_or_else(|f| panic!("{nations} nations, seed {seed}: {f:#?}"));
                assert!(
                    l.cu <= STEP_CU,
                    "{nations} nations, seed {seed}: a step took {} CU",
                    l.cu
                );
                worst.0 = worst.0.max(l.cu);
                steps += 1;
                assert!(steps <= MAX_STEPS, "{nations} nations, seed {seed}");
            }
            worst.1 = worst.1.max(steps);
            assert_eq!(s.season(&c).status, SeasonStatus::Seating);
        }
    }
    println!(
        "genesis, Blitz 2..=6 nations × {} seeds: worst step {} CU, most steps {}",
        seeds(),
        worst.0,
        worst.1
    );
}

/// `work` is clamped to 1..=MAX_GENESIS_WORK: a caller asking for
/// `u32::MAX` gets the same steps and the same world as the crank's 50,
/// and `work = 0` still steps.
#[test]
fn genesis_work_is_clamped() {
    let run = |work: u32| {
        let mut c = Chain::new();
        let bare = SeasonFx::bare(&mut c, Params::default());
        let a = CreateArgs {
            nations: 4,
            world_seed: [5; 32],
            ..bare.create_args()
        };
        let s = created(&mut c, &a);
        let crank = s.crank.insecure_clone();
        c.send(vec![s.start_ix(&crank.pubkey())], &[&crank])
            .expect("StartSeason");
        s.seed(&mut c);
        let mut steps = 0;
        loop {
            let l = c
                .send(vec![s.genesis_step_ix(work)], &[&crank])
                .expect("GenesisStep, whatever the work");
            steps += 1;
            if s.season(&c).status == SeasonStatus::Seating {
                return (steps, record(&l.logs, b"PS_GENESIS"));
            }
            if work == 0 && steps == 3 {
                // Enough to show a zero step moves the job along.
                return (steps, vec![]);
            }
        }
    };
    let (steps, root) = run(50);
    assert_eq!(run(u32::MAX), (steps, root));
    assert_eq!(run(0).0, 3);
}

/// WP13: StartSeason checks `creatable` again. A Season-preset season (its
/// genesis step needs about 2M CU) never leaves registration.
#[test]
fn start_season_refuses_uncreatable() {
    let mut c = Chain::new();
    let mut s = SeasonFx::create(&mut c, Params::default());
    s.register(&mut c, 0, 0);
    s.register(&mut c, 1, 0);
    // As an older build could have created it (bound to that preset's rules).
    c.edit::<Season>(&s.season, |x| {
        x.preset = PRESET_SEASON;
        x.rules_hash =
            permutation_chain::rules::pinned_ruleset_hash(PRESET_SEASON, x.market).unwrap();
    });
    let crank = s.crank.insecure_clone();
    assert_err(
        c.send(vec![s.start_ix(&crank.pubkey())], &[&crank]),
        E::InvalidParams,
    );
    assert_eq!(s.season(&c).status, SeasonStatus::Registering);
}
