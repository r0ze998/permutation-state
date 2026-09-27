//! Frontier shared kernels (M1 unit W1-C): `beacon` (CL-19, CL-20),
//! `addr` (CL-21, I-02), `seal` (I-06, I-27, I-28), `fees` (CL-22, I-08,
//! I-45), `office` (CL-31), `camp`, `explore`, `catalog` (I-56) and the
//! ruleset hash input.
//!
//! Vector files (`permutation-rules/vectors/`):
//! - `clock-vectors-v1.json` and `addr-vectors-v1.json` are produced by
//!   this file: `vectors_are_fresh` rebuilds them and fails if the
//!   committed copy differs; `PSF_WRITE_VECTORS=1 cargo test --test
//!   frontier_shared` rewrites them.
//! - `seal-vectors-v1.json` is produced by `vectors/seal-vectors-gen.rs`
//!   (it needs BLS12-381); `seal_vectors_match_the_kernel` recomputes every
//!   non-pairing field with the kernel.
//!
//! Randomness is a seeded xorshift (contract §3.5), no proptest.

use permutation_rules::frontier::addr::{self, SeedKind, SeedStr};
use permutation_rules::frontier::beacon::{self, BeaconClock, WindowSchedule, QUICKNET};
use permutation_rules::frontier::camp;
use permutation_rules::frontier::catalog;
use permutation_rules::frontier::clash;
use permutation_rules::frontier::doctrine::NEUTRAL;
use permutation_rules::frontier::explore;
use permutation_rules::frontier::fees::{self, DefenceParams, Evidence};
use permutation_rules::frontier::geometry::ProvinceCoord;
use permutation_rules::frontier::office::{
    counts_toward_limit, may_stand, seat_vacant, GovernanceError, GovernanceParams, TermKind,
};
use permutation_rules::frontier::seal::{self, Plain, PlainError};
use permutation_rules::frontier::terrain::generate_province;
use permutation_rules::frontier::travel;
use permutation_rules::frontier::{ruleset_hash, ruleset_hash_input};
use permutation_rules::hash::sha256;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::PathBuf;

// ------------------------------------------------------------ helpers

struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vectors")
}

/// Compares a produced vector file with the committed one, or rewrites it
/// under `PSF_WRITE_VECTORS=1`.
fn check_fresh(name: &str, produced: &str) {
    let path = vectors_dir().join(name);
    if std::env::var("PSF_WRITE_VECTORS").as_deref() == Ok("1") {
        std::fs::create_dir_all(vectors_dir()).unwrap();
        std::fs::write(&path, produced).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("{name}: {e}; run PSF_WRITE_VECTORS=1 cargo test --test frontier_shared")
    });
    assert!(
        committed == produced,
        "{name} is stale: rerun with PSF_WRITE_VECTORS=1 and review the diff"
    );
}

// ------------------------------------------------------------ a minimal JSON reader

#[derive(Debug, Clone)]
enum J {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    fn get(&self, k: &str) -> &J {
        match self {
            J::Obj(v) => {
                &v.iter()
                    .find(|(a, _)| a == k)
                    .unwrap_or_else(|| panic!("no key {k}"))
                    .1
            }
            _ => panic!("not an object"),
        }
    }
    fn s(&self) -> &str {
        match self {
            J::Str(s) => s,
            x => panic!("not a string: {x:?}"),
        }
    }
    fn i(&self) -> i64 {
        match self {
            J::Num(s) => s.parse().unwrap(),
            x => panic!("not a number: {x:?}"),
        }
    }
    fn b(&self) -> bool {
        match self {
            J::Bool(b) => *b,
            x => panic!("not a bool: {x:?}"),
        }
    }
    fn arr(&self) -> &[J] {
        match self {
            J::Arr(v) => v,
            x => panic!("not an array: {x:?}"),
        }
    }
}

