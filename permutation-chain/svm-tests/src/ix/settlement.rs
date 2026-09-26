//! FinishSeason, Claim, WithdrawOps.

use solana_address::Address;
use solana_instruction::Instruction;

use crate::instruction::ChainInstruction as I;
use crate::season::SeasonFx;
use crate::{addr, r, rs, w, TOKEN};

impl SeasonFx {
    /// `finishSeason` (`chain.mjs:160`): season, the world chunks (read),
    /// with a revealed roster the roster account. Permissionless.
    pub fn finish_ix(&self, roster: bool) -> Instruction {
        let mut m = vec![w(&self.season)];
        m.extend(self.chunks.iter().map(r));
        if roster {
            m.push(r(&self.roster));
        }
        self.ix(&I::FinishSeason, m)
    }

    /// `claim` (`chain.mjs:172`): wallet (s), season, member PDA of member
    /// `i`, vault, destination, mint, token program.
    pub fn claim_ix(&self, i: usize, signer: &Address, dest: &Address) -> Instruction {
        self.ix(
            &I::Claim,
            vec![
                rs(signer),
                w(&self.season),
                w(&self.members[i].member),
                w(&self.vault),
                w(dest),
                r(&self.mint),
                r(&addr(TOKEN)),
            ],
        )
    }

    /// `withdrawOps` (`chain.mjs:175`): admin (s), season, vault,
    /// destination, mint, token program.
    pub fn withdraw_ix(&self, signer: &Address, dest: &Address) -> Instruction {
        self.ix(
            &I::WithdrawOps,
            vec![
                rs(signer),
                w(&self.season),
                w(&self.vault),
                w(dest),
                r(&self.mint),
                r(&addr(TOKEN)),
            ],
        )
    }
}
