//! The wind-up after the last tick on the SBF build (WP02): UndelegatePart
//! is anyone's, one world chunk or up to three nations per intent, in
//! `undelegation_order` (chunks 1..19, the nations, chunk 0 last) counted in
//! chunk 0's header, one step per transaction; a target that already left
//! the ER is skipped; nations go back cleared, read head-only. The recording
//! Magic stand-in shows exactly what each step schedules.

use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::instruction::ChainInstruction as I;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use permutation_rules::gov::{GovAction, GovEntry, Role};
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_signer::Signer;

fn finished(c: &mut Chain, nations: u8) -> SeasonFx {
    let s = SeasonFx::running(
        c,
        Params {
            nations,
            ..Params::default()
        },
        2 * nations as usize,
    );
    s.fast_forward_to_end(c);
    mocks::take();
    s
}

fn counter(c: &mut Chain, s: &SeasonFx) -> u8 {
    c.with_world(&s.chunks, |w| w.meta().unwrap().undelegated)
}

/// The accounts each Magic call since the last `take` commits and
/// undelegates (no plain commit).
fn scheduled() -> Vec<Vec<Address>> {
    mocks::take()
        .iter()
        .map(|call| {
            assert_eq!(call.0, "magic");
            let (commit, undelegate) = intent_accounts(call);
            assert!(commit.is_empty());
            undelegate
        })
        .collect()
}

/// The crank's next group of `order` from `at`: a world chunk alone, or up
/// to three nations.
fn next_group(order: &[u16], at: usize) -> Vec<u16> {
    if (order[at] as usize) < WORLD_CHUNKS {
        return vec![order[at]];
    }
    order[at..]
        .iter()
        .take(MAX_NATIONS_PER_INTENT)
        .take_while(|&&t| t >= NATION_TARGET)
        .copied()
        .collect()
}

/// Sends one step under the Light profile (no budget instructions, the
/// default 200k CU and 32 KiB heap) and checks it stays under `LIGHT_CU`.
fn step(c: &mut Chain, s: &SeasonFx, payer: &Keypair, targets: &[u16], gone: &[u16]) -> Landed {
    let ix = s.undelegate_ix(&payer.pubkey(), targets.to_vec(), gone);
    let l = c
        .send_with(Budget::Light, vec![ix], &[payer])
        .unwrap_or_else(|f| panic!("UndelegatePart {targets:?}: {f:#?}"));
    assert!(l.cu <= LIGHT_CU, "{targets:?}: {} CU", l.cu);
    l
}

/// Oversized shapes, chunk 0 or the nations first, and tag 9 fail for a
/// stranger before any Magic CPI; nothing is scheduled or counted.
#[test]
fn adversarial_shapes_and_orders_schedule_nothing() {
    let mut c = Chain::new().with_magicblock();
    let s = finished(&mut c, 6);
    let stranger = c.funded();
    let sp = stranger.pubkey();
    let n = NATION_TARGET;
    let mut all26: Vec<u16> = (1..WORLD_CHUNKS as u16)
        .chain((0..6).map(|k| n + k))
        .collect();
    all26.push(0);
    for (targets, code) in [
        (all26, E::InvalidParams),
        (vec![1, 2, 3, 4, 5], E::InvalidParams),
        (vec![1, n], E::InvalidParams),
        (vec![n, n + 1, n + 2, n + 3], E::InvalidParams),
        (vec![0], E::UndelegationOrder),
        (vec![n, n + 1, n + 2], E::UndelegationOrder),
        (vec![19], E::UndelegationOrder),
    ] {
        let r = c.send(vec![s.part_ix(&sp, targets.clone(), true)], &[&stranger]);
        if let Err(f) = &r {
            println!("stranger {targets:?}: {:?}", f.code);
        }
        assert_err(r, code);
    }
    assert_err(
        c.send(vec![s.commit_ix(&sp, true)], &[&stranger]),
        E::Retired,
    );
    assert!(mocks::take().is_empty(), "no Magic CPI");
    assert_eq!(counter(&mut c, &s), 0);
}

