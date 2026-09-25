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
//!   created through CPI, which Solana caps at 10 KiB, and the committor's
//!   finalize has a compute limit, so the world is stored in `CHUNK`-byte
//!   (4 KiB) chunks: chunk 0 starts with a fixed header (magic, body length,
//!   `WorldMeta`), and the borsh `WorldState` (or the `GenesisJob` while
//!   genesis runs) continues across the chunks in order.
//! * **Nation** `["nation", id, civ]` — delegated to the ER. The nation's
//!   office batches for the open tick, its governance inbox, and a cache of
//!   the office holders' keys and budgets so submissions never decode the world.
//! * **Vault** `["vault", id]` — an SPL token account owned by the Season PDA:
//!   entry fees (pool and operations), nation treasuries, and the operator's
//!   AI bounties and bond (V5 §18).
//! * **Roster** `["roster", id]` — base layer, only with operator AI
//!   members: their salts, revealed after the season (`RevealRoster`).

use borsh::{BorshDeserialize, BorshSerialize};
use permutation_rules::genesis::Entry;
use permutation_rules::gov::{GovEntry, NOBODY};
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
pub const ROSTER_SEED: &[u8] = b"roster";

pub const SEASON_SPACE: usize = 4 * 1024;
pub const MEMBER_SPACE: usize = 320;
pub const NATION_SPACE: usize = 8 * 1024;
/// Size of each world chunk. Small enough that the MagicBlock committor's
/// base-layer finalize of one densely written chunk fits its compute limit
/// (measured on devnet: a full 10 KiB chunk exceeded it, ~2.5 KiB of data
/// fit), and far below the 10 KiB CPI allocation limit.
pub const CHUNK: usize = 4 * 1024;
/// 80 KiB in total. A Blitz world with ~15 members ends near 23 KB; the rest
/// is room for up to 256 members and Season-sized worlds.
pub const WORLD_CHUNKS: usize = 20;
pub const MAX_NAME: usize = 24;
pub const MAX_NATIONS: usize = 8;
/// Members per season: bounded by the world account and the payout table in
/// the Season account (V5 §11; sharding members out of the world is roadmap).
pub const MAX_MEMBERS: u32 = 256;
/// Governance actions one signer may queue per tick.
pub const MAX_GOV_PER_SIGNER: usize = 8;
/// Room kept free in a nation account for each office's reveal (a batch
/// fits one transaction, so well under this): governance actions may not
/// fill the account so far that an officer's reveal no longer fits.
pub const REVEAL_ROOM: usize = 1100;
/// Largest history record logged in `PS_HISTORY` (bytes before base64).
pub const HISTORY_LOG_MAX: usize = 6000;
/// Operator AI members per season (V5 §18.2).
pub const MAX_AI: u16 = 64;
/// After the last tick, how long the operator has to reveal its roster
/// before anyone may finish the season without it (the bounties and the
/// bond then join the pool, V5 §18.2).
pub const ROSTER_GRACE_SECONDS: i64 = 3600;

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
    /// Runs genesis, seating, delegation and commits (any key can close,
    /// publish and resolve ticks).
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
    /// The history layer (2026-09-25, `permutation_rules::history`): the
    /// season this one follows (0: none) and its history root, taken when
    /// this season was created and mixed into the season seed; and this
    /// season's own history root, set by FinishSeason.
    pub prev_season_id: u64,
    pub prev_history_root: [u8; 32],
    pub history_root: [u8; 32],
    /// Operator AI members (V5 §18.2): how many, and the chain of their
    /// registration tags (`permutation_rules::roster::roster_chain`),
    /// committed before registration opens.
    pub ai_count: u16,
    pub roster_chain: [u8; 32],
    /// Bounty per AI and the operator's bond, both escrowed in the vault at
    /// creation (V5 §18.4).
    pub bounty_each: u64,
    pub bond: u64,
    /// The roster as revealed so far: the chain over the revealed tags, and
    /// how many.
    pub roster_acc: [u8; 32],
    pub roster_revealed: u16,
    /// Set by FinishSeason: `ROSTER_NONE`, `ROSTER_REVEALED` or
    /// `ROSTER_FORFEITED` (not revealed in time: bounties and bond joined
    /// the pool).
    pub roster_outcome: u8,
    /// Bounty paid into each nation's share (FinishSeason).
    pub bounty_paid: Vec<u64>,
}

pub const SEASON_MAGIC: [u8; 8] = *b"PSSEASN7";

/// `Season::roster_outcome`.
pub const ROSTER_NONE: u8 = 0;
pub const ROSTER_REVEALED: u8 = 1;
pub const ROSTER_FORFEITED: u8 = 2;

