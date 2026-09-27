//! What a transaction needs and the ceilings it must stay under (§5.5,
//! §10.2, §13.1): CU, heap, tx bytes, account locks, loaded data.
//!
//! - **CU**: measured on a fork at the retry-ladder limit (1.4M), so the
//!   number is the instruction's real need, not a budget-capped failure.
//! - **Heap**: a heap-frame bisection cannot see below the default 32 KiB,
//!   and the gate is 28 KiB (I-50), so the peak comes from the **trace
//!   build** (`--features trace`, W2-A): [`heap_peak`] reads its
//!   checkpoints, `sol_log_64(0x4355 "CU", tag, heap_peak, 0, 0)` →
//!   `Program log: 0x4355, 0x<tag>, 0x<peak>, 0x0, 0x0` (W2-A's `heap.rs`),
//!   and also a plain `heap_peak=<bytes>` form.
//! - **Tx bytes, locks**: from the signed wire transaction and its message.
//! - **Loaded data**: the harness's SIMD-0186 size ([`Chain::loaded_size`])
//!   against `L(kind)` at the deployed programdata length.

use frontier_abi::budgets as ab;
use frontier_abi::tags::Ix;
use solana_instruction::Instruction;
use solana_keypair::Keypair;

use crate::chain::{loaded_limit, Chain, Fail, Profile};

/// What one transaction needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Need {
    pub cu: u64,
    /// Heap peak (trace build only).
    pub heap: Option<u32>,
    pub tx_bytes: usize,
    /// Account keys of the message (the runtime locks every one).
    pub locks: usize,
    pub write_locks: usize,
    /// SIMD-0186 loaded size.
    pub loaded: u64,
}

impl std::fmt::Display for Need {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} CU, heap {}, {} B, {} locks ({} writable), loaded {} B",
            self.cu,
            self.heap
                .map_or("n/a (plain build)".to_string(), |h| format!("{h} B")),
            self.tx_bytes,
            self.locks,
            self.write_locks,
            self.loaded
        )
    }
}

/// The ceilings of one instruction kind (§5.5; tx bytes per the byte model
/// `budgets::tx_ceiling`, v1.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ceilings {
    pub cu: u32,
    pub heap: u32,
    pub tx_bytes: u32,
    pub locks: u32,
    pub loaded: u32,
}

/// The ceilings of `ix` doing `units` of work (SkipQuiet: recomputed bells)
/// at `programdata_len`.
pub fn ceilings(ix: Ix, units: u32, programdata_len: u32) -> Ceilings {
    Ceilings {
        cu: ab::cu_gate(ix, units),
        heap: ab::HEAP_GATE,
        tx_bytes: ab::tx_ceiling(ix).min(ab::TX_MAX),
        locks: ab::LOCKS_MAX,
        loaded: loaded_limit(ix, programdata_len),
    }
}

/// First argument of the trace build's checkpoints (`"CU"`).
pub const TRACE_TAG: u64 = 0x4355;

/// The trace build's heap peak in `logs` (the largest checkpoint).
pub fn heap_peak(logs: &[String]) -> Option<u32> {
    logs.iter()
        .filter_map(|l| l.strip_prefix("Program log: "))
        .filter_map(|l| {
            if let Some(i) = l.find("heap_peak") {
                let rest = l[i + "heap_peak".len()..].trim_start_matches(['=', ' ', ':']);
                let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                return n.parse().ok();
            }
            let f: Vec<u64> = l
                .split(", ")
                .map(|x| u64::from_str_radix(x.trim().trim_start_matches("0x"), 16))
                .collect::<Result<_, _>>()
                .ok()?;
            (f.len() == 5 && f[0] == TRACE_TAG).then(|| f[2].min(u32::MAX as u64) as u32)
        })
        .max()
}

impl Chain {
    /// Runs `ixs` on a fork at the ladder profile and returns what they
    /// need; the chain itself is not changed.
    pub fn measure(&self, ixs: &[Instruction], signers: &[&Keypair]) -> Result<Need, Fail> {
        let p = self.profile_of(ixs, Profile::ladder);
        self.measure_with(&p, ixs, signers)
    }

    pub fn measure_with(
        &self,
        p: &Profile,
        ixs: &[Instruction],
        signers: &[&Keypair],
    ) -> Result<Need, Fail> {
        let mut fork = self.fork();
        let t = fork.transaction(p, ixs, signers);
        let locks = t.message.account_keys.len();
        let write_locks = (0..locks)
            .filter(|&i| fclient::tx::is_writable_index(&t.message, i))
            .count();
        let l = fork.submit(t)?;
        Ok(Need {
            cu: l.cu,
            heap: heap_peak(&l.logs),
            tx_bytes: l.tx_bytes,
            locks,
            write_locks,
            loaded: l.loaded,
        })
    }
}

/// Prints `need` and asserts every ceiling it can see.
#[track_caller]
pub fn assert_within(label: &str, need: &Need, c: &Ceilings) {
    println!("{label}: {need}");
    assert!(need.cu <= c.cu as u64, "{label}: {} CU > {}", need.cu, c.cu);
    if let Some(h) = need.heap {
        assert!(h <= c.heap, "{label}: heap {h} B > {} B", c.heap);
    }
    assert!(
        need.tx_bytes as u32 <= c.tx_bytes,
        "{label}: {} B > {} B",
        need.tx_bytes,
        c.tx_bytes
    );
    assert!(
        need.locks as u32 <= c.locks,
        "{label}: {} locks",
        need.locks
    );
    assert!(
        need.loaded <= c.loaded as u64,
        "{label}: loaded {} B > L = {} B",
        need.loaded,
        c.loaded
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heap_peak_parses_both_forms_and_takes_the_max() {
        let logs = vec![
            "Program log: PSF_TRACE cu=1200 heap_peak=4096".to_string(),
            "Program log: heap_peak 21024".to_string(),
            "Program data: heap_peak=99999".to_string(),
            "Program log: 0x4355, 0x3, 0x5220, 0x0, 0x0".to_string(),
            "Program log: 0x1, 0x2, 0xffffff, 0x0, 0x0".to_string(),
        ];
        assert_eq!(heap_peak(&logs), Some(21_024));
        assert_eq!(heap_peak(&logs[3..]), Some(0x5220));
        assert_eq!(heap_peak(&[]), None);
    }

    #[test]
    fn ceilings_follow_the_tables() {
        let c = ceilings(Ix::Reveal, 0, ab::PLACEHOLDER_PROGRAMDATA_LEN);
        assert_eq!(c.cu, 26_000);
        assert_eq!(c.heap, 28_672);
        assert_eq!(c.locks, 64);
        assert!(c.tx_bytes >= 1_100 && c.tx_bytes <= 1_232);
        assert_eq!(c.loaded % 32_768, 0);
        assert_eq!(ceilings(Ix::SkipQuiet, 3, 0).cu, 150_000);
    }
}
