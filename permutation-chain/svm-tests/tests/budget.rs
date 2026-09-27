//! Compute and heap budgets on the SBF build, at the worst shapes the rules
//! and caps allow, each under the budget the client sends it with:
//!
//! * heavy instructions (1.4M CU, 256 KiB) need at most `CU_CEILING` and
//!   `HEAP_CEILING` in every smallest part the crank really sends (the
//!   ResolveTick parts are the consecutive `crank_stops()`, read from
//!   `ticks.mjs`; LogTickInput is one chunk);
//! * player transactions (128 KiB frame, default CU) at most `MEDIUM_CU`
//!   and `MEDIUM_HEAP_CEILING`;
//! * Light instructions (no budget instructions) at most `LIGHT_CU`, and
//!   they land in the default 32 KiB frame.
//!
//! Numbers are printed (`--nocapture`); with the seeded bots they reproduce
//! for a given tree.

use permutation_chain_svm_tests::error::ChainError;
use permutation_chain_svm_tests::payout::claim_amount;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use permutation_rules::gov::GovAction;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_signer::Signer;
use std::time::Instant;

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Sends `ixs` with no compute-budget instruction, as the client sends a
/// Light instruction, and asserts the Light ceiling.
#[track_caller]
fn light(c: &mut Chain, label: &str, ixs: Vec<Instruction>, signers: &[&Keypair]) {
    let l = c
        .send_with(Budget::Light, ixs, signers)
        .unwrap_or_else(|f| panic!("{label} does not land in the default frame: {f:#?}"));
    println!("{label}: {} CU (default 32 KiB frame)", l.cu);
    assert!(l.cu <= LIGHT_CU, "{label}: {} CU over {LIGHT_CU}", l.cu);
}

/// `SEASON_MEMBER_CAP` idle members standing for every office in six
/// nations (the creatable presets: Blitz at HEAD): OpenGovernment,
/// LogTickInput and every crank part for three ticks, FinishSeason, and
/// Claim under Light. (The rules seat at most `Ruleset.max_members`, which
/// is the season cap from rules v9 on.)
#[test]
fn idle_at_member_cap() {
    let n: usize = env("MEMBERS", SEASON_MEMBER_CAP as usize);
    let started = Instant::now();
    let mut c = Chain::new();
    let mut s = SeasonFx::create(
        &mut c,
        Params {
            nations: 6,
            ..Params::default()
        },
    );
    for i in 0..n {
        s.register(&mut c, (i % 6) as u16, 0);
    }
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    for (k, cu) in s.genesis(&mut c).into_iter().enumerate() {
        assert!(cu <= CU_CEILING, "GenesisStep {k}: {cu} CU");
    }
    s.seat_all(&mut c);
    let open = c
        .need(&[s.open_ix(&cp)], &[&crank])
        .expect("OpenGovernment lands");
    assert_fits(&format!("{n} members: OpenGovernment"), open, Budget::Heavy);
    s.open(&mut c);
    for t in 0..3 {
        s.close(&mut c);
        s.freeze(&mut c);
        let log = c
            .need(&[s.log_ix(0)], &[&crank])
            .expect("LogTickInput lands");
        assert_fits(&format!("tick {t}: LogTickInput#0"), log, Budget::Heavy);
        s.log_input(&mut c);
        for p in resolve_parts(&mut c, &s, &crank, true) {
            assert_fits(
                &format!("tick {t}: ResolveTick {}->{}", p.from, p.to),
                p.need.unwrap(),
                Budget::Heavy,
            );
        }
    }
    s.fast_forward_to_end(&mut c);
    let fin = c
        .need(&[s.finish_ix(false)], &[&crank])
        .expect("FinishSeason lands");
    assert_fits(&format!("{n} members: FinishSeason"), fin, Budget::Heavy);
    c.send(vec![s.finish_ix(false)], &[&crank])
        .expect("FinishSeason");
    let season = s.season(&c);
    let i = (0..n)
        .rev()
        .find(|i| claim_amount(&season, &s.member(&c, *i)) > 0)
        .expect("someone is paid");
    let w = s.members[i].wallet.insecure_clone();
    light(
        &mut c,
        &format!("{n} members: Claim"),
        vec![s.claim_ix(i, &w.pubkey(), &s.members[i].token)],
        &[&w],
    );
    println!("idle_at_member_cap: {:?}", started.elapsed());
}

