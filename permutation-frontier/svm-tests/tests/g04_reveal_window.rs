//! G4 (§13.3), the anchor side that exists from wave 2: every input of the
//! "reveal open" rule (§5.1) is written by W2-A's instructions exactly as
//! the rule needs it — `A` = the Clock at THE anchor's creation, `W(b)`
//! from the Season's window schedule (changed only with ≥ 144 bells'
//! notice), `S(A)` from them, the BeaconLog's `latest_round` moving forward
//! only, and a tombstoned bell refusing a new anchor. The Reveal-side
//! property ("no Reveal lands at `now ≥ A + W`, once the BeaconLog holds a
//! round ≥ S, after the first gather, or for a tombstoned bell") is W3-B's,
//! over these same accounts.

mod common;

use frontier_abi::layout::beacon::bell_anchor as BA;
use frontier_abi::layout::world::season as S;
use frontier_abi::log::Kind;
use permutation_frontier_svm_tests::chain::{assert_code, assert_refused, expect_lands};
use permutation_frontier_svm_tests::ix::season::set_window_schedule;
use permutation_frontier_svm_tests::records::{self, le};
use permutation_frontier_svm_tests::world::{archive_part, World};
use permutation_frontier_svm_tests::{FrontierError as E, Rng, Signer};
use permutation_rules::frontier::beacon as kb;
use permutation_rules::frontier::clash::QUICKNET;

#[test]
fn g04_anchor_records_the_clock_at_creation() {
    let (mut c, w) = common::test_beacon();
    let mut rng = Rng::new(0x6404);
    for bell in 0..24u32 {
        let region = rng.below(16) as u8;
        // Land at a random moment after T(bell) is published (0–900 s late).
        let t = World::round_time(w.tlock_round(bell)) + rng.range(0, 900) as i64;
        if c.now < t {
            c.set_time(t);
        }
        let l = expect_lands(w.post_anchor(&mut c, bell, region), "PostAnchor");
        let d = c.data(&w.a.anchor(bell, region));
        assert_eq!(
            le(&d[BA::A..BA::A + 8]) as i64,
            c.now,
            "A = Clock.unix_timestamp"
        );
        assert_eq!(le(&d[BA::SLOT..BA::SLOT + 8]), c.slot, "slot = Clock.slot");
        assert_eq!(
            le(&d[BA::ROUND..BA::ROUND + 8]),
            w.tlock_round(bell),
            "round = T(b)"
        );
        assert_eq!(le(&d[BA::BELL..BA::BELL + 4]) as u32, bell);
        assert_eq!(d[BA::REGION], region);
        let rec = records::one(&l.logs, Kind::ANCHOR);
        assert_eq!(rec.u64("a") as i64, c.now, "the ANCHOR record carries A");
        assert_eq!(rec.u64("round"), w.tlock_round(bell));
        assert_eq!(rec.key_u64("bell") as u32, bell);
    }
}

#[test]
fn g04_tombstoned_bell_refuses_an_anchor() {
    let (mut c, w) = common::test_beacon();
    for (bell, region) in [(0u32, 0u8), (143, 15), (144, 7)] {
        w.craft_archive(&mut c, region, archive_part(bell), &[bell]);
        assert_code(w.post_anchor(&mut c, bell, region), E::Archived);
        // The same archive does not block the day's other bells.
        let other = if bell % 72 == 71 { bell - 1 } else { bell + 1 };
        expect_lands(
            w.post_anchor(&mut c, other, region),
            "PostAnchor of an untombstoned bell",
        );
    }
}

#[test]
fn g04_beacon_log_only_moves_forward() {
    let (mut c, w) = common::release();
    let rounds = w.beacons.rounds();
    let (r1, r2, r3) = (rounds[10], rounds[11], rounds[20]);
    let l = expect_lands(w.post_beacon(&mut c, 3, r2), "PostBeacon");
    assert_eq!(w.latest_round(&c, 3), r2);
    assert_eq!(records::one(&l.logs, Kind::BEACON).u64("round"), r2);
    assert_refused(
        w.post_beacon(&mut c, 3, r2),
        "PostBeacon of the logged round",
    );
    assert_refused(w.post_beacon(&mut c, 3, r1), "PostBeacon of an older round");
    assert_eq!(w.latest_round(&c, 3), r2, "unchanged by refusals");
    expect_lands(w.post_beacon(&mut c, 3, r3), "PostBeacon of a newer round");
    assert_eq!(w.latest_round(&c, 3), r3);
    assert_eq!(w.latest_round(&c, 4), 0, "one log per region");
    // A signature that is not the round's.
    let forged = w.beacons.forged(rounds[25]);
    let ix = permutation_frontier_svm_tests::ix::beacon::post_beacon(
        &w.a,
        w.keeper.pubkey(),
        3,
        &forged,
    );
    assert_code(c.send(&[ix], &[&w.keeper]), E::Crypto);
}

