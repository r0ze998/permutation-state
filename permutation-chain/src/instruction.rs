//! Instructions. Accounts are listed as (s) signer, (w) writable.
//! The borsh enum tag is the variant index; never reorder or remove variants.
//! Base layer: 0–5, 10, 11, 13–16, 18. Ephemeral Rollup: 6–9, 12, 17, 19–23.

use borsh::{BorshDeserialize, BorshSerialize};
use permutation_rules::gov::{GovAction, Role};
use permutation_rules::orders::Order;

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub enum ChainInstruction {
    /// Create a season and its USDC vault. With `prev_season_id` (≠ 0) it
    /// follows that finalized season of the same admin in the history
    /// layer: its history root is taken over (account 6).
    /// 0 admin (s,w) · 1 season PDA (w) · 2 vault PDA (w) · 3 USDC mint · 4 token program · 5 system
    /// · 6 previous season PDA (only with `prev_season_id`)
    CreateSeason {
        season_id: u64,
        /// 0 Blitz, 1 Season.
        preset: u8,
        nations: u8,
        entry_fee: u64,
        tick_seconds: u32,
        world_seed: [u8; 32],
        crank: [u8; 32],
        /// Open the USDC market this season (V5 §7.5).
        market: bool,
        /// The season this one follows in the history layer (0: none).
        prev_season_id: u64,
    },
    /// Create one `CHUNK`-byte world chunk `["world", id, chunk]`. Anyone may pay.
    /// 0 payer (s,w) · 1 season · 2 world chunk PDA (w) · 3 system
    AllocWorld { chunk: u8 },
    /// Become a member of a nation (V5 §4): pay the entry fee (80% prize pool,
    /// 20% operations, V5 D10) and optionally deposit into the nation
    /// treasury. The x402 gateway submits this with itself as fee payer and
    /// the member's signature (V5 D14).
    /// 0 wallet (s) · 1 fee payer (s,w) · 2 season (w) · 3 member PDA `["member", id, wallet]` (w)
    /// · 4 wallet USDC account (w) · 5 vault (w) · 6 USDC mint · 7 token program · 8 system
    Register {
        civ: u16,
        name: String,
        kind: u8,
        session: [u8; 32],
        attestation: [u8; 32],
        /// Offices to stand for (`Role::bit` mask) and votes of the first election.
        stand: u8,
        votes: [u32; 4],
        deposit: u64,
    },
    /// Close registration and begin genesis (admin or crank).
    /// 0 authority (s) · 1 season (w) · 2 SlotHashes sysvar · 3.. world chunks (w)
    StartSeason,
    /// Advance genesis one step; the last step writes tick 0. Permissionless.
    /// 0 season (w) · 1.. world chunks (w)
    GenesisStep { work: u32 },
    /// Delegate one world chunk or one nation account to the ER (crank/admin).
    /// 0 authority (s,w) · 1 system · 2 season · 3 PDA to delegate (w) · 4 owner program
    /// · 5 delegation buffer (w) · 6 delegation record (w) · 7 delegation metadata (w)
    /// · 8 delegation program · 9 validator (optional)
    Delegate {
        /// 0..WORLD_CHUNKS = that world chunk; 1000 + civ = that nation's account.
        target: u16,
    },
    /// Retired (always fails with `Retired`): orders are sealed now
    /// (`CommitOrders`, `RevealOrders`). Kept so the tags never move.
    SubmitOrders {
        role: Role,
        tick: u16,
        decision_digest: [u8; 32],
        orders: Vec<Order>,
        adopt: Vec<u32>,
    },
    /// ER: resolve the open tick once its input was published in full
    /// (`LogTickInput`). Runs phases up to `to` (12 = the whole tick) and logs
    /// `PS_TICK` with the input's hash. Permissionless.
    /// 0.. world chunks (w) · then nation PDAs of every civ, in civ order (w)
    ResolveTick { to: u8 },
    /// ER: commit every world and nation account to the base layer in one
    /// intent (the crank only, as `CommitPart`). Local stacks only: on devnet one intent this large exceeds the
    /// committor's limits (64 account keys, finalize compute); use `CommitPart`.
    /// 0 payer (s,w) · 1 magic program · 2 magic context (w) · 3.. world chunks (w) · then nations (w)
    Commit,
    /// ER: once the last tick resolved, commit and undelegate everything.
    /// Same accounts and limits as `Commit`; use `UndelegatePart`.
    CommitAndUndelegate,
    /// Base: compute the payouts (V5 §7) from the final world. Permissionless.
    /// 0 season (w) · 1.. world chunks
    FinishSeason,
    /// Base: pay a member's prize and treasury refund to its wallet's token account.
    /// 0 wallet (s) · 1 season (w) · 2 member PDA (w) · 3 vault (w) · 4 destination token account (w)
    /// · 5 USDC mint · 6 token program
    Claim,
    /// ER: once the last tick resolved, commit and undelegate some of the
    /// season's accounts. The base-layer finalize of one intent runs every
    /// target in one transaction, so intents must stay small (account keys,
    /// instruction trace, compute): the crank sends each world chunk alone and
    /// the nation accounts in threes; world chunk 0 (whose header says the
    /// season is over) goes last.
    /// 0 payer (s,w) · 1 magic program · 2 magic context (w) · 3 world chunk 0
    /// · 4.. the target accounts, in `targets` order, except chunk 0 which is account 3 (w)
    UndelegatePart {
        /// 0..WORLD_CHUNKS = that world chunk; 1000 + civ = that nation's account.
        targets: Vec<u16>,
    },
    /// Base, during registration: change the pre-season candidacy and votes.
    /// 0 signer: wallet or session (s) · 1 season · 2 member PDA (w)
    UpdateMember { stand: u8, votes: [u32; 4] },
    /// Create one nation account `["nation", id, civ]`. Anyone may pay.
    /// 0 payer (s,w) · 1 season · 2 nation PDA (w) · 3 system
    AllocNation { civ: u16 },
    /// Base, after genesis: add the next members to the world, in
    /// registration order, with their pre-season candidacy and votes
    /// (operator liveness step).
    /// 0 authority (s) · 1 season (w) · 2.. world chunks (w) · then member PDAs (read)
    SeatMembers,
    /// Base, once every member is seated: hold the first election, open tick
    /// 0 on the nation accounts and start the clock.
    /// 0 authority (s) · 1 season (w) · 2.. world chunks (w) · then nation PDAs in civ order (w)
    OpenGovernment,
    /// ER: queue a governance action (vote, proposal, support, recall,
    /// candidacy) for the open tick. The engine checks the signer against the
    /// member's registered key when the tick resolves.
    /// 0 signer (s) · 1 nation PDA of the member's nation (w)
    SubmitGov { member: u32, action: GovAction },
    /// Base, after the season: pay the operations share to the admin.
    /// 0 admin (s) · 1 season (w) · 2 vault (w) · 3 destination (w) · 4 mint · 5 token program
    WithdrawOps,
    /// ER: publish the open tick's input, `INPUT_CHUNK` bytes per call, as
    /// `PS_INPUT` records, so every resolved tick can be replayed from the
    /// chain alone. Chunk 0 is allowed in the reveal window once it closed
    /// (its deadline passed, or every commitment was revealed): it freezes
    /// the input, draws the tick randomness from the world root and the
    /// revealed salts (`permutation_rules::rng::tick_vrf`) and logs them as
    /// `PS_SALTS`. Chunks are logged in order; ResolveTick needs them all.
    /// Permissionless.
    /// 0.. world chunks (w) · then nation PDAs of every civ, in civ order (w)
    LogTickInput { chunk: u16 },
    /// ER: commit some of the season's accounts to the base layer (during
    /// play, for durability), in small intents like `UndelegatePart`. Only
    /// the season's crank may send it (MagicBlock sponsors a limited number
    /// of commits per account; the last is kept for the undelegation).
    /// 0 crank (s,w) · 1 magic program · 2 magic context (w) · 3 world chunk 0
    /// · 4 any nation PDA of the season (it records the crank's key)
    /// · 5.. the target accounts, in `targets` order, except chunk 0 which is account 3 (w)
    CommitPart {
        /// 0..WORLD_CHUNKS = that world chunk; 1000 + civ = that nation's account.
        targets: Vec<u16>,
    },
    /// ER: close the open tick's commitments once its deadline passed and
    /// open the reveal window (`state::reveal_seconds`). Governance closes
    /// too. Logs `PS_COMMITS ‖ tick ‖ borsh(Vec<(civ, role, member,
    /// commitment)>)`. Permissionless.
    /// 0.. world chunks (w) · then nation PDAs of every civ, in civ order (w)
    CloseCommits,
    /// ER: seal one office's orders for the open tick, before its deadline:
    /// `commitment = permutation_rules::orders::order_commitment(batch, salt)`.
    /// Replaces an earlier commitment of the same tick. The signer is the
    /// office holder's session key; a vacant office takes none (the rules'
    /// caretaker fills it).
    /// 0 signer (s) · 1 nation PDA (w)
    CommitOrders {
        role: Role,
        tick: u16,
        commitment: [u8; 32],
    },
    /// ER: reveal one office's sealed orders in the reveal window. They must
    /// hash to the office's commitment with `salt`, and pass the same checks
    /// a batch always had (office, structure, budget). Anyone holding the
    /// plaintext may send it.
    /// 0 signer (s) · 1 nation PDA (w)
    RevealOrders {
        role: Role,
        tick: u16,
        decision_digest: [u8; 32],
        orders: Vec<Order>,
        adopt: Vec<u32>,
        salt: [u8; 32],
    },
}