/// Ticks whose crank parts and largest player transactions are bisected.
fn measured(tick: u16) -> bool {
    match env("NEED_EVERY", 0u16) {
        0 => [44, 89, 134, 179].contains(&tick),
        k => tick % k == k - 1 || tick == 179,
    }
}

/// The seeded 15-member Blitz bot season: every crank part at ticks 44, 89,
/// 134 and 179 (`NEED_EVERY=k`: every k-th), the largest RevealOrders and
/// SubmitGov of those ticks, and every player transaction's CU all season.
#[test]
fn played_blitz_15() {
    let started = Instant::now();
    let mut c = Chain::new_opts(false);
    let mut bots = BotSeason::new(
        &mut c,
        Params {
            nations: 6,
            ..Params::default()
        },
        &[3, 3, 3, 2, 2, 2],
    );
    let (mut gov, mut commit, mut reveal, mut log) = (0, 0, 0, 0);
    while !bots.done() {
        let tick = bots.state.tick;
        let r = bots.step(&mut c, measured(tick));
        assert_eq!(r.refused, 0, "tick {tick}");
        (gov, commit, reveal, log) = (
            gov.max(r.submit_gov),
            commit.max(r.commit),
            reveal.max(r.reveal),
            log.max(r.log),
        );
        for p in &r.parts {
            if let Some(n) = p.need {
                assert_fits(
                    &format!("tick {tick}: ResolveTick {}->{}", p.from, p.to),
                    n,
                    Budget::Heavy,
                );
            }
        }
        if let Some(n) = r.reveal_need {
            assert_fits(
                &format!("tick {tick}: largest RevealOrders"),
                n,
                Budget::Medium,
            );
        }
        if let Some(n) = r.gov_need {
            assert_fits(
                &format!("tick {tick}: largest SubmitGov"),
                n,
                Budget::Medium,
            );
        }
    }
    println!("player transactions, worst CU: SubmitGov {gov}, CommitOrders {commit}, RevealOrders {reveal}; LogTickInput/CloseCommits {log}");
    for (what, cu) in [
        ("SubmitGov", gov),
        ("CommitOrders", commit),
        ("RevealOrders", reveal),
    ] {
        assert!(cu <= MEDIUM_CU, "{what}: {cu} CU over {MEDIUM_CU}");
    }
    assert!(log <= CU_CEILING);
    println!("played_blitz_15: {:?}", started.elapsed());
}

/// FinishSeason on the world the bots leave at tick 180.
#[test]
fn finish_after_played_season() {
    let mut c = Chain::new_opts(false);
    let mut bots = BotSeason::new(
        &mut c,
        Params {
            nations: 6,
            ..Params::default()
        },
        &[3, 3, 3, 2, 2, 2],
    );
    while !bots.done() {
        bots.step(&mut c, false);
    }
    let crank = bots.s.crank.insecure_clone();
    let fin = c
        .need(&[bots.s.finish_ix(false)], &[&crank])
        .expect("FinishSeason lands");
    assert_fits("FinishSeason after the played season", fin, Budget::Heavy);
}

