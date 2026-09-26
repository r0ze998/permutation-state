//! Instruction builders, one file per area of the program (the same split
//! as `permutation-chain/src/processor`). Each returns an `Instruction`
//! without sending it, so negative tests can change an account or a field
//! first. **The account order equals the client's**
//! (`permutation-gateway/client/src/chain.mjs`); each builder cites its line.
//! Builders are `SeasonFx` methods; the budget a transaction is sent with is
//! the client's (`Chain::send`, `chain::client_budget`).

use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};

use crate::instruction::ChainInstruction;
use crate::season::SeasonFx;
use crate::state::{NATION_TARGET, WORLD_CHUNKS};
use crate::w;

pub mod delegation;
pub mod escape;
pub mod genesis;
pub mod play;
pub mod registration;
pub mod roster;
pub mod settlement;

pub use registration::{CreateArgs, RegisterArgs};

impl SeasonFx {
    /// An instruction of this program.
    pub fn ix(&self, data: &ChainInstruction, metas: Vec<AccountMeta>) -> Instruction {
        Instruction::new_with_bytes(self.program, &borsh::to_vec(data).unwrap(), metas)
    }

    /// The world chunks, writable (`chunkKeys()`, `chain.mjs:55`).
    pub fn chunk_metas(&self) -> Vec<AccountMeta> {
        self.chunks.iter().map(w).collect()
    }

    /// The world chunks then every nation in civ order, writable: the
    /// accounts of CloseCommits, LogTickInput and ResolveTick.
    pub fn tick_metas(&self) -> Vec<AccountMeta> {
        self.chunks.iter().chain(&self.nations).map(w).collect()
    }

    /// The account of `Delegate`/`CommitPart` target `t`: a world chunk
    /// below `WORLD_CHUNKS`, nation `t - NATION_TARGET` from `NATION_TARGET`
    /// (`target()`, `chain.mjs:53`).
    pub fn target(&self, t: u16) -> Address {
        if (t as usize) < WORLD_CHUNKS {
            self.chunks[t as usize]
        } else {
            self.nations[(t - NATION_TARGET) as usize]
        }
    }

    /// Every delegated target: the chunks, then the nations.
    pub fn all_targets(&self) -> Vec<u16> {
        (0..WORLD_CHUNKS as u16)
            .chain((0..self.p.nations as u16).map(|c| NATION_TARGET + c))
            .collect()
    }
}
