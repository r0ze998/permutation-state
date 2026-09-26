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
//!   genesis runs) continues across the chunks in order, followed (for a
//!   world) by the 32-byte root of the body.
//! * **Nation** `["nation", id, civ]` — delegated to the ER. The nation's
//!   office batches for the open tick, its governance inbox, a cache of the
//!   office holders' keys and budgets so submissions never decode the world,
//!   and its members' seated keys (the roll).
//! * **Vault** `["vault", id]` — an SPL token account owned by the Season PDA:
//!   entry fees (pool and operations), nation treasuries, and the operator's
//!   AI bounties and bond (V5 §18).
//! * **Roster** `["roster", id]` — base layer, only with operator AI
//!   members: their salts, revealed after the season (`RevealRoster`).
//!
//! Layout policy (DESIGN "Upgrades"): Season, Member, Nation and Roster are
//! append-only, with new fields at the end where zero means absent; their
//! magic changes only on a breaking change (including a change of the claim
//! formulas). World, WorldMeta and GenesisJob bump their magic on any layout
//! change. `store` zeroes an account's tail, so a shorter value never leaves
//! old bytes behind.

use borsh::{BorshDeserialize, BorshSerialize};
use permutation_rules::genesis::Entry;
use permutation_rules::gov::{GovAction, GovEntry, NOBODY};
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
/// is room for up to `SEASON_MEMBER_CAP` members.
pub const WORLD_CHUNKS: usize = 20;
/// `Delegate { target }`: world chunks are 0..WORLD_CHUNKS, nations are NATION_TARGET + civ.
pub const NATION_TARGET: u16 = 1000;
pub const MAX_NAME: usize = 24;
pub const MAX_NATIONS: usize = 8;
/// Members per season the Season account is sized for (its payout table,
/// V5 §11). A layout bound only: the binding limit is `SEASON_MEMBER_CAP`.
pub const MAX_MEMBERS: u32 = 256;
/// Governance actions one member may queue per tick. Seated keys are unique
/// (`seat::seat_key`) and `SubmitGov` takes only the member's seated key, so
/// this is also the limit per signer.
pub const MAX_GOV_PER_SIGNER: usize = 8;
/// Largest serialized `GovAction` that `SubmitGov` accepts (four 12-hex
/// moves are 426 B).
pub const MAX_GOV_ACTION_BYTES: usize = 480;
/// Stored bytes one governance slot pays for: a `GovEntry` is
/// `4 + 32 + borsh(action)` bytes and costs at least `ceil(len / 48)` slots.
pub const GOV_SLOT_BYTES: usize = 48;
/// Governance slots per tick for the whole world (heap budget of the tick
/// input, see DESIGN.md "Governance quota"). Split evenly between members.
pub const GOV_SLOTS_PER_TICK: u32 = 96;
/// Bounds of one member's quota per tick, in slots.
pub const GOV_QUOTA_MIN: u16 = 2;
pub const GOV_QUOTA_MAX: u16 = 12;
/// Most members a season may ever have for the governance bound to hold
/// (every member keeps `GOV_QUOTA_MIN` and the world stays within
/// `GOV_SLOTS_PER_TICK`).
pub const GOV_MEMBER_CEILING: u32 = GOV_SLOTS_PER_TICK / GOV_QUOTA_MIN as u32;
/// Members `Register` admits: the seats of one world. 48 is measured on the
/// WP07 prototype; the joint capacity gate on the integrated build is
/// pending until wave 4 (fallback 36, 24, 18).
pub const SEASON_MEMBER_CAP: u32 = 48;
/// Members of one nation `Register` admits: keeps `gov_quota` at
/// `GOV_QUOTA_MIN` or more.
pub const NATION_MEMBER_CAP: u32 = 24;
const _: () = assert!(SEASON_MEMBER_CAP <= GOV_MEMBER_CEILING && SEASON_MEMBER_CAP <= MAX_MEMBERS);
const _: () = assert!(GOV_MEMBER_CEILING * GOV_QUOTA_MIN as u32 <= GOV_SLOTS_PER_TICK);
/// Room kept free in a nation account for each office's reveal (a batch
/// fits one transaction, so well under this): governance actions may not
/// fill the account so far that an officer's reveal no longer fits. The
/// largest revealed batch under the caps encodes to 1,092 B.
pub const REVEAL_ROOM: usize = 1100;
/// Most encoded bytes of one revealed batch's own orders. A 1,232-byte
/// transaction carries at most 912 B of orders, so this never refuses a
/// sendable batch; it bounds the tick input if packets grow.
pub const MAX_REVEAL_BYTES: usize = 950;
/// Largest history record logged in `PS_HISTORY` (bytes before base64).
pub const HISTORY_LOG_MAX: usize = 6000;
/// Operator AI members per season (V5 §18.2); `CreateSeason` also keeps
/// them below `SEASON_MEMBER_CAP`.
pub const MAX_AI: u16 = 64;
/// After the last tick, how long the operator has to reveal its roster
/// before anyone may finish the season without it (the bounties and the
/// bond then join the pool, V5 §18.2).
pub const ROSTER_GRACE_SECONDS: i64 = 3600;

/// `Season::preset`: the Blitz ruleset (radius-13 map). The only preset a
/// season can be created with; tick length is `tick_seconds`, so a slow
/// (e.g. 4-hour) season is this preset with a longer tick.
pub const PRESET_BLITZ: u8 = 0;
/// `Season::preset`: the Season ruleset (radius-19 map). Readable, but not
/// creatable: its genesis needs about 2.0M compute units in one step (over
/// the 1.4M a transaction can use), and its ticks and heap were never
/// measured on chain.
pub const PRESET_SEASON: u8 = 1;
/// Most `work` one `GenesisStep` does (start candidates checked; balancing
/// rounds are `work / 25`). The crank sends 50; Blitz steps then peak near
/// 0.98M compute units. 200 already exceeds 1.4M on a 4-nation Blitz map.
pub const MAX_GENESIS_WORK: u32 = 50;
/// Fewest nations in a season.
pub const MIN_NATIONS: u8 = 2;

/// Whether `CreateSeason` (and `StartSeason`) accept `(preset, nations)`:
/// only pairs whose genesis was measured on the SBF build (svm-tests
/// `genesis_every_creatable`). Today: Blitz, 2..=6 nations. Widen it only
/// together with that test and a measured end-to-end season.
pub fn creatable(preset: u8, nations: u8) -> bool {
    let max = MAX_NATIONS.min(permutation_rules::genesis::NATIONS.len()) as u8;
    preset == PRESET_BLITZ && (MIN_NATIONS..=max).contains(&nations)
}

/// The rules' USDC amounts (spend consent, tariff curve) are in 6-decimal
/// base units: the season's mint must have 6 decimals.
pub const USDC_DECIMALS: u8 = 6;
/// Largest entry fee, deposit and bounty per AI: 1,000 USDC each.
pub const MAX_ENTRY_FEE: u64 = 1_000_000_000;
pub const MAX_DEPOSIT: u64 = 1_000_000_000;
pub const MAX_BOUNTY: u64 = 1_000_000_000;
/// Largest operator bond: 100,000,000 USDC. It only bounds arithmetic: it
/// stays at or above twice the most AI entry fees and above `bond_floor`
/// under the caps above, so no legitimate bond is refused.
pub const MAX_BOND: u64 = 100_000_000_000_000;

/// Escape hatches (`lifecycle`): a stage that has not moved for this long
/// may be aborted by anyone.
pub const ABORT_GRACE_SECONDS: i64 = 86_400;
/// After the running deadline, how long a finishable season is left to
/// `FinishSeason` before anyone may abort it anyway.
pub const FINISH_GRACE_SECONDS: i64 = 7 * 86_400;
/// `CreateSeason`: `start_by` is at most this far ahead.
pub const MAX_REGISTRATION_SECONDS: i64 = 14 * 86_400;
/// Longest tick (the Season preset's own 4 h tick).
pub const MAX_TICK_SECONDS: u32 = 14_400;
/// Allowance per tick on top of twice its length (`running_deadline`).
pub const TICK_OVERHEAD_SECONDS: i64 = 60;
/// `SeatMembers` and `OpenGovernment` fall to anyone this long after the
/// stage began.
pub const TAKEOVER_SECONDS: i64 = 600;