/// As `finish_after_played_season`, with a 256-member bot season (WP07's
/// heavy fixture; minutes). Opt-in: `SVM_HEAVY=1 run.sh --ignored finish_after_played_season_at_the_cap`.
#[test]
#[ignore = "SVM_HEAVY: a 256-member played season (minutes)"]
fn finish_after_played_season_at_the_cap() {
    if std::env::var("SVM_HEAVY").is_err() {
        eprintln!("SVM_HEAVY unset: skipped");
        return;
    }
    let mut c = Chain::new_opts(false);
    let per = [43, 43, 43, 43, 42, 42];
    let mut bots = BotSeason::new(
        &mut c,
        Params {
            nations: 6,
            ..Params::default()
        },
        &per,
    );
    while !bots.done() {
        let tick = bots.state.tick;
        let r = bots.step(&mut c, tick % 45 == 44);
        for p in &r.parts {
            if let Some(n) = p.need {
                assert_fits(
                    &format!("tick {tick}: ResolveTick {}->{}", p.from, p.to),
                    n,
                    Budget::Heavy,
                );
            }
        }
    }
    let crank = bots.s.crank.insecure_clone();
    let fin = c
        .need(&[bots.s.finish_ix(false)], &[&crank])
        .expect("FinishSeason lands");
    assert_fits(
        "FinishSeason after the 256-member played season",
        fin,
        Budget::Heavy,
    );
}

/// 15 members, 10 idle ticks, then every nation's governance inbox filled
/// to what SubmitGov admits: every member sends one-slot entries with its
/// seated session key until InboxFull (its `gov_quota`, at most
/// `MAX_GOV_PER_SIGNER`; outsiders are refused, WP03), and the commitments
/// closed. The flood stops only on InboxFull, after exactly the
/// `members × min(gov_quota, MAX_GOV_PER_SIGNER)` bound in each nation.
fn flooded(c: &mut Chain) -> SeasonFx {
    let s = SeasonFx::running(
        c,
        Params {
            nations: 6,
            ..Params::default()
        },
        15,
    );
    s.ensure_rolls(c);
    for _ in 0..10 {
        s.play_tick(c);
    }
    let crank = s.crank.insecure_clone();
    let mut entries = 0;
    for civ in 0..6u16 {
        let mut here = 0;
        let n = s.nation(c, civ as usize);
        for seat in &n.roll {
            let k = s.members[seat.member as usize].session.insecure_clone();
            loop {
                let ix =
                    s.submit_gov_ix(&k.pubkey(), civ, seat.member, GovAction::Stand { roles: 1 });
                let r = c.send(vec![ix], &[&crank, &k]);
                if r.is_err() {
                    // Only a full quota ends a member's flood.
                    assert_err(r, ChainError::InboxFull);
                    break;
                }
                here += 1;
            }
        }
        let bound = n.roll.len() * (n.gov_quota as usize).min(MAX_GOV_PER_SIGNER);
        assert!(bound > 0);
        assert_eq!(here, bound, "nation {civ}: the flood wrote {here} entries");
        entries += here;
    }
    println!("{entries} governance entries across the 6 inboxes");
    s.close(c);
    s
}

/// LogTickInput's first chunk decodes every nation account with a full inbox.
#[test]
fn inbox_full_quota_log_input() {
    let mut c = Chain::new();
    let s = flooded(&mut c);
    let crank = s.crank.insecure_clone();
    s.freeze(&mut c);
    let log = c
        .need(&[s.log_ix(0)], &[&crank])
        .expect("LogTickInput lands");
    assert_fits("full inboxes: LogTickInput#0", log, Budget::Heavy);
}

/// The crank's parts of the flooded tick (green while the LogTickInput
/// assertion waits, so a regression in the parts is still caught). Waits
/// too since OpenGovernment writes each nation's roll (WP03, unit P2): the
/// flood's LogTickInput, which still decodes every nation in full, then
/// runs out of the 256 KiB heap before any part runs; P1's two-pass
/// `pending_input` and SubmitGov's roll check replace the flood.
#[test]
#[ignore = "until WP03/WP07 (unit P1): the flooded LogTickInput runs out of heap with the rolls"]
fn inbox_full_quota_parts() {
    let mut c = Chain::new();
    let s = flooded(&mut c);
    let crank = s.crank.insecure_clone();
    s.log_input(&mut c);
    for p in resolve_parts(&mut c, &s, &crank, true) {
        assert_fits(
            &format!("full inboxes: ResolveTick {}->{}", p.from, p.to),
            p.need.unwrap(),
            Budget::Heavy,
        );
    }
}

