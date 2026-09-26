//! Compute and heap budgets: what an instruction needs (`Chain::need`), the
//! ceilings every client profile must stay under, and the crank's split
//! points for ResolveTick (`crank_stops`, read from the crank itself).
//!
//! The margins are the same for every profile, about 14% CU and 12.5% heap
//! below what the client requests: room for ER/base CU drift between Agave
//! versions and for rules changes.

use solana_address::Address;
use solana_instruction::Instruction;
use solana_keypair::Keypair;

use crate::chain::{Budget, Chain, Fail, Landed, HEAP};
use crate::season::SeasonFx;

/// Heavy (1.4M CU, 256 KiB): 1.4M minus 14%.
pub const CU_CEILING: u64 = 1_200_000;
/// Heavy: 256 KiB minus 12.5%.
pub const HEAP_CEILING: u32 = 224 * 1024;
/// Medium (default 200k CU, 128 KiB frame): the default CU minus 15%.
pub const MEDIUM_CU: u64 = 170_000;
/// Medium: the 128 KiB frame minus 12.5%.
pub const MEDIUM_HEAP_CEILING: u32 = 112 * 1024;
/// Light (no compute-budget instructions): the default CU minus 15%. Its
/// heap margin cannot be bisected (`RequestHeapFrame` takes nothing below
/// 32 KiB), so the rule is structural: an instruction whose heap grows with
/// the season must not be Light in the client, and a Light instruction must
/// land at its worst shape with no budget instructions.
pub const LIGHT_CU: u64 = 170_000;
/// The default heap frame.
pub const DEFAULT_HEAP: u32 = 32 * 1024;

/// What one instruction needs: its CU, and the smallest heap frame it lands
/// with (1 KiB steps; the program's bump allocator faults past the frame).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Need {
    pub cu: u64,
    pub heap: u32,
}

impl Need {
    pub fn max(self, o: Need) -> Need {
        Need {
            cu: self.cu.max(o.cu),
            heap: self.heap.max(o.heap),
        }
    }
}

impl std::fmt::Display for Need {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} CU / {} KiB", self.cu, self.heap / 1024)
    }
}

impl Chain {
    /// Runs `ixs` on forks of the chain at heap frames between 32 and 256
    /// KiB (bisected, about 9 forks) with 1.4M CU and returns what they
    /// need; the chain itself is not changed. Fails as the 256 KiB run does.
    pub fn need(&self, ixs: &[Instruction], signers: &[&Keypair]) -> Result<Need, Fail> {
        self.need_with(|c| c.send_with(Budget::Heavy, ixs.to_vec(), signers))
    }

    /// `need` for a transaction sent with `send_as` (sigverify off).
    pub fn need_as(&self, ixs: &[Instruction], payer: &Address) -> Result<Need, Fail> {
        self.need_with(|c| c.send_as_with(Budget::Heavy, ixs.to_vec(), payer))
    }

    fn need_with(&self, send: impl Fn(&mut Chain) -> Result<Landed, Fail>) -> Result<Need, Fail> {
        let try_at = |heap: u32| {
            let mut fork = self.fork();
            fork.heap = heap;
            send(&mut fork)
        };
        let top = try_at(HEAP)?;
        if try_at(DEFAULT_HEAP).is_ok() {
            return Ok(Need {
                cu: top.cu,
                heap: DEFAULT_HEAP,
            });
        }
        // KiB: `lo` fails, `hi` lands.
        let (mut lo, mut hi) = (DEFAULT_HEAP / 1024, HEAP / 1024);
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if try_at(mid * 1024).is_ok() {
                hi = mid
            } else {
                lo = mid
            }
        }
        Ok(Need {
            cu: top.cu,
            heap: hi * 1024,
        })
    }
}

/// Prints `need` and asserts the ceilings of the client profile it is sent with.
#[track_caller]
pub fn assert_fits(label: &str, need: Need, profile: Budget) {
    println!("{label}: {need}");
    let (cu, heap) = match profile {
        Budget::Heavy => (CU_CEILING, HEAP_CEILING),
        Budget::Medium => (MEDIUM_CU, MEDIUM_HEAP_CEILING),
        Budget::Light => (LIGHT_CU, DEFAULT_HEAP),
    };
    assert!(
        need.cu <= cu,
        "{label}: {} CU over the {profile:?} ceiling {cu}",
        need.cu
    );
    assert!(
        need.heap <= heap,
        "{label}: {} KiB over the {profile:?} ceiling {} KiB",
        need.heap / 1024,
        heap / 1024
    );
}