/// Tick 0's deadline when the government opens on the base layer: long
/// enough for delegation, after which the crank's `StartClock` gives tick 0
/// a full `tick_seconds` on the ER. If no crank ever starts the clock,
/// anyone may close tick 0 once this passes (the season never waits on it).
pub const TICK0_GRACE_SECONDS: i64 = 600;
/// `ResolveTick { to }` with this bit: one degraded phase (the input is not
/// applied), for a tick no normal part fits.
pub const DEGRADED: u8 = 0x80;

/// How long after a tick's deadline a degraded step is allowed: twice the
/// tick, at least ten minutes.
pub const fn degrade_after(tick_seconds: u32) -> i64 {
    let t = 2 * tick_seconds as i64;
    if t < 600 {
        600
    } else {
        t
    }
}

/// `RetryTickRandomness`: how long to wait for the oracle before asking again.
pub const VRF_RETRY_SECONDS: i64 = 10;
/// `RetryTickRandomness`: after this long since the freeze, the tick takes
/// the fallback randomness (flagged).
pub const VRF_GIVEUP_SECONDS: i64 = 600;
/// `RetrySeasonSeed`: how long to wait for the oracle before asking again.
pub const SEED_RETRY_SECONDS: i64 = 60;

// ------------------------------------------------------------------ delegation targets

/// The bits of `Season::delegated` / `rolled_back` that are world chunks.
pub const CHUNK_BITS: u32 = (1 << WORLD_CHUNKS) - 1;
const _: () = assert!(WORLD_CHUNKS + MAX_NATIONS <= 32);
/// Nations a `CommitPart` / `UndelegatePart` intent may carry.
pub const MAX_NATIONS_PER_INTENT: usize = 3;

/// The bit of `target` in `Season::delegated` / `rolled_back`: `1 << k` for
/// world chunk k, `1 << (20 + civ)` for nation `NATION_TARGET + civ`.
pub fn target_bit(target: u16) -> Option<u32> {
    if (target as usize) < WORLD_CHUNKS {
        Some(1 << target)
    } else {
        let civ = target.checked_sub(NATION_TARGET)? as usize;
        (civ < MAX_NATIONS).then(|| 1 << (WORLD_CHUNKS + civ))
    }
}

/// Every target of a season of `nations` nations, as bits.
pub const fn all_targets(nations: u8) -> u32 {
    CHUNK_BITS | (((1u32 << nations) - 1) << WORLD_CHUNKS)
}

/// The order `UndelegatePart` takes a season's accounts back: world chunks
/// 1..19 (each alone), the nations in civ order, and world chunk 0 last
/// (its header counts the steps and says the season is over).
pub fn undelegation_order(civs: u8) -> Vec<u16> {
    (1..WORLD_CHUNKS as u16)
        .chain((0..civs as u16).map(|c| NATION_TARGET + c))
        .chain([0])
        .collect()
}

/// Inverse of `undelegation_order`: chunk k > 0 → k − 1; nation civ → 19 +
/// civ; chunk 0 → 19 + civs. `None` for a target not in the order.
pub fn undelegation_position(civs: u8, target: u16) -> Option<usize> {
    let last = WORLD_CHUNKS - 1;
    if target == 0 {
        Some(last + civs as usize)
    } else if (target as usize) < WORLD_CHUNKS {
        Some(target as usize - 1)
    } else {
        let civ = target.checked_sub(NATION_TARGET)?;
        (civ < civs as u16).then_some(last + civ as usize)
    }
}

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
    /// Registration closed; waiting for the VRF season seed
    /// (`ConsumeSeasonSeed`).
    Seeding,
    /// Aborted (`lifecycle::check_abort`): members claim refunds.
    Aborted,
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
    /// `PRESET_BLITZ`; `PRESET_SEASON` is readable but no longer creatable
    /// (`creatable`).
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
    /// Operator AI members (V5 §18.2): how many, and the blinded commitment
    /// to the chain of their registration tags
    /// (`permutation_rules::roster::roster_commit(blind, roster_chain(tags))`),
    /// committed before registration opens: hiding until the blind is
    /// revealed.
    pub ai_count: u16,
    pub roster_commit: [u8; 32],
    /// Bounty per AI and the operator's bond, both escrowed in the vault
    /// (V5 §18.4); `PostBond` adds to the bond while registering.
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
    // ---- v8 tail (append-only; zero = absent). Block 1: delegation.
    /// Targets delegated this season (`target_bit`), set by `Delegate`
    /// before its CPI: each target is delegated once.
    pub delegated: u32,
    // Block 2: the roster (WP09).
    /// Stored by the `RevealRoster` that completes the roster; zero before.
    pub roster_blind: [u8; 32],
    // Block 3: treasury refunds (WP10).
    /// Per nation, the deposit shares that `treasury_final` is refunded over
    /// (FinishSeason): the deposits less the revealed AI members' shares,
    /// whose refund is inside their `payouts` instead. Empty: legacy rule
    /// (refund over `treasury`).
    pub refund_base: Vec<u64>,
    /// Bitset over member indices (FinishSeason): members whose treasury
    /// refund is already inside `payouts` (the revealed AI members); their
    /// claim is `payouts[index]` alone. Empty: nobody.
    pub refund_in_payout: Vec<u8>,
    // Block 4: the season seed (WP11).
    /// `randomness::SEED_*`: none, pending (requested from the VRF), VRF,
    /// or dev (in-transaction, `dev-randomness` builds).
    pub seed_state: u8,
    /// The VRF output the season seed was derived from.
    pub seed_oracle: [u8; 32],
    /// Unix time of the latest seed request, and how many were made.
    pub seed_requested_at: i64,
    pub seed_requests: u8,
    // Block 5: solvency (WP12).
    /// The treasury deposit every member pays at Register (V5 §7.5, uniform
    /// per season; 0: no deposits). Set by CreateSeason.
    pub deposit: u64,
    /// Set by FinishSeason (or Abort): what the vault still owes (payouts,
    /// treasury refunds, operations). Claim and WithdrawOps subtract what
    /// they pay.
    pub outstanding: u64,
    /// Set by FinishSeason: the final world did not conserve USDC, or the
    /// settlement did not balance, so every member got back its entry fee
    /// and deposit and the operator its escrow (`finalize::void`).
    pub voided: bool,
    // Block 6: escape hatches (WP14).
    /// `StartSeason` is due by then (unix time, base clock).
    pub start_by: i64,
    /// Unix time the current status began.
    pub stage_at: i64,
    /// Targets taken back by `RollbackUndelegation` while Running
    /// (`target_bit`): a rolled-back world is never finished, only aborted.
    pub rolled_back: u32,
    /// `SeasonStatus as u8` the season was aborted from (0 otherwise).
    pub aborted_from: u8,
    /// The ER validator every account of the season is delegated to
    /// (`Delegate` refuses another).
    pub validator: [u8; 32],
    // Block 7: code bound at creation (WP15).
    /// Rules the season was created under (`rules::PINNED_RULES_VERSION` /
    /// `PINNED_RULESET_HASHES`), fixed before anyone registers; every
    /// rules-applying instruction refuses another build's rules.
    pub rules_version: u16,
    pub rules_hash: [u8; 32],
    /// `rules::CHAIN_LOGIC_VERSION` at creation (finalize, seating,
    /// budgets); checked with the rules.
    pub logic_version: u16,
    /// Base-layer slot of CreateSeason (the verifier compares program
    /// upgrades with it).
    pub created_slot: u64,
}

pub const SEASON_MAGIC: [u8; 8] = *b"PSSEASN8";
/// The previous Season layout: every field above up to `bounty_paid`, with
/// a zero tail. Readable by `load_season_compat` only (a predecessor for
/// CreateSeason's history, Claim, WithdrawOps).
pub const LEGACY_SEASON_MAGIC: [u8; 8] = *b"PSSEASN7";

/// `Season::roster_outcome`.
pub const ROSTER_NONE: u8 = 0;
pub const ROSTER_REVEALED: u8 = 1;
pub const ROSTER_FORFEITED: u8 = 2;

/// What withholding the roster costs the operator at least, with the bond at
/// its floor: the AI members' own entry fees.
pub fn forfeit_penalty(season: &Season) -> u64 {
    season.entry_fee.saturating_mul(season.ai_count as u64)
}

