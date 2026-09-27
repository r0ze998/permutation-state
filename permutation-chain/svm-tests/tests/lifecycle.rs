//! Whole seasons through the SBF build, settled to the last token: an idle
//! season with deposits (180 ticks), the play server's bots playing a
//! 15-member Blitz season (180 ticks, chain equals native every tick), and
//! AI seasons whose roster is revealed or forfeited. Every one conserves
//! USDC exactly: in = claims + operations + what is left (rounding dust).

use permutation_chain_svm_tests::drive::Settled;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use std::time::Instant;

/// FinishSeason (the crank), then every claim and the operations share.
fn finish_and_settle(c: &mut Chain, s: &SeasonFx, roster: bool, vault_in: u64) -> Settled {
    let crank = s.crank.insecure_clone();
    c.send(vec![s.finish_ix(roster)], &[&crank])
        .expect("FinishSeason");
    assert_eq!(
        c.balance(&s.vault),
        vault_in,
        "nothing left the vault before the claims"
    );
    let out = s.settle(c);
    println!(
        "vault in {vault_in} = claims {} + ops {} + left {}",
        out.claims, out.ops, out.left
    );
    assert_eq!(
        vault_in,
        out.claims + out.ops + out.left,
        "USDC is conserved"
    );
    assert_eq!(out.ops, s.season(c).ops);
    let dust = (s.members.len() + s.p.nations as usize) as u64;
    assert!(out.left <= dust, "only rounding dust stays: {}", out.left);
    out
}

#[test]
fn idle_season_with_deposits_conserves_usdc() {
    let started = Instant::now();
    let mut c = Chain::new();
    // One deposit per season (WP12).
    let d = 4_000_000;
    let mut s = SeasonFx::create(
        &mut c,
        Params {
            deposit: d,
            ..Params::default()
        },
    );
    let deposits = [d; 4];
    for (i, d) in deposits.iter().enumerate() {
        s.register(&mut c, (i % 2) as u16, *d);
    }
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    let mut worst = 0;
    while !s.meta(&c).finished {
        worst = worst.max(s.play_tick(&mut c).worst_cu);
    }
    assert_eq!(s.world(&mut c).tick, s.rules().ticks_per_season);
    println!("180 idle ticks: worst {worst} CU, {:?}", started.elapsed());
    let vault_in = 4 * s.p.fee + deposits.iter().sum::<u64>();
    finish_and_settle(&mut c, &s, false, vault_in);
}

/// The bots play a whole Blitz season (`PLAYED_TICKS=n` stops early): every
/// batch and governance action lands, every tick equals the native engine
/// on the published input, and the settlement conserves USDC exactly.
#[test]
fn bots_play_a_blitz_season_on_the_sbf_build() {
    let stop: u16 = std::env::var("PLAYED_TICKS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(180);
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
    let (mut batches, mut gov) = (0, 0);
    while !bots.done() && bots.state.tick < stop {
        let r = bots.step(&mut c, false);
        assert_eq!(
            r.refused, 0,
            "tick {}: an honest bot submission was refused",
            r.tick
        );
        batches += r.batches;
        gov += r.gov;
    }
    println!(
        "{} ticks: {batches} batches, {gov} governance entries, {:?}",
        bots.state.tick,
        started.elapsed()
    );
    if bots.done() {
        let vault_in = 15 * bots.s.p.fee;
        finish_and_settle(&mut c, &bots.s, false, vault_in);
    }
}

/// Two AIs and four people; returns (season, vault in).
fn ai_season(c: &mut Chain) -> (SeasonFx, u64) {
    let (bounty, bond) = (1_000_000, 4_000_000);
    // One deposit per season (WP12), the AIs' too.
    let d = 1_000_000;
    let p = Params {
        deposit: d,
        ..Params::default()
    };
    let mut s = SeasonFx::create_ai(c, p, &[0, 1], bounty, bond);
    s.register_ai(c, 0);
    s.register_ai(c, 1);
    let deposits = [d; 4];
    for (i, d) in deposits.iter().enumerate() {
        s.register(c, (i % 2) as u16, *d);
    }
    s.genesis(c);
    s.seat_and_open(c);
    for _ in 0..3 {
        s.play_tick(c);
    }
    s.fast_forward_to_end(c);
    // `genesis` topped the bond up to its floor (PostBond, WP09).
    let bond = s.season(c).bond;
    let vault_in = 6 * s.p.fee + 6 * d + 2 * bounty + bond;
    assert_eq!(c.balance(&s.vault), vault_in);
    (s, vault_in)
}

#[test]
fn ai_season_revealed_conserves_usdc() {
    let mut c = Chain::new();
    let (s, vault_in) = ai_season(&mut c);
    let admin = s.admin.insecure_clone();
    c.send(vec![s.reveal_ai_ix(&[0, 1])], &[&admin])
        .expect("RevealRoster");
    finish_and_settle(&mut c, &s, true, vault_in);
    assert_eq!(s.season(&c).roster_outcome, ROSTER_REVEALED);
}

#[test]
fn ai_season_forfeited_conserves_usdc() {
    let mut c = Chain::new();
    let (s, vault_in) = ai_season(&mut c);
    c.advance(ROSTER_GRACE_SECONDS);
    finish_and_settle(&mut c, &s, false, vault_in);
    assert_eq!(s.season(&c).roster_outcome, ROSTER_FORFEITED);
}
