//! Account layouts.
//!
//! * **Season** `["season", id]` — base layer. Entry, the USDC pool, the civ
//!   registry (wallet, session key, payout owner) and, after the season, the
//!   payouts and claims.
//! * **World** `["world", id, k]`, k = 0..WORLD_CHUNKS — delegated to the ER
//!   during play. Every account the ER delegates, commits or undelegates is
//!   created through CPI, which Solana caps at 10 KiB, so the world is stored
//!   in 10 KiB chunks: chunk 0 starts with a fixed header (magic, body length,
//!   `WorldMeta`), and the borsh `WorldState` (or the `GenesisJob` while
//!   genesis runs) continues across the chunks in order.
//! * **Orders** `["orders", id, civ]` — delegated to the ER. One civ's batch
//!   for the open tick, plus the cached budget so `SubmitOrders` can check it
//!   without decoding the world.
//! * **Vault** `["vault", id]` — an SPL token account owned by the Season PDA.

use borsh::{BorshDeserialize, BorshSerialize};
use permutation_rules::genesis::Entry;
use permutation_rules::map::MapJob;
use permutation_rules::orders::OrderBatch;
use permutation_rules::state::WorldState;
use solana_program::program_error::ProgramError;

use crate::error::ChainError;

pub const SEASON_SEED: &[u8] = b"season";
pub const WORLD_SEED: &[u8] = b"world";
pub const ORDERS_SEED: &[u8] = b"orders";
pub const VAULT_SEED: &[u8] = b"vault";

pub const SEASON_SPACE: usize = 4 * 1024;
pub const ORDERS_SPACE: usize = 2 * 1024;
/// Size of each world chunk: the CPI allocation limit.
pub const CHUNK: usize = 10 * 1024;
/// Blitz peaks near 22 KB; 8 chunks leave room for Season-sized worlds.
pub const WORLD_CHUNKS: usize = 8;
pub const MAX_NAME: usize = 24;
pub const MAX_CIVS: usize = 16;