/// The phase boundaries the crank splits a tick at, read from the crank
/// itself (`permutation-gateway/src/ticks.mjs`, the one line
/// `export const STOPS = Object.freeze([…]);`), so the budget tests measure
/// the parts it really sends. Panics if the line is missing or unparsable,
/// not strictly increasing, or does not end at `LAST_PHASE`: keep `STOPS` a
/// literal in that file.
pub fn crank_stops() -> Vec<u8> {
    parse_stops(include_str!("../../../permutation-gateway/src/ticks.mjs"))
}

fn parse_stops(src: &str) -> Vec<u8> {
    let rule = "keep `export const STOPS = Object.freeze([…]);` and `export const LAST_PHASE = n;` literal in permutation-gateway/src/ticks.mjs (svm-tests budget.rs reads them)";
    let line = src
        .lines()
        .find(|l| l.trim_start().starts_with("export const STOPS"))
        .unwrap_or_else(|| panic!("no STOPS line: {rule}"));
    let list = line
        .split_once('[')
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(list, _)| list)
        .unwrap_or_else(|| panic!("STOPS is not a literal list: {rule}"));
    let stops: Vec<u8> = list
        .split(',')
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
        .map(|x| {
            x.parse()
                .unwrap_or_else(|_| panic!("STOPS entry {x:?}: {rule}"))
        })
        .collect();
    let last: u8 = src
        .lines()
        .find_map(|l| l.trim_start().strip_prefix("export const LAST_PHASE = "))
        .and_then(|v| v.trim_end_matches(';').trim().parse().ok())
        .unwrap_or_else(|| panic!("no LAST_PHASE: {rule}"));
    assert!(
        !stops.is_empty() && stops.windows(2).all(|p| p[0] < p[1]),
        "STOPS not strictly increasing: {rule}"
    );
    assert_eq!(
        stops.last(),
        Some(&last),
        "STOPS must end at LAST_PHASE: {rule}"
    );
    assert_eq!(
        last,
        permutation_rules::tick::PHASE_COUNT,
        "LAST_PHASE is the engine's phase count"
    );
    stops
}

/// One ResolveTick part as the crank sends it: phases `from..to`.
#[derive(Debug, Clone, Copy)]
pub struct Part {
    pub from: u8,
    pub to: u8,
    pub cu: u64,
    /// With `measure`: bisected on a fork before sending.
    pub need: Option<Need>,
}

/// Resolves the open tick (input published) in the crank's smallest parts:
/// from phase 0, each consecutive stop of `crank_stops()` — exactly the
/// part `resolveInParts` falls back to when every longer part fails as too
/// heavy. With `measure`, each part is measured with `need` first, and the
/// whole tick (`to = 12`, not asserted: the crank falls back from it) is
/// printed. `payer` pays (ResolveTick is permissionless).
pub fn resolve_parts(c: &mut Chain, s: &SeasonFx, payer: &Keypair, measure: bool) -> Vec<Part> {
    if measure {
        let whole = c.need(
            &[s.resolve_ix(permutation_rules::tick::PHASE_COUNT)],
            &[payer],
        );
        println!(
            "  whole tick (not asserted): {}",
            whole.map(|n| n.to_string()).unwrap_or_else(|f| f.err)
        );
    }
    let mut parts = vec![];
    let mut from = 0;
    for to in crank_stops() {
        let ix = s.resolve_ix(to);
        let need = measure.then(|| {
            c.need(std::slice::from_ref(&ix), &[payer])
                .unwrap_or_else(|f| panic!("ResolveTick {from}->{to} does not land: {f:#?}"))
        });
        let l = c
            .send(vec![ix], &[payer])
            .unwrap_or_else(|f| panic!("ResolveTick {from}->{to}: {f:#?}"));
        parts.push(Part {
            from,
            to,
            cu: l.cu,
            need,
        });
        from = to;
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::parse_stops;

    #[test]
    fn stops_are_read_from_the_crank() {
        let s = parse_stops(include_str!("../../../permutation-gateway/src/ticks.mjs"));
        assert_eq!(s.last(), Some(&12));
        let fake = "export const STOPS = Object.freeze([3, 12]);\nexport const LAST_PHASE = 12;\n";
        assert_eq!(parse_stops(fake), vec![3, 12]);
    }

    #[test]
    #[should_panic(expected = "strictly increasing")]
    fn stops_must_increase() {
        parse_stops(
            "export const STOPS = Object.freeze([4, 2, 12]);\nexport const LAST_PHASE = 12;\n",
        );
    }
}
