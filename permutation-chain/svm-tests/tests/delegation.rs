//! Delegate, the undelegate callback and the commit instructions, against
//! the recording stand-ins for the delegation and Magic programs. The
//! wind-up itself (UndelegatePart in order, skips, one step per
//! transaction) is `tests/undelegation.rs`.

use permutation_chain::rules::{pinned_ruleset_hash, CHAIN_LOGIC_VERSION, PINNED_RULES_VERSION};
use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::instruction::ChainInstruction as I;
use permutation_chain_svm_tests::ix::registration::ER_VALIDATOR;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use solana_address::Address;
use solana_signer::Signer;

fn running(c: &mut Chain, nations: u8) -> SeasonFx {
    let s = SeasonFx::running(
        c,
        Params {
            nations,
            ..Params::default()
        },
        2 * nations as usize,
    );
    bind_rules(c, &s);
    s
}

/// The season as CreateSeason binds it to this build's rules (WP15; the
/// same values CreateSeason stores, so this is a no-op once it does).
fn bind_rules(c: &mut Chain, s: &SeasonFx) {
    c.edit::<Season>(&s.season, |x| {
        x.rules_version = PINNED_RULES_VERSION;
        x.rules_hash = pinned_ruleset_hash(x.preset, x.market).unwrap();
        x.logic_version = CHAIN_LOGIC_VERSION;
    });
}

/// The one Magic call since the last `take`, as (committed, undelegated).
fn intent() -> (Vec<Address>, Vec<Address>) {
    let calls = mocks::take();
    assert_eq!(calls.len(), 1, "one intent");
    assert_eq!(calls[0].0, "magic");
    intent_accounts(&calls[0])
}

/// The validator a DLP `delegate` call names (after the frequency and
/// the seeds of `delegate_args`).
fn delegate_validator(call: &Call) -> Option<Address> {
    let mut rest = &call.2[8..];
    let _: u32 = borsh::BorshDeserialize::deserialize(&mut rest).unwrap();
    let _: Vec<Vec<u8>> = borsh::BorshDeserialize::deserialize(&mut rest).unwrap();
    let v: Option<[u8; 32]> = borsh::BorshDeserialize::deserialize(&mut rest).unwrap();
    v.map(Address::new_from_array)
}

/// As the ER holds a delegated account: back to the program with `data`
/// (the stand-in DLP leaves it delegation-owned and zeroed on base).
fn restore(c: &mut Chain, key: &Address, data: Vec<u8>) {
    let program = c.program;
    c.put(*key, program, data);
}