#[test]
fn g04_window_schedule_needs_notice_and_range() {
    let (mut c, w) = common::release();
    let now_bell = w.bell_at(c.now).expect("running");
    let ix = |win: u32, from: u32| set_window_schedule(&w.a, w.authority.pubkey(), win, from);
    // Not the authority.
    let stranger = c.funded(b"stranger", 1);
    let mut bad = ix(900, now_bell + 144);
    bad.accounts[0].pubkey = stranger.pubkey();
    assert_code(c.send(&[bad], &[&stranger]), E::Auth);
    // Range 600–1,800 s.
    assert_refused(
        c.send(&[ix(599, now_bell + 144)], &[&w.authority]),
        "window 599 s",
    );
    assert_refused(
        c.send(&[ix(1_801, now_bell + 144)], &[&w.authority]),
        "window 1,801 s",
    );
    // ≥ 144 bells' notice.
    assert_refused(
        c.send(&[ix(900, now_bell + 143)], &[&w.authority]),
        "143 bells' notice",
    );
    // Valid: stored, and W(b) switches at from_bell.
    let from = now_bell + 144;
    let l = expect_lands(
        c.send(&[ix(1_200, from)], &[&w.authority]),
        "SetWindowSchedule",
    );
    assert_eq!(w.season_u32(&c, S::WINDOW_NEXT), 1_200);
    assert_eq!(w.season_u32(&c, S::WINDOW_FROM_BELL), from);
    assert_eq!(w.window(&c, from - 1), w.params.season.reveal_window);
    assert_eq!(w.window(&c, from), 1_200);
    let rec = records::one(&l.logs, Kind::WINDOW);
    assert_eq!(rec.u64("window_next"), 1_200);
    // One change per season (v1.3 §5.7, integ-W2 review): a second change
    // is `AlreadyDone`, while the first is pending and long after it took
    // effect, and `reveal_window` is never rewritten, so `W(b)` of every
    // bell stays what it was when the bell existed.
    assert_code(
        c.send(&[ix(900, from + 10)], &[&w.authority]),
        E::AlreadyDone,
    );
    c.set_time(w.genesis_ts() + 600 * i64::from(from + 150));
    let later = w.bell_at(c.now).expect("running");
    assert!(
        later >= from + 144,
        "the first change has been in effect a day"
    );
    assert_code(
        c.send(&[ix(900, later + 144)], &[&w.authority]),
        E::AlreadyDone,
    );
    assert_eq!(
        w.season_u32(&c, S::REVEAL_WINDOW),
        w.params.season.reveal_window,
        "no fold"
    );
    assert_eq!(w.window(&c, from - 1), w.params.season.reveal_window);
    assert_eq!(w.window(&c, from), 1_200);
}

#[test]
fn g04_window_schedule_from_bell_inside_the_season() {
    // `from_bell ≥ end_bell` (including the u32::MAX "no change" sentinel,
    // which would log a change that never takes effect) is `BadData`; the
    // range refusals are `BadData` too.
    let (mut c, w) = common::release();
    let now_bell = w.bell_at(c.now).expect("running");
    let end = w.params.season.end_bell;
    let ix = |win: u32, from: u32| set_window_schedule(&w.a, w.authority.pubkey(), win, from);
    for (win, from, what) in [
        (900, u32::MAX, "from_bell = u32::MAX"),
        (900, end, "from_bell = end_bell"),
        (599, now_bell + 144, "window 599 s"),
        (1_801, now_bell + 144, "window 1,801 s"),
    ] {
        let r = c.send(&[ix(win, from)], &[&w.authority]);
        assert!(r.is_err(), "{what} accepted");
        assert_code(r, E::BadData);
    }
    assert_eq!(w.season_u32(&c, S::WINDOW_FROM_BELL), S::WINDOW_NONE);
    expect_lands(
        c.send(&[ix(900, end - 1)], &[&w.authority]),
        "SetWindowSchedule at the last bell",
    );
}

#[test]
fn g04_reveal_close_inputs_over_random_clocks() {
    // Over random landing times of THE anchor and random schedules, the
    // stored A, the Season's W(b) and the region's BeaconLog give the §5.1
    // predicate its two closing conditions: open at close − 1 with the log
    // below S(A), closed at close, and closed before close once PostBeacon
    // posts S(A).
    let (base, w) = common::test_beacon();
    let mut rng = Rng::new(0x6444);
    for case in 0..12u32 {
        let mut c = base.fork();
        let now_bell = w.bell_at(c.now).expect("running");
        let win = 600 + 60 * rng.range(0, 20) as u32;
        let from = now_bell + 144 + rng.range(0, 3) as u32;
        expect_lands(
            c.send(
                &[set_window_schedule(&w.a, w.authority.pubkey(), win, from)],
                &[&w.authority],
            ),
            "SetWindowSchedule",
        );
        let bell = from - 1 + rng.range(0, 2) as u32; // just before or at the switch
        let region = (case % 16) as u8;
        let t = World::round_time(w.tlock_round(bell)) + rng.range(0, 1_200) as i64;
        c.set_time(t);
        expect_lands(w.post_anchor(&mut c, bell, region), "PostAnchor");
        let a = w.anchor_a(&c, bell, region).expect("anchor");
        let wb = w.window(&c, bell);
        assert_eq!(
            wb,
            if bell >= from {
                win
            } else {
                w.params.season.reveal_window
            }
        );
        let margin = w.params.season.seed_margin;
        let close = kb::reveal_close(a, wb);
        let s = w.seed_round(&c, bell, region);
        assert_eq!(s, kb::seed_round(&QUICKNET, close, margin));
        let latest = w.latest_round(&c, region);
        assert!(latest < s);
        assert!(kb::reveal_open(&QUICKNET, close - 1, a, wb, margin, latest));
        assert!(!kb::reveal_open(&QUICKNET, close, a, wb, margin, latest));
        // A keeper posts S(A) to the region's log: closed even before close.
        expect_lands(w.post_beacon(&mut c, region, s), "PostBeacon S(A)");
        let latest = w.latest_round(&c, region);
        assert_eq!(latest, s);
        assert!(!kb::reveal_open(
            &QUICKNET,
            close - 1,
            a,
            wb,
            margin,
            latest
        ));
    }
}
