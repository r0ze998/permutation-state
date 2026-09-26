//! Instructions. Accounts are listed as (s) signer, (w) writable.
//! The borsh enum tag is the variant index; never reorder or remove variants.
//! Base layer: 0–5, 10, 11, 13–16, 18, 24, 27, 31–36. Ephemeral Rollup:
//! 6–9, 12, 17, 19–23, 25, 28–30. StartClock (26): the layer that plays
//! tick 0.
//! "The sysvar" is the Instructions sysvar
//! (`Sysvar1nstructions1111111111111111111111111`); an instruction that takes
//! it must be the only one of its transaction besides compute-budget ones
//! (`NotAlone`).

use borsh::{BorshDeserialize, BorshSerialize};
use permutation_rules::gov::{GovAction, Role};
use permutation_rules::orders::Order;

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub enum ChainInstruction {
    /// Create a season and its USDC vault. With `prev_season_id` (≠ 0) it
    /// follows that finalized (or aborted) season of the same admin in the
    /// history layer: its history root is taken over (account 6). The season
    /// is bound to this build's rules and settlement logic
    /// (`rules::PINNED_RULESET_HASHES`, `rules::CHAIN_LOGIC_VERSION`).
    /// With operator AI members (`ai_count` > 0, V5 §18) the admin escrows
    /// `ai_count × bounty_each + bond` into the vault and the roster account
    /// is created.
    /// 0 admin (s,w) · 1 season PDA (w) · 2 vault PDA (w) · 3 USDC mint · 4 token program · 5 system
    /// · 6 previous season PDA (only with `prev_season_id`)
    /// · then, with AIs: admin USDC account (w) · roster PDA `["roster", id]` (w)
    CreateSeason {
        season_id: u64,
        /// 0 Blitz. 1 (Season, radius-19 map) is refused: only
        /// `state::creatable` pairs can be created.
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
        /// Operator AI members, the blinded commitment to the chain of their
        /// tags (`permutation_rules::roster::roster_commit`), the bounty per
        /// AI and the bond (V5 §18.2–§18.4).
        ai_count: u16,
        roster_commit: [u8; 32],
        bounty_each: u64,
        bond: u64,
        /// The treasury deposit every member pays at Register (V5 §7.5,
        /// uniform per season; 0 = none; only with the market).
        deposit: u64,
        /// `StartSeason` is due by then (unix time, base clock); anyone may
        /// abort a season still registering a day later.
        start_by: i64,
        /// The ER validator every account of the season is delegated to.
        validator: [u8; 32],
    },
    /// Create one `CHUNK`-byte world chunk `["world", id, chunk]`. Anyone may
    /// pay; while registering only.
    /// 0 payer (s,w) · 1 season · 2 world chunk PDA (w) · 3 system
    AllocWorld { chunk: u8 },
    /// Become a member of a nation (V5 §4): pay the entry fee (80% prize pool,
    /// 20% operations, V5 D10) and optionally deposit into the nation
    /// treasury. The x402 gateway submits this with itself as fee payer and
    /// the member's signature (V5 D14).
    /// The session key co-signs (it must be `session` and differ from the
    /// wallet when it pays the fees); the wallet owns the USDC account.
    /// 0 wallet (s) · 1 fee payer (s,w) · 2 season (w) · 3 member PDA `["member", id, wallet]` (w)
    /// · 4 wallet USDC account (w) · 5 vault (w) · 6 USDC mint · 7 token program · 8 system
    /// · 9 session key (s)
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
        /// 32 random bytes, or an operator AI's roster tag (V5 §18.2).
        tag: [u8; 32],
    },
    /// Close registration (status Seeding). It makes no VRF request:
    /// `RetrySeasonSeed` requests the season seed (the base queue), in the
    /// same transaction, and `ConsumeSeasonSeed` then opens genesis. The
    /// operator's bond must reach `state::bond_floor`. Admin or crank.
    /// 0 authority (s,w) · 1 season (w) · 2 program identity PDA `["identity"]`
    /// · 3 VRF base queue (w) · 4 VRF program · 5 system · 6 SlotHashes sysvar
    StartSeason,
    /// Advance genesis one step; the first call writes the genesis job, the
    /// last step writes tick 0. Permissionless; `work` is clamped to
    /// 1..=`state::MAX_GENESIS_WORK`.
    /// 0 season (w) · 1.. world chunks (w)
    GenesisStep { work: u32 },
    /// Delegate one world chunk or one nation account to the season's ER
    /// validator (crank/admin): each target once, world chunk 0 last.
    /// 0 authority (s,w) · 1 system · 2 season (w) · 3 PDA to delegate (w) · 4 owner program
    /// · 5 delegation buffer (w) · 6 delegation record (w) · 7 delegation metadata (w)
    /// · 8 delegation program · 9 validator (`Season::validator`)
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
    /// (`LogTickInput`) and its randomness drawn. Runs phases up to `to` (12
    /// = the whole tick); with bit 7 (`state::DEGRADED`) set, one degraded
    /// phase, once `state::degrade_after` passed the deadline. Logs
    /// `PS_TICK` with the input's hash and the randomness. Permissionless.
    /// 0.. world chunks (w) · then nation PDAs of every civ, in civ order (w) · the sysvar
    ResolveTick { to: u8 },
    /// Retired (always fails with `Retired`): one intent this large exceeds
    /// the committor's limits on devnet (`CommitPart`). Kept so the tags
    /// never move.
    Commit,
    /// Retired (always fails with `Retired`); `UndelegatePart` winds a
    /// season up. Kept so the tags never move.
    CommitAndUndelegate,
    /// Base: compute the payouts (V5 §7) from the final world, once every
    /// world chunk is back on base and consistent (its root trailer), and
    /// the vault holds what they owe. Permissionless.
    /// 0 season (w) · 1..=20 world chunks · 21 vault · 22 roster (with a revealed roster)
    FinishSeason,
    /// Base: pay a member's prize and treasury refund (Finalized) or its
    /// refund (Aborted) to its wallet's token account.
    /// 0 wallet (s) · 1 season (w) · 2 member PDA (w) · 3 vault (w) · 4 destination token account (w)
    /// · 5 USDC mint · 6 token program
    Claim,
    /// ER: anyone, once the last tick resolved, strictly in
    /// `state::undelegation_order` (world chunks 1..19 each alone, then the
    /// nations 1–3 per intent in civ order, then chunk 0 alone). Chunk 0
    /// counts the steps (`WorldMeta::undelegated`). A target that already
    /// left the ER is skipped. Nation accounts go back cleared and frozen.
    /// Must be alone in its transaction besides compute-budget instructions.
    /// 0 payer (s,w) · 1 magic program · 2 magic context (w) · 3 world chunk 0 (w)
    /// · 4.. the targets except chunk 0, in `targets` order (w; read-only if
    /// already gone) · last: the sysvar
    UndelegatePart {
        /// 0..WORLD_CHUNKS = that world chunk; 1000 + civ = that nation's account.
        targets: Vec<u16>,
    },
    /// Base, during registration: change the pre-season candidacy and votes
    /// (no votes in seasons with operator AI members).
    /// 0 signer: wallet or session (s) · 1 season · 2 member PDA (w)
    UpdateMember { stand: u8, votes: [u32; 4] },
    /// Create one nation account `["nation", id, civ]`. Anyone may pay;
    /// while registering only.
    /// 0 payer (s,w) · 1 season · 2 nation PDA (w) · 3 system
    AllocNation { civ: u16 },
    /// Base, after genesis: add the next members to the world, in
    /// registration order, with their pre-season candidacy and votes
    /// (operator liveness step). Admin or crank; anyone from
    /// `stage_at + state::TAKEOVER_SECONDS`.
    /// 0 authority (s) · 1 season (w) · 2.. world chunks (w) · then member PDAs (read)
    SeatMembers,
    /// Base, once every member is seated: hold the first election, open tick
    /// 0 on the nation accounts (with each nation's roll and governance
    /// quota) and start the clock with `state::TICK0_GRACE_SECONDS` of
    /// grace. Admin or crank; anyone from `stage_at + state::TAKEOVER_SECONDS`.
    /// 0 authority (s) · 1 season (w) · 2.. world chunks (w) · then nation PDAs in civ order (w)
    OpenGovernment,
    /// ER: queue a governance action (vote, proposal, support, recall,
    /// candidacy) for the open tick, before its deadline. The signer must be
    /// the member's seated key (the nation's roll), within its quota
    /// (`state::gov_slots`, `NationAccount::gov_quota`).
    /// 0 signer (s) · 1 nation PDA of the member's nation (w)
    SubmitGov { member: u32, action: GovAction },
    /// Base, after the season (Finalized: the operations share; Aborted:
    /// `lifecycle::ops_after_abort`): pay the admin.
    /// 0 admin (s) · 1 season (w) · 2 vault (w) · 3 destination (w) · 4 mint · 5 token program
    WithdrawOps,
    /// ER: publish the open tick's input, `INPUT_CHUNK` bytes per call, as
    /// `PS_INPUT` records, so every resolved tick can be replayed from the
    /// chain alone. Requires the input frozen (`FreezeTick`) and its
    /// randomness drawn; chunk 0 also logs the revealed salts (`PS_SALTS`).
    /// Chunks are logged in order; ResolveTick needs them all.
    /// Permissionless.
    /// 0.. world chunks (w) · then nation PDAs of every civ, in civ order (w) · the sysvar
    LogTickInput { chunk: u16 },
    /// ER: commit one world chunk or 1–3 nations to the base layer (during
    /// play, for durability). Only the season's crank may send it
    /// (MagicBlock sponsors a limited number of commits per account; the
    /// last is kept for the undelegation), during play and after the last
    /// tick until the first `UndelegatePart` (`WrongPhase` after that).
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
    /// 0.. world chunks (w) · then nation PDAs of every civ, in civ order (w) · the sysvar
    CloseCommits,
    /// ER: seal one office's orders for the open tick, before its deadline
    /// (refused from the deadline on):
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
    /// a batch always had (office, structure, budget) and the batch caps
    /// (`state::MAX_REVEAL_BYTES`). Anyone holding the plaintext may send it.
    /// 0 signer (s) · 1 nation PDA (w)
    RevealOrders {
        role: Role,
        tick: u16,
        decision_digest: [u8; 32],
        orders: Vec<Order>,
        adopt: Vec<u32>,
        salt: [u8; 32],
    },
    /// Base, after the last tick with the final world back on base: reveal
    /// the next operator AI members of the roster, in the order committed
    /// (V5 §18.2). Each salt must turn that member's wallet into its
    /// registration tag; the batch that completes the roster must satisfy
    /// `roster_commit(blind, chain) == Season::roster_commit`, and stores
    /// `blind`. `from == 0` restarts; otherwise it must equal the number
    /// revealed so far. A complete roster is final. Admin or crank.
    /// 0 authority (s) · 1 season (w) · 2 roster PDA (w) · 3 world chunk 0
    /// · 4.. member PDAs, one per salt
    RevealRoster {
        from: u16,
        salts: Vec<[u8; 32]>,
        blind: [u8; 32],
    },
    /// ER: anchor the members' messages of a tick (V5 §18.7): logs
    /// `PS_TALK ‖ tick ‖ count ‖ root`, the Merkle root of the signed
    /// messages the gateway relayed, so anyone can later prove a message was
    /// sent by then. The season's crank only.
    /// 0 crank (s) · 1 any nation PDA of the season (it records the crank's key)
    AnchorTalk {
        tick: u16,
        count: u32,
        root: [u8; 32],
    },
    /// ER (or base, when a season is played there): start tick 0's clock
    /// once the accounts are on the layer that plays it: its deadline
    /// becomes `now + tick_seconds` if that is earlier than the one
    /// OpenGovernment set (`state::TICK0_GRACE_SECONDS`). The season's crank
    /// only; tick 0 only, before its commitments close.
    /// 0 crank (s) · 1..=20 world chunks (w) · then nation PDAs of every civ, in civ order (w)
    StartClock,
    /// Base, during registration: add to the operator's bond (V5 §18.2) so
    /// it reaches `state::bond_floor` before `StartSeason` (at most
    /// `state::MAX_BOND`). Seasons with operator AI members; admin or crank.
    /// 0 authority (s) · 1 season (w) · 2 authority's USDC account (w) · 3 vault (w)
    /// · 4 USDC mint · 5 token program
    PostBond { amount: u64 },
    /// ER: close the reveal window and freeze the open tick's input once
    /// every commitment was revealed or the reveal deadline passed; offices
    /// that committed but did not reveal are logged (`PS_FREEZE`), and the
    /// tick randomness is pending, seeded with the frozen salts' hash
    /// (`randomness::salts_hash`). It makes no VRF request (and starts the
    /// give-up clock): the crank sends `[FreezeTick, RetryTickRandomness]`
    /// in one transaction, or `FreezeTick` alone if that does not fit.
    /// Permissionless; not subject to the alone rule. The queue is the one
    /// of the recorded play mode (delegated: the ER queue; base play: the
    /// base queue), never either.
    /// 0 payer (s,w) · 1 world chunk 0 (w) · 2 program identity PDA `["identity"]`
    /// · 3 VRF queue (w) · 4 VRF program · 5 system · 6 SlotHashes sysvar
    /// · 7.. nation PDAs of every civ, in civ order (w)
    FreezeTick,
    /// ER: the VRF program's callback with the tick's randomness. Only the
    /// VRF program can sign as its scoped identity
    /// (`randomness::scoped_vrf_identity`); a stale callback is ignored.
    /// Logs `PS_RAND`.
    /// 0 VRF scoped identity (s) · 1 world chunk 0 (w)
    ConsumeTickRandomness {
        randomness: [u8; 32],
        season_id: u64,
        tick: u16,
    },
    /// ER: request the tick randomness from the MagicBlock VRF: the first
    /// request after `FreezeTick`, and again when none came
    /// `state::VRF_RETRY_SECONDS` after the last one; `state::VRF_GIVEUP_SECONDS`
    /// after the freeze: the fallback (`randomness::RAND_FALLBACK`, logged;
    /// the verifier flags it). Permissionless; not subject to the alone rule.
    /// Accounts 0–6 as `FreezeTick`.
    RetryTickRandomness,
    /// Base: the VRF program's callback with the season seed's randomness
    /// (status Seeding → Genesis). A stale callback is ignored. Logs
    /// `PS_SEED`.
    /// 0 VRF scoped identity (s) · 1 season (w)
    ConsumeSeasonSeed {
        randomness: [u8; 32],
        season_id: u64,
    },
    /// Base: status Seeding: request the season seed from the MagicBlock
    /// VRF (the base queue): the first request after `StartSeason`, and
    /// again `state::SEED_RETRY_SECONDS` after the last one. Permissionless.
    /// 0 payer (s,w) · 1 season (w) · 2 program identity PDA · 3 VRF base queue (w)
    /// · 4 VRF program · 5 system · 6 SlotHashes sysvar
    RetrySeasonSeed,
    /// Base: abort a season that cannot move on (`lifecycle::check_abort`):
    /// the operator while registering; anyone once a stage is overdue.
    /// Members then claim refunds. Logs `PS_ABORT`.
    /// 0 caller (s) · 1 season (w) · 2..=21 world chunks 0..19 (read)
    Abort,
    /// Base: ask the delegation program to give `target` back to its owner
    /// without the validator (after the season, or once a Running season is
    /// past `lifecycle::running_deadline`). Operator.
    /// 0 operator (s,w) · 1 season · 2 target PDA · 3 owner program
    /// · 4 undelegation request PDA (w) · 5 delegation record · 6 delegation metadata (w)
    /// · 7 system · 8 delegation program
    RequestUndelegation { target: u16 },
    /// Base: once the request expired, take `target` back with its last
    /// committed data. Permissionless. Logs `PS_ROLLBACK`.
    /// 0 season (w) · 1 target (w) · 2 owner program · 3 request (w) · 4 record (w)
    /// · 5 metadata (w) · 6 rent payer (w) · 7 commit state (w) · 8 commit record (w)
    /// · 9 reimbursement (w) · 10 delegation program
    RollbackUndelegation { target: u16 },
    /// Base, after the season (Finalized or Aborted): close world chunks and
    /// nation accounts, their rent to the operator. Clients send at most 13
    /// targets per transaction.
    /// 0 operator (s,w) · 1 season · 2.. targets in `targets` order (w)
    CloseSeasonAccounts { targets: Vec<u16> },
}