#[test]
fn delegate_checks_and_happy_path() {
    let mut c = Chain::new().with_magicblock();
    let mut s = SeasonFx::create(&mut c, Params::default());
    s.register(&mut c, 0, 0);
    s.register(&mut c, 1, 0);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    assert_err(
        c.send(vec![s.delegate_ix(&cp, 3)], &[&crank]),
        E::WrongStatus,
    );
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    let outsider = c.funded();
    assert_err(
        c.send(vec![s.delegate_ix(&outsider.pubkey(), 3)], &[&outsider]),
        E::Unauthorized,
    );
    // A season created under other rules or settlement logic.
    c.edit::<Season>(&s.season, |x| x.rules_hash = [9; 32]);
    assert_err(
        c.send(vec![s.delegate_ix(&cp, 3)], &[&crank]),
        E::RulesMismatch,
    );
    bind_rules(&mut c, &s);
    // Another delegation program.
    let mut ix = s.delegate_ix(&cp, 3);
    ix.accounts[8].pubkey = c.program;
    assert_err(c.send(vec![ix], &[&crank]), E::WrongDelegationProgram);
    // Another owner program, another system program.
    for k in [4, 1] {
        let mut ix = s.delegate_ix(&cp, 3);
        ix.accounts[k].pubkey = addr(TOKEN);
        assert_program_err(c.send(vec![ix], &[&crank]), "IncorrectProgramId");
    }
    // The validator is required.
    let mut ix = s.delegate_ix(&cp, 3);
    ix.accounts.pop();
    assert_program_err(c.send(vec![ix], &[&crank]), "NotEnoughAccountKeys");
    // Targets past the chunks and past the nations.
    for t in [WORLD_CHUNKS as u16, NATION_TARGET + s.p.nations as u16] {
        let mut ix = s.delegate_ix(&cp, 3);
        ix.data = borsh::to_vec(&I::Delegate { target: t }).unwrap();
        assert_err(c.send(vec![ix], &[&crank]), E::InvalidParams);
    }
    assert_eq!(c.load::<Season>(&s.season).delegated, 0, "nothing recorded");
    // The crank (or the admin) delegates chunk 3 and nation 1: the PDA goes
    // to the delegation program, with one delegate call that commits every
    // 30 s on the season's validator, and the target's bit is recorded.
    let id = s.p.id.to_le_bytes();
    let admin = s.admin.insecure_clone();
    for (t, seeds, who) in [
        (
            3u16,
            vec![WORLD_SEED.to_vec(), id.to_vec(), vec![3]],
            &crank,
        ),
        (
            NATION_TARGET + 1,
            vec![
                NATION_SEED.to_vec(),
                id.to_vec(),
                1u16.to_le_bytes().to_vec(),
            ],
            &admin,
        ),
    ] {
        mocks::take();
        let l = c
            .send(vec![s.delegate_ix(&who.pubkey(), t)], &[who])
            .expect("Delegate");
        assert_eq!(c.owner(&s.target(t)), Some(addr(DLP)));
        let calls = mocks::take();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "dlp");
        assert_eq!(calls[0].1[1], s.target(t), "the delegated PDA");
        assert_eq!(delegate_args(&calls[0]), (ER_COMMIT_FREQUENCY_MS, seeds));
        assert_eq!(delegate_validator(&calls[0]), Some(addr(ER_VALIDATOR)));
        println!("Delegate {t}: {} CU", l.cu);
    }
    assert_eq!(
        c.load::<Season>(&s.season).delegated,
        target_bit(3).unwrap() | target_bit(NATION_TARGET + 1).unwrap()
    );
}

/// WP14 §3.4 with WP02's once-only rule and WP15's validator pin: chunk 0
/// last, each target once (also after it came back), only to the season's
/// validator; the bit is recorded before the delegation CPI.
#[test]
fn delegation_guards() {
    let mut c = Chain::new().with_magicblock();
    let s = running(&mut c, 3);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    let season = |c: &Chain| c.load::<Season>(&s.season);
    assert_err(
        c.send(vec![s.delegate_ix(&cp, 0)], &[&crank]),
        E::DelegationOrder,
    );
    // Another validator than the season's.
    let other = c.funded().pubkey();
    assert_err(
        c.send(vec![s.delegate_to_ix(&cp, 1, &other)], &[&crank]),
        E::WrongValidator,
    );
    assert_eq!(season(&c).delegated, 0);
    let before: Vec<(u16, Vec<u8>)> = s
        .all_targets()
        .into_iter()
        .map(|t| (t, c.data(&s.target(t))))
        .collect();
    c.send(vec![s.delegate_ix(&cp, 1)], &[&crank])
        .expect("Delegate 1");
    assert_eq!(season(&c).delegated, 1 << 1);
    assert_err(
        c.send(vec![s.delegate_ix(&cp, 1)], &[&crank]),
        E::AlreadyDelegated,
    );
    // Every other target, in any order; chunk 0 still refused until the
    // last of them.
    let mut rest: Vec<u16> = s
        .all_targets()
        .into_iter()
        .filter(|t| *t > 1)
        .rev()
        .collect();
    while let Some(t) = rest.pop() {
        assert_err(
            c.send(vec![s.delegate_ix(&cp, 0)], &[&crank]),
            E::DelegationOrder,
        );
        c.send(vec![s.delegate_ix(&cp, t)], &[&crank])
            .expect("Delegate");
    }
    c.send(vec![s.delegate_ix(&cp, 0)], &[&crank])
        .expect("Delegate chunk 0 last");
    assert_eq!(season(&c).delegated, all_targets(s.p.nations));
    // Back home (as after the wind-up): never delegated again.
    for (t, data) in before {
        restore(&mut c, &s.target(t), data);
    }
    for t in [0, 3, NATION_TARGET + 2] {
        assert_err(
            c.send(vec![s.delegate_ix(&cp, t)], &[&crank]),
            E::AlreadyDelegated,
        );
    }
}

