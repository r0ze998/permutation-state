//! StartSeason, RetrySeasonSeed, ConsumeSeasonSeed, GenesisStep,
//! SeatMembers, OpenGovernment.

use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};

use crate::instruction::ChainInstruction as I;
use crate::season::SeasonFx;
use crate::{addr, r, rs, vrf, w, ws, SLOT_HASHES, SYSTEM};

impl SeasonFx {
    /// The VRF accounts of StartSeason and RetrySeasonSeed: our identity
    /// PDA, the base queue (w), the VRF program, system, SlotHashes.
    pub fn seed_vrf_metas(&self) -> Vec<AccountMeta> {
        vec![
            r(&vrf::program_identity(&self.program)),
            w(&vrf::queue_base()),
            r(&vrf::vrf_program()),
            r(&addr(SYSTEM)),
            r(&addr(SLOT_HASHES)),
        ]
    }

    /// `startSeason` (contract §1.3 tag 3): authority (s,w), season, then
    /// the VRF accounts (`seed_vrf_metas`). No world chunks.
    pub fn start_ix(&self, authority: &Address) -> Instruction {
        let mut m = vec![ws(authority), w(&self.season)];
        m.extend(self.seed_vrf_metas());
        self.ix(&I::StartSeason, m)
    }

    /// `retrySeasonSeed` (contract §1.3 tag 32): payer (s,w), season, then
    /// the VRF accounts. Permissionless.
    pub fn retry_seed_ix(&self, payer: &Address) -> Instruction {
        let mut m = vec![ws(payer), w(&self.season)];
        m.extend(self.seed_vrf_metas());
        self.ix(&I::RetrySeasonSeed, m)
    }

    /// `ConsumeSeasonSeed` as the VRF would call it (tag 31), but sent
    /// directly by `signer` (the real callback comes from `vrf::fulfil`).
    pub fn consume_seed_ix(&self, signer: &Address, e: [u8; 32], season_id: u64) -> Instruction {
        self.ix(
            &I::ConsumeSeasonSeed {
                randomness: e,
                season_id,
            },
            vec![rs(signer), w(&self.season)],
        )
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
