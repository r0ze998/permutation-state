//! The simulator's generator (SplitMix64, `frontier-sim/src/rng.rs`),
//! copied so the roster and the policies draw exactly as the simulator
//! does. `tests/profile_equality.rs` includes the simulator's file and
//! checks both streams agree draw for draw.

#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x5DEE_CE66_D1CE_4E5B)
    }

    /// An independent stream derived from this seed and a label.
    pub fn fork(seed: u64, label: u64) -> Self {
        let mut r = Rng::new(seed ^ label.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        r.next_u64();
        r
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next_u64() % n
        }
    }

    pub fn chance(&mut self, p: f64) -> bool {
        self.f64() < p
    }

    /// Fills `out` with pseudo-random bytes (seal keys and salts of the
    /// adversarial personas come from the OS generator instead; this is for
    /// deterministic test material only).
    pub fn fill(&mut self, out: &mut [u8]) {
        for c in out.chunks_mut(8) {
            let v = self.next_u64().to_le_bytes();
            c.copy_from_slice(&v[..c.len()]);
        }
    }
}