/// Every Light instruction at its worst shape, sent with no compute-budget
/// instruction exactly as the client sends it.
#[test]
fn light_worst_shapes() {
    let mut c = Chain::new().with_magicblock();
    // Season A: the most AIs (one seat is left for people, WP07),
    // registering, with a deposit.
    let civs: Vec<u16> = (0..(SEASON_MEMBER_CAP - 1) as u16).map(|i| i % 6).collect();
    let mut a = SeasonFx::with_ai(
        &mut c,
        Params {
            nations: 6,
            deposit: 1_000_000,
            ..Params::default()
        },
        &civs,
        1_000_000,
        4_000_000,
    );
    let admin = a.admin.insecure_clone();
    light(
        &mut c,
        "CreateSeason, 47 AIs",
        vec![a.create_ai_ix(&admin.pubkey(), a.roster_chain())],
        &[&admin],
    );
    let payer = c.funded();
    let p = payer.pubkey();
    light(
        &mut c,
        "AllocWorld",
        vec![a.alloc_world_ix(&p, 0)],
        &[&payer],
    );
    light(
        &mut c,
        "AllocNation",
        vec![a.alloc_nation_ix(&p, 0)],
        &[&payer],
    );
    let batch = 16;
    for i in 0..batch {
        a.register_ai(&mut c, i);
    }
    // RevealRoster: the largest batch a transaction carries (the operator,
    // after the last tick: on a fork where the season runs and chunk 0
    // holds a finished world).
    let ap = admin.pubkey();
    let fits = (1..=batch)
        .rev()
        .find(|k| tx_size(&[a.reveal_ai_ix(&(0..*k).collect::<Vec<_>>())], &ap) <= PACKET_DATA_SIZE)
        .unwrap();
    let ai: Vec<usize> = (0..fits).collect();
    let mut over = c.fork();
    over.edit::<Season>(&a.season, |x| x.status = SeasonStatus::Running);
    let mut chunk0 = over.data(&a.chunks[0]);
    chunk0[..8].copy_from_slice(&WORLD_MAGIC);
    let mut meta = WorldMeta::from_chunk0(&chunk0).unwrap();
    (meta.season_id, meta.finished) = (a.p.id, true);
    meta.write_chunk0(&mut chunk0).unwrap();
    over.set_data(&a.chunks[0], chunk0);
    light(
        &mut over,
        &format!("RevealRoster, {fits} salts"),
        vec![a.reveal_ai_ix(&ai)],
        &[&admin],
    );
    let m = a.ai[0].member.unwrap();
    let w = a.members[m].wallet.insecure_clone();
    light(
        &mut c,
        "UpdateMember",
        vec![a.update_member_ix(&w.pubkey(), m, 0x0f, [u32::MAX; 4])],
        &[&w],
    );
    c.edit::<Season>(&a.season, |x| x.member_count = SEASON_MEMBER_CAP - 1);
    let last = a.new_member(&mut c, 5, a.p.fee + 1_000_000);
    let (lw, ls) = (last.wallet.insecure_clone(), last.session.insecure_clone());
    // Operator-paid, as every join of a season with AIs (WP07).
    let ac = a.crank.insecure_clone();
    light(
        &mut c,
        "Register, the last seat, with a deposit",
        vec![a.register_ix(&last, &ac.pubkey(), 1_000_000)],
        &[&ac, &lw, &ls],
    );

    // Season B: running, six nations.
    let b = SeasonFx::running(
        &mut c,
        Params {
            id: 8,
            nations: 6,
            ..Params::default()
        },
        12,
    );
    let crank = b.crank.insecure_clone();
    let cp = crank.pubkey();
    let mut delegated = c.fork();
    light(
        &mut delegated,
        "Delegate chunk",
        vec![b.delegate_ix(&cp, 5)],
        &[&crank],
    );
    light(
        &mut delegated,
        "Delegate nation",
        vec![b.delegate_ix(&cp, NATION_TARGET + 5)],
        &[&crank],
    );
    // The intents the crank sends: one world chunk, or three nations.
    let three: Vec<u16> = (0..3).map(|k| NATION_TARGET + k).collect();
    light(
        &mut c,
        "CommitPart, one chunk",
        vec![b.part_ix(&cp, vec![1], false)],
        &[&crank],
    );
    light(
        &mut c,
        "CommitPart, three nations",
        vec![b.part_ix(&cp, three.clone(), false)],
        &[&crank],
    );
    light(
        &mut c,
        "AnchorTalk",
        vec![b.anchor_talk_ix(&cp, u16::MAX, u32::MAX, [0xff; 32])],
        &[&crank],
    );
    b.fast_forward_to_end(&mut c);
    light(
        &mut c,
        "UndelegatePart, one chunk",
        vec![b.part_ix(&cp, vec![1], true)],
        &[&crank],
    );
    light(
        &mut c,
        "UndelegatePart, three nations",
        vec![b.part_ix(&cp, three, true)],
        &[&crank],
    );
    c.send(vec![b.finish_ix(false)], &[&crank])
        .expect("FinishSeason");
    // Claim against a payout table of MAX_MEMBERS entries.
    c.edit::<Season>(&b.season, |x| {
        x.payouts = vec![1; MAX_MEMBERS as usize];
        x.member_count = MAX_MEMBERS;
    });
    let w = b.members[11].wallet.insecure_clone();
    light(
        &mut c,
        "Claim, 256 payouts",
        vec![b.claim_ix(11, &w.pubkey(), &b.members[11].token)],
        &[&w],
    );
    let admin = b.admin.insecure_clone();
    let dest = c.token_account(&b.mint, &admin.pubkey(), 0);
    light(
        &mut c,
        "WithdrawOps",
        vec![b.withdraw_ix(&admin.pubkey(), &dest)],
        &[&admin],
    );
}