/// The smallest bond with which withholding the roster costs the operator
/// at least `forfeit_penalty` (checked by `StartSeason`). A forfeited roster
/// splits the pool equally among every member (`finalize`), and that pool is
/// at most the fees' pool + the bounties + the deposits (in-play income
/// comes out of the deposits) + the bond. While at least one person plays,
/// at most `a = min(ai_count, member_count − 1)` members are the operator's,
/// and together they must receive at least `penalty` less than the bond:
/// `a · (p0 + bond) / n <= bond − penalty`, i.e.
/// `bond >= ceil((a · p0 + n · penalty) / (n − a))`.
///
/// When every member may be an AI (`member_count == ai_count`, an AI-only
/// season) nobody is left to protect and the floor is 0. The floor is not
/// capped (a cap on it would let a season start with a free forfeit); under
/// the caps it stays below `MAX_BOND`.
pub fn bond_floor(season: &Season) -> u64 {
    let n = season.member_count as u128;
    let ai = season.ai_count as u128;
    if ai == 0 || n == 0 || n == ai {
        return 0;
    }
    let a = ai.min(n - 1);
    let deposits: u128 = season.treasury.iter().map(|t| *t as u128).sum();
    let p0 = season.pool as u128 + season.bounty_each as u128 * ai + deposits;
    let penalty = forfeit_penalty(season) as u128;
    (a * p0 + n * penalty).div_ceil(n - a).min(u64::MAX as u128) as u64
}

/// A season's address (`["season", id]`), as base58, from the program id as
/// base58; `None` if the program id is not a valid key. For tools (the
/// verifier follows a season's history to the season before it).
pub fn season_address(program_id: &str, season_id: u64) -> Option<String> {
    pda_address(program_id, &[SEASON_SEED, &season_id.to_le_bytes()])
}

/// A season's roster address (`["roster", id]`), as `season_address`.
pub fn roster_address(program_id: &str, season_id: u64) -> Option<String> {
    pda_address(program_id, &[ROSTER_SEED, &season_id.to_le_bytes()])
}

/// A season's vault address (`["vault", id]`), as `season_address`.
#[cfg(not(target_os = "solana"))]
pub fn vault_address(program_id: &str, season_id: u64) -> Option<String> {
    pda_address(program_id, &[VAULT_SEED, &season_id.to_le_bytes()])
}

/// World chunk `chunk`'s address (`["world", id, chunk]`), as `season_address`.
#[cfg(not(target_os = "solana"))]
pub fn world_chunk_address(program_id: &str, season_id: u64, chunk: u8) -> Option<String> {
    pda_address(
        program_id,
        &[WORLD_SEED, &season_id.to_le_bytes(), &[chunk]],
    )
}

fn pda_address(program_id: &str, seeds: &[&[u8]]) -> Option<String> {
    use core::str::FromStr;
    let program = solana_program::pubkey::Pubkey::from_str(program_id).ok()?;
    let (pda, _) = solana_program::pubkey::Pubkey::find_program_address(seeds, &program);
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
    /// The member's deposit shares, copied from its `MemberAccount` at reveal.
    pub shares: u64,
}

pub const ROSTER_MAGIC: [u8; 8] = *b"PSROSTR2";

/// Space for a roster of `n` AIs.
pub const fn roster_space(n: u16) -> usize {
    8 + 8 + 1 + 4 + n as usize * (4 + 2 + 32 + 8)
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
    /// While the commitments are open: the tick's deadline (the world's
    /// `meta.deadline`); commitments and governance are refused from then on.
    /// While revealing: the end of the reveal window; later reveals are
    /// refused, so nobody can wait to reveal until it has seen every salt.
    pub deadline: i64,
    /// Tick of each office's sealed commitment (`NO_TICK` = none), and the
    /// commitment (`permutation_rules::orders::order_commitment`).
    pub committed: [u16; 4],
    pub commits: [[u8; 32]; 4],
    /// Each revealed batch's salt (part of the tick randomness).
    pub salts: [[u8; 32]; 4],
    /// Governance slots each member of this nation may use per tick
    /// (`gov_quota`), set by `OpenGovernment`. Fixed offset (497): clients
    /// read it with the header.
    pub gov_quota: u16,
    /// Revealed batches (they match their commitments).
    pub batches: [Option<OrderBatch>; 4],
    /// Governance actions for the open tick, in arrival order.
    #[borsh(deserialize_with = "vec_exact")]
    pub inbox: Vec<GovEntry>,
    /// The nation's members and their seated keys (first 8 bytes), set by
    /// `OpenGovernment` from the world: `SubmitGov` accepts only these.
    pub roll: Vec<RollSeat>,
}

pub const NATION_MAGIC: [u8; 8] = *b"PSNATN08";

/// "No tick": an office with nothing submitted or committed, a nation whose
/// government has not opened yet.
pub const NO_TICK: u16 = u16::MAX;

/// One member of a nation, as `SubmitGov` authenticates it.
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RollSeat {
    pub member: u32,
    /// The first 8 bytes of the member's seated key (the engine checks the
    /// whole key again when the tick resolves).
    pub key8: [u8; 8],
}

/// The first 8 bytes of a key (`RollSeat::key8`).
pub fn key8(key: &[u8; 32]) -> [u8; 8] {
    let mut k = [0u8; 8];
    k.copy_from_slice(&key[..8]);
    k
}

/// Slots a governance action costs: one per entry and one per proposed
/// order (each is a 72-byte value once decoded), and at least one per
/// `GOV_SLOT_BYTES` stored. `None` if it is larger than `MAX_GOV_ACTION_BYTES`.
pub fn gov_slots(action: &GovAction) -> Option<u16> {
    let bytes = borsh::object_length(action).ok()?;
    if bytes > MAX_GOV_ACTION_BYTES {
        return None;
    }
    let orders = match action {
        GovAction::Propose { orders, .. } => orders.len(),
        _ => 0,
    };
    let stored = (4 + 32 + bytes).div_ceil(GOV_SLOT_BYTES);
    Some((1 + orders).max(stored) as u16)
}

/// Per-member quota for a nation of `here` members in a season of
/// `members`: an even share of `GOV_SLOTS_PER_TICK` within
/// `GOV_QUOTA_MIN..=GOV_QUOTA_MAX`, lowered so that every member can use its
/// whole quota in the nation account next to every office's `REVEAL_ROOM`.
pub fn gov_quota(members: u32, here: u32) -> u16 {
    let share = (GOV_SLOTS_PER_TICK / members.max(1)).min(u16::MAX as u32) as u16;
    let q = share.clamp(GOV_QUOTA_MIN, GOV_QUOTA_MAX);
    let base = NationAccount::base_len(here as usize);
    let space = NATION_SPACE.saturating_sub(4 * REVEAL_ROOM + base);
    let fits = space / (GOV_SLOT_BYTES * here.max(1) as usize);
    q.min(fits.min(u16::MAX as usize) as u16)
}

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
            deadline: 0,
            committed: [NO_TICK; 4],
            commits: [[0; 32]; 4],
            salts: [[0; 32]; 4],
            gov_quota: 0,
            batches: [None, None, None, None],
            inbox: Vec::new(),
            roll: Vec::new(),
        }
    }

    /// Serialized length of an open nation account with `members` on its
    /// roll and nothing submitted (no batch, empty inbox).
    pub fn base_len(members: usize) -> usize {
        let empty = NationAccount::new(0, 0, 0, 0, false, [0; 32]);
        borsh::object_length(&empty).unwrap_or(0) + members * 12
    }

    /// Fill the roll (the members of this civ, in id order, with the first
    /// 8 bytes of their seated key) and `gov_quota` from the world. Members
    /// never change nation or key after seating, so the roll is built once
    /// (`OpenGovernment`).
    pub fn seat_roll(&mut self, state: &WorldState) {
        let civ = self.civ;
        self.roll = state
            .members
            .iter()
            .enumerate()
            .filter(|(_, m)| m.civ == civ)
            .map(|(i, m)| RollSeat {
                member: i as u32,
                key8: key8(&m.key),
            })
            .collect();
        self.gov_quota = gov_quota(state.members.len() as u32, self.roll.len() as u32);
    }

    /// Forget the previous tick: batches, commitments, salts, the inbox and
    /// the freeze / reveal flags and the deadline (callers set it next).
    pub fn clear_tick(&mut self) {
        self.batches = [None, None, None, None];
        self.submitted = [NO_TICK; 4];
        self.frozen = false;
        self.revealing = false;
        self.deadline = 0;
        self.committed = [NO_TICK; 4];
        self.commits = [[0; 32]; 4];
        self.salts = [[0; 32]; 4];
        self.inbox.clear();
    }
}

