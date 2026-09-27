//! StartClock, CommitOrders, CloseCommits, RevealOrders, SubmitGov,
//! FreezeTick, RetryTickRandomness, ConsumeTickRandomness (sent directly:
//! the oracle's is `vrf::fulfil`), LogTickInput, ResolveTick, AnchorTalk,
//! SubmitOrders (retired).

use permutation_rules::gov::{GovAction, Role};
use permutation_rules::orders::{Order, OrderBatch};
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};

use crate::chain::Chain;
use crate::instruction::ChainInstruction as I;
use crate::season::SeasonFx;
use crate::vrf::{program_identity, queue_base, queue_er, vrf_program};
use crate::{addr, r, rs, w, ws, SLOT_HASHES, SYSTEM};

/// The Instructions sysvar: the last account of every instruction that must
/// be alone in its transaction (CloseCommits, LogTickInput, ResolveTick).
pub const INSTRUCTIONS_SYSVAR: &str = "Sysvar1nstructions1111111111111111111111111";

impl SeasonFx {
    /// The world chunks, every nation, then the Instructions sysvar: the
    /// accounts of CloseCommits, LogTickInput and ResolveTick.
    pub fn alone_metas(&self) -> Vec<AccountMeta> {
        let mut m = self.tick_metas();
        m.push(r(&addr(INSTRUCTIONS_SYSVAR)));
        m
    }

    /// The VRF queue of the season's recorded play mode: the ER queue once
    /// world chunk 0 was delegated (`Season::delegated`), else the base
    /// queue (A20).
    pub fn vrf_queue(&self, c: &Chain) -> Address {
        if self.season(c).delegated & 1 != 0 {
            queue_er()
        } else {
            queue_base()
        }
    }

    /// `startClock`: crank (s), the world chunks, every nation.
    pub fn start_clock_ix(&self, crank: &Address) -> Instruction {
        let mut m = vec![rs(crank)];
        m.extend(self.tick_metas());
        self.ix(&I::StartClock, m)
    }

    /// Accounts 0–6 of FreezeTick and RetryTickRandomness: payer (s,w),
    /// chunk 0 (w), our identity PDA, `queue` (w), the VRF program, system,
    /// SlotHashes.
    fn vrf_metas(&self, payer: &Address, queue: &Address) -> Vec<AccountMeta> {
        vec![
            ws(payer),
            w(&self.chunks[0]),
            r(&program_identity(&self.program)),
            w(queue),
            r(&vrf_program()),
            r(&addr(SYSTEM)),
            r(&addr(SLOT_HASHES)),
        ]
    }

    /// `freezeTick`: accounts 0–6 with `queue`, every nation (w), then the
    /// season (the recorded play mode picks the queue). Permissionless.
    pub fn freeze_ix(&self, payer: &Address, queue: &Address) -> Instruction {
        let mut m = self.vrf_metas(payer, queue);
        m.extend(self.nations.iter().map(w));
        m.push(r(&self.season));
        self.ix(&I::FreezeTick, m)
    }

    /// `retryTickRandomness`: accounts 0–6 with `queue`, then the season.
    /// Permissionless.
    pub fn retry_ix(&self, payer: &Address, queue: &Address) -> Instruction {
        let mut m = self.vrf_metas(payer, queue);
        m.push(r(&self.season));
        self.ix(&I::RetryTickRandomness, m)
    }

    /// `ConsumeTickRandomness` as a top-level instruction from `signer`
    /// (only the VRF program can sign as the scoped identity; the oracle's
    /// real call is `vrf::fulfil`): signer (s), chunk 0 (w).
    pub fn consume_ix(
        &self,
        signer: &Address,
        randomness: [u8; 32],
        season_id: u64,
        tick: u16,
    ) -> Instruction {
        self.ix(
            &I::ConsumeTickRandomness {
                randomness,
                season_id,
                tick,
            },
            vec![rs(signer), w(&self.chunks[0])],
        )
    }

    /// `commitOrders` (`chain.mjs:116`): signer (the office holder's
    /// session key), nation.
    pub fn commit_orders_ix(
        &self,
        signer: &Address,
        civ: u16,
        role: Role,
        tick: u16,
        commitment: [u8; 32],
    ) -> Instruction {
        self.ix(
            &I::CommitOrders {
                role,
                tick,
                commitment,
            },
            vec![rs(signer), w(&self.nations[civ as usize])],
        )
    }

    /// `revealOrders` (`chain.mjs:120`): signer (anyone), nation.
    pub fn reveal_orders_ix(
        &self,
        signer: &Address,
        b: &OrderBatch,
        salt: [u8; 32],
    ) -> Instruction {
        self.ix(
            &I::RevealOrders {
                role: b.role,
                tick: b.tick,
                decision_digest: b.decision_digest,
                orders: b.orders.clone(),
                adopt: b.adopt.clone(),
                salt,
            },
            vec![rs(signer), w(&self.nations[b.civ as usize])],
        )
    }

    /// `closeCommits` (`chain.mjs:124`): the world chunks, every nation,
    /// the sysvar. Permissionless.
    pub fn close_ix(&self) -> Instruction {
        self.ix(&I::CloseCommits, self.alone_metas())
    }

    /// `submitGov` (`chain.mjs:128`): signer (the member's session key),
    /// the member's nation.
    pub fn submit_gov_ix(
        &self,
        signer: &Address,
        civ: u16,
        member: u32,
        action: GovAction,
    ) -> Instruction {
        self.ix(
            &I::SubmitGov { member, action },
            vec![rs(signer), w(&self.nations[civ as usize])],
        )
    }

    /// `logTickInput` (`chain.mjs:136`): the world chunks, every nation,
    /// the sysvar. Permissionless.
    pub fn log_ix(&self, chunk: u16) -> Instruction {
        self.ix(&I::LogTickInput { chunk }, self.alone_metas())
    }

    /// `resolveTick` (`chain.mjs:139`): the world chunks, every nation, the
    /// sysvar. Permissionless. `to | DEGRADED` is the degraded step.
    pub fn resolve_ix(&self, to: u8) -> Instruction {
        self.ix(&I::ResolveTick { to }, self.alone_metas())
    }

    /// `anchorTalk` (`chain.mjs:168`): the crank (s), nation 0.
    pub fn anchor_talk_ix(
        &self,
        crank: &Address,
        tick: u16,
        count: u32,
        root: [u8; 32],
    ) -> Instruction {
        self.ix(
            &I::AnchorTalk { tick, count, root },
            vec![rs(crank), r(&self.nations[0])],
        )
    }

    /// The retired `SubmitOrders` (no client builder any more).
    pub fn submit_orders_ix(
        &self,
        signer: &Address,
        civ: u16,
        role: Role,
        tick: u16,
        orders: Vec<Order>,
    ) -> Instruction {
        self.ix(
            &I::SubmitOrders {
                role,
                tick,
                decision_digest: [1; 32],
                orders,
                adopt: vec![],
            },
            vec![rs(signer), w(&self.nations[civ as usize])],
        )
    }
}
