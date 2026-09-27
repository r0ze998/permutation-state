//! RevealRoster (AnchorTalk, in the program's `roster` module, is in `play`).

use solana_address::Address;
use solana_instruction::Instruction;
use solana_signer::Signer;

use crate::instruction::ChainInstruction as I;
use crate::season::SeasonFx;
use crate::{r, rs, w};

impl SeasonFx {
    /// `revealRoster` (`chain.mjs:164`): authority (s), season, roster,
    /// world chunk 0, then the member PDAs of `members` (indices into
    /// `self.members`), one per salt. `from`: 0 restarts, else the count
    /// revealed so far; `blind` is read by the completing batch only.
    pub fn reveal_roster_ix(
        &self,
        authority: &Address,
        from: u16,
        members: &[usize],
        salts: Vec<[u8; 32]>,
        blind: [u8; 32],
    ) -> Instruction {
        let mut m = vec![
            rs(authority),
            w(&self.season),
            w(&self.roster),
            r(&self.chunks[0]),
        ];
        m.extend(members.iter().map(|i| r(&self.members[*i].member)));
        self.ix(&I::RevealRoster { from, salts, blind }, m)
    }

    /// RevealRoster of AIs `ai` (roster positions, registered, consecutive)
    /// with their own salts, by the admin, from `ai[0]`, with the season's
    /// blind (`roster_blind`).
    pub fn reveal_ai_ix(&self, ai: &[usize]) -> Instruction {
        let members: Vec<usize> = ai
            .iter()
            .map(|a| self.ai[*a].member.expect("the AI registered"))
            .collect();
        let salts = ai.iter().map(|a| self.ai[*a].salt).collect();
        let from = ai.first().copied().unwrap_or(0) as u16;
        self.reveal_roster_ix(
            &self.admin.pubkey(),
            from,
            &members,
            salts,
            self.roster_blind(),
        )
    }

    /// The operator's secret blind for this season's roster commitment.
    pub fn roster_blind(&self) -> [u8; 32] {
        let mut b = [0xb1; 32];
        b[..8].copy_from_slice(&self.p.id.to_le_bytes());
        b
    }

    /// The blinded roster commitment CreateSeason stores (WP09):
    /// `roster_commit(roster_blind, roster_chain)`.
    pub fn roster_commit(&self) -> [u8; 32] {
        permutation_rules::roster::roster_commit(&self.roster_blind(), &self.roster_chain())
    }
}