// ------------------------------------------------------------------ season

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeasonStatus {
    /// Accepting entries.
    Registering,
    /// Genesis is being built in the world account.
    Genesis,
    /// Tick 0 is ready; play runs on the ER once delegated.
    Running,
    /// Payouts computed from the final world; claims open.
    Finalized,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct CivSlot {
    /// Paid the entry; may submit orders.
    pub player: [u8; 32],
    /// Low-value key allowed to submit orders for this civ (session key).
    pub session: [u8; 32],
    /// Owner of the token account that receives prizes.
    pub payout: [u8; 32],
    /// 0 Human, 1 Agent, 2 Undeclared (cosmetic, §3.1).
    pub kind: u8,
    pub name: String,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Season {
    pub magic: [u8; 8],
    pub season_id: u64,
    pub bump: u8,
    pub vault_bump: u8,
    pub admin: [u8; 32],
    /// Runs genesis, delegation and commits (any key can resolve ticks).
    pub crank: [u8; 32],
    pub usdc_mint: [u8; 32],
    pub usdc_decimals: u8,
    pub preset: u8,
    pub max_civs: u8,
    pub entry_fee: u64,
    /// Exchange credit per civ in USDC base units (test credits, §11.3).
    pub exchange_credit: u64,
    pub tick_seconds: u32,
    pub status: SeasonStatus,
    pub world_seed: [u8; 32],
    pub season_seed: [u8; 32],
    pub civs: Vec<CivSlot>,
    /// USDC held for prizes (entry fees).
    pub pool: u64,
    pub payouts: Vec<u64>,
    pub claimed: Vec<bool>,
    pub rollover: u64,
    /// `state_root` of the final world the payouts were computed from.
    pub final_root: [u8; 32],
}

pub const SEASON_MAGIC: [u8; 8] = *b"PSSEASN1";

// ------------------------------------------------------------------ orders

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Orders {
    pub magic: [u8; 8],
    pub season_id: u64,
    pub civ: u16,
    pub bump: u8,
    pub preset: u8,
    pub player: [u8; 32],
    pub session: [u8; 32],
    /// Tick the batch must be for; set by genesis and every resolved tick.
    pub open_tick: u16,
    /// This tick's budget + bank (§4.1), cached from the world.
    pub spendable: u32,
    pub batch: Option<OrderBatch>,
}

pub const ORDERS_MAGIC: [u8; 8] = *b"PSORDER1";

// ------------------------------------------------------------------ world

pub const WORLD_MAGIC: [u8; 8] = *b"PSWORLD1";
/// Magic of a world account whose genesis is still being built.
pub const GENESIS_MAGIC: [u8; 8] = *b"PSGENJB1";
/// magic (8) + body length (4) + `WorldMeta` (padded to 64).
pub const WORLD_HEADER: usize = 8 + 4 + 64;

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WorldMeta {
    pub season_id: u64,
    pub preset: u8,
    pub civs: u8,
    pub tick_seconds: u32,
    /// Unix time after which anyone may resolve the open tick.
    pub deadline: i64,
    /// Set once the last tick resolved.
    pub finished: bool,
}

/// Genesis in progress (§2.4): map generation is too heavy for one
/// transaction, so it advances one `MapJob` step per transaction.
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct GenesisJob {
    pub world_seed: [u8; 32],
    pub season_seed: [u8; 32],
    pub entries: Vec<Entry>,
    pub map: MapJob,
}

/// The world's chunk accounts, in order, as borrowed byte slices.
pub struct Chunks<'a, 'info> {
    pub accounts: &'a [solana_program::account_info::AccountInfo<'info>],
}

impl<'a, 'info> Chunks<'a, 'info> {
    pub fn new(accounts: &'a [solana_program::account_info::AccountInfo<'info>]) -> Result<Self, ProgramError> {
        if accounts.len() != WORLD_CHUNKS || accounts.iter().any(|a| a.data_len() != CHUNK) {
            return Err(ChainError::WorldTooSmall.into());
        }
        Ok(Chunks { accounts })
    }

    pub fn magic(&self) -> Result<[u8; 8], ProgramError> {
        Ok(self.accounts[0].try_borrow_data()?[..8].try_into().unwrap())
    }

    pub fn meta(&self) -> Result<WorldMeta, ProgramError> {
        let d = self.accounts[0].try_borrow_data()?;
        WorldMeta::deserialize(&mut &d[12..WORLD_HEADER]).map_err(|_| ChainError::WrongWorld.into())
    }

    pub fn set_meta(&self, meta: &WorldMeta) -> Result<(), ProgramError> {
        let bytes = borsh::to_vec(meta).map_err(|_| ChainError::WrongWorld)?;
        self.accounts[0].try_borrow_mut_data()?[12..12 + bytes.len()].copy_from_slice(&bytes);
        Ok(())
    }

    /// The body (after the header), reassembled across chunks.
    pub fn body(&self, magic: &[u8; 8]) -> Result<Vec<u8>, ProgramError> {
        let len = {
            let d = self.accounts[0].try_borrow_data()?;
            if &d[..8] != magic {
                return Err(ChainError::WrongWorld.into());
            }
            u32::from_le_bytes(d[8..12].try_into().unwrap()) as usize
        };
        if len > WORLD_CHUNKS * CHUNK - WORLD_HEADER {
            return Err(ChainError::WrongWorld.into());
        }
        let mut out = Vec::with_capacity(len);
        let mut pos = WORLD_HEADER; // offset within the concatenated chunks
        while out.len() < len {
            let (k, off) = (pos / CHUNK, pos % CHUNK);
            let d = self.accounts[k].try_borrow_data()?;
            let take = (CHUNK - off).min(len - out.len());
            out.extend_from_slice(&d[off..off + take]);
            pos += take;
        }
        Ok(out)
    }

    pub fn write_body(&self, magic: &[u8; 8], bytes: &[u8]) -> Result<(), ProgramError> {
        if bytes.len() > WORLD_CHUNKS * CHUNK - WORLD_HEADER {
            return Err(ChainError::WorldTooSmall.into());
        }
        let mut written = 0;
        let mut pos = WORLD_HEADER;
        while written < bytes.len() {
            let (k, off) = (pos / CHUNK, pos % CHUNK);
            let mut d = self.accounts[k].try_borrow_mut_data()?;
            let take = (CHUNK - off).min(bytes.len() - written);
            d[off..off + take].copy_from_slice(&bytes[written..written + take]);
            written += take;
            pos += take;
        }
        let mut d0 = self.accounts[0].try_borrow_mut_data()?;
        d0[..8].copy_from_slice(magic);
        d0[8..12].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        Ok(())
    }

    pub fn is_world(&self) -> bool {
        self.magic().map(|m| m == WORLD_MAGIC).unwrap_or(false)
    }

    pub fn read_world(&self) -> Result<WorldState, ProgramError> {
        WorldState::try_from_slice(&self.body(&WORLD_MAGIC)?).map_err(|_| ChainError::WrongWorld.into())
    }

    /// Returns the new `state_root` (sha256 of exactly these bytes, §15).
    pub fn write_world(&self, state: &WorldState) -> Result<[u8; 32], ProgramError> {
        let buf = borsh::to_vec(state).map_err(|_| ChainError::WrongWorld)?;
        self.write_body(&WORLD_MAGIC, &buf)?;
        Ok(solana_program::hash::hashv(&[&buf]).to_bytes())
    }

    /// Root of the stored world without decoding it.
    pub fn root(&self) -> Result<[u8; 32], ProgramError> {
        Ok(solana_program::hash::hashv(&[&self.body(&WORLD_MAGIC)?]).to_bytes())
    }

    pub fn read_genesis(&self) -> Result<GenesisJob, ProgramError> {
        GenesisJob::try_from_slice(&self.body(&GENESIS_MAGIC)?).map_err(|_| ChainError::WrongWorld.into())
    }

    pub fn write_genesis(&self, job: &GenesisJob) -> Result<(), ProgramError> {
        let buf = borsh::to_vec(job).map_err(|_| ChainError::WrongWorld)?;
        self.write_body(&GENESIS_MAGIC, &buf)
    }
}

// ------------------------------------------------------------------ small helpers

pub fn load<T: BorshDeserialize>(data: &[u8]) -> Result<T, ProgramError> {
    T::deserialize(&mut &data[..]).map_err(|_| ChainError::NotInitialized.into())
}

pub fn store<T: BorshSerialize>(data: &mut [u8], value: &T) -> Result<(), ProgramError> {
    let bytes = borsh::to_vec(value).map_err(|_| ChainError::InvalidParams)?;
    data.get_mut(..bytes.len()).ok_or(ProgramError::AccountDataTooSmall)?.copy_from_slice(&bytes);
    Ok(())
}