/// The delegation program's callback: our PDA is recreated from its buffer;
/// content of an older layout (magic family) still comes home.
#[test]
fn undelegate_callback_restores_the_account() {
    let mut c = Chain::new_opts(false).with_magicblock();
    let s = running(&mut c, 2);
    let id = s.p.id.to_le_bytes();
    let seeds = |k: u8| vec![WORLD_SEED.to_vec(), id.to_vec(), vec![k]];
    let chunk3 = s.chunks[3];
    let content = c.data(&chunk3);
    let world0 = c.data(&s.chunks[0]);
    let nation1 = c.data(&s.nations[1]);
    let program = c.program;
    // A buffer the delegation program does not own.
    assert_err(
        undelegate_callback(&mut c, &chunk3, &seeds(3), content.clone(), program),
        E::WrongDelegationProgram,
    );
    // Chunk 0 content that is not a world or a genesis job.
    assert_err(
        undelegate_callback(&mut c, &s.chunks[0], &seeds(0), vec![7; CHUNK], addr(DLP)),
        E::WrongPda,
    );
    // Seeds of another PDA.
    assert_err(
        undelegate_callback(&mut c, &chunk3, &seeds(0), content.clone(), addr(DLP)),
        E::WrongPda,
    );
    // Seed bytes that do not decode.
    let payer = c.funded();
    let ix = callback_ix(&program, &chunk3, &payer.pubkey(), &[1, 2, 3], true);
    assert_err(c.send_as(vec![ix], &payer.pubkey()), E::InvalidInstruction);
    // The buffer must sign (the delegation program signs it by CPI).
    let ix = callback_ix(
        &program,
        &chunk3,
        &payer.pubkey(),
        &borsh::to_vec(&seeds(3)).unwrap(),
        false,
    );
    assert_program_err(
        c.send_as(vec![ix], &payer.pubkey()),
        "MissingRequiredSignature",
    );
    // Chunk 3, chunk 0 and nation 1 come back program-owned with their bytes.
    let cu = undelegate_callback(&mut c, &chunk3, &seeds(3), content.clone(), addr(DLP))
        .expect("callback chunk 3")
        .cu;
    assert_eq!(
        (c.owner(&chunk3), c.data(&chunk3)),
        (Some(program), content)
    );
    undelegate_callback(&mut c, &s.chunks[0], &seeds(0), world0.clone(), addr(DLP))
        .expect("callback chunk 0");
    assert_eq!(c.data(&s.chunks[0]), world0);
    let nseeds = vec![
        NATION_SEED.to_vec(),
        id.to_vec(),
        1u16.to_le_bytes().to_vec(),
    ];
    undelegate_callback(&mut c, &s.nations[1], &nseeds, nation1.clone(), addr(DLP))
        .expect("callback nation 1");
    assert_eq!(
        (c.owner(&s.nations[1]), c.data(&s.nations[1])),
        (Some(program), nation1.clone())
    );
    // An account of the previous layouts (PSWORLD5 chunk 0, PSNATN07).
    let mut old0 = world0.clone();
    old0[..8].copy_from_slice(b"PSWORLD5");
    undelegate_callback(&mut c, &s.chunks[0], &seeds(0), old0.clone(), addr(DLP))
        .expect("callback of an older world");
    assert_eq!(c.data(&s.chunks[0]), old0);
    let mut old1 = nation1;
    old1[..8].copy_from_slice(b"PSNATN07");
    undelegate_callback(&mut c, &s.nations[1], &nseeds, old1.clone(), addr(DLP))
        .expect("callback of an older nation");
    assert_eq!(c.data(&s.nations[1]), old1);
    // A nation seed with a season inside.
    let season = c.data(&s.season);
    assert_err(
        undelegate_callback(&mut c, &s.nations[1], &nseeds, season, addr(DLP)),
        E::WrongPda,
    );
    println!("undelegate callback: {cu} CU");
}