/// A season's address (`["season", id]`), as base58, from the program id as
/// base58; `None` if the program id is not a valid key. For tools (the
/// verifier follows a season's history to the season before it).
pub fn season_address(program_id: &str, season_id: u64) -> Option<String> {
    use core::str::FromStr;
    let program = solana_program::pubkey::Pubkey::from_str(program_id).ok()?;
    let (pda, _) = solana_program::pubkey::Pubkey::find_program_address(
        &[SEASON_SEED, &season_id.to_le_bytes()],
        &program,
    );
    Some(pda.to_string())
}

/// A season's roster address (`["roster", id]`), as `season_address`.
pub fn roster_address(program_id: &str, season_id: u64) -> Option<String> {
    use core::str::FromStr;
    let program = solana_program::pubkey::Pubkey::from_str(program_id).ok()?;
    let (pda, _) = solana_program::pubkey::Pubkey::find_program_address(
        &[ROSTER_SEED, &season_id.to_le_bytes()],
        &program,
    );
    Some(pda.to_string())
}

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
    /// 0 Human, 1 Agent, 2 Undeclared (self-declared, V5 D16); at most `MAX_KIND`.
    pub kind: u8,
    pub name: String,
    /// Optional agent registration proof (e.g. an ERC-8004 id hash), all zero if none.
    pub attestation: [u8; 32],
    /// Pre-season candidacy (`Role::bit` mask, within `STAND_MASK`) and
    /// votes per office (`u32::MAX` = none).
    pub stand: u8,
    pub votes: [u32; 4],
    /// Treasury shares: USDC deposited into the nation treasury.
    pub shares: u64,
    pub claimed: bool,
    /// Random bytes, or for an operator AI `roster_tag(season, wallet,
    /// salt)` (V5 §18.2): the wallet signed it at registration.
    pub tag: [u8; 32],
}

pub const MEMBER_MAGIC: [u8; 8] = *b"PSMEMBR6";
/// `MemberAccount::kind`: 0 Human, 1 Agent, 2 Undeclared.
pub const MAX_KIND: u8 = 2;
/// Every office's `Role::bit`.
pub const STAND_MASK: u8 = 0x0f;

// ------------------------------------------------------------------ roster

/// The operator's AI members, revealed after the season (V5 §18.2).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct RosterAccount {
    pub magic: [u8; 8],
    pub season_id: u64,
    pub bump: u8,
    /// In roster order.
    pub entries: Vec<RosterEntry>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RosterEntry {
    pub member: u32,
    pub civ: u16,
    pub salt: [u8; 32],
}

pub const ROSTER_MAGIC: [u8; 8] = *b"PSROSTR1";

/// Space for a roster of `n` AIs.
pub const fn roster_space(n: u16) -> usize {
    8 + 8 + 1 + 4 + n as usize * (4 + 2 + 32)
}

// ------------------------------------------------------------------ nation

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct NationAccount {
    pub magic: [u8; 8],
    pub season_id: u64,
    pub civ: u16,
    pub bump: u8,
    pub preset: u8,
    pub market: bool,
    /// The season's crank (only it may send `CommitPart`).
    pub crank: [u8; 32],
    /// Tick the batches must be for; set when the government opens and by every resolved tick.
    pub open_tick: u16,
    /// Office holders (`Role::index`), `u32::MAX` = vacant; and their session keys.
    pub officers: [u32; 4],
    pub keys: [[u8; 32]; 4],
    /// This tick's budget + bank per office, cached from the world.
    pub spendable: [u32; 4],
    /// Tick of each office's revealed batch (`NO_TICK` = none), so clients
    /// can see who revealed without decoding the batches.
    pub submitted: [u16; 4],
    /// The open tick's input is frozen (see `WorldMeta::frozen`).
    pub frozen: bool,
    /// Commitments are closed and the reveal window is open (see
    /// `WorldMeta::revealing`): commitments and governance are refused.
    pub revealing: bool,
    /// End of the reveal window (unix time): later reveals are refused, so
    /// nobody can wait to reveal until it has seen everyone else's salt.
    pub reveal_deadline: i64,
    /// Tick of each office's sealed commitment (`NO_TICK` = none), and the
    /// commitment (`permutation_rules::orders::order_commitment`).
    pub committed: [u16; 4],
    pub commits: [[u8; 32]; 4],
    /// Each revealed batch's salt (part of the tick randomness).
    pub salts: [[u8; 32]; 4],
    /// Revealed batches (they match their commitments).
    pub batches: [Option<OrderBatch>; 4],
    /// Governance actions for the open tick, in arrival order.
    pub inbox: Vec<GovEntry>,
}

pub const NATION_MAGIC: [u8; 8] = *b"PSNATN07";

