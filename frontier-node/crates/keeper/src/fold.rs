//! FoldOccupancy (M1 contract §8.2 "Fold", §5.9 v1.2, I-48). Role `fold`,
//! class D.
//!
//! Every bell while joins are open (`bell < join_close_bell`, season
//! Running), the three parts of one bell in order: part 0 sums the 24
//! JoinShards of factions 0–2 and stamps `fold_bell`, part 1 adds factions
//! 3–5 and writes `occupied_sites`/`wedge_occupied`, part 2 folds the six
//! ProvinceFund shards into `open_sites`/`wedge_open`/`provinces_opened`.
//! A part of an older bell is refused `FoldStale` (44), so a part still
//! pending when the bell turns is dropped and the fold restarts at part 0.
//! Join's capacity rule and OpenRing's crowding rule read these values.

use std::collections::BTreeSet;
use std::sync::Arc;

use fclient::abi::{status, tag, Class};
use fclient::decode::Frontier;
use fclient::ix;
use fclient::ports::{ChainPort, PortResult};

use crate::engine::{BuildCtx, Engine, WriteSpec};
use crate::Tick;

pub fn fold_key(bell: u32, part: u8) -> String {
    format!("fold:{bell}:{part}")
}

/// Which part the fold of `bell` needs next, given the Frontier's
/// `(fold_bell, fold_part)`; `None` when the bell is folded.
/// `done` is this keeper's memory of a part 2 landed for `bell` (the
/// Frontier's `fold_part = 0` after part 2 is the same as before any fold,
/// which only matters at bell 0).
pub fn next_part(fold_bell: u32, fold_part: u8, bell: u32, done: bool) -> Option<u8> {
    if fold_bell != bell {
        return Some(0);
    }
    match fold_part {
        1 => Some(1),
        2 => Some(2),
        _ if done || bell > 0 => None,
        _ => Some(0),
    }
}

#[derive(Default)]
pub struct FoldDuty {
    /// Bells seen folded (all three parts landed).
    pub folded: BTreeSet<u32>,
    /// Bells whose part 2 this keeper saw land (the bell-0 ambiguity).
    part2_landed: BTreeSet<u32>,
    /// `(bell, slots from the bell's first slot to part 2 landing)`.
    pub latency: Vec<(u32, u64)>,
    bell_first_slot: Option<(u32, u64)>,
    pending: Option<(u32, u8)>,
}

impl FoldDuty {
    pub async fn plan<P: ChainPort>(
        &mut self,
        t: &Tick<'_>,
        port: &P,
        engine: &mut Engine,
    ) -> PortResult<()> {
        let s = t.season;
        if !t.cfg.has_role("fold") || s.effective_status(t.now) != status::RUNNING {
            return Ok(());
        }
        let Some(bell) = t.clock.bell_at(t.now) else {
            return Ok(());
        };
        if bell >= s.join_close_bell {
            return Ok(());
        }
        if self.bell_first_slot.is_none_or(|(b, _)| b != bell) {
            self.bell_first_slot = Some((bell, t.slot));
        }
        // A part of an older bell is stale: drop it.
        if let Some((b, p)) = self.pending {
            if b != bell {
                engine.cancel(&fold_key(b, p));
                self.pending = None;
            } else if engine.is_pending(&fold_key(b, p)) {
                return Ok(());
            }
        }
        let got = port.accounts(&[t.addrs.frontier()], 0).await?;
        let Some(fr) = got[0].as_ref().and_then(|a| Frontier::decode(&a.data).ok()) else {
            return Ok(());
        };
        let done = self.part2_landed.contains(&bell);
        let Some(part) = next_part(fr.fold_bell, fr.fold_part, bell, done) else {
            if self.folded.insert(bell) {
                let first = self.bell_first_slot.map_or(t.slot, |x| x.1);
                self.latency.push((bell, t.slot.saturating_sub(first)));
            }
            return Ok(());
        };
        let a = t.addrs.clone();
        let added = engine.ensure(
            WriteSpec {
                key: fold_key(bell, part),
                kind: "fold",
                tag: tag::FOLD_OCCUPANCY,
                class: Class::D,
                bell: Some(bell),
                region: None,
                build: Arc::new(move |c: &BuildCtx| vec![ix::fold_occupancy(&a, c.payer, part)]),
                deadline_slot: None,
                not_before_slot: 0,
                fixed_payer: None,
            },
            t.slot,
        );
        if added || engine.is_pending(&fold_key(bell, part)) {
            self.pending = Some((bell, part));
        }
        Ok(())
    }

    /// A landed part 2 marks the bell folded (the bell-0 ambiguity).
    pub fn on_landed(&mut self, key: &str) {
        if let Some(rest) = key.strip_prefix("fold:") {
            if let Some((b, p)) = rest.split_once(':') {
                if p == "2" {
                    if let Ok(b) = b.parse() {
                        self.part2_landed.insert(b);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_follow_the_frontier() {
        assert_eq!(next_part(4, 0, 5, false), Some(0), "a new bell");
        assert_eq!(next_part(5, 1, 5, false), Some(1));
        assert_eq!(next_part(5, 2, 5, false), Some(2));
        assert_eq!(next_part(5, 0, 5, false), None, "folded");
        assert_eq!(next_part(0, 0, 0, false), Some(0), "bell 0 before any fold");
        assert_eq!(next_part(0, 0, 0, true), None, "bell 0 after part 2");
        assert_eq!(
            next_part(9, 1, 5, false),
            Some(0),
            "a foreign newer stamp restarts"
        );
    }
}
