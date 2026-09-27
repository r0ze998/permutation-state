//! W1 smoke: a virtual-time chain at 20× walks through one bell; the
//! tlock round of the bell becomes available from the test-key replay once
//! the chain's Clock passes it, and a seal to that round opens and judges
//! valid with the served signature.

use drand_replay::{ClockSource, Replay, Source};
use fclient::beacon::TestKey;
use fclient::clock::{Drand, SeasonClock};
use fclient::ports::ChainPort;
use localnet::{Config, InProcess};

#[tokio::test]
async fn inproc_smoke_one_bell_with_the_test_key() {
    let genesis_ts = 1_785_542_400 + 600;
    let ip = InProcess::new(
        Config {
            scale: 20.0,
            g0: genesis_ts - 8,
            ..Config::default()
        },
        None,
    );
    let sc = SeasonClock {
        drand: Drand::QUICKNET,
        genesis_ts,
        reveal_window: 600,
        window_next: 600,
        window_from_bell: u32::MAX,
        seed_margin: 60,
        archive_after: 172_800,
    };
    // Walk just past the end of bell 0 (8 s per slot): T(0) is published
    // at most 3 s after the bell's end, plus the 1-s publication delay.
    ip.step(77);
    let now = ip.clock().await.unwrap().unix_timestamp;
    assert_eq!(sc.bell_at(now), Some(1));
    let t = sc.tlock_round(0);
    let replay = Replay {
        source: Source::TestKey(TestKey::new()),
        clock: ClockSource::Fixed(now),
        delay_ms: 1_000,
    };
    let early = Replay {
        source: Source::TestKey(TestKey::new()),
        clock: ClockSource::Fixed(genesis_ts + 599),
        delay_ms: 1_000,
    };
    assert!(
        early.round(t).is_err(),
        "T(0) is not public before the bell ends"
    );
    let b = replay.round(t).expect("T(0) public after the bell");
    let k = TestKey::new();
    let p = fclient::seal::Plain {
        version: 1,
        host_id: 42,
        arrive_bell: 0,
        stance: 3,
        ..Default::default()
    };
    let s = fclient::seal::seal(&fclient::seal::pack(&p), &k.pk96, t).unwrap();
    assert_eq!(
        fclient::seal::judge(&s.seal, &s.commit, &b.sig48, 42, 0),
        (0, Some(p))
    );
}
