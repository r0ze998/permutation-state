//! Account layouts.
//!
//! * **Season** `["season", id]` — base layer. Entry, the USDC pool and the
//!   operations share, member and treasury totals, and after the season the
//!   per-member payouts and the final world root.
//! * **Member** `["member", id, wallet]` — base layer. One per wallet per
//!   season (V5 D1): nation, session key, self-declared kind, pre-season
//!   candidacy and votes, treasury shares, claim flag.
//! * **World** `["world", id, k]`, k = 0..WORLD_CHUNKS — delegated to the ER
//!   during play. Every account the ER delegates, commits or undelegates is
//!   created through CPI, which Solana caps at 10 KiB, so the world is stored
//!   in 10 KiB chunks: chunk 0 starts with a fixed header (magic, body length,
//!   `WorldMeta`), and the borsh `WorldState` (or the `GenesisJob` while
//!   genesis runs) continues across the chunks in order.
//! * **Nation** `["nation", id, civ]` — delegated to the ER. The nation's
//!   office batches for the open tick, its governance inbox, and a cache of
//!   the office holders' keys and budgets so submissions never decode the world.
//! * **Vault** `["vault", id]` — an SPL token account owned by the Season PDA:
//!   entry fees (pool and operations) and nation treasuries.

use borsh::{BorshDeserialize, BorshSerialize};
use permutation_rules::genesis::Entry;
use permutation_rules::gov::GovEntry;
use permutation_rules::map::MapJob;
use permutation_rules::orders::OrderBatch;
use permutation_rules::state::WorldState;
use solana_program::program_error::ProgramError;

use crate::error::ChainError;

pub const SEASON_SEED: &[u8] = b"season";
pub const WORLD_SEED: &[u8] = b"world";
pub const NATION_SEED: &[u8] = b"nation";
pub const MEMBER_SEED: &[u8] = b"member";
pub const VAULT_SEED: &[u8] = b"vault";

pub const SEASON_SPACE: usize = 4 * 1024;
pub const MEMBER_SPACE: usize = 320;
pub const NATION_SPACE: usize = 8 * 1024;
/// Size of each world chunk: the CPI allocation limit.
pub const CHUNK: usize = 10 * 1024;
/// Blitz peaks near 22 KB without members; 8 chunks leave room for members
/// and Season-sized worlds.
pub const WORLD_CHUNKS: usize = 8;
pub const MAX_NAME: usize = 24;
pub const MAX_NATIONS: usize = 8;
/// Members per season: bounded by the world account and the payout table in
/// the Season account (V5 §11; sharding members out of the world is roadmap).
pub const MAX_MEMBERS: u32 = 256;
/// Governance actions one signer may queue per tick.
pub const MAX_GOV_PER_SIGNER: usize = 8;