/// `Commit` and `CommitAndUndelegate` are retired (WP02): one intent over
/// every account exceeds the committor's limits on devnet. The crank's
/// commit during play and anyone's after the last tick are both refused,
/// and nothing is scheduled.
#[test]
fn whole_world_commits_are_retired() {
    let mut c = Chain::new().with_magicblock();
    let s = running(&mut c, 2);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    let stranger = c.funded();
    let sp = stranger.pubkey();
    mocks::take();
    assert_err(c.send(vec![s.commit_ix(&cp, false)], &[&crank]), E::Retired);
    s.fast_forward_to_end(&mut c);
    assert_err(c.send(vec![s.commit_ix(&cp, true)], &[&crank]), E::Retired);
    assert_err(
        c.send(vec![s.commit_ix(&sp, true)], &[&stranger]),
        E::Retired,
    );
    assert!(mocks::take().is_empty(), "nothing scheduled");
}

#[test]
fn commit_part_checks() {
    let mut c = Chain::new().with_magicblock();
    let s = running(&mut c, 6);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    let stranger = c.funded();
    let n = NATION_TARGET;
    assert_err(
        c.send(
            vec![s.part_ix(&stranger.pubkey(), vec![1], false)],
            &[&stranger],
        ),
        E::Unauthorized,
    );
    // Another program in the Magic program's or the context's place.
    for k in [1, 2] {
        let mut ix = s.part_ix(&cp, vec![1], false);
        ix.accounts[k].pubkey = stranger.pubkey();
        assert_err(c.send(vec![ix], &[&crank]), E::WrongMagicProgram);
    }
    // Only one world chunk, or up to three distinct nations.
    for targets in [
        vec![],
        s.all_targets(),
        vec![1, 1],
        vec![1, 2],
        vec![0, 1],
        vec![1, n],
        vec![n, n + 1, n + 2, n + 3],
    ] {
        let mut ix = s.part_ix(&cp, vec![1], false);
        ix.data = borsh::to_vec(&I::CommitPart { targets }).unwrap();
        assert_err(c.send(vec![ix], &[&crank]), E::InvalidParams);
    }
    // A nation past the season's.
    let mut ix = s.part_ix(&cp, vec![n + 1], false);
    ix.data = borsh::to_vec(&I::CommitPart {
        targets: vec![n + 6],
    })
    .unwrap();
    assert_err(c.send(vec![ix], &[&crank]), E::InvalidParams);
    // A chunk the program does not own.
    c.set_owner(&s.chunks[5], addr(SYSTEM));
    assert_err(
        c.send(vec![s.part_ix(&cp, vec![5], false)], &[&crank]),
        E::WrongWorld,
    );
    c.set_owner(&s.chunks[5], c.program);
    // The intent holds exactly the targets, chunk 0 from its fixed slot.
    for targets in [vec![1], vec![0], vec![n + 1, n + 2, n + 5], vec![n]] {
        mocks::take();
        c.send(vec![s.part_ix(&cp, targets.clone(), false)], &[&crank])
            .expect("crank CommitPart");
        let want: Vec<Address> = targets.iter().map(|t| s.target(*t)).collect();
        assert_eq!(intent(), (want, vec![]));
    }
    // After the last tick: the final round is still the crank's, until the
    // first UndelegatePart.
    s.fast_forward_to_end(&mut c);
    c.send(vec![s.part_ix(&cp, vec![7], false)], &[&crank])
        .expect("the final commit round");
    c.send(vec![s.part_ix(&cp, vec![1], true)], &[&crank])
        .expect("the wind-up begins");
    for payer in [&crank, &stranger] {
        assert_err(
            c.send(vec![s.part_ix(&payer.pubkey(), vec![7], false)], &[payer]),
            E::WrongPhase,
        );
    }
}

