//! CommitOrders, CloseCommits, RevealOrders, SubmitGov, LogTickInput,
//! ResolveTick, AnchorTalk, SubmitOrders (retired).

use permutation_rules::gov::{GovAction, Role};
use permutation_rules::orders::{Order, OrderBatch};
use solana_address::Address;
use solana_instruction::Instruction;

use crate::instruction::ChainInstruction as I;
use crate::season::SeasonFx;
use crate::{r, rs, w};

impl SeasonFx {
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

    /// `closeCommits` (`chain.mjs:124`): the world chunks, every nation. Permissionless.
    pub fn close_ix(&self) -> Instruction {
        self.ix(&I::CloseCommits, self.tick_metas())
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

    /// `logTickInput` (`chain.mjs:136`): the world chunks, every nation. Permissionless.
    pub fn log_ix(&self, chunk: u16) -> Instruction {
        self.ix(&I::LogTickInput { chunk }, self.tick_metas())
    }

    /// `resolveTick` (`chain.mjs:139`): the world chunks, every nation. Permissionless.
    pub fn resolve_ix(&self, to: u8) -> Instruction {
        self.ix(&I::ResolveTick { to }, self.tick_metas())
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