// ------------------------------------------------------------------ season

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeasonStatus {
    /// Accepting members.
    Registering,
    /// Genesis is being built in the world account.
    Genesis,
    /// Tick 0 exists; members are being seated in the world.
    Seating,
    /// The first election ran; play runs on the ER once delegated.
    Running,
    /// Payouts computed from the final world; claims open.
    Finalized,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Season {
    pub magic: [u8; 8],
    pub season_id: u64,
    pub bump: u8,
    pub vault_bump: u8,
    pub admin: [u8; 32],
    /// Runs genesis, seating, delegation and commits, and the acting
    /// officials of vacant offices (any key can resolve ticks).
    pub crank: [u8; 32],
    pub usdc_mint: [u8; 32],
    pub usdc_decimals: u8,
    pub preset: u8,
    pub nations: u8,
    /// The same for every member (V5 D4).
    pub entry_fee: u64,
    pub tick_seconds: u32,
    /// Whether the USDC market is open this season (V5 §7.5).
    pub market: bool,
    pub status: SeasonStatus,
    pub world_seed: [u8; 32],
    pub season_seed: [u8; 32],
    pub member_count: u32,
    pub nation_members: Vec<u32>,
    /// Members already added to the world (Seating).
    pub seated: u32,
    /// Prize pool: 80% of entry fees, plus the pool's share of in-play
    /// income at the end (V5 D10, D11).
    pub pool: u64,
    /// Operations: 20% of entry fees, plus its share of in-play income and
    /// rounding dust at the end.
    pub ops: u64,
    pub ops_withdrawn: bool,
    /// Treasury deposits per nation (V5 §7.5).
    pub treasury: Vec<u64>,
    /// Treasury left per nation in the final world, returned to depositors.
    pub treasury_final: Vec<u64>,
    /// USDC per member (index = member id), set by FinishSeason.
    pub payouts: Vec<u64>,
    /// `state_root` of the final world the payouts were computed from.
    pub final_root: [u8; 32],
}

pub const SEASON_MAGIC: [u8; 8] = *b"PSSEASN5";

// ------------------------------------------------------------------ member

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct MemberAccount {
    pub magic: [u8; 8],
    pub season_id: u64,
    pub bump: u8,
    /// Member id in the world (registration order).
    pub index: u32,
    pub civ: u16,
    /// Pays the entry, owns the prize and the treasury shares.
    pub wallet: [u8; 32],
    /// Signs governance actions and, in office, orders. Cannot move USDC.
    pub session: [u8; 32],
    /// 0 Human, 1 Agent, 2 Undeclared (self-declared, V5 D16).
    pub kind: u8,
    pub name: String,
    /// Optional agent registration proof (e.g. an ERC-8004 id hash), all zero if none.
    pub attestation: [u8; 32],
    /// Pre-season candidacy (`Role::bit` mask) and votes per office (`u32::MAX` = none).
    pub stand: u8,
    pub votes: [u32; 4],
    /// Treasury shares: USDC deposited into the nation treasury.
    pub shares: u64,
    pub claimed: bool,
}

pub const MEMBER_MAGIC: [u8; 8] = *b"PSMEMBR5";

// ------------------------------------------------------------------ nation

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct NationAccount {
    pub magic: [u8; 8],
    pub season_id: u64,
    pub civ: u16,
    pub bump: u8,
    pub preset: u8,
    pub market: bool,
    /// Signs for the acting official of a vacant office.
    pub crank: [u8; 32],
    /// Tick the batches must be for; set when the government opens and by every resolved tick.
    pub open_tick: u16,
    /// Office holders (`Role::index`), `u32::MAX` = vacant; and their session keys.
    pub officers: [u32; 4],
    pub keys: [[u8; 32]; 4],
    /// This tick's budget + bank per office, cached from the world.
    pub spendable: [u32; 4],
    /// Tick of each office's stored batch (`u16::MAX` = none), so clients
    /// can see who submitted without decoding the batches.
    pub submitted: [u16; 4],
    /// The open tick's input is frozen (see `WorldMeta::frozen`).
    pub frozen: bool,
    pub batches: [Option<OrderBatch>; 4],
    /// Governance actions for the open tick, in arrival order.
    pub inbox: Vec<GovEntry>,
}

pub const NATION_MAGIC: [u8; 8] = *b"PSNATN06";

// ------------------------------------------------------------------ world

pub const WORLD_MAGIC: [u8; 8] = *b"PSWORLD5";
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
    /// The USDC market is open this season (part of the ruleset).
    pub market: bool,
    /// The open tick's input is frozen and being published (`LogTickInput`):
    /// submissions are refused until the tick resolves.
    pub frozen: bool,
    /// Tick randomness, drawn when the input froze.
    pub vrf: [u8; 32],
    /// Chunks the frozen input takes, and how many were logged (in order).
    pub input_chunks: u16,
    pub input_logged: u16,
}

/// Bytes of tick input per `PS_INPUT` log: a transaction's logs are capped at
/// 10 000 bytes and `sol_log_data` writes base64.
pub const INPUT_CHUNK: usize = 6000;

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