#[test]
fn undelegate_part_after_the_last_tick() {
    let mut c = Chain::new().with_magicblock();
    let s = running(&mut c, 2);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    assert_err(
        c.send(vec![s.part_ix(&cp, vec![1], true)], &[&crank]),
        E::SeasonNotOver,
    );
    s.fast_forward_to_end(&mut c);
    // The crank's order: each chunk alone, the nations, chunk 0 last.
    let mut groups: Vec<Vec<u16>> = (1..WORLD_CHUNKS as u16).map(|t| vec![t]).collect();
    groups.push(vec![NATION_TARGET, NATION_TARGET + 1]);
    groups.push(vec![0]);
    for targets in groups {
        mocks::take();
        c.send(vec![s.part_ix(&cp, targets.clone(), true)], &[&crank])
            .expect("UndelegatePart");
        let want: Vec<Address> = targets.iter().map(|t| s.target(*t)).collect();
        assert_eq!(intent(), (vec![], want));
    }
    let meta = c.with_world(&s.chunks, |w| w.meta().unwrap());
    assert_eq!(meta.undelegated, 22);
}

/// Fixed behaviour (WP02, contract §1.3 tag 12): anyone may send
/// UndelegatePart, but only in the crank's shape and order; tag 9 is
/// retired. A stranger's UndelegatePart of several chunks, of more than
/// three nations or of every target is refused, and its in-shape part
/// (chunk 1, first in `undelegation_order`) lands. No crank-only check.
#[test]
fn undelegation_is_anyones_in_the_crank_shape() {
    let mut c = Chain::new().with_magicblock();
    let s = running(&mut c, 6);
    s.fast_forward_to_end(&mut c);
    let stranger = c.funded();
    let sp = stranger.pubkey();
    mocks::take();
    assert_err(
        c.send(vec![s.commit_ix(&sp, true)], &[&stranger]),
        E::Retired,
    );
    assert_err(
        c.send(vec![s.part_ix(&sp, vec![1, 2], true)], &[&stranger]),
        E::InvalidParams,
    );
    let nations: Vec<u16> = (0..4).map(|k| NATION_TARGET + k).collect();
    assert_err(
        c.send(vec![s.part_ix(&sp, nations, true)], &[&stranger]),
        E::InvalidParams,
    );
    let mut all = s.all_targets();
    all.rotate_left(1);
    assert_err(
        c.send(vec![s.part_ix(&sp, all, true)], &[&stranger]),
        E::InvalidParams,
    );
    assert!(mocks::take().is_empty(), "nothing scheduled");
    c.send(vec![s.part_ix(&sp, vec![1], true)], &[&stranger])
        .expect("anyone's UndelegatePart in the crank's shape");
    assert_eq!(intent(), (vec![], vec![s.chunks[1]]));
}

/// Fixed behaviour (WP02): an undelegation intent is one world chunk or up
/// to three nations, in `undelegation_order`, with chunk 0 last.
#[test]
fn undelegation_intents_have_the_crank_shape() {
    let mut c = Chain::new().with_magicblock();
    let s = running(&mut c, 6);
    s.fast_forward_to_end(&mut c);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    // Chunks and nations mixed: InvalidParams.
    assert_err(
        c.send(
            vec![s.part_ix(&cp, vec![1, NATION_TARGET], true)],
            &[&crank],
        ),
        E::InvalidParams,
    );
    // Chunk 0 before the others, or out of order: UndelegationOrder.
    for first in [vec![0], vec![2], vec![NATION_TARGET]] {
        assert_err(
            c.send(vec![s.part_ix(&cp, first, true)], &[&crank]),
            E::UndelegationOrder,
        );
    }
    c.send(vec![s.part_ix(&cp, vec![1], true)], &[&crank])
        .expect("chunk 1 first");
    assert_err(
        c.send(vec![s.part_ix(&cp, vec![1], true)], &[&crank]),
        E::UndelegationOrder,
    );
}