/// Nations per intent (`MAX_NATIONS_PER_INTENT` in state.rs from WP02).
const NATIONS_PER_INTENT: usize = 3;

/// Intents under Light, as WP02 fixes them. At HEAD, Commit (tag 8) over a
/// six-nation season's 26 accounts (177k CU) and CommitPart and
/// UndelegatePart at every target (188k CU) land, over the Light ceiling.
/// Fixed: Commit is Retired; CommitPart and UndelegatePart over every
/// target are refused (InvalidParams: one world chunk, or at most
/// `NATIONS_PER_INTENT` nations); the largest shapes the program accepts
/// land under `LIGHT_CU`: CommitPart of one chunk and of the most nations,
/// and the whole undelegation in `undelegation_order` (chunks 1..=19 one at
/// a time, the nations `NATIONS_PER_INTENT` at a time, chunk 0 last).
#[test]
#[ignore = "until WP02: intents over every account exceed the Light ceiling (WP02 caps the shape)"]
fn light_intents_at_every_target() {
    let mut c = Chain::new().with_magicblock();
    let b = SeasonFx::running(
        &mut c,
        Params {
            nations: 6,
            ..Params::default()
        },
        12,
    );
    let crank = b.crank.insecure_clone();
    let cp = crank.pubkey();
    assert_err(
        c.send(vec![b.commit_ix(&cp, false)], &[&crank]),
        ChainError::Retired,
    );
    let all = b.all_targets();
    assert_err(
        c.send(vec![b.part_ix(&cp, all.clone(), false)], &[&crank]),
        ChainError::InvalidParams,
    );
    let nations: Vec<u16> = all
        .iter()
        .copied()
        .filter(|t| *t >= NATION_TARGET)
        .collect();
    light(
        &mut c,
        "CommitPart, one chunk",
        vec![b.part_ix(&cp, vec![1], false)],
        &[&crank],
    );
    light(
        &mut c,
        &format!("CommitPart, {NATIONS_PER_INTENT} nations"),
        vec![b.part_ix(&cp, nations[..NATIONS_PER_INTENT].to_vec(), false)],
        &[&crank],
    );
    b.fast_forward_to_end(&mut c);
    let mut order = all;
    order.rotate_left(1); // chunk 0 last
    assert_err(
        c.send(vec![b.part_ix(&cp, order, true)], &[&crank]),
        ChainError::InvalidParams,
    );
    let mut parts: Vec<Vec<u16>> = (1..WORLD_CHUNKS as u16).map(|k| vec![k]).collect();
    parts.extend(nations.chunks(NATIONS_PER_INTENT).map(|n| n.to_vec()));
    parts.push(vec![0]);
    for targets in parts {
        light(
            &mut c,
            &format!("UndelegatePart {targets:?}"),
            vec![b.part_ix(&cp, targets, true)],
            &[&crank],
        );
    }
}

