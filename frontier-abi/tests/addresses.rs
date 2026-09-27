//! §4.1: the seed grammar matches SP-V2 `acct.rs` byte for byte for `an`,
//! `sd`, `ar`, `po`, `ci`, `sv`, `aa`. `spv2` below is a verbatim copy of
//! SP-V2's `SeedStr` builders (scratchpad/frontier/m0b/spikes/SP-V2/program/src/acct.rs,
//! Solana types removed), fed the same seeded inputs as `frontier_abi::addr`.

use frontier_abi::addr;

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

#[test]
fn seeds_match_sp_v2_byte_for_byte() {
    let mut r = XorShift(0x5EED_F00D_1234_5678);
    let extremes_i = [i32::MIN, i32::MAX, 0, -1, 1];
    let extremes_u = [0u32, u32::MAX, 1, 143, 144];
    let mut cases = 0;
    for k in 0..20_000 {
        let (p, q, bell) = if k < 25 {
            (extremes_i[k % 5], extremes_i[k / 5], extremes_u[k % 5])
        } else {
            (r.next() as i32, r.next() as i32, r.next() as u32)
        };
        let (a, b) = (r.next() as u8, r.next() as u8);
        let host = if k == 0 { u64::MAX } else { r.next() };
        assert_eq!(
            addr::bell_anchor_seed(bell, a).as_bytes(),
            spv2::anchor_seed(bell, a).as_bytes()
        );
        assert_eq!(
            addr::seed_cache_seed(bell, a, b).as_bytes(),
            spv2::cache_seed(bell, a, b).as_bytes()
        );
        assert_eq!(
            addr::arrival_slot_seed(p, q, bell, a, b).as_bytes(),
            spv2::arrival_seed(p, q, bell, a, b).as_bytes()
        );
        assert_eq!(
            addr::posture_seed(p, q, bell, a).as_bytes(),
            spv2::posture_seed(p, q, bell, a).as_bytes()
        );
        assert_eq!(
            addr::clash_inputs_seed(p, q, bell).as_bytes(),
            spv2::inputs_seed(p, q, bell).as_bytes()
        );
        assert_eq!(
            addr::anchor_archive_seed(a, bell).as_bytes(),
            spv2::archive_seed(a, bell).as_bytes()
        );
        assert_eq!(
            addr::seal_verdict_seed(host, bell).as_bytes(),
            spv2::verdict_seed(host, bell).as_bytes()
        );
        cases += 1;
    }
    assert_eq!(cases, 20_000);
}

#[test]
fn with_seed_is_sha256_of_base_seed_owner() {
    let base = [7u8; 32];
    let owner = [9u8; 32];
    let seed = addr::province_seed(-3, 4);
    let want = permutation_rules::hash::sha256(&[&base, seed.as_bytes(), &owner]);
    assert_eq!(addr::with_seed(&base, &seed, &owner), want);
    let ctx = addr::AddrCtx {
        season: base,
        program: owner,
    };
    assert_eq!(ctx.province(-3, 4), want);
    // a host id names its holding
    let id = addr::host_id(-3, 4, 9, 2, 17).unwrap();
    assert_eq!(ctx.holding_of_host(id), Some(ctx.holding(-3, 4, 9)));
}
