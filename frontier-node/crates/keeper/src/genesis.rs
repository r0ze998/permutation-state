//! Genesis seed (M1 contract §8.2 "Genesis", §5.7).
//!
//! Role `beacon`: once the season is Created and its `genesis_round` is
//! published, `ConsumeGenesisSeed` (class D). Nothing expires but the
//! 7-day abort guard. The genesis rings and provinces are the ring duty's
//! ([`crate::rings`], W3-C), which opens every ring by the same code.

use std::sync::Arc;

use fclient::abi::{status, tag, Class};
use fclient::ix;
use fclient::ports::{ChainPort, DrandPort, PortResult};

use crate::engine::{BuildCtx, Engine, WriteSpec};
use crate::rounds::Rounds;
use crate::Tick;

pub const GENESIS_SEED_KEY: &str = "genesis-seed";

#[derive(Default)]
pub struct GenesisDuty {
    /// Slot the genesis round was first available, and the slot the seed landed.
    pub seed_available_slot: Option<u64>,
}

impl GenesisDuty {
    pub async fn plan<P: ChainPort, D: DrandPort>(
        &mut self,
        t: &Tick<'_>,
        _port: &P,
        drand: &D,
        rounds: &mut Rounds,
        engine: &mut Engine,
    ) -> PortResult<()> {
        let s = t.season;
        if t.cfg.has_role("beacon") && s.status == status::CREATED {
            let g = s.genesis_round;
            if t.clock.drand.round_time(g) <= t.now {
                if let Some(arg) = rounds.get(drand, g, t.slot).await {
                    self.seed_available_slot.get_or_insert(t.slot);
                    let a = t.addrs.clone();
                    engine.ensure(
                        WriteSpec {
                            key: GENESIS_SEED_KEY.into(),
                            kind: "genesis-seed",
                            tag: tag::CONSUME_GENESIS_SEED,
                            class: Class::D,
                            bell: None,
                            region: None,
                            build: Arc::new(move |c: &BuildCtx| {
                                vec![ix::consume_genesis_seed(&a, c.payer, &arg)]
                            }),
                            deadline_slot: None,
                            not_before_slot: 0,
                            fixed_payer: None,
                        },
                        t.slot,
                    );
                }
            }
        }
        Ok(())
    }
}