fn parse_json(s: &str) -> J {
    fn ws(b: &[u8], i: &mut usize) {
        while *i < b.len() && (b[*i] as char).is_ascii_whitespace() {
            *i += 1;
        }
    }
    fn val(b: &[u8], i: &mut usize) -> J {
        ws(b, i);
        match b[*i] {
            b'{' => {
                *i += 1;
                let mut v = vec![];
                loop {
                    ws(b, i);
                    if b[*i] == b'}' {
                        *i += 1;
                        return J::Obj(v);
                    }
                    let k = match val(b, i) {
                        J::Str(s) => s,
                        _ => panic!("key"),
                    };
                    ws(b, i);
                    assert_eq!(b[*i], b':');
                    *i += 1;
                    v.push((k, val(b, i)));
                    ws(b, i);
                    if b[*i] == b',' {
                        *i += 1;
                    }
                }
            }
            b'[' => {
                *i += 1;
                let mut v = vec![];
                loop {
                    ws(b, i);
                    if b[*i] == b']' {
                        *i += 1;
                        return J::Arr(v);
                    }
                    v.push(val(b, i));
                    ws(b, i);
                    if b[*i] == b',' {
                        *i += 1;
                    }
                }
            }
            b'"' => {
                *i += 1;
                let mut s = String::new();
                while b[*i] != b'"' {
                    if b[*i] == b'\\' {
                        *i += 1;
                        s.push(match b[*i] {
                            b'n' => '\n',
                            c => c as char,
                        });
                    } else {
                        s.push(b[*i] as char);
                    }
                    *i += 1;
                }
                *i += 1;
                J::Str(s)
            }
            b't' => {
                *i += 4;
                J::Bool(true)
            }
            b'f' => {
                *i += 5;
                J::Bool(false)
            }
            b'n' => {
                *i += 4;
                J::Null
            }
            _ => {
                let st = *i;
                while *i < b.len()
                    && matches!(b[*i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                {
                    *i += 1;
                }
                J::Num(String::from_utf8(b[st..*i].to_vec()).unwrap())
            }
        }
    }
    let mut i = 0;
    val(s.as_bytes(), &mut i)
}

// ------------------------------------------------------------ CL-19 / CL-20

fn clock(g_off: i64, period: i64) -> BeaconClock {
    BeaconClock {
        genesis: QUICKNET.genesis + g_off,
        period,
    }
}

/// CL-19: all three draws round up, over 7,200 times × every drand phase:
/// `round_time(r) ≥ t + 660` and `< t + 660 + period` (Δ = 60).
#[test]
fn seed_rounds_round_up() {
    for period in [3i64, 30] {
        for g_off in 0..period {
            let c = clock(g_off, period);
            for t in 1_790_000_000..1_790_000_000 + 7_200 {
                for (r, target) in [
                    (beacon::ring_seed_round(&c, t, 60), t + 660),
                    (beacon::genesis_seed_round(&c, t, 60), t + 660),
                    (beacon::seed_round(&c, t + 600, 60), t + 660),
                ] {
                    let rt = c.round_time(r);
                    assert!(
                        rt >= target && rt < target + period,
                        "p {period} g {g_off} t {t}"
                    );
                    assert!(c.round_time(r - 1) < target, "not the first such round");
                }
            }
        }
    }
}

/// SP-V2 `seed_round_margin`, on the kernel functions: the seed round is
/// published ≥ Δ after the reveal close A + 600 and is the first such
/// round, for every anchor time and drand genesis offset.
#[test]
fn spv2_seed_round_margin_vectors_pass() {
    for period in [3i64, 30] {
        for g_off in 0..period {
            let c = clock(g_off, period);
            for a in 1_790_000_000..1_790_000_000 + 7_200 {
                let close = beacon::reveal_close(a, 600);
                assert_eq!(close, a + 600);
                let sr = beacon::seed_round(&c, close, 60);
                let t = c.round_time(sr);
                assert!(t >= close + 60);
                assert!(c.round_time(sr - 1) < close + 60);
            }
        }
    }
    // SP-V2 reveal_close_boundary.
    let a = 1_790_000_123;
    assert!(a + 599 < beacon::reveal_close(a, 600));
    assert!(a + 600 >= beacon::reveal_close(a, 600));
}

/// CL-20: `0 ≤ round_time(T(b)) − bell_end(b) < period`.
#[test]
fn tlock_round_is_at_or_after_bell_end() {
    for period in [3i64, 30] {
        for g_off in 0..period {
            let c = clock(g_off, period);
            for gts in [1_790_000_000i64, 1_790_000_001, 1_790_000_002] {
                for b in (0..4_032u32).chain([u32::MAX / 2]) {
                    let t = beacon::tlock_round(&c, gts, b);
                    let d = c.round_time(t) - beacon::bell_end(gts, b);
                    assert!((0..period).contains(&d), "p {period} g {g_off} b {b}: {d}");
                }
            }
        }
    }
}

/// CL-20: the roster freeze and posture close are `bell_start(b)`, which
/// does not depend on the drand phase, while `T(b)` does. **A definition
/// pin**, not the property: that the program freezes rosters at
/// `bell_start` rather than at a `T(b)`-derived time is a program-level
/// test (G-gate, W2-A/W3 owners).
#[test]
fn cutoffs_do_not_depend_on_round_phase() {
    let gts = 1_790_000_000;
    let mut t_rounds = HashSet::new();
    for g_off in 0..3 {
        let c = clock(g_off, 3);
        for b in 0..1_008u32 {
            assert_eq!(beacon::bell_start(gts, b), gts + 600 * b as i64);
            assert_eq!(beacon::bell_start(gts, b), travel::bell_start(gts, b));
            assert_eq!(beacon::bell_end(gts, b), beacon::bell_start(gts, b + 1));
        }
        t_rounds.insert(beacon::tlock_round(&c, gts, 0));
    }
    // T(0) moves with the phase (the cutoff above did not).
    assert!(t_rounds.len() > 1);
}

#[test]
fn bells_days_and_windows() {
    let gts = 1_790_000_000;
    assert_eq!(beacon::bell_at(gts, gts - 1), None);
    assert_eq!(beacon::bell_at(gts, gts + 599), Some(0));
    assert_eq!(beacon::bell_at(gts, gts + 600), Some(1));
    assert_eq!(beacon::day(143), 0);
    assert_eq!(beacon::day(144), 1);
    assert_eq!(beacon::day_start_bell(7), 1_008);
    let s = WindowSchedule {
        reveal_window: 600,
        window_next: 1_800,
        window_from_bell: 1_000,
    };
    assert_eq!(beacon::window(&s, 999), 600);
    assert_eq!(beacon::window(&s, 1_000), 1_800);
    // A genesis round's season starts 600 s after it.
    let r = beacon::genesis_seed_round(&QUICKNET, 1_790_000_000, 60);
    assert_eq!(
        beacon::genesis_ts(&QUICKNET, r),
        QUICKNET.round_time(r) + 600
    );
}

fn clock_vectors() -> String {
    let c = QUICKNET;
    let mut s = String::new();
    s.push_str("{\n  \"version\": 1,\n");
    let _ = writeln!(
        s,
        "  \"clock\": {{\"genesis\": {}, \"period\": {}}},",
        c.genesis, c.period
    );
    s.push_str("  \"note\": \"T(b) = first round at or after bell_end(b); S = first round at or after close + margin; ring/genesis = first round at or after t + 600 + margin (contract 5.1). phase = genesis_ts offset against the drand round grid.\",\n");
    // Bells, over every drand phase.
    s.push_str("  \"bells\": [\n");
    let base = c.round_time(32_556_137);
    let mut rows = vec![];
    for phase in 0..3i64 {
        let gts = base + phase;
        for b in [0u32, 1, 2, 143, 144, 1_007, 1_008, 4_031, u32::MAX] {
            let t = beacon::tlock_round(&c, gts, b);
            rows.push(format!(
                "    {{\"phase\": {phase}, \"genesis_ts\": {gts}, \"bell\": {b}, \"bell_start\": {}, \"bell_end\": {}, \"day\": {}, \"tlock_round\": {t}, \"tlock_round_time\": {}}}",
                beacon::bell_start(gts, b),
                beacon::bell_end(gts, b),
                beacon::day(b),
                c.round_time(t)
            ));
        }
    }
    s.push_str(&rows.join(",\n"));
    s.push_str("\n  ],\n");
    // Seed rounds.
    s.push_str("  \"seed_rounds\": [\n");
    let mut rows = vec![];
    for a in base..base + 6 {
        for w in [600u32, 1_800] {
            for margin in [60u32, 61, 90] {
                let close = beacon::reveal_close(a, w);
                let r = beacon::seed_round(&c, close, margin);
                rows.push(format!(
                    "    {{\"anchor_ts\": {a}, \"window\": {w}, \"close\": {close}, \"margin\": {margin}, \"seed_round\": {r}, \"round_time\": {}}}",
                    c.round_time(r)
                ));
            }
        }
    }
    s.push_str(&rows.join(",\n"));
    s.push_str("\n  ],\n");
    for (name, f) in [
        (
            "ring_seed_rounds",
            beacon::ring_seed_round as fn(&BeaconClock, i64, u32) -> u64,
        ),
        ("genesis_seed_rounds", beacon::genesis_seed_round),
    ] {
        let _ = writeln!(s, "  \"{name}\": [");
        let mut rows = vec![];
        for t in base..base + 6 {
            for margin in [60u32, 90] {
                let r = f(&c, t, margin);
                rows.push(format!(
                    "    {{\"t\": {t}, \"margin\": {margin}, \"round\": {r}, \"round_time\": {}}}",
                    c.round_time(r)
                ));
            }
        }
        s.push_str(&rows.join(",\n"));
        s.push_str("\n  ],\n");
    }
    // Genesis timestamps.
    s.push_str("  \"genesis_ts\": [\n");
    let rows: Vec<String> = [1u64, 32_556_137, 32_556_350]
        .iter()
        .map(|&r| {
            format!(
                "    {{\"genesis_round\": {r}, \"genesis_ts\": {}}}",
                beacon::genesis_ts(&c, r)
            )
        })
        .collect();
    s.push_str(&rows.join(",\n"));
    s.push_str("\n  ],\n");
    // Window schedule.
    s.push_str("  \"windows\": [\n");
    let sch = WindowSchedule {
        reveal_window: 600,
        window_next: 900,
        window_from_bell: 200,
    };
    let rows: Vec<String> = [0u32, 199, 200, u32::MAX]
        .iter()
        .map(|&b| {
            format!(
                "    {{\"reveal_window\": 600, \"window_next\": 900, \"window_from_bell\": 200, \"bell\": {b}, \"window\": {}}}",
                beacon::window(&sch, b)
            )
        })
        .collect();
    s.push_str(&rows.join(",\n"));
    s.push_str("\n  ]\n}\n");
    s
}

// ------------------------------------------------------------ CL-21

/// SP-V2 `acct.rs`, transcribed verbatim (the reference the grammar must
/// match byte for byte, I-02).
mod spv2 {
    pub struct SeedStr {
        buf: [u8; 32],
        len: usize,
    }
    impl SeedStr {
        pub fn new(tag: &[u8; 2], raw: &[u8]) -> SeedStr {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let mut buf = [0u8; 32];
            buf[..2].copy_from_slice(tag);
            let mut n = 2;
            for &b in raw.iter().take(15) {
                buf[n] = HEX[(b >> 4) as usize];
                buf[n + 1] = HEX[(b & 15) as usize];
                n += 2;
            }
            SeedStr { buf, len: n }
        }
        pub fn as_bytes(&self) -> &[u8] {
            &self.buf[..self.len]
        }
    }
    pub fn anchor_seed(bell: u32, region: u8) -> SeedStr {
        let mut raw = [0u8; 5];
        raw[..4].copy_from_slice(&bell.to_le_bytes());
        raw[4] = region;
        SeedStr::new(b"an", &raw)
    }
    pub fn cache_seed(bell: u32, region: u8, nonce: u8) -> SeedStr {
        let mut raw = [0u8; 6];
        raw[..4].copy_from_slice(&bell.to_le_bytes());
        raw[4] = region;
        raw[5] = nonce;
        SeedStr::new(b"sd", &raw)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn slot_seed(tag: &[u8; 2], p: i32, q: i32, bell: u32, x: u8, y: u8, n: usize) -> SeedStr {
        let mut raw = [0u8; 14];
        raw[..4].copy_from_slice(&p.to_le_bytes());
        raw[4..8].copy_from_slice(&q.to_le_bytes());
        raw[8..12].copy_from_slice(&bell.to_le_bytes());
        raw[12] = x;
        raw[13] = y;
        SeedStr::new(tag, &raw[..n])
    }
    pub fn arrival_seed(p: i32, q: i32, bell: u32, f: u8, i: u8) -> SeedStr {
        slot_seed(b"ar", p, q, bell, f, i, 14)
    }
    pub fn posture_seed(p: i32, q: i32, bell: u32, pos: u8) -> SeedStr {
        slot_seed(b"po", p, q, bell, pos, 0, 13)
    }
    pub fn inputs_seed(p: i32, q: i32, bell: u32) -> SeedStr {
        slot_seed(b"ci", p, q, bell, 0, 0, 12)
    }
    pub fn archive_seed(region: u8, day: u32) -> SeedStr {
        let mut raw = [0u8; 5];
        raw[0] = region;
        raw[1..].copy_from_slice(&day.to_le_bytes());
        SeedStr::new(b"aa", &raw)
    }
    pub fn verdict_seed(host: u64, bell: u32) -> SeedStr {
        let mut raw = [0u8; 12];
        raw[..8].copy_from_slice(&host.to_le_bytes());
        raw[8..].copy_from_slice(&bell.to_le_bytes());
        SeedStr::new(b"sv", &raw)
    }
}

/// I-02 / CL-21: the grammar matches SP-V2 `acct.rs` byte for byte for
/// `an`, `sd`, `ar`, `po`, `ci`, `sv`, `aa` (edge values and 200,000 random
/// keys each).
#[test]
fn seeds_match_spv2_byte_for_byte() {
    let mut r = XorShift(0x5eed_5eed_0000_0001);
    let edges_i = [i32::MIN, -1, 0, 1, i32::MAX, -65_536, 65_536];
    let edges_u = [0u32, 1, 143, 144, u32::MAX];
    for &p in &edges_i {
        for &q in &edges_i {
            for &b in &edges_u {
                for x in [0u8, 5, 255] {
                    assert_eq!(
                        addr::slot(p, q, b, x, x ^ 3).as_bytes(),
                        spv2::arrival_seed(p, q, b, x, x ^ 3).as_bytes()
                    );
                    assert_eq!(
                        addr::posture(p, q, b, x).as_bytes(),
                        spv2::posture_seed(p, q, b, x).as_bytes()
                    );
                }
                assert_eq!(
                    addr::clash_inputs(p, q, b).as_bytes(),
                    spv2::inputs_seed(p, q, b).as_bytes()
                );
            }
        }
    }
    for _ in 0..200_000 {
        let (a, b) = (r.next(), r.next());
        let (p, q, bell) = (a as i32, (a >> 32) as i32, b as u32);
        let (x, y) = ((b >> 32) as u8, (b >> 40) as u8);
        assert_eq!(
            addr::anchor(bell, x).as_bytes(),
            spv2::anchor_seed(bell, x).as_bytes()
        );
        assert_eq!(
            addr::seed_cache(bell, x, y).as_bytes(),
            spv2::cache_seed(bell, x, y).as_bytes()
        );
        assert_eq!(
            addr::slot(p, q, bell, x, y).as_bytes(),
            spv2::arrival_seed(p, q, bell, x, y).as_bytes()
        );
        assert_eq!(
            addr::posture(p, q, bell, x).as_bytes(),
            spv2::posture_seed(p, q, bell, x).as_bytes()
        );
        assert_eq!(
            addr::clash_inputs(p, q, bell).as_bytes(),
            spv2::inputs_seed(p, q, bell).as_bytes()
        );
        assert_eq!(
            addr::anchor_archive(x, bell).as_bytes(),
            spv2::archive_seed(x, bell).as_bytes()
        );
        assert_eq!(
            addr::seal_verdict_reserved(a, bell).as_bytes(),
            spv2::verdict_seed(a, bell).as_bytes()
        );
    }
}

fn all_seeds_at(r: &mut XorShift) -> Vec<SeedStr> {
    let (a, b, c) = (r.next(), r.next(), r.next());
    let (p, q, bell) = (a as i32, (a >> 32) as i32, b as u32);
    let (x, y) = ((b >> 32) as u8, (b >> 40) as u8);
    let mut w = [0u8; 32];
    w[..8].copy_from_slice(&c.to_le_bytes());
    w[8..16].copy_from_slice(&a.to_le_bytes());
    vec![
        addr::frontier(),
        addr::ring_seed(bell as u16),
        addr::province_fund(x),
        addr::join_shard(x, y),
        addr::beacon_log(x),
        addr::defence_pool(),
        addr::citizen(&w),
        addr::holding(p, q, x),
        addr::province(p, q),
        addr::slot(p, q, bell, x, y),
        addr::arrival_day(p, q, bell),
        addr::clash_inputs(p, q, bell),
        addr::seal_verdict_reserved(c, bell),
        addr::anchor(bell, x),
        addr::seed_cache(bell, x, y),
        addr::anchor_archive(x, bell),
        addr::defence_claim(&w, bell),
        addr::posture(p, q, bell, x),
    ]
}

/// CL-21: every seed ≤ 32 B at the extremes (i32::MIN/MAX coordinates,
/// u32::MAX bell, u64::MAX host).
#[test]
fn seed_strings_fit_32_bytes() {
    let w = [0xffu8; 32];
    let longest = [
        addr::slot(i32::MIN, i32::MAX, u32::MAX, 255, 255),
        addr::posture(i32::MIN, i32::MAX, u32::MAX, 255),
        addr::clash_inputs(i32::MIN, i32::MAX, u32::MAX),
        addr::arrival_day(i32::MAX, i32::MIN, u32::MAX),
        addr::holding(i32::MIN, i32::MIN, 255),
        addr::seal_verdict_reserved(u64::MAX, u32::MAX),
        addr::defence_claim(&w, u32::MAX),
        addr::citizen(&w),
        addr::seed_cache(u32::MAX, 255, 255),
    ];
    for s in longest {
        assert!(s.len() <= 32, "{s:?}");
    }
    assert_eq!(addr::slot(i32::MIN, i32::MAX, u32::MAX, 5, 3).len(), 30);
    assert_eq!(addr::citizen(&w).len(), 32);
    let mut r = XorShift(7);
    for _ in 0..10_000 {
        for s in all_seeds_at(&mut r) {
            assert!(s.len() <= 32);
        }
    }
}

/// CL-21: no two keys share a seed. Per kind: 1,000,000 random keys parse
/// back to exactly their raw bytes (so the map key → seed is injective),
/// every kind has one fixed length and a distinct tag (so kinds cannot
/// collide); plus a direct collision check over 20,000 × 18 seeds.
fn raw_of(k: SeedKind) -> Vec<u8> {
    vec![7u8; k.raw_len()]
}

#[test]
fn seed_strings_are_injective() {
    let mut r = XorShift(0x1234_5678_9abc_def1);
    for _ in 0..1_000_000 {
        let (a, b) = (r.next(), r.next());
        let mut raw = [0u8; 15];
        raw[..8].copy_from_slice(&a.to_le_bytes());
        raw[8..15].copy_from_slice(&b.to_le_bytes()[..7]);
        for k in SeedKind::ALL {
            let (buf, n) = addr::seed(k, &raw[..k.raw_len()]);
            assert_eq!(n, k.seed_len());
            let (kk, back, m) = addr::parse_seed(&buf[..n]).unwrap();
            assert_eq!((kk, m), (k, k.raw_len()));
            assert_eq!(&back[..m], &raw[..m]);
        }
    }
    // A raw key of the wrong length builds no seed (integ-W1 review): not a
    // shorter one ("ho01") and not a truncated one.
    for k in SeedKind::ALL {
        for len in [0, k.raw_len().saturating_sub(1), k.raw_len() + 1, 40] {
            if len == k.raw_len() {
                continue;
            }
            let raw = vec![1u8; len];
            assert_eq!(addr::seed(k, &raw).1, 0, "{k:?} with {len} bytes");
            assert!(addr::try_seed(k, &raw).is_none());
        }
        assert!(addr::try_seed(k, &raw_of(k)).is_some());
    }
    let tags: HashSet<&[u8; 2]> = SeedKind::ALL.iter().map(|k| k.tag()).collect();
    assert_eq!(tags.len(), SeedKind::ALL.len());
    // Direct check: a seed string never names two kinds.
    let mut seen = std::collections::HashMap::new();
    let mut r = XorShift(99);
    for _ in 0..20_000 {
        for (i, s) in all_seeds_at(&mut r).into_iter().enumerate() {
            assert_eq!(*seen.entry(s.as_bytes().to_vec()).or_insert(i), i, "{s:?}");
        }
    }
}

/// **A definition pin**: `with_seed_address` is compared with the
/// expression it is defined as; the independent checks are the SP-V2
/// transcriptions and frontier-abi's `addresses.json`.
#[test]
fn with_seed_addresses_and_tags() {
    let base = sha256(&[b"season-pda"]);
    let program = sha256(&[b"program-id"]);
    let s = addr::anchor(7, 3);
    let a = addr::with_seed_address(&base, s.as_bytes(), &program);
    assert_eq!(a, sha256(&[&base, s.as_bytes(), &program]));
    let wallet = [3u8; 32];
    assert_eq!(
        &addr::citizen_tag15(&wallet)[..],
        &sha256(&[b"PSF-CIT", &wallet])[..15]
    );
    assert_eq!(
        &addr::keeper_tag8(&wallet)[..],
        &sha256(&[b"PSF-KPR", &wallet])[..8]
    );
    let cit = addr::with_seed_address(&base, addr::citizen(&wallet).as_bytes(), &program);
    assert_eq!(
        addr::citizen_tag(&cit),
        u64::from_le_bytes(cit[..8].try_into().unwrap())
    );
}

fn addr_vectors() -> String {
    let mut s = String::from("{\n  \"version\": 1,\n");
    s.push_str("  \"grammar\": \"seed = tag(2 ASCII) || lowercase hex of the little-endian fixed-width key fields (<= 15 raw bytes, <= 32 B); address = sha256(season_pda || seed || program_id) (contract 4.1; SP-V2 acct.rs byte for byte)\",\n");
    s.push_str("  \"kinds\": [\n");
    let rows: Vec<String> = SeedKind::ALL
        .iter()
        .map(|k| {
            format!(
                "    {{\"kind\": \"{k:?}\", \"tag\": \"{}\", \"raw_len\": {}, \"seed_len\": {}}}",
                std::str::from_utf8(k.tag()).unwrap(),
                k.raw_len(),
                k.seed_len()
            )
        })
        .collect();
    s.push_str(&rows.join(",\n"));
    s.push_str("\n  ],\n  \"seeds\": [\n");
    let w0 = [0u8; 32];
    let w1 = [0xffu8; 32];
    let w2: [u8; 32] = core::array::from_fn(|i| i as u8);
    let mut rows = vec![];
    let mut row = |kind: &str, args: String, sd: SeedStr| {
        rows.push(format!(
            "    {{\"kind\": \"{kind}\", \"args\": {{{args}}}, \"seed\": \"{}\", \"len\": {}}}",
            std::str::from_utf8(sd.as_bytes()).unwrap(),
            sd.len()
        ));
    };
    row("fr", String::new(), addr::frontier());
    row("dp", String::new(), addr::defence_pool());
    for d in [0u16, 2, 128, u16::MAX] {
        row("rs", format!("\"d\": {d}"), addr::ring_seed(d));
    }
    for w in [0u8, 5, 255] {
        row("pf", format!("\"wedge\": {w}"), addr::province_fund(w));
    }
    for (f, sh) in [(0u8, 0u8), (5, 7), (255, 255)] {
        row(
            "js",
            format!("\"faction\": {f}, \"shard\": {sh}"),
            addr::join_shard(f, sh),
        );
    }
    for rg in [0u8, 15, 255] {
        row("bl", format!("\"region\": {rg}"), addr::beacon_log(rg));
    }
    for w in [&w0, &w1, &w2] {
        row(
            "ct",
            format!("\"wallet\": \"{}\"", hex(w)),
            addr::citizen(w),
        );
    }
    let coords = [
        (0i32, 0i32),
        (-1, 2),
        (128, -128),
        (i32::MIN, i32::MAX),
        (i32::MAX, i32::MIN),
    ];
    for (p, q) in coords {
        row(
            "pv",
            format!("\"p\": {p}, \"q\": {q}"),
            addr::province(p, q),
        );
        for site in [0u8, 11, 255] {
            row(
                "ho",
                format!("\"p\": {p}, \"q\": {q}, \"site\": {site}"),
                addr::holding(p, q, site),
            );
        }
        for bell in [0u32, 144, u32::MAX] {
            row(
                "ar",
                format!("\"p\": {p}, \"q\": {q}, \"bell\": {bell}, \"faction\": 5, \"i\": 3"),
                addr::slot(p, q, bell, 5, 3),
            );
            row(
                "ad",
                format!("\"p\": {p}, \"q\": {q}, \"day\": {bell}"),
                addr::arrival_day(p, q, bell),
            );
            row(
                "ci",
                format!("\"p\": {p}, \"q\": {q}, \"bell\": {bell}"),
                addr::clash_inputs(p, q, bell),
            );
            row(
                "po",
                format!("\"p\": {p}, \"q\": {q}, \"bell\": {bell}, \"pos\": 59"),
                addr::posture(p, q, bell, 59),
            );
        }
    }
    for (h, b) in [
        (0u64, 0u32),
        (u64::MAX, u32::MAX),
        (0x0123_4567_89ab_cdef, 1_008),
    ] {
        row(
            "sv",
            format!("\"host_id\": \"{h}\", \"arrive_bell\": {b}"),
            addr::seal_verdict_reserved(h, b),
        );
    }
    for (b, rg) in [(0u32, 0u8), (1_008, 15), (u32::MAX, 255)] {
        row(
            "an",
            format!("\"bell\": {b}, \"region\": {rg}"),
            addr::anchor(b, rg),
        );
        for n in [0u8, 255] {
            row(
                "sd",
                format!("\"bell\": {b}, \"region\": {rg}, \"nonce\": {n}"),
                addr::seed_cache(b, rg, n),
            );
        }
        row(
            "aa",
            format!("\"region\": {rg}, \"day\": {b}"),
            addr::anchor_archive(rg, b),
        );
    }
    for w in [&w0, &w1, &w2] {
        for d in [0u32, 6, u32::MAX] {
            row(
                "dc",
                format!("\"beneficiary\": \"{}\", \"day\": {d}", hex(w)),
                addr::defence_claim(w, d),
            );
        }
    }
    s.push_str(&rows.join(",\n"));
    s.push_str("\n  ],\n  \"tags\": [\n");
    let rows: Vec<String> = [&w0, &w1, &w2]
        .iter()
        .map(|w| {
            format!(
                "    {{\"wallet\": \"{}\", \"citizen_tag15\": \"{}\", \"keeper_tag8\": \"{}\"}}",
                hex(*w),
                hex(&addr::citizen_tag15(w)),
                hex(&addr::keeper_tag8(w))
            )
        })
        .collect();
    s.push_str(&rows.join(",\n"));
    s.push_str("\n  ],\n  \"host_ids\": [\n");
    let mut rows = vec![];
    for (p, q, site, gen, seq) in [
        (0i32, 0i32, 0u8, 0u8, 0u32),
        (1, 0, 1, 0, 1),
        (2, -1, 4, 0, 7),
        (-128, 64, 11, 255, u32::MAX),
        (128, -128, 11, 255, u32::MAX),
        (0, 128, 5, 3, 12_345),
        (129, 0, 0, 0, 0),
        (0, 0, 12, 0, 0),
    ] {
        let id = addr::host_id(p, q, site, gen, seq);
        let idx = addr::host_parts(id).map_or("null".to_string(), |h| h.province_index.to_string());
        rows.push(format!(
            "    {{\"p\": {p}, \"q\": {q}, \"site\": {site}, \"gen\": {gen}, \"seq\": {seq}, \"host_id\": \"{id}\", \"province_index\": {idx}}}"
        ));
    }
    s.push_str(&rows.join(",\n"));
    s.push_str("\n  ],\n  \"addresses\": [\n");
    let base = sha256(&[b"PSF-VECTOR-SEASON-PDA"]);
    let program = sha256(&[b"PSF-VECTOR-PROGRAM-ID"]);
    let mut rows = vec![];
    for sd in [
        addr::frontier(),
        addr::anchor(1_008, 7),
        addr::slot(-3, 5, 1_010, 2, 1),
        addr::citizen(&w2),
        addr::holding(2, -1, 4),
    ] {
        rows.push(format!(
            "    {{\"base\": \"{}\", \"program\": \"{}\", \"seed\": \"{}\", \"address\": \"{}\"}}",
            hex(&base),
            hex(&program),
            std::str::from_utf8(sd.as_bytes()).unwrap(),
            hex(&addr::with_seed_address(&base, sd.as_bytes(), &program))
        ));
    }
    s.push_str(&rows.join(",\n"));
    s.push_str("\n  ]\n}\n");
    s
}

#[test]
fn vectors_are_fresh() {
    check_fresh("clock-vectors-v1.json", &clock_vectors());
    check_fresh("addr-vectors-v1.json", &addr_vectors());
    // The clock file agrees with the clash kernel's QUICKNET (sanity).
    let j = parse_json(&clock_vectors());
    assert_eq!(j.get("clock").get("genesis").i(), clash::QUICKNET.genesis);
    for row in j.get("bells").arr() {
        let d = row.get("tlock_round_time").i() - row.get("bell_end").i();
        if row.get("bell").i() != u32::MAX as i64 {
            assert!((0..3).contains(&d));
        }
    }
}

// ------------------------------------------------------------ seal (I-06, I-27, I-28)

#[test]
fn seal_vectors_match_the_kernel() {
    let text = std::fs::read_to_string(vectors_dir().join("seal-vectors-v1.json")).unwrap();
    let j = parse_json(&text);
    assert_eq!(j.get("version").i(), 1);
    let cases = j.get("cases").arr();
    assert!(cases.len() >= 24);
    let mut classes = HashSet::new();
    let mut refusals = HashSet::new();
    for c in cases {
        let name = c.get("name").s();
        let expect = c.get("expect").s();
        classes.insert(expect.to_string());
        let plain: [u8; 37] = unhex(c.get("plain").s()).try_into().unwrap();
        let salt: [u8; 32] = unhex(c.get("salt").s()).try_into().unwrap();
        let k: [u8; 16] = unhex(c.get("k").s()).try_into().unwrap();
        let commit: [u8; 32] = unhex(c.get("commit").s()).try_into().unwrap();
        let sl: [u8; 165] = unhex(c.get("seal").s()).try_into().unwrap();
        let ct_hash: [u8; 32] = unhex(c.get("ct_hash").s()).try_into().unwrap();
        let root: [u8; 32] = unhex(c.get("seal_root").s()).try_into().unwrap();
        // The web never stores k; the salt it sends is salt_of(k).
        assert_eq!(seal::salt_of(&k), salt, "{name}");
        assert_eq!(seal::ct_hash(&sl), ct_hash, "{name}");
        assert_eq!(seal::seal_root(&commit, &ct_hash), root, "{name}");
        // Validate (I-28) as the generator recorded it.
        let pt = seal::unpack(&plain);
        assert_eq!(seal::pack(&pt), plain);
        let host: u64 = c.get("host_id").s().parse().unwrap();
        let arrive = c.get("arrive_bell").i() as u32;
        let v = match seal::validate(&pt, host, arrive) {
            Ok(()) => "ok".to_string(),
            Err(e) => format!("{e:?}"),
        };
        assert_eq!(v, c.get("validate").s(), "{name}");
        refusals.insert(v.clone());
        let committed = seal::commit(&plain, &salt) == commit;
        let body_opens = seal::body_xor(&k, &seal::seal_body(&sl)) == plain;
        match expect {
            "valid" => {
                assert!(committed && body_opens && v == "ok", "{name}");
                assert_eq!(seal::open_body(&k, &sl, &commit), Some(plain));
                assert!(c.get("stock_tlock_opens").b());
                // A season where T(arrive) is exactly this round.
                let gts = c.get("genesis_ts").i();
                assert_eq!(
                    beacon::tlock_round(&QUICKNET, gts, arrive),
                    c.get("round").i() as u64,
                    "{name}"
                );
            }
            "bad_plaintext" => {
                assert!(committed && body_opens && v != "ok", "{name}");
                assert!(c.get("stock_tlock_opens").b());
            }
            "commit_mismatch" => {
                assert!(c.get("stock_tlock_opens").b(), "{name}");
                assert_eq!(seal::open_body(&k, &sl, &commit), None, "{name}");
            }
            _ => assert!(!c.get("stock_tlock_opens").b(), "{name}"),
        }
    }
    for cl in ["valid", "bad_plaintext", "commit_mismatch", "fo_fail"] {
        assert!(classes.contains(cl), "missing class {cl}");
    }
    // Every refusal of `seal::validate` has a vector (integ-W1 review:
    // Tile, Direction, PathTooLong, HostMismatch, ArriveMismatch were
    // missing), so every twin (fclient, web) that consumes the file is
    // checked against each rule.
    for e in [
        "Version",
        "Reserved",
        "HostMismatch",
        "ArriveMismatch",
        "PathTooLong",
        "PathBits",
        "Direction",
        "Tile",
        "Stance",
        "Retreat",
    ] {
        assert!(refusals.contains(e), "no seal vector for {e}");
    }
}

#[test]
fn retreat_encoding_is_pinned() {
    // I-27: 0 = never, 1..=60,000 a ratio, above invalid.
    assert_eq!(seal::RETREAT_MAX_BPS, 60_000);
    let (n, path) = seal::encode_path(&[0, 1]).unwrap();
    let mut p = Plain {
        version: 1,
        host_id: 9,
        arrive_bell: 10,
        dest_p: 0,
        dest_q: 0,
        dest_tile: 0,
        stance: 0,
        retreat_bps: 0,
        path_len: n,
        path,
        reserved: [0; 3],
    };
    assert_eq!(seal::validate(&p, 9, 10), Ok(()));
    p.retreat_bps = u16::MAX;
    assert_eq!(seal::validate(&p, 9, 10), Err(PlainError::Retreat));
    // Directions are hex::DIRECTIONS order: 0 = E (+q), then counter-clockwise.
    assert_eq!(permutation_rules::hex::DIRECTIONS[0], (1, 0));
}

// ------------------------------------------------------------ fees (CL-22, I-08, I-45)

/// CL-22: the SP-FEE default tip (10,000 lamports) spent whole on a Reveal
/// with the 64-KiB loaded limit has priority 0.433 / 0.35 / 0.27 at
/// 16k / 20k / 26k CU (DESIGN §8.7), and `min_tip_lamports` is the least
/// tip reaching a priority.
#[test]
fn min_tip_matches_priority() {
    for (limit, want) in [(16_000u32, 433u64), (20_000, 351), (26_000, 274)] {
        let got = fees::tip_priority_milli(10_000, limit, 65_536);
        assert!(got.abs_diff(want) <= 1, "{limit}: {got}");
        // Same through the general formula: a keeper pays fee = tip − 5,000.
        let c = fees::cost(limit, 1, 2, 65_536);
        assert_eq!(fees::priority_milli(10_000 - 5_000, c), got);
    }
    for p in [100u32, 270, 350, 433, 1_000, 2_000] {
        for limit in [16_000u32, 20_000, 26_000, 40_000] {
            for loaded in [65_536u32, 1 << 20, 4 << 20] {
                let tip = fees::min_tip_lamports(p, limit, loaded);
                assert!(fees::tip_priority_milli(tip, limit, loaded) >= p as u64);
                assert!(fees::tip_priority_milli(tip - 1, limit, loaded) < p as u64);
            }
        }
    }
    // §10.1 pinned values.
    assert_eq!(fees::min_tip_lamports(433, 26_000, 1 << 20), 14_441);
    assert_eq!(fees::min_tip_lamports(433, 16_000, 1 << 20), 10_111);
}

#[test]
fn fee_round_trips_and_loaded_limit() {
    for p in [1u64, 433, 500, 2_000] {
        for limit in [16_000u32, 26_000, 60_000, 340_000] {
            let c = fees::cost(limit, 1, 3, 1 << 20);
            let fee = fees::fee_for_priority(p, c);
            let price = fees::cu_price_micro(fee, limit);
            let paid = fees::fee_of_price(price, limit);
            assert!(paid >= fee);
            if fee > 0 {
                assert!(fees::priority_milli(paid, c) >= p);
            }
        }
    }
    // I-45: L = round_up(programdata + 45 + Σ(data + 64), 32 KiB).
    // SP-V2's 540,608-B kprobe deployed at 1.25×: 675,840 B programdata.
    let pd = fees::deploy_max_len(540_608);
    let l = fees::loaded_limit(pd, 1_280 + 160 + 144 + 128 + 96, 7);
    assert_eq!(l % 32_768, 0);
    assert!(l as u64 >= pd as u64 + 45 + 1_808 + 7 * 64);
    assert!((l as u64) < pd as u64 + 45 + 1_808 + 7 * 64 + 32_768);
}

#[test]
fn defence_refund_follows_the_formula() {
    let s = DefenceParams {
        defence_cap_milli: 2_000,
        tip_min: 14_441,
    };
    // Paid at exactly the tip level: nothing to refund.
    let ev = |price: u64, day: bool| Evidence {
        price_micro: price,
        limit: 26_000,
        loaded: 1 << 20,
        created_day: day,
    };
    assert_eq!(fees::defence_refund(&ev(0, false), &s), 0);
    let tip_fee = s.tip_min - 2_500; // 11,941
    let price_at_tip = tip_fee * 1_000_000 / 26_000;
    assert_eq!(fees::defence_refund(&ev(price_at_tip, false), &s), 0);
    // Above: the excess over the tip level.
    let paid = fees::fee_of_price(2 * price_at_tip, 26_000);
    assert_eq!(
        fees::defence_refund(&ev(2 * price_at_tip, false), &s),
        paid - tip_fee
    );
    // The paid fee rounds up like the runtime's (1 lamport above floor
    // here: 389,639 µlamports × 294,471 CU).
    assert_eq!(fees::fee_of_price(389_639, 294_471), 114_738);
    assert_eq!(fees::fee_of_price(1, 1), 1);
    assert_eq!(fees::fee_of_price(1_000_000, 1), 1);
    // Capped at priority 2.0: cost 27,576 (+300 with the ArrivalDay).
    let huge = ev(u64::MAX / 1_000_000, false);
    assert_eq!(
        fees::defence_refund(&huge, &s),
        2 * 27_576 - 2_500 - tip_fee
    );
    let huge_day = ev(u64::MAX / 1_000_000, true);
    assert_eq!(
        fees::defence_refund(&huge_day, &s),
        2 * 27_876 - 2_500 - tip_fee
    );
}

// ------------------------------------------------------------ office (CL-31)

#[test]
fn second_office_term_refused() {
    let p = GovernanceParams::default();
    assert_eq!(p.validate(), Ok(()));
    assert!(may_stand(0, &p));
    assert!(!may_stand(1, &p));
    assert!(!may_stand(u8::MAX, &p));
    assert_eq!(
        GovernanceParams {
            office_terms_per_wallet: 2
        }
        .validate(),
        Err(GovernanceError::TermLimit)
    );
    assert_eq!(
        GovernanceParams {
            office_terms_per_wallet: 0
        }
        .validate(),
        Err(GovernanceError::TermLimit)
    );
    // By-election and recalled terms count; the caretaker term does not.
    assert!(counts_toward_limit(TermKind::Elected));
    assert!(counts_toward_limit(TermKind::ByElection));
    assert!(counts_toward_limit(TermKind::Recalled));
    assert!(!counts_toward_limit(TermKind::Caretaker));
    let mut used = 0u8;
    for k in [TermKind::Caretaker, TermKind::Recalled] {
        assert!(may_stand(used, &p));
        used += counts_toward_limit(k) as u8;
    }
    assert!(!may_stand(used, &p));
    assert!(seat_vacant(0) && !seat_vacant(1));
}

// ------------------------------------------------------------ camps, explore, catalog (I-56)

#[test]
fn camps_are_deterministic_and_bounded() {
    let seed = sha256(&[b"ring-seed-vector"]);
    let mut troops = [0u32; 2];
    let mut n = 0u32;
    for (p, q) in [(2, 0), (-2, 2), (3, -1), (0, -4), (6, -6), (-5, 1)] {
        let c = ProvinceCoord::new(p, q);
        let t = generate_province(&seed, c);
        let init = camp::place(&seed, c, &t, 0, false, true).expect("initial camp");
        assert_eq!(init, camp::place(&seed, c, &t, 0, false, true).unwrap());
        for day in 0..500 {
            if let Some(x) = camp::place(&seed, c, &t, day, true, false) {
                assert!(camp::camp_tile_ok(&t, x.tile));
                assert!(!t.is_site(x.tile) && t.passable(x.tile) && x.tile != 0);
                assert!((100..=400).contains(&x.troops));
                troops[(x.troops > 250) as usize] += 1;
                n += 1;
            }
        }
    }
    // ½ of 3,000 days; troops spread both sides of the middle.
    assert!((1_380..=1_620).contains(&n), "{n}");
    assert!(troops[0] > 600 && troops[1] > 600);
    // A different ring seed moves the camps.
    let other = sha256(&[b"other"]);
    let c = ProvinceCoord::new(2, 0);
    let (ta, tb) = (generate_province(&seed, c), generate_province(&other, c));
    let a: Vec<_> = (0..50)
        .map(|d| camp::place(&seed, c, &ta, d, true, false))
        .collect();
    let b: Vec<_> = (0..50)
        .map(|d| camp::place(&other, c, &tb, d, true, false))
        .collect();
    assert_ne!(a, b);
}

#[test]
fn explore_rolls() {
    let seed = sha256(&[b"bell-seed"]);
    let p = ProvinceCoord::new(4, -2);
    let finds = (0..4_000u64)
        .filter(|&h| explore::roll(&seed, p, (h % 61) as u8, h, false).works == 4)
        .count();
    assert!((1_850..=2_150).contains(&finds), "{finds}");
    assert!((0..100u64).all(|h| explore::roll(&seed, p, 3, h, true).works == 4));
    assert_eq!(explore::EXPLORE_FLOOR, 3);
}

#[test]
fn catalog_matches_the_simulator_tables() {
    // Spot values of frontier-sim/src/model.rs (the field-equality test is
    // frontier-sim/tests/catalog_equality.rs, W1-D).
    assert_eq!(catalog::BASE_PROD, [40, 30, 20, 15, 0, 15, 5, 0]);
    assert_eq!(catalog::BUILDINGS[5].cost, [0, 60, 60, 0, 0, 20, 0, 0]);
    assert_eq!(catalog::build_secs(1), 5_400);
    assert_eq!(catalog::TROOP_COST_PER_100, [60, 20, 10]);
    assert_eq!(catalog::STARTER_KIT, [300, 300, 200, 100, 0, 100, 0, 0]);
    assert_eq!(
        (
            catalog::WORKS_EXPLORE,
            catalog::WORKS_CAMP,
            catalog::WORKS_DAY_CAP
        ),
        (4, 10, 20)
    );
    // Every item the Build instruction can name has a cost, an effect and a
    // duration for copies 1..=MAX_COPIES.
    for item in 0..catalog::ITEM_COUNT {
        for n in [1, 2, catalog::MAX_COPIES] {
            let (c, _, secs) = catalog::building(item, n, &NEUTRAL).unwrap();
            assert!(c.iter().all(|&x| x >= 0) && c.iter().any(|&x| x > 0));
            assert!(secs > 0);
        }
    }
    for u in 0..=6u8 {
        let c = catalog::train(u, 1_000).unwrap();
        assert_eq!(c[0], 600_000);
    }
}

// ------------------------------------------------------------ ruleset hash

#[test]
fn ruleset_hash_binds_versions_and_catalog() {
    let a = ruleset_hash_input();
    assert!(a.starts_with(b"PSF-RULESET-v1"));
    assert_eq!(ruleset_hash(), sha256(&[&a]));
    assert_eq!(ruleset_hash_input(), a, "deterministic");
    // The catalog tables are inside it.
    let mut t = Vec::new();
    catalog::write_tables(&mut t);
    assert!(a.windows(t.len()).any(|w| w == t.as_slice()));
    // So are the doctrine table and the kernel constants (integ-W1).
    let mut d = Vec::new();
    permutation_rules::frontier::doctrine::write_table(&mut d);
    assert!(a.windows(d.len()).any(|w| w == d.as_slice()));
    let k: Vec<u8> = permutation_rules::frontier::KERNEL_CONSTANTS
        .iter()
        .flat_map(|x| x.to_le_bytes())
        .collect();
    assert!(a.ends_with(&k));
    // Every module of the frontier tree has a version in the hash.
    let names: Vec<&str> = permutation_rules::frontier::KERNEL_VERSIONS
        .iter()
        .map(|(n, _)| *n)
        .collect();
    let src = include_str!("../src/frontier/mod.rs");
    let mut modules = 0;
    for line in src.lines() {
        if let Some(m) = line
            .strip_prefix("pub mod ")
            .and_then(|r| r.strip_suffix(';'))
        {
            assert!(
                names.contains(&m),
                "module {m} has no KERNEL_VERSIONS entry"
            );
            modules += 1;
        }
    }
    assert_eq!(modules + 1, names.len(), "one entry per module + frontier");
    // Golden value: a change to any bound constant or table changes it on
    // purpose (update here, in frontier-abi and in the notes). Moved from
    // 3c374846… (W1-C, nine versions) when every kernel was bound.
    assert_eq!(
        hex(&ruleset_hash()),
        "1ac11f85fde3b898ebcd8c246964d9be2a29b4144a7a7ddfa81006999a6dd03f",
        "ruleset hash changed"
    );
}