/// The whole wind-up in the crank's groups, a stranger sending every third
/// step: each step schedules exactly its targets, a replay is refused, chunk
/// 0 is refused until it is last, and the counter ends at 26. Every step
/// fits the default budget.
#[test]
fn a_stranger_and_the_crank_wind_up_every_account() {
    let mut c = Chain::new().with_magicblock();
    let s = finished(&mut c, 6);
    let crank = s.crank.insecure_clone();
    let stranger = c.funded();
    let order = undelegation_order(6);
    let (mut at, mut k, mut worst) = (0, 0, 0);
    while at < order.len() {
        let group = next_group(&order, at);
        if at + 1 < order.len() {
            assert_err(
                c.send(
                    vec![s.part_ix(&stranger.pubkey(), vec![0], true)],
                    &[&stranger],
                ),
                E::UndelegationOrder,
            );
        }
        let payer = if k % 3 == 2 { &stranger } else { &crank };
        let l = step(&mut c, &s, payer, &group, &[]);
        worst = worst.max(l.cu);
        let want: Vec<Address> = group.iter().map(|t| s.target(*t)).collect();
        assert_eq!(scheduled(), vec![want], "{group:?}");
        assert_err(
            c.send(
                vec![s.part_ix(&crank.pubkey(), group.clone(), true)],
                &[&crank],
            ),
            E::UndelegationOrder,
        );
        at += group.len();
        assert_eq!(counter(&mut c, &s) as usize, at);
        k += 1;
    }
    assert_eq!(k, 19 + 2 + 1, "22 steps for six nations");
    assert_eq!(counter(&mut c, &s), 26);
    println!("UndelegatePart: worst step {worst} CU");
}

/// Nation inboxes stuffed after the last tick (up to the account's size)
/// with stale bytes behind: three of them leave in one step at the default
/// budget, read head-only, and go back cleared, frozen and zero-padded. A
/// stuffed nation 0 still vouches for the crank's CommitPart (head-only).
#[test]
fn stuffed_nations_leave_at_the_default_budget() {
    let mut c = Chain::new().with_magicblock();
    let s = finished(&mut c, 3);
    let crank = s.crank.insecure_clone();
    for (k, key) in s.nations.iter().enumerate() {
        c.edit::<NationAccount>(key, |n| {
            n.inbox = (0..180)
                .map(|i| GovEntry {
                    member: i,
                    signer: [i as u8 ^ k as u8; 32],
                    action: GovAction::Vote {
                        role: Role::Steward,
                        candidate: i,
                    },
                })
                .collect();
            n.roll = (0..8)
                .map(|m| RollSeat {
                    member: m,
                    key8: [m as u8; 8],
                })
                .collect();
        });
        let mut d = c.data(key);
        let len = borsh::to_vec(&load::<NationAccount>(&d).unwrap())
            .unwrap()
            .len();
        assert!(len > NATION_SPACE - 64, "stuffed: {len} B");
        for (i, b) in d[len..].iter_mut().enumerate() {
            *b = i as u8 | 1;
        }
        c.set_data(key, d);
    }
    let l = c
        .send_with(
            Budget::Light,
            vec![s.part_ix(&crank.pubkey(), vec![7], false)],
            &[&crank],
        )
        .expect("CommitPart with a stuffed nation 0 as the crank's witness");
    println!("CommitPart [7], stuffed nation 0: {} CU", l.cu);
    for t in 1..WORLD_CHUNKS as u16 {
        step(&mut c, &s, &crank, &[t], &[]);
    }
    mocks::take();
    let nations: Vec<u16> = (0..3).map(|k| NATION_TARGET + k).collect();
    let l = step(&mut c, &s, &crank, &nations, &[]);
    println!("UndelegatePart, three stuffed nations: {} CU", l.cu);
    assert_eq!(scheduled(), vec![s.nations.clone()]);
    for (k, key) in s.nations.iter().enumerate() {
        let d = c.data(key);
        let n: NationAccount = load(&d).unwrap();
        assert!(n.inbox.is_empty() && n.roll.is_empty());
        assert!(n.batches.iter().all(Option::is_none) && n.frozen);
        assert_eq!((n.season_id, n.civ), (s.p.id, k as u16));
        let len = borsh::to_vec(&n).unwrap().len();
        assert!(d[len..].iter().all(|b| *b == 0), "no stale bytes");
    }
}

