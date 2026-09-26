//! StartSeason, GenesisStep, SeatMembers, OpenGovernment.

use solana_address::Address;
use solana_instruction::Instruction;

use crate::instruction::ChainInstruction as I;
use crate::season::SeasonFx;
use crate::{addr, r, rs, w, SLOT_HASHES};

impl SeasonFx {
    /// `startSeason` (`chain.mjs:89`): authority (s), season, SlotHashes,
    /// the world chunks.
    pub fn start_ix(&self, authority: &Address) -> Instruction {
        let mut m = vec![rs(authority), w(&self.season), r(&addr(SLOT_HASHES))];
        m.extend(self.chunk_metas());
        self.ix(&I::StartSeason, m)
    }

    /// `genesisStep` (`chain.mjs:92`): season, the world chunks. Permissionless.
    pub fn genesis_step_ix(&self, work: u32) -> Instruction {
        let mut m = vec![w(&self.season)];
        m.extend(self.chunk_metas());
        self.ix(&I::GenesisStep { work }, m)
    }

    /// `seatMembers` (`chain.mjs:96`): authority (s), season, the world
    /// chunks, then the member PDAs `members` (indices into `self.members`).
    pub fn seat_ix(&self, authority: &Address, members: &[usize]) -> Instruction {
        let mut m = vec![rs(authority), w(&self.season)];
        m.extend(self.chunk_metas());
        m.extend(members.iter().map(|i| r(&self.members[*i].member)));
        self.ix(&I::SeatMembers, m)
    }

    /// `openGovernment` (`chain.mjs:99`): authority (s), season, the world
    /// chunks, every nation in civ order.
    pub fn open_ix(&self, authority: &Address) -> Instruction {
        let mut m = vec![rs(authority), w(&self.season)];
        m.extend(self.tick_metas());
        self.ix(&I::OpenGovernment, m)
    }
}