/// The fixed-size front of a nation account (every field before `batches`,
/// 499 bytes): what the ER instructions that do not need the batches or the
/// inbox decode, with no heap.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct NationHead {
    pub magic: [u8; 8],
    pub season_id: u64,
    pub civ: u16,
    pub bump: u8,
    pub preset: u8,
    pub market: bool,
    pub crank: [u8; 32],
    pub open_tick: u16,
    pub officers: [u32; 4],
    pub keys: [[u8; 32]; 4],
    pub spendable: [u32; 4],
    pub submitted: [u16; 4],
    pub frozen: bool,
    pub revealing: bool,
    pub deadline: i64,
    pub committed: [u16; 4],
    pub commits: [[u8; 32]; 4],
    pub salts: [[u8; 32]; 4],
    pub gov_quota: u16,
}

/// Bytes of a `NationHead`.
pub const NATION_HEAD_LEN: usize = 499;

impl NationHead {
    /// The whole account as `UndelegatePart` gives it back: this front with
    /// `clear_tick()`, frozen, no batches, an empty inbox, and every field
    /// after the inbox at its `NationAccount::new` default (the roll empty).
    pub fn cleared(&self) -> NationAccount {
        let mut n = NationAccount::new(
            self.season_id,
            self.civ,
            self.bump,
            self.preset,
            self.market,
            self.crank,
        );
        n.magic = self.magic;
        n.open_tick = self.open_tick;
        n.officers = self.officers;
        n.keys = self.keys;
        n.spendable = self.spendable;
        n.gov_quota = self.gov_quota;
        n.clear_tick();
        n.frozen = true;
        n
    }
}

// ------------------------------------------------------------------ world

pub const WORLD_MAGIC: [u8; 8] = *b"PSWORLD6";
/// Magic of a world account whose genesis is still being built.
pub const GENESIS_MAGIC: [u8; 8] = *b"PSGENJB2";
/// Magic families the undelegation callback accepts (any version of the
/// layout: an account delegated under an older build must still come home).
pub const WORLD_FAMILY: &[u8] = b"PSWORLD";
pub const GENESIS_FAMILY: &[u8] = b"PSGENJB";
pub const NATION_FAMILY: &[u8] = b"PSNATN";
/// Space reserved for the borsh `WorldMeta` in chunk 0's header.
pub const WORLD_META_SPACE: usize = 244;
/// magic (8) + body length (4) + `WorldMeta` (padded to `WORLD_META_SPACE`).
pub const WORLD_HEADER: usize = 8 + 4 + WORLD_META_SPACE;
/// Most bytes after the header: a world body and its 32-byte root trailer
/// (so the largest world body is `WORLD_BODY_MAX − 32`), or a genesis job.
pub const WORLD_BODY_MAX: usize = WORLD_CHUNKS * CHUNK - WORLD_HEADER;

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WorldMeta {
    pub season_id: u64,
    pub preset: u8,
    pub civs: u8,
    pub tick_seconds: u32,
    /// Unix time the open tick's commitments close (then, while revealing,
    /// the reveal window's end).
    pub deadline: i64,
    /// Set once the last tick resolved.
    pub finished: bool,
    /// The USDC market is open this season (part of the ruleset).
    pub market: bool,
    /// The open tick's input is frozen (`FreezeTick`) and being published
    /// (`LogTickInput`): submissions are refused until the tick resolves.
    pub frozen: bool,
    /// The tick's final randomness (`randomness::tick_vrf`), once drawn.
    pub vrf: [u8; 32],
    /// Chunks the frozen input takes, and how many were logged (in order).
    pub input_chunks: u16,
    pub input_logged: u16,
    /// Sealed orders (commit–reveal): commitments closed at the tick's
    /// deadline (`CloseCommits`) and the reveal window is open until
    /// `deadline` (now the reveal deadline). Governance is closed too.
    pub revealing: bool,
    /// After the last tick: how many targets of `undelegation_order(civs)`
    /// `UndelegatePart` has scheduled or skipped (chunk 0 comes last).
    pub undelegated: u8,
    /// sha256 of the frozen tick input, set by every `LogTickInput`; every
    /// `ResolveTick` part logs it; zero while not frozen.
    pub input_hash: [u8; 32],
    /// `randomness::RAND_*`: none (not frozen), pending (requested from the
    /// VRF), VRF, fallback or dev.
    pub rand_state: u8,
    /// The tick the randomness request is for (`NO_TICK` when none).
    pub rand_tick: u16,
    /// `randomness::salts_hash` of the frozen input's revealed salts.
    pub rand_pre: [u8; 32],
    /// The VRF output (zero for the fallback and dev).
    pub rand_out: [u8; 32],
    /// ER unix time of the freeze, and of the latest randomness request.
    pub frozen_at: i64,
    pub rand_requested_at: i64,
    /// Requests made this tick (saturating).
    pub rand_requests: u8,
    /// Set by ResolveTick when a tick completes with USDC not conserved;
    /// never cleared. FinishSeason voids the season when it is set, even if
    /// a later tick healed the sum.
    pub usdc_broken: bool,
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

    /// Write the meta into world chunk 0's header (at byte 12).
    pub fn write_chunk0(&self, data: &mut [u8]) -> Result<(), ProgramError> {
        let mut header = data
            .get_mut(12..WORLD_HEADER)
            .ok_or(ChainError::WrongWorld)?;
        self.serialize(&mut header)
            .map_err(|_| ChainError::WrongWorld.into())
    }
}