/// "No tick": an office with nothing submitted or committed, a nation whose
/// government has not opened yet.
pub const NO_TICK: u16 = u16::MAX;

impl NationAccount {
    /// A nation account as `AllocNation` creates it: no tick open until the
    /// government opens (`open_nation`).
    pub fn new(
        season_id: u64,
        civ: u16,
        bump: u8,
        preset: u8,
        market: bool,
        crank: [u8; 32],
    ) -> Self {
        NationAccount {
            magic: NATION_MAGIC,
            season_id,
            civ,
            bump,
            preset,
            market,
            crank,
            open_tick: NO_TICK,
            officers: [NOBODY; 4],
            keys: [[0; 32]; 4],
            spendable: [0; 4],
            submitted: [NO_TICK; 4],
            frozen: false,
            revealing: false,
            reveal_deadline: 0,
            committed: [NO_TICK; 4],
            commits: [[0; 32]; 4],
            salts: [[0; 32]; 4],
            batches: [None, None, None, None],
            inbox: Vec::new(),
        }
    }

    /// Forget the previous tick: batches, commitments, salts, the inbox and
    /// the freeze / reveal flags.
    pub fn clear_tick(&mut self) {
        self.batches = [None, None, None, None];
        self.submitted = [NO_TICK; 4];
        self.frozen = false;
        self.revealing = false;
        self.reveal_deadline = 0;
        self.committed = [NO_TICK; 4];
        self.commits = [[0; 32]; 4];
        self.salts = [[0; 32]; 4];
        self.inbox.clear();
    }
}

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
    /// Sealed orders (commit–reveal): commitments closed at the tick's
    /// deadline (`CloseCommits`) and the reveal window is open until
    /// `deadline` (now the reveal deadline). Governance is closed too.
    pub revealing: bool,
}

/// Length of the reveal window after the commitments close: a sixth of the
/// tick (5 s of a 30 s Blitz tick), at least 2 s.
pub const fn reveal_seconds(tick_seconds: u32) -> i64 {
    let s = tick_seconds as i64 / 6;
    if s < 2 {
        2
    } else {
        s
    }
}