/// Every creatable season shape (Blitz, 2 to 6 nations): each GenesisStep
/// fits the heavy ceilings.
#[test]
fn genesis_every_creatable() {
    for nations in 2..=6u8 {
        let mut c = Chain::new();
        let mut s = SeasonFx::create(
            &mut c,
            Params {
                nations,
                ..Params::default()
            },
        );
        for i in 0..2 * nations as usize {
            s.register(&mut c, (i % nations as usize) as u16, 0);
        }
        let crank = s.crank.insecure_clone();
        c.send(vec![s.start_ix(&crank.pubkey())], &[&crank])
            .expect("StartSeason");
        let mut worst = Need::default();
        let mut steps = 0;
        while s.season(&c).status == SeasonStatus::Genesis {
            let n = c
                .need(&[s.genesis_step_ix(50)], &[&crank])
                .expect("GenesisStep lands");
            assert_fits(
                &format!("{nations} nations: GenesisStep {steps}"),
                n,
                Budget::Heavy,
            );
            worst = worst.max(n);
            c.send(vec![s.genesis_step_ix(50)], &[&crank])
                .expect("GenesisStep");
            steps += 1;
        }
        println!("{nations} nations: {steps} steps, worst {worst}");
    }
}

/// Fixed behaviour (WP13): every season CreateSeason accepts can run its
/// genesis. At HEAD it accepts the Season preset, whose first GenesisStep
/// runs out of the 1.4M CU.
#[test]
fn only_buildable_seasons_are_creatable() {
    for nations in 2..=6u8 {
        let mut c = Chain::new();
        let mut s = SeasonFx::bare(
            &mut c,
            Params {
                preset: 1,
                nations,
                ..Params::default()
            },
        );
        let admin = s.admin.insecure_clone();
        let created = c.send(vec![s.create_ix(&admin.pubkey())], &[&admin]);
        if created.is_err() {
            // Not creatable: fine, but only as `creatable` refuses it.
            assert_err(created, ChainError::InvalidParams);
            continue;
        }
        s.alloc_all(&mut c);
        for i in 0..2 * nations as usize {
            s.register(&mut c, (i % nations as usize) as u16, 0);
        }
        let crank = s.crank.insecure_clone();
        c.send(vec![s.start_ix(&crank.pubkey())], &[&crank])
            .expect("StartSeason");
        while s.season(&c).status == SeasonStatus::Genesis {
            let n = c
                .need(&[s.genesis_step_ix(50)], &[&crank])
                .unwrap_or_else(|f| panic!("Season preset, {nations} nations: GenesisStep {f:#?}"));
            assert_fits("Season preset: GenesisStep", n, Budget::Heavy);
            c.send(vec![s.genesis_step_ix(50)], &[&crank]).unwrap();
        }
    }
}
