//! RevealRoster (AnchorTalk, in the program's `roster` module, is in `play`).

use solana_instruction::Instruction;

use crate::instruction::ChainInstruction as I;
use crate::season::SeasonFx;
use crate::{r, w};

impl SeasonFx {
    /// `revealRoster` (`chain.mjs:164`): season, roster, then the member
    /// PDAs of `members` (indices into `self.members`), one per salt.
    pub fn reveal_roster_ix(&self, members: &[usize], salts: Vec<[u8; 32]>) -> Instruction {
        let mut m = vec![w(&self.season), w(&self.roster)];
        m.extend(members.iter().map(|i| r(&self.members[*i].member)));
        self.ix(
            &I::RevealRoster {
                from: 0,
                salts,
                blind: [0; 32],
            },
            m,
        )
    }

    /// RevealRoster of AIs `ai` (indices into `self.ai`, registered) with
    /// their own salts.
    pub fn reveal_ai_ix(&self, ai: &[usize]) -> Instruction {
        let members: Vec<usize> = ai
            .iter()
            .map(|a| self.ai[*a].member.expect("the AI registered"))
            .collect();
        let salts = ai.iter().map(|a| self.ai[*a].salt).collect();
        self.reveal_roster_ix(&members, salts)
    }
}