impl WorldMeta {
    /// Decode the meta from world chunk 0's header.
    pub fn from_chunk0(data: &[u8]) -> Result<Self, ProgramError> {
        let header = data.get(12..WORLD_HEADER).ok_or(ChainError::WrongWorld)?;
        WorldMeta::deserialize(&mut &header[..]).map_err(|_| ChainError::WrongWorld.into())
    }
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
    pub fn new(
        accounts: &'a [solana_program::account_info::AccountInfo<'info>],
    ) -> Result<Self, ProgramError> {
        if accounts.len() != WORLD_CHUNKS || accounts.iter().any(|a| a.data_len() != CHUNK) {
            return Err(ChainError::WorldTooSmall.into());
        }
        Ok(Chunks { accounts })
    }

    pub fn magic(&self) -> Result<[u8; 8], ProgramError> {
        Ok(self.accounts[0].try_borrow_data()?[..8].try_into().unwrap())
    }

    pub fn meta(&self) -> Result<WorldMeta, ProgramError> {
        WorldMeta::from_chunk0(&self.accounts[0].try_borrow_data()?)
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
        WorldState::try_from_slice(&self.body(&WORLD_MAGIC)?)
            .map_err(|_| ChainError::WrongWorld.into())
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
        GenesisJob::try_from_slice(&self.body(&GENESIS_MAGIC)?)
            .map_err(|_| ChainError::WrongWorld.into())
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
    data.get_mut(..bytes.len())
        .ok_or(ProgramError::AccountDataTooSmall)?
        .copy_from_slice(&bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

    fn season(nations: usize, members: usize) -> Season {
        Season {
            magic: SEASON_MAGIC,
            season_id: u64::MAX,
            bump: 255,
            vault_bump: 255,
            admin: [1; 32],
            crank: [2; 32],
            usdc_mint: [3; 32],
            usdc_decimals: 6,
            preset: 1,
            nations: nations as u8,
            entry_fee: u64::MAX,
            tick_seconds: u32::MAX,
            market: true,
            status: SeasonStatus::Finalized,
            world_seed: [4; 32],
            season_seed: [5; 32],
            member_count: members as u32,
            nation_members: vec![u32::MAX; nations],
            seated: members as u32,
            pool: u64::MAX,
            ops: u64::MAX,
            ops_withdrawn: true,
            treasury: vec![u64::MAX; nations],
            treasury_final: vec![u64::MAX; nations],
            payouts: vec![u64::MAX; members],
            final_root: [6; 32],
            prev_season_id: 0,
            prev_history_root: [0; 32],
            history_root: [0; 32],
            ai_count: 0,
            roster_chain: [0; 32],
            bounty_each: 0,
            bond: 0,
            roster_acc: [0; 32],
            roster_revealed: 0,
            roster_outcome: 0,
            bounty_paid: vec![u64::MAX; nations],
        }
    }

    #[test]
    fn a_full_roster_fits_its_account() {
        let r = RosterAccount {
            magic: ROSTER_MAGIC,
            season_id: u64::MAX,
            bump: 255,
            entries: vec![
                RosterEntry {
                    member: u32::MAX,
                    civ: u16::MAX,
                    salt: [7; 32],
                };
                MAX_AI as usize
            ],
        };
        assert_eq!(borsh::to_vec(&r).unwrap().len(), roster_space(MAX_AI));
    }

    #[test]
    fn a_full_season_fits_its_account() {
        let bytes = borsh::to_vec(&season(MAX_NATIONS, MAX_MEMBERS as usize)).unwrap();
        assert!(
            bytes.len() <= SEASON_SPACE,
            "{} > {}",
            bytes.len(),
            SEASON_SPACE
        );
    }

    #[test]
    fn a_member_with_the_longest_name_fits_its_account() {
        let m = MemberAccount {
            magic: MEMBER_MAGIC,
            season_id: u64::MAX,
            bump: 255,
            index: u32::MAX,
            civ: u16::MAX,
            wallet: [1; 32],
            session: [2; 32],
            kind: 2,
            name: "x".repeat(MAX_NAME),
            attestation: [3; 32],
            stand: 0x0f,
            votes: [u32::MAX; 4],
            shares: u64::MAX,
            claimed: true,
            tag: [0; 32],
        };
        assert!(borsh::to_vec(&m).unwrap().len() <= MEMBER_SPACE);
    }

    #[test]
    fn world_meta_fits_the_header() {
        let meta = WorldMeta {
            season_id: u64::MAX,
            deadline: i64::MAX,
            input_chunks: u16::MAX,
            ..Default::default()
        };
        assert!(borsh::to_vec(&meta).unwrap().len() <= WORLD_HEADER - 12);
        // The season id is the first field: `world_season_id` reads it at 12..20.
        let mut chunk0 = vec![0u8; CHUNK];
        chunk0[12..12 + borsh::to_vec(&meta).unwrap().len()]
            .copy_from_slice(&borsh::to_vec(&meta).unwrap());
        assert_eq!(
            u64::from_le_bytes(chunk0[12..20].try_into().unwrap()),
            u64::MAX
        );
        assert_eq!(WorldMeta::from_chunk0(&chunk0).unwrap(), meta);
        assert!(WorldMeta::from_chunk0(&chunk0[..20]).is_err());
    }

    #[test]
    fn world_body_round_trips_across_chunks() {
        let owner = Pubkey::new_unique();
        let keys: Vec<Pubkey> = (0..WORLD_CHUNKS).map(|_| Pubkey::new_unique()).collect();
        let mut lamports = [0u64; WORLD_CHUNKS];
        let mut data = vec![vec![0u8; CHUNK]; WORLD_CHUNKS];
        let accounts: Vec<AccountInfo> = keys
            .iter()
            .zip(lamports.iter_mut())
            .zip(data.iter_mut())
            .map(|((k, l), d)| AccountInfo::new(k, false, true, l, d, &owner, false))
            .collect();
        let chunks = Chunks::new(&accounts).unwrap();
        let meta = WorldMeta {
            season_id: 7,
            civs: 6,
            ..Default::default()
        };
        chunks.set_meta(&meta).unwrap();
        // Spans several chunk boundaries.
        let body: Vec<u8> = (0..3 * CHUNK + 123).map(|i| (i * 31 % 251) as u8).collect();
        chunks.write_body(&WORLD_MAGIC, &body).unwrap();
        assert_eq!(chunks.body(&WORLD_MAGIC).unwrap(), body);
        assert!(chunks.body(&GENESIS_MAGIC).is_err());
        assert_eq!(
            chunks.meta().unwrap(),
            meta,
            "writing the body keeps the meta"
        );
        let too_big = vec![0u8; WORLD_CHUNKS * CHUNK - WORLD_HEADER + 1];
        assert!(chunks.write_body(&WORLD_MAGIC, &too_big).is_err());
    }

    #[test]
    fn chunks_need_every_account_at_full_size() {
        let owner = Pubkey::new_unique();
        let key = Pubkey::new_unique();
        let mut l = 0u64;
        let mut d = vec![0u8; CHUNK - 1];
        let one = [AccountInfo::new(
            &key, false, true, &mut l, &mut d, &owner, false,
        )];
        assert!(Chunks::new(&one).is_err());
    }
}