/// Bytes of tick input per `PS_INPUT` log: a transaction's logs are capped at
/// 10 000 bytes and `sol_log_data` writes base64 (a message is dropped once
/// the total would reach the cap). Chunk 0's transaction also logs the
/// salts: 87 + 4·⌈5000/3⌉ = 6,755 B of `PS_INPUT`, 1,205 B of `PS_SALTS`
/// (24 salts) and about 700 B of runtime lines leave 1,340 B of margin.
pub const INPUT_CHUNK: usize = 5000;
/// The longest `PS_SALTS` line (base64, with the runtime's prefix): six
/// nations (`creatable`) of four offices.
const SALTS_LINE_MAX: usize = 14 + 12 + 1 + 4 + 1 + 44 + 1 + 4 * (4 + 35 * 4 * 6usize).div_ceil(3);
const _: () = assert!(87 + 4 * INPUT_CHUNK.div_ceil(3) + SALTS_LINE_MAX + 700 < 9_500);

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
        meta.write_chunk0(&mut self.accounts[0].try_borrow_mut_data()?)
    }

    /// The body length chunk 0 records, if its magic is `magic` and it fits.
    fn body_len(&self, magic: &[u8; 8]) -> Result<usize, ProgramError> {
        let d = self.accounts[0].try_borrow_data()?;
        if &d[..8] != magic {
            return Err(ChainError::WrongWorld.into());
        }
        let len = u32::from_le_bytes(d[8..12].try_into().unwrap()) as usize;
        if len > WORLD_BODY_MAX {
            return Err(ChainError::WrongWorld.into());
        }
        Ok(len)
    }

    /// `out.len()` bytes at `pos` of the concatenated chunks.
    fn read_at(&self, mut pos: usize, out: &mut [u8]) -> Result<(), ProgramError> {
        let mut read = 0;
        while read < out.len() {
            let (k, off) = (pos / CHUNK, pos % CHUNK);
            let d = self
                .accounts
                .get(k)
                .ok_or(ChainError::WrongWorld)?
                .try_borrow_data()?;
            let take = (CHUNK - off).min(out.len() - read);
            out[read..read + take].copy_from_slice(&d[off..off + take]);
            read += take;
            pos += take;
        }
        Ok(())
    }

    /// `bytes` at `pos` of the concatenated chunks.
    fn write_at(&self, mut pos: usize, bytes: &[u8]) -> Result<(), ProgramError> {
        let mut written = 0;
        while written < bytes.len() {
            let (k, off) = (pos / CHUNK, pos % CHUNK);
            let mut d = self
                .accounts
                .get(k)
                .ok_or(ChainError::WorldTooSmall)?
                .try_borrow_mut_data()?;
            let take = (CHUNK - off).min(bytes.len() - written);
            d[off..off + take].copy_from_slice(&bytes[written..written + take]);
            written += take;
            pos += take;
        }
        Ok(())
    }

    /// The body (after the header), reassembled across chunks. Copies the
    /// whole body: tests and tools.
    pub fn body(&self, magic: &[u8; 8]) -> Result<Vec<u8>, ProgramError> {
        let len = self.body_len(magic)?;
        let mut out = vec![0u8; len];
        self.read_at(WORLD_HEADER, &mut out)?;
        Ok(out)
    }

    /// Write `bytes` as the body with `magic` (no root trailer). Tests and
    /// tools; `write_world` / `write_genesis` in instructions.
    pub fn write_body(&self, magic: &[u8; 8], bytes: &[u8]) -> Result<(), ProgramError> {
        if bytes.len() > WORLD_BODY_MAX {
            return Err(ChainError::WorldTooSmall.into());
        }
        self.write_at(WORLD_HEADER, bytes)?;
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
    /// The root is also stored right after the body (not counted in its
    /// length), so a world reassembled from chunks of different writes is
    /// refused (`check_root`). What a longer earlier body (or genesis job)
    /// and its trailer left behind is zeroed: the chunks hold this world
    /// and nothing else, whatever wrote them before.
    pub fn write_world(&self, state: &WorldState) -> Result<[u8; 32], ProgramError> {
        let buf = borsh::to_vec(state).map_err(|_| ChainError::WrongWorld)?;
        self.write_world_body(&buf)
    }

    /// `write_world` of an encoded world.
    fn write_world_body(&self, buf: &[u8]) -> Result<[u8; 32], ProgramError> {
        if buf.len() + 32 > WORLD_BODY_MAX {
            return Err(ChainError::WorldTooSmall.into());
        }
        let old_end = {
            let d = self.accounts[0].try_borrow_data()?;
            let len = u32::from_le_bytes(d[8..12].try_into().unwrap()) as usize;
            WORLD_HEADER + len.min(WORLD_BODY_MAX - 32) + 32
        };
        self.write_body(&WORLD_MAGIC, buf)?;
        let root = solana_program::hash::hashv(&[buf]).to_bytes();
        let end = WORLD_HEADER + buf.len() + 32;
        self.write_at(end - 32, &root)?;
        if old_end > end {
            self.zero_at(end, old_end - end)?;
        }
        Ok(root)
    }

    /// Zero `n` bytes at `pos` of the concatenated chunks.
    fn zero_at(&self, mut pos: usize, n: usize) -> Result<(), ProgramError> {
        let end = pos + n;
        while pos < end {
            let (k, off) = (pos / CHUNK, pos % CHUNK);
            let mut d = self
                .accounts
                .get(k)
                .ok_or(ChainError::WorldTooSmall)?
                .try_borrow_mut_data()?;
            let take = (CHUNK - off).min(end - pos);
            d[off..off + take].fill(0);
            pos += take;
        }
        Ok(())
    }

    /// Root of the stored world (sha256 of its body) without decoding it.
    pub fn root(&self) -> Result<[u8; 32], ProgramError> {
        Ok(solana_program::hash::hashv(&[&self.body(&WORLD_MAGIC)?]).to_bytes())
    }

    /// The root the last `write_world` stored after the body.
    pub fn stored_root(&self) -> Result<[u8; 32], ProgramError> {
        let len = self.body_len(&WORLD_MAGIC)?;
        if len + 32 > WORLD_BODY_MAX {
            return Err(ChainError::WrongWorld.into());
        }
        let mut out = [0u8; 32];
        self.read_at(WORLD_HEADER + len, &mut out)?;
        Ok(out)
    }

    /// `root` (the hash of the body just read) is the one the last
    /// `write_world` stored: every chunk comes from that same write.
    pub fn check_root(&self, root: &[u8; 32]) -> Result<(), ProgramError> {
        if &self.stored_root()? != root {
            return Err(ChainError::WrongWorld.into());
        }
        Ok(())
    }

    /// The world's `ruleset_hash` (its first 32 body bytes), read in place.
    pub fn ruleset_hash(&self) -> Result<[u8; 32], ProgramError> {
        if self.body_len(&WORLD_MAGIC)? < 32 {
            return Err(ChainError::WrongWorld.into());
        }
        let mut out = [0u8; 32];
        self.read_at(WORLD_HEADER, &mut out)?;
        Ok(out)
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

/// Decode a `Vec` into exactly its length. borsh's own decoder starts small
/// and doubles, and on the chain's bump heap (which never frees) every
/// outgrown buffer stays allocated. The encoding is unchanged.
pub fn vec_exact<R: borsh::io::Read, T: BorshDeserialize>(r: &mut R) -> borsh::io::Result<Vec<T>> {
    let len = u32::deserialize_reader(r)? as usize;
    // No account holds more; refuse lengths that could only be garbage.
    if len > 1 << 16 {
        return Err(borsh::io::ErrorKind::InvalidData.into());
    }
    let mut v = Vec::with_capacity(len);
    for _ in 0..len {
        v.push(T::deserialize_reader(r)?);
    }
    Ok(v)
}

pub fn load<T: BorshDeserialize>(data: &[u8]) -> Result<T, ProgramError> {
    T::deserialize(&mut &data[..]).map_err(|_| ChainError::NotInitialized.into())
}

/// Serialize `value` into the front of `data` in place, and zero what an
/// earlier, longer value left behind (a cleared inbox), so an account is
/// never denser than its content when it is committed.
pub fn store<T: BorshSerialize>(data: &mut [u8], value: &T) -> Result<(), ProgramError> {
    let total = data.len();
    let written = {
        let mut w: &mut [u8] = data;
        value
            .serialize(&mut w)
            .map_err(|_| ProgramError::AccountDataTooSmall)?;
        total - w.len()
    };
    data[written..].fill(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use permutation_rules::orders::Order;
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
            roster_commit: [0; 32],
            bounty_each: 0,
            bond: 0,
            roster_acc: [0; 32],
            roster_revealed: 0,
            roster_outcome: 0,
            bounty_paid: vec![u64::MAX; nations],
            delegated: u32::MAX,
            roster_blind: [7; 32],
            refund_base: vec![u64::MAX; nations],
            refund_in_payout: vec![u8::MAX; members.div_ceil(8)],
            seed_state: u8::MAX,
            seed_oracle: [8; 32],
            seed_requested_at: i64::MAX,
            seed_requests: u8::MAX,
            deposit: u64::MAX,
            outstanding: u64::MAX,
            voided: true,
            start_by: i64::MAX,
            stage_at: i64::MAX,
            rolled_back: u32::MAX,
            aborted_from: 6,
            validator: [9; 32],
            rules_version: u16::MAX,
            rules_hash: [10; 32],
            logic_version: u16::MAX,
            created_slot: u64::MAX,
        }
    }

    /// A season at the money caps with `n` members of which `a` are the
    /// operator's AIs.
    fn capped(n: u32, a: u16) -> Season {
        let mut s = season(6, 0);
        s.member_count = n;
        s.ai_count = a;
        s.entry_fee = MAX_ENTRY_FEE;
        s.pool = n as u64 * (MAX_ENTRY_FEE - MAX_ENTRY_FEE / 5);
        s.bounty_each = MAX_BOUNTY;
        s.treasury = vec![n as u64 * MAX_DEPOSIT, 0, 0, 0, 0, 0];
        s
    }

    fn chunk_infos<'a>(
        keys: &'a [Pubkey],
        lamports: &'a mut [u64],
        data: &'a mut [Vec<u8>],
        owner: &'a Pubkey,
    ) -> Vec<AccountInfo<'a>> {
        keys.iter()
            .zip(lamports.iter_mut())
            .zip(data.iter_mut())
            .map(|((k, l), d)| AccountInfo::new(k, false, true, l, d, owner, false))
            .collect()
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
                    shares: u64::MAX,
                };
                MAX_AI as usize
            ],
        };
        assert_eq!(borsh::to_vec(&r).unwrap().len(), roster_space(MAX_AI));
        assert_eq!(roster_space(1), 21 + 46);
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

    /// The v8 tail is appended: a v7 account (the fields up to
    /// `bounty_paid`, zero tail) decodes with every new field at zero.
    #[test]
    fn a_legacy_season_decodes_with_a_zero_tail() {
        let mut s = season(3, 5);
        s.magic = LEGACY_SEASON_MAGIC;
        let full = borsh::to_vec(&s).unwrap();
        let tail = borsh::to_vec(&(
            s.delegated,
            s.roster_blind,
            &s.refund_base,
            &s.refund_in_payout,
            (
                s.seed_state,
                s.seed_oracle,
                s.seed_requested_at,
                s.seed_requests,
            ),
            (s.deposit, s.outstanding, s.voided),
            (
                s.start_by,
                s.stage_at,
                s.rolled_back,
                s.aborted_from,
                s.validator,
            ),
            (
                s.rules_version,
                s.rules_hash,
                s.logic_version,
                s.created_slot,
            ),
        ))
        .unwrap();
        let mut account = full[..full.len() - tail.len()].to_vec();
        account.resize(SEASON_SPACE, 0);
        let v7: Season = load(&account).unwrap();
        assert_eq!(v7.bounty_paid, s.bounty_paid);
        assert_eq!(v7.magic, LEGACY_SEASON_MAGIC);
        assert_eq!(
            (
                v7.delegated,
                v7.refund_base.len(),
                v7.deposit,
                v7.outstanding
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(
            (v7.stage_at, v7.validator, v7.rules_hash),
            (0, [0; 32], [0; 32])
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
        assert!(borsh::to_vec(&meta).unwrap().len() <= WORLD_META_SPACE);
        // The season id is the first field: `world_season_id` reads it at 12..20.
        let mut chunk0 = vec![0u8; CHUNK];
        meta.write_chunk0(&mut chunk0).unwrap();
        assert_eq!(
            u64::from_le_bytes(chunk0[12..20].try_into().unwrap()),
            u64::MAX
        );
        assert_eq!(WorldMeta::from_chunk0(&chunk0).unwrap(), meta);
        assert!(WorldMeta::from_chunk0(&chunk0[..20]).is_err());
        assert_eq!(WORLD_HEADER, 256);
        assert_eq!(WORLD_BODY_MAX, 81_664);
    }

    /// Chunk 0's header bytes are a wire format (gateway, verifier, crank):
    /// every field at its pinned offset. Moving a field, or changing the
    /// layout without bumping `WORLD_MAGIC`, fails here.
    #[test]
    fn world_meta_header_is_pinned() {
        assert_eq!(
            &WORLD_MAGIC, b"PSWORLD6",
            "bump WORLD_MAGIC with the layout"
        );
        let meta = WorldMeta {
            season_id: 0x0102_0304_0506_0708,
            preset: 0x11,
            civs: 0x12,
            tick_seconds: 0x1314_1516,
            deadline: 0x2122_2324_2526_2728,
            finished: true,
            market: true,
            frozen: true,
            vrf: [0x31; 32],
            input_chunks: 0x4142,
            input_logged: 0x4344,
            revealing: true,
            undelegated: 0x51,
            input_hash: [0x52; 32],
            rand_state: 0x53,
            rand_tick: 0x5455,
            rand_pre: [0x56; 32],
            rand_out: [0x57; 32],
            frozen_at: 0x6162_6364_6566_6768,
            rand_requested_at: 0x7172_7374_7576_7778,
            rand_requests: 0x79,
            usdc_broken: true,
        };
        let mut c = vec![0u8; CHUNK];
        meta.write_chunk0(&mut c).unwrap();
        let le64 = |at: usize| u64::from_le_bytes(c[at..at + 8].try_into().unwrap());
        let le16 = |at: usize| u16::from_le_bytes(c[at..at + 2].try_into().unwrap());
        assert_eq!(le64(12), 0x0102_0304_0506_0708);
        assert_eq!((c[20], c[21]), (0x11, 0x12));
        assert_eq!(
            u32::from_le_bytes(c[22..26].try_into().unwrap()),
            0x1314_1516
        );
        assert_eq!(le64(26), 0x2122_2324_2526_2728);
        assert_eq!((c[34], c[35], c[36]), (1, 1, 1));
        assert_eq!(c[37..69], [0x31; 32]);
        assert_eq!((le16(69), le16(71)), (0x4142, 0x4344));
        assert_eq!((c[73], c[74]), (1, 0x51));
        assert_eq!(c[75..107], [0x52; 32]);
        assert_eq!((c[107], le16(108)), (0x53, 0x5455));
        assert_eq!(c[110..142], [0x56; 32]);
        assert_eq!(c[142..174], [0x57; 32]);
        assert_eq!(
            (le64(174), le64(182)),
            (0x6162_6364_6566_6768, 0x7172_7374_7576_7778)
        );
        assert_eq!((c[190], c[191]), (0x79, 1));
        assert!(c[192..].iter().all(|b| *b == 0), "180 B used of 244");
    }

    fn with_chunks(f: impl FnOnce(&Chunks)) {
        let owner = Pubkey::new_unique();
        let keys: Vec<Pubkey> = (0..WORLD_CHUNKS).map(|_| Pubkey::new_unique()).collect();
        let mut lamports = [0u64; WORLD_CHUNKS];
        let mut data = vec![vec![0u8; CHUNK]; WORLD_CHUNKS];
        let accounts = chunk_infos(&keys, &mut lamports, &mut data, &owner);
        f(&Chunks::new(&accounts).unwrap());
    }

    #[test]
    fn world_body_round_trips_across_chunks() {
        with_chunks(|chunks| {
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
            let too_big = vec![0u8; WORLD_BODY_MAX + 1];
            assert!(chunks.write_body(&WORLD_MAGIC, &too_big).is_err());
        });
    }

    /// A raw body and its root trailer, as `write_world` stores them.
    fn write_raw_world(chunks: &Chunks, body: &[u8]) -> [u8; 32] {
        let root = chunks.write_world_body(body).unwrap();
        assert_eq!(root, solana_program::hash::hashv(&[body]).to_bytes());
        root
    }

    #[test]
    fn the_root_trailer_round_trips_across_a_chunk_boundary() {
        with_chunks(|chunks| {
            // The trailer straddles chunks 0 and 1, then 3 and 4.
            for len in [CHUNK - WORLD_HEADER - 10, 4 * CHUNK - WORLD_HEADER - 17] {
                let body: Vec<u8> = (0..len).map(|i| (i % 253) as u8).collect();
                let root = write_raw_world(chunks, &body);
                assert_eq!(chunks.stored_root().unwrap(), root, "len {len}");
                assert_eq!(chunks.root().unwrap(), root);
                chunks.check_root(&root).unwrap();
                assert_eq!(chunks.ruleset_hash().unwrap()[..], body[..32]);
            }
        });
        // write_world writes the trailer after the encoded state.
        with_chunks(|chunks| {
            let rules = crate::rules::rules_for(PRESET_BLITZ, true).unwrap();
            let state = permutation_rules::genesis::new_season(
                &rules,
                &[1; 32],
                &[2; 32],
                &permutation_rules::genesis::nation_entries(2),
            )
            .unwrap();
            let root = chunks.write_world(&state).unwrap();
            assert_eq!(root, state.state_root().unwrap());
            chunks.check_root(&root).unwrap();
            assert_eq!(chunks.read_world().unwrap(), state);
            assert_eq!(chunks.ruleset_hash().unwrap(), state.ruleset_hash);
        });
    }

    /// A shorter world leaves nothing of the longer one before it (body or
    /// trailer): the chunks are byte-equal to a fresh write of that world.
    #[test]
    fn a_shorter_world_leaves_no_stale_bytes() {
        let rules = crate::rules::rules_for(PRESET_BLITZ, true).unwrap();
        let entries = permutation_rules::genesis::nation_entries(2);
        let short =
            permutation_rules::genesis::new_season(&rules, &[1; 32], &[2; 32], &entries).unwrap();
        let mut long = short.clone();
        for i in 0..40u8 {
            permutation_rules::gov::join(&mut long, &rules, (i % 2) as u16, [i + 1; 32]).unwrap();
        }
        let owner = Pubkey::new_unique();
        let keys: Vec<Pubkey> = (0..WORLD_CHUNKS).map(|_| Pubkey::new_unique()).collect();
        let (mut la, mut lb) = ([0u64; WORLD_CHUNKS], [0u64; WORLD_CHUNKS]);
        let mut da = vec![vec![0u8; CHUNK]; WORLD_CHUNKS];
        let mut db = vec![vec![0u8; CHUNK]; WORLD_CHUNKS];
        {
            let a = chunk_infos(&keys, &mut la, &mut da, &owner);
            let a = Chunks::new(&a).unwrap();
            a.write_body(&GENESIS_MAGIC, &vec![9u8; 3 * CHUNK]).unwrap();
            a.write_world(&long).unwrap();
            a.write_world(&short).unwrap();
            let b = chunk_infos(&keys, &mut lb, &mut db, &owner);
            Chunks::new(&b).unwrap().write_world(&short).unwrap();
        }
        assert_eq!(da, db);
    }

    /// Every chunk must come from the same write: a chunk of another world
    /// fails `check_root` (the hash of what is stored now differs from the
    /// trailer the last write left).
    #[test]
    fn mixed_chunks_are_refused() {
        let owner = Pubkey::new_unique();
        let keys: Vec<Pubkey> = (0..WORLD_CHUNKS).map(|_| Pubkey::new_unique()).collect();
        let len = 3 * CHUNK;
        let body_a: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
        let body_b: Vec<u8> = (0..len).map(|i| (i % 241) as u8 ^ 0x5a).collect();
        let (mut la, mut lb) = ([0u64; WORLD_CHUNKS], [0u64; WORLD_CHUNKS]);
        let mut da = vec![vec![0u8; CHUNK]; WORLD_CHUNKS];
        let mut db = vec![vec![0u8; CHUNK]; WORLD_CHUNKS];
        {
            let a = chunk_infos(&keys, &mut la, &mut da, &owner);
            write_raw_world(&Chunks::new(&a).unwrap(), &body_a);
            let b = chunk_infos(&keys, &mut lb, &mut db, &owner);
            write_raw_world(&Chunks::new(&b).unwrap(), &body_b);
        }
        // Chunk 2 of world B inside world A.
        da[2] = db[2].clone();
        let a = chunk_infos(&keys, &mut la, &mut da, &owner);
        let chunks = Chunks::new(&a).unwrap();
        let now = chunks.root().unwrap();
        assert_eq!(
            chunks.check_root(&now).unwrap_err(),
            ChainError::WrongWorld.into()
        );
    }

    /// The largest world body leaves room for its trailer.
    #[test]
    fn capacity_leaves_room_for_the_trailer() {
        with_chunks(|chunks| {
            let fits = vec![3u8; WORLD_BODY_MAX - 32];
            let root = write_raw_world(chunks, &fits);
            chunks.check_root(&root).unwrap();
            // One more byte: the trailer would not fit.
            assert_eq!(
                chunks
                    .write_world_body(&vec![3u8; WORLD_BODY_MAX - 31])
                    .unwrap_err(),
                ChainError::WorldTooSmall.into()
            );
            chunks.check_root(&root).unwrap();
            // A body written without its trailer room has no stored root.
            chunks
                .write_body(&WORLD_MAGIC, &vec![3u8; WORLD_BODY_MAX - 31])
                .unwrap();
            assert_eq!(
                chunks.stored_root().unwrap_err(),
                ChainError::WrongWorld.into()
            );
        });
        assert_eq!(WORLD_BODY_MAX - 32, 81_632);
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

    fn populated_nation() -> NationAccount {
        let mut n = NationAccount::new(9, 2, 250, 0, true, [4; 32]);
        n.open_tick = 17;
        n.officers = [1, NOBODY, 3, 4];
        n.keys = [[5; 32], [0; 32], [6; 32], [7; 32]];
        n.spendable = [8, 0, 9, 10];
        n.submitted = [17, NO_TICK, 17, NO_TICK];
        n.revealing = true;
        n.deadline = 1_800_000_000;
        n.committed = [17, NO_TICK, 17, 17];
        n.commits = [[11; 32], [0; 32], [12; 32], [13; 32]];
        n.salts = [[14; 32], [0; 32], [15; 32], [0; 32]];
        n.gov_quota = 6;
        n.batches[0] = Some(OrderBatch {
            civ: 2,
            tick: 17,
            role: permutation_rules::gov::Role::General,
            member: 1,
            decision_digest: [16; 32],
            orders: vec![Order::DeclareWar { civ: 1 }],
            adopt: vec![],
        });
        n.inbox = vec![GovEntry {
            member: 3,
            signer: [6; 32],
            action: GovAction::Stand { roles: 3 },
        }];
        n.roll = vec![
            RollSeat {
                member: 1,
                key8: [5; 8],
            },
            RollSeat {
                member: 3,
                key8: [6; 8],
            },
        ];
        n
    }

    #[test]
    fn nation_head_is_the_front_of_a_nation_account() {
        let n = populated_nation();
        let bytes = borsh::to_vec(&n).unwrap();
        let head: NationHead = load(&bytes).unwrap();
        let head_bytes = borsh::to_vec(&head).unwrap();
        assert_eq!(head_bytes.len(), NATION_HEAD_LEN);
        assert_eq!(bytes[..NATION_HEAD_LEN], head_bytes[..]);
        // `gov_quota` sits at its fixed offset.
        assert_eq!(u16::from_le_bytes([bytes[497], bytes[498]]), 6);
        // Cleared: the tick forgotten, frozen, no batches or inbox, and the
        // fields after the inbox at their `new` defaults.
        let cleared = head.cleared();
        let mut expect = n.clone();
        expect.clear_tick();
        expect.frozen = true;
        expect.roll = Vec::new();
        assert_eq!(cleared, expect);
        let c = borsh::to_vec(&cleared).unwrap();
        let fresh = borsh::to_vec(&NationAccount::new(0, 0, 0, 0, false, [0; 32])).unwrap();
        let after_inbox = |b: &[u8]| {
            let empty_batches_and_inbox = 4 + 4;
            b[NATION_HEAD_LEN + empty_batches_and_inbox..].to_vec()
        };
        assert_eq!(after_inbox(&c), after_inbox(&fresh));
    }

    #[test]
    fn store_zeroes_the_tail() {
        let mut n = populated_nation();
        n.inbox = (0..3)
            .map(|i| GovEntry {
                member: i,
                signer: [i as u8; 32],
                action: GovAction::Support { proposal: i },
            })
            .collect();
        let mut data = vec![0xAAu8; NATION_SPACE];
        store(&mut data, &n).unwrap();
        let long = borsh::object_length(&n).unwrap();
        assert!(data[long..].iter().all(|b| *b == 0));
        n.clear_tick();
        store(&mut data, &n).unwrap();
        let short = borsh::object_length(&n).unwrap();
        assert!(short < long);
        assert!(data[short..].iter().all(|b| *b == 0), "nothing left behind");
        assert_eq!(load::<NationAccount>(&data).unwrap(), n);
        // Too small: refused.
        assert_eq!(
            store(&mut data[..10], &n).unwrap_err(),
            ProgramError::AccountDataTooSmall
        );
    }

    /// At every season size `Register` admits and every nation size, each
    /// member keeps at least `GOV_QUOTA_MIN` slots, can use its whole quota
    /// next to every office's `REVEAL_ROOM`, and the world stays within
    /// `GOV_SLOTS_PER_TICK`.
    #[test]
    fn quota_floor_holds_at_the_caps() {
        for m in 1..=SEASON_MEMBER_CAP {
            for k in 1..=m.min(NATION_MEMBER_CAP) {
                let q = gov_quota(m, k);
                assert!(q >= GOV_QUOTA_MIN, "M={m} k={k} q={q}");
                let worst = NationAccount::base_len(k as usize)
                    + (k as usize) * (q as usize) * GOV_SLOT_BYTES;
                assert!(worst + 4 * REVEAL_ROOM <= NATION_SPACE, "M={m} k={k} q={q}");
            }
            assert!(m * gov_quota(m, 1) as u32 <= GOV_SLOTS_PER_TICK, "M={m}");
        }
        assert_eq!(NationAccount::base_len(0) + 12, NationAccount::base_len(1));
    }

    #[test]
    fn gov_slots_bound_the_stored_bytes() {
        let stand = GovAction::Stand { roles: 1 };
        assert_eq!(gov_slots(&stand), Some(1));
        let propose = |n: usize| GovAction::Propose {
            role: permutation_rules::gov::Role::Diplomat,
            orders: vec![Order::DeclareWar { civ: 1 }; n],
        };
        assert_eq!(
            gov_slots(&propose(3)),
            Some(4),
            "one per order and the entry"
        );
        for n in 0..40 {
            let a = propose(n);
            match gov_slots(&a) {
                Some(w) => {
                    let e = GovEntry {
                        member: 1,
                        signer: [1; 32],
                        action: a,
                    };
                    assert!(borsh::object_length(&e).unwrap() <= GOV_SLOT_BYTES * w as usize);
                }
                None => assert!(borsh::object_length(&a).unwrap() > MAX_GOV_ACTION_BYTES),
            }
        }
    }

    #[test]
    fn the_roll_is_the_nations_seated_members() {
        let rules = crate::rules::rules_for(PRESET_BLITZ, true).unwrap();
        let mut state = permutation_rules::genesis::new_season(
            &rules,
            &[1; 32],
            &[2; 32],
            &permutation_rules::genesis::nation_entries(2),
        )
        .unwrap();
        for (i, civ) in [0u16, 1, 0].into_iter().enumerate() {
            permutation_rules::gov::join(&mut state, &rules, civ, [i as u8 + 1; 32]).unwrap();
        }
        let mut n = NationAccount::new(1, 0, 0, 0, true, [0; 32]);
        n.seat_roll(&state);
        assert_eq!(
            n.roll,
            vec![
                RollSeat {
                    member: 0,
                    key8: [1; 8]
                },
                RollSeat {
                    member: 2,
                    key8: [3; 8]
                }
            ]
        );
        assert_eq!(n.gov_quota, gov_quota(3, 2));
    }

    #[test]
    fn only_measured_presets_are_creatable() {
        for n in 0..=10u8 {
            assert_eq!(creatable(PRESET_BLITZ, n), (2..=6).contains(&n), "n={n}");
            assert!(!creatable(PRESET_SEASON, n));
            assert!(!creatable(2, n));
        }
    }

    /// Hand-computed floors: `ceil((a·p0 + n·π) / (n − a))`.
    #[test]
    fn bond_floor_table() {
        let mut s = season(2, 0);
        s.treasury = vec![0, 0];
        s.entry_fee = 10;
        s.pool = 80;
        s.bounty_each = 5;
        for (n, ai, expect) in [
            // No AIs, nobody, or AI-only (A7): no floor.
            (10u32, 0u16, 0u64),
            (0, 3, 0),
            (3, 3, 0),
            // n = 10, a = 3: p0 = 80 + 15 = 95, π = 30: (285 + 300) / 7 = 83.57.
            (10, 3, 84),
            // n = 2, a = 1: p0 = 80 + 5, π = 10: (85 + 20) / 1.
            (2, 1, 105),
            // More AIs committed than members: at most n − 1 are the
            // operator's. n = 4, a = 3: p0 = 80 + 25, π = 50: (315 + 200) / 1.
            (4, 5, 515),
        ] {
            s.member_count = n;
            s.ai_count = ai;
            assert_eq!(bond_floor(&s), expect, "n={n} ai={ai}");
        }
        s.member_count = 10;
        s.ai_count = 3;
        s.treasury = vec![14, 0];
        // Deposits count: p0 = 109, (327 + 300) / 7 = 89.57.
        assert_eq!(bond_floor(&s), 90);
        assert_eq!(forfeit_penalty(&s), 30);
    }

    /// More money in the season, a higher fee or more AIs never lower the floor.
    #[test]
    fn bond_floor_is_monotone() {
        for n in 2..=SEASON_MEMBER_CAP {
            for a in 1..(n as u16).min(SEASON_MEMBER_CAP as u16) {
                let s = capped(n, a);
                let f = bond_floor(&s);
                if a + 1 < n as u16 {
                    assert!(bond_floor(&capped(n, a + 1)) >= f, "more AIs n={n} a={a}");
                }
                let mut more = s.clone();
                more.pool += 1_000;
                assert!(bond_floor(&more) >= f);
                let mut more = s.clone();
                more.treasury[1] += 1_000;
                assert!(bond_floor(&more) >= f);
                let mut more = s.clone();
                more.bounty_each += 1_000;
                assert!(bond_floor(&more) >= f);
                let mut more = s.clone();
                more.entry_fee += 1_000;
                assert!(bond_floor(&more) >= f);
            }
        }
    }

    /// `MAX_BOND` never refuses a legitimate bond: the gateway's default
    /// (two entry fees per AI) and the floor of every season the caps admit
    /// (`ai_count < SEASON_MEMBER_CAP`, members 2..=cap, AI-only included).
    #[test]
    fn max_bond_covers_the_default_and_the_floor() {
        assert!(MAX_BOND >= 2 * MAX_AI as u64 * MAX_ENTRY_FEE);
        let mut worst = 0;
        for n in 2..=SEASON_MEMBER_CAP {
            for a in 1..SEASON_MEMBER_CAP as u16 {
                let f = bond_floor(&capped(n, a));
                if n == a as u32 {
                    assert_eq!(f, 0, "AI-only season n={n}");
                }
                assert!(f <= MAX_BOND, "n={n} a={a}: {f}");
                worst = worst.max(f);
            }
        }
        assert!(worst > 0);
    }

    #[test]
    fn undelegation_order_ends_with_chunk_zero() {
        for civs in [2u8, 6] {
            let order = undelegation_order(civs);
            assert_eq!(order.len(), WORLD_CHUNKS + civs as usize);
            assert_eq!(order[0], 1);
            assert_eq!(order[18], 19);
            assert_eq!(order[19], NATION_TARGET);
            assert_eq!(*order.last().unwrap(), 0);
            for (i, t) in order.iter().enumerate() {
                assert_eq!(undelegation_position(civs, *t), Some(i), "{t}");
            }
            assert_eq!(
                undelegation_position(civs, NATION_TARGET + civs as u16),
                None
            );
            assert_eq!(undelegation_position(civs, WORLD_CHUNKS as u16), None);
        }
    }

    #[test]
    fn target_bits_cover_every_account_once() {
        let mut all = 0u32;
        for t in (0..WORLD_CHUNKS as u16).chain((0..6).map(|c| NATION_TARGET + c)) {
            let b = target_bit(t).unwrap();
            assert_eq!(all & b, 0, "{t}");
            all |= b;
        }
        assert_eq!(all, all_targets(6));
        assert_eq!(all_targets(6) & CHUNK_BITS, CHUNK_BITS);
        assert_eq!(target_bit(0), Some(1));
        assert_eq!(target_bit(NATION_TARGET + 2), Some(1 << 22));
        assert_eq!(target_bit(WORLD_CHUNKS as u16), None);
        assert_eq!(target_bit(NATION_TARGET + MAX_NATIONS as u16), None);
    }

    #[test]
    fn degraded_steps_wait_at_least_ten_minutes() {
        assert_eq!(degrade_after(30), 600);
        assert_eq!(degrade_after(300), 600);
        assert_eq!(degrade_after(14_400), 28_800);
    }

    #[test]
    fn addresses_are_the_program_pdas() {
        let program = Pubkey::new_unique();
        let id = 7u64.to_le_bytes();
        let pda = |seeds: &[&[u8]]| Pubkey::find_program_address(seeds, &program).0.to_string();
        let p = program.to_string();
        assert_eq!(season_address(&p, 7).unwrap(), pda(&[SEASON_SEED, &id]));
        assert_eq!(vault_address(&p, 7).unwrap(), pda(&[VAULT_SEED, &id]));
        assert_eq!(
            world_chunk_address(&p, 7, 3).unwrap(),
            pda(&[WORLD_SEED, &id, &[3]])
        );
        assert!(vault_address("not a key", 7).is_none());
    }
}