/// One step per transaction: two steps, or a step with another program's
/// instruction, are `NotAlone` and count nothing; compute-budget
/// instructions are fine. The sysvar is required.
#[test]
fn one_step_per_transaction() {
    let mut c = Chain::new().with_magicblock();
    let s = finished(&mut c, 2);
    let stranger = c.funded();
    let sp = stranger.pubkey();
    let to = c.funded().pubkey();
    let mut transfer = vec![2u8, 0, 0, 0];
    transfer.extend(1u64.to_le_bytes());
    let other = Instruction {
        program_id: addr(SYSTEM),
        accounts: vec![AccountMeta::new(sp, true), AccountMeta::new(to, false)],
        data: transfer,
    };
    for ixs in [
        vec![s.part_ix(&sp, vec![1], true), s.part_ix(&sp, vec![2], true)],
        vec![other.clone(), s.part_ix(&sp, vec![1], true)],
        vec![s.part_ix(&sp, vec![1], true), other],
    ] {
        assert_err(c.send_with(Budget::Light, ixs, &[&stranger]), E::NotAlone);
    }
    // Without the sysvar, or another account in its place.
    let mut ix = s.part_ix(&sp, vec![1], true);
    ix.accounts.pop();
    assert_err(c.send(vec![ix], &[&stranger]), E::NotAlone);
    let mut ix = s.part_ix(&sp, vec![1], true);
    ix.accounts.last_mut().unwrap().pubkey = addr(SYSTEM);
    assert_err(c.send(vec![ix], &[&stranger]), E::NotAlone);
    assert!(mocks::take().is_empty());
    assert_eq!(counter(&mut c, &s), 0);
    c.send_with(
        Budget::Heavy,
        vec![s.part_ix(&sp, vec![1], true)],
        &[&stranger],
    )
    .expect("a step behind compute-budget instructions");
    assert_eq!(counter(&mut c, &s), 1);
}

/// Targets that left the ER out of band (delegation-owned there: chunk 7,
/// nation 3, chunk 19) are passed read-only, skipped and counted; the rest
/// winds up, chunk 0 last: 26 counted, 23 scheduled. Chunk 0 is never
/// skipped, an impostor is not a skip, another owner is refused.
#[test]
fn gone_targets_are_skipped_and_the_rest_winds_up() {
    let mut c = Chain::new().with_magicblock();
    let s = finished(&mut c, 6);
    let base = c.fork();
    let crank = s.crank.insecure_clone();
    let stranger = c.funded();
    let gone = [7, NATION_TARGET + 3, 19];
    let before: Vec<Vec<u8>> = gone.iter().map(|t| c.data(&s.target(*t))).collect();
    for t in gone {
        c.set_owner(&s.target(t), addr(DLP));
    }
    let order = undelegation_order(6);
    let (mut at, mut total, mut skips) = (0, 0, 0);
    while at < order.len() {
        let group = next_group(&order, at);
        let l = step(&mut c, &s, &stranger, &group, &gone);
        skips += l.logs.iter().filter(|x| x.contains("PS skip")).count();
        let want: Vec<Address> = group
            .iter()
            .filter(|t| !gone.contains(t))
            .map(|t| s.target(*t))
            .collect();
        let got = scheduled();
        assert_eq!(got.len(), usize::from(!want.is_empty()), "{group:?}");
        total += got.iter().map(Vec::len).sum::<usize>();
        if !want.is_empty() {
            assert_eq!(got[0], want, "{group:?}");
        }
        at += group.len();
        assert_eq!(counter(&mut c, &s) as usize, at);
    }
    assert_eq!((counter(&mut c, &s), total, skips), (26, 23, 3));
    for (t, d) in gone.iter().zip(before) {
        assert_eq!(c.data(&s.target(*t)), d, "skipped {t} untouched");
    }
    // Chunk 0 gone: nothing runs (the counter lives there).
    let mut c = base;
    c.set_owner(&s.chunks[0], addr(DLP));
    assert_err(
        c.send(vec![s.part_ix(&crank.pubkey(), vec![1], true)], &[&crank]),
        E::WrongWorld,
    );
    c.set_owner(&s.chunks[0], c.program);
    // A delegation-owned impostor in chunk 1's place.
    let impostor = c.funded().pubkey();
    c.put(impostor, addr(DLP), vec![0; CHUNK]);
    let mut ix = s.part_ix(&crank.pubkey(), vec![1], true);
    ix.accounts[4].pubkey = impostor;
    assert_err(c.send(vec![ix], &[&crank]), E::WrongPda);
    // Owned by another program: neither ours nor gone.
    c.set_owner(&s.chunks[1], addr(SYSTEM));
    assert_err(
        c.send(vec![s.part_ix(&crank.pubkey(), vec![1], true)], &[&crank]),
        E::WrongWorld,
    );
    assert_eq!(counter(&mut c, &s), 0);
}

/// The data of tag 12 and 20 is unchanged on the wire: a borsh `Vec<u16>`.
#[test]
fn part_data_is_a_target_list() {
    let data = borsh::to_vec(&I::UndelegatePart {
        targets: vec![1000, 1001],
    })
    .unwrap();
    assert_eq!(data, [12, 2, 0, 0, 0, 0xe8, 3, 0xe9, 3]);
    let data = borsh::to_vec(&I::CommitPart { targets: vec![0] }).unwrap();
    assert_eq!(data, [20, 1, 0, 0, 0, 0, 0]);
}