#[cfg(test)]
mod tests {
    use super::ChainInstruction as I;
    use permutation_rules::gov::Role;

    /// Clients (the gateway codec) hardcode these tags: every variant,
    /// append-only (the v9 contract, §1.1).
    #[test]
    fn tags_are_stable() {
        let tag = |ix: I| borsh::to_vec(&ix).unwrap()[0];
        let k = [0u8; 32];
        let all = [
            I::CreateSeason {
                season_id: 0,
                preset: 0,
                nations: 0,
                entry_fee: 0,
                tick_seconds: 0,
                world_seed: k,
                crank: k,
                market: false,
                prev_season_id: 0,
                ai_count: 0,
                roster_commit: k,
                bounty_each: 0,
                bond: 0,
                deposit: 0,
                start_by: 0,
                validator: k,
            },
            I::AllocWorld { chunk: 0 },
            I::Register {
                civ: 0,
                name: String::new(),
                kind: 0,
                session: k,
                attestation: k,
                stand: 0,
                votes: [0; 4],
                deposit: 0,
                tag: k,
            },
            I::StartSeason,
            I::GenesisStep { work: 0 },
            I::Delegate { target: 0 },
            I::SubmitOrders {
                role: Role::General,
                tick: 0,
                decision_digest: k,
                orders: vec![],
                adopt: vec![],
            },
            I::ResolveTick { to: 12 },
            I::Commit,
            I::CommitAndUndelegate,
            I::FinishSeason,
            I::Claim,
            I::UndelegatePart { targets: vec![] },
            I::UpdateMember {
                stand: 0,
                votes: [0; 4],
            },
            I::AllocNation { civ: 0 },
            I::SeatMembers,
            I::OpenGovernment,
            I::SubmitGov {
                member: 0,
                action: permutation_rules::gov::GovAction::Stand { roles: 0 },
            },
            I::WithdrawOps,
            I::LogTickInput { chunk: 0 },
            I::CommitPart { targets: vec![] },
            I::CloseCommits,
            I::CommitOrders {
                role: Role::General,
                tick: 0,
                commitment: k,
            },
            I::RevealOrders {
                role: Role::General,
                tick: 0,
                decision_digest: k,
                orders: vec![],
                adopt: vec![],
                salt: k,
            },
            I::RevealRoster {
                from: 0,
                salts: vec![],
                blind: k,
            },
            I::AnchorTalk {
                tick: 0,
                count: 0,
                root: k,
            },
            I::StartClock,
            I::PostBond { amount: 0 },
            I::FreezeTick,
            I::ConsumeTickRandomness {
                randomness: k,
                season_id: 0,
                tick: 0,
            },
            I::RetryTickRandomness,
            I::ConsumeSeasonSeed {
                randomness: k,
                season_id: 0,
            },
            I::RetrySeasonSeed,
            I::Abort,
            I::RequestUndelegation { target: 0 },
            I::RollbackUndelegation { target: 0 },
            I::CloseSeasonAccounts { targets: vec![] },
        ];
        assert_eq!(all.len(), 37, "tags 0..=36");
        for (i, ix) in all.into_iter().enumerate() {
            let name = format!("{ix:?}");
            assert_eq!(tag(ix) as usize, i, "{name}");
        }
    }

    /// The VRF program calls us with `tag ‖ randomness[32] ‖ callback args`
    /// (the v9 contract, §1.1): a one-byte borsh tag that decodes straight
    /// into the callback variants.
    #[test]
    fn vrf_callbacks_decode_from_the_oracle_layout() {
        let e = [7u8; 32];
        let mut tick = vec![29u8];
        tick.extend_from_slice(&e);
        tick.extend_from_slice(&42u64.to_le_bytes());
        tick.extend_from_slice(&5u16.to_le_bytes());
        match borsh::from_slice::<I>(&tick).unwrap() {
            I::ConsumeTickRandomness {
                randomness,
                season_id,
                tick,
            } => assert_eq!((randomness, season_id, tick), (e, 42, 5)),
            other => panic!("{other:?}"),
        }
        let mut seed = vec![31u8];
        seed.extend_from_slice(&e);
        seed.extend_from_slice(&42u64.to_le_bytes());
        match borsh::from_slice::<I>(&seed).unwrap() {
            I::ConsumeSeasonSeed {
                randomness,
                season_id,
            } => assert_eq!((randomness, season_id), (e, 42)),
            other => panic!("{other:?}"),
        }
    }
}
