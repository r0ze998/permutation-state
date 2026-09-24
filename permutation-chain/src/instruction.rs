//! Instructions. Accounts are listed as (s) signer, (w) writable.
//! Base layer: 0–5, 10–11. Ephemeral Rollup: 6–9, 12.

use borsh::{BorshDeserialize, BorshSerialize};
use permutation_rules::orders::Order;

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub enum ChainInstruction {
    /// Create a season and its USDC vault.
    /// 0 admin (s,w) · 1 season PDA (w) · 2 vault PDA (w) · 3 USDC mint · 4 token program · 5 system
    CreateSeason {
        season_id: u64,
        /// 0 Blitz, 1 Season.
        preset: u8,
        max_civs: u8,
        entry_fee: u64,
        exchange_credit: u64,
        tick_seconds: u32,
        world_seed: [u8; 32],
        crank: [u8; 32],
    },
    /// Create one 10 KiB world chunk `["world", id, chunk]`. Anyone may pay.
    /// 0 payer (s,w) · 1 season · 2 world chunk PDA (w) · 3 system
    AllocWorld { chunk: u8 },
    /// Pay the entry fee in USDC and register a civilization (§3.1). The x402
    /// gateway submits this with itself as fee payer and the player's signature.
    /// 0 player (s) · 1 fee payer (s,w) · 2 season (w) · 3 orders PDA for the new civ (w)
    /// · 4 player USDC account (w) · 5 vault (w) · 6 USDC mint · 7 token program · 8 system
    JoinSeason { name: String, kind: u8, session: [u8; 32], payout: [u8; 32] },
    /// Close entry and begin genesis (admin or crank).
    /// 0 authority (s) · 1 season (w) · 2 SlotHashes sysvar · 3.. world chunks (w)
    StartSeason,
    /// Advance genesis one step; the last step writes tick 0 and opens it for
    /// orders. Permissionless.
    /// 0 season (w) · 1.. world chunks (w) · then orders PDAs of every civ, in civ order (w)
    GenesisStep { work: u32 },
    /// Delegate one world chunk or one civ's orders account to the ER (crank/admin).
    /// 0 authority (s,w) · 1 system · 2 season · 3 PDA to delegate (w) · 4 owner program
    /// · 5 delegation buffer (w) · 6 delegation record (w) · 7 delegation metadata (w)
    /// · 8 delegation program · 9 validator (optional)
    Delegate {
        /// 0..WORLD_CHUNKS = that world chunk; 1000 + civ = that civ's orders account.
        target: u16,
    },
    /// ER: replace this civ's batch for the open tick. Signer is the civ's
    /// session key or player wallet. Reveals of earlier decisions ride along.
    /// 0 signer (s) · 1 orders PDA (w)
    SubmitOrders { tick: u16, decision_digest: [u8; 32], orders: Vec<Order> },
    /// ER: resolve the open tick once its deadline passed or every civ has
    /// submitted. Runs phases up to `to` (12 = the whole tick). Permissionless.
    /// 0.. world chunks (w) · then orders PDAs of every civ, in civ order (w)
    ResolveTick { to: u8 },
    /// ER: commit the world and orders accounts to the base layer.
    /// 0 payer (s,w) · 1 magic program · 2 magic context (w) · 3.. world chunks (w) · then orders (w)
    Commit,
    /// ER: once the last tick resolved, commit and undelegate everything.
    /// Same accounts as `Commit`.
    CommitAndUndelegate,
    /// Base: compute payouts (§14.5) from the final world. Permissionless.
    /// 0 season (w) · 1.. world chunks
    FinishSeason,
    /// Base: pay a civ's prize from the vault to its payout owner's token account.
    /// 0 payout owner (s) · 1 season (w) · 2 vault (w) · 3 destination token account (w)
    /// · 4 USDC mint · 5 token program
    Claim { civ: u16 },
    /// ER: once the last tick resolved, commit and undelegate some of the
    /// season's accounts. The base-layer finalize of one intent runs every
    /// undelegation in one transaction, and 14 of them exceed Solana's
    /// instruction-trace limit, so the crank sends small groups; world chunk 0
    /// (whose header says the season is over) goes in the last group.
    /// 0 payer (s,w) · 1 magic program · 2 magic context (w) · 3 world chunk 0
    /// · 4.. the target accounts, in `targets` order, except chunk 0 which is account 3 (w)
    UndelegatePart {
        /// 0..WORLD_CHUNKS = that world chunk; 1000 + civ = that civ's orders account.
        targets: Vec<u16>,
    },
}