#[cfg(test)]
mod tests {
    use super::ChainInstruction as I;

    /// Clients (the gateway codec) hardcode these tags.
    #[test]
    fn tags_are_stable() {
        let tag = |ix: I| borsh::to_vec(&ix).unwrap()[0];
        assert_eq!(tag(I::AllocWorld { chunk: 0 }), 1);
        assert_eq!(tag(I::StartSeason), 3);
        assert_eq!(tag(I::GenesisStep { work: 0 }), 4);
        assert_eq!(tag(I::Delegate { target: 0 }), 5);
        assert_eq!(tag(I::ResolveTick { to: 12 }), 7);
        assert_eq!(tag(I::Commit), 8);
        assert_eq!(tag(I::CommitAndUndelegate), 9);
        assert_eq!(tag(I::FinishSeason), 10);
        assert_eq!(tag(I::Claim), 11);
        assert_eq!(tag(I::UndelegatePart { targets: vec![] }), 12);
        assert_eq!(
            tag(I::UpdateMember {
                stand: 0,
                votes: [0; 4]
            }),
            13
        );
        assert_eq!(tag(I::AllocNation { civ: 0 }), 14);
        assert_eq!(tag(I::SeatMembers), 15);
        assert_eq!(tag(I::OpenGovernment), 16);
        assert_eq!(tag(I::WithdrawOps), 18);
        assert_eq!(tag(I::LogTickInput { chunk: 0 }), 19);
        assert_eq!(tag(I::CommitPart { targets: vec![] }), 20);
        assert_eq!(tag(I::CloseCommits), 21);
        assert_eq!(
            tag(I::CommitOrders {
                role: permutation_rules::gov::Role::General,
                tick: 0,
                commitment: [0; 32]
            }),
            22
        );
        assert_eq!(
            tag(I::RevealOrders {
                role: permutation_rules::gov::Role::General,
                tick: 0,
                decision_digest: [0; 32],
                orders: vec![],
                adopt: vec![],
                salt: [0; 32]
            }),
            23
        );
    }
}
