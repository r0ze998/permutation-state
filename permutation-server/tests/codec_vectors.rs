//! Vectors for the JS client in permutation-gateway, produced by the Rust
//! types themselves:
//!
//! * `orders`, `gov`, `instructions`: borsh bytes of every order DTO variant,
//!   every governance action and every program instruction;
//! * `accounts`: borsh bytes of a Season, a MemberAccount, a NationAccount and
//!   a world chunk-0 header, with the values the JS decoders must return;
//! * `errors`: program error codes and names;
//! * `constants`: layout sizes, limits, PDA seeds, magics and names;
//! * `offices`: which offices may give each order (`role_allows_static`);
//! * `claims`: inputs and outputs of the program's `claim_amount`;
//! * `govSlots`, `govQuota`, `bond`, `lifecycle`, `vrf`: the pure helpers the
//!   clients mirror (`state::gov_slots`, `gov_quota`, `bond_floor`,
//!   `forfeit_penalty`, `lifecycle::*`, `randomness::*`).
//!
//! `UPDATE_VECTORS=1 cargo test --test codec_vectors` rewrites the file;
//! otherwise the file must match (so a Rust-side change fails loudly).

// The account vectors are one large `json!` literal.
#![recursion_limit = "256"]

use borsh::BorshDeserialize;
use permutation_chain::error::ChainError;
use permutation_chain::instruction::ChainInstruction;
use permutation_chain::lifecycle::{
    check_abort, escrow, ops_after_abort, refund_amount, running_deadline, WorldView,
};
use permutation_chain::randomness::{
    self, RAND_DEV, RAND_FALLBACK, RAND_NONE, RAND_PENDING, RAND_VRF, SEED_DEV, SEED_NONE,
    SEED_PENDING, SEED_VRF, VRF_PROGRAM_ID, VRF_QUEUE_BASE, VRF_QUEUE_ER,
};
use permutation_chain::state::{
    all_targets, bond_floor, degrade_after, forfeit_penalty, gov_quota, gov_slots, MemberAccount,
    NationAccount, RollSeat, RosterAccount, RosterEntry, Season, SeasonStatus, WorldMeta,
    ABORT_GRACE_SECONDS, CHUNK, DEGRADED, FINISH_GRACE_SECONDS, GENESIS_MAGIC, GOV_QUOTA_MAX,
    GOV_QUOTA_MIN, GOV_SLOTS_PER_TICK, GOV_SLOT_BYTES, INPUT_CHUNK, LEGACY_SEASON_MAGIC, MAX_AI,
    MAX_BOND, MAX_BOUNTY, MAX_DEPOSIT, MAX_ENTRY_FEE, MAX_GENESIS_WORK, MAX_GOV_ACTION_BYTES,
    MAX_GOV_PER_SIGNER, MAX_MEMBERS, MAX_NAME, MAX_NATIONS, MAX_NATIONS_PER_INTENT,
    MAX_REGISTRATION_SECONDS, MAX_REVEAL_BYTES, MAX_TICK_SECONDS, MEMBER_MAGIC, MEMBER_SEED,
    MIN_NATIONS, NATION_HEAD_LEN, NATION_MAGIC, NATION_MEMBER_CAP, NATION_SEED, PRESET_BLITZ,
    REVEAL_ROOM, ROSTER_GRACE_SECONDS, ROSTER_MAGIC, ROSTER_SEED, SEASON_MAGIC, SEASON_MEMBER_CAP,
    SEASON_SEED, SEED_RETRY_SECONDS, TAKEOVER_SECONDS, TICK0_GRACE_SECONDS, TICK_OVERHEAD_SECONDS,
    USDC_DECIMALS, VAULT_SEED, VRF_GIVEUP_SECONDS, VRF_RETRY_SECONDS, WORLD_BODY_MAX, WORLD_CHUNKS,
    WORLD_HEADER, WORLD_MAGIC, WORLD_META_SPACE, WORLD_SEED,
};
use permutation_chain::{payout::claim_amount, state::NATION_TARGET, CANONICAL_PROGRAM_ID};
use permutation_rules::decision::{MAX_POLICY, MAX_RATIONALE};
use permutation_rules::genesis::NATIONS;
use permutation_rules::gov::Role;
use permutation_rules::orders::{
    role_allows_static, MAX_BATCH_ORDERS, MAX_FREE_ORDERS, MAX_ORDER_COORD,
};
use permutation_rules::params::{Preset, Ruleset};
use permutation_server::api::{GovDto, OrderDto};
use permutation_server::ledger::BATCH_BYTES;
use serde_json::{json, Value};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn text(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

/// u64 values go to JSON as strings (JS numbers lose precision past 2^53).
fn u64s(v: &[u64]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn sample_season() -> Season {
    Season {
        magic: SEASON_MAGIC,
        season_id: 1_790_000_000_123,
        bump: 254,
        vault_bump: 253,
        admin: [1; 32],
        crank: [2; 32],
        usdc_mint: [3; 32],
        usdc_decimals: 6,
        preset: 0,
        nations: 6,
        entry_fee: 10_000_000,
        tick_seconds: 30,
        market: true,
        status: SeasonStatus::Finalized,
        world_seed: [4; 32],
        season_seed: [5; 32],
        member_count: 3,
        nation_members: vec![2, 1, 0, 0, 0, 0],
        seated: 3,
        pool: 24_000_000,
        ops: 6_000_123,
        ops_withdrawn: false,
        treasury: vec![5_000_000, 0, 0, 0, 0, 0],
        treasury_final: vec![4_000_000, 0, 0, 0, 0, 0],
        payouts: vec![12_000_000, 0, u64::MAX / 3],
        final_root: [6; 32],
        prev_season_id: 1_789_999_999_999,
        prev_history_root: [14; 32],
        history_root: [15; 32],
        ai_count: 3,
        roster_commit: [16; 32],
        bounty_each: 5_000_000,
        bond: 60_000_000,
        roster_acc: [16; 32],
        roster_revealed: 3,
        roster_outcome: 1,
        bounty_paid: vec![0, 5_000_000, 0, 0, 0, 0],
        delegated: 0x03ff_ffff,
        roster_blind: [26; 32],
        refund_base: vec![4_000_000, 0, 0, 0, 0, 0],
        refund_in_payout: vec![0b0000_0010],
        seed_state: 2,
        seed_oracle: [27; 32],
        seed_requested_at: 1_789_999_990,
        seed_requests: 1,
        deposit: 1_000_000,
        outstanding: 30_000_000,
        voided: false,
        start_by: 1_790_003_600,
        stage_at: 1_790_000_000,
        rolled_back: 0,
        aborted_from: 0,
        validator: [28; 32],
        rules_version: 9,
        rules_hash: [29; 32],
        logic_version: 1,
        created_slot: 412_345_678,
    }
}

fn sample_roster() -> RosterAccount {
    RosterAccount {
        magic: ROSTER_MAGIC,
        season_id: 1_790_000_000_123,
        bump: 249,
        entries: vec![
            RosterEntry {
                member: 1,
                civ: 0,
                salt: [17; 32],
                shares: 1_000_000,
            },
            RosterEntry {
                member: 2,
                civ: 1,
                salt: [18; 32],
                shares: 0,
            },
        ],
    }
}

fn sample_member() -> MemberAccount {
    MemberAccount {
        magic: MEMBER_MAGIC,
        season_id: 1_790_000_000_123,
        bump: 251,
        index: 2,
        civ: 1,
        wallet: [7; 32],
        session: [8; 32],
        kind: 1,
        name: "Hypatia-アステル".into(),
        attestation: [9; 32],
        stand: 0b1010,
        votes: [2, u32::MAX, 0, 1],
        shares: 2_500_000,
        claimed: false,
        tag: [19; 32],
    }
}

fn sample_nation() -> NationAccount {
    NationAccount {
        magic: NATION_MAGIC,
        season_id: 1_790_000_000_123,
        civ: 4,
        bump: 250,
        preset: 1,
        market: false,
        crank: [2; 32],
        open_tick: 17,
        officers: [0, u32::MAX, 5, 7],
        keys: [[10; 32], [0; 32], [11; 32], [12; 32]],
        spendable: [3, 0, 8, 2],
        submitted: [17, u16::MAX, 16, 17],
        frozen: true,
        revealing: true,
        deadline: 1_790_000_140,
        committed: [17, u16::MAX, 17, 16],
        commits: [[20; 32], [0; 32], [21; 32], [22; 32]],
        salts: [[23; 32], [0; 32], [24; 32], [25; 32]],
        gov_quota: 6,
        batches: [None, None, None, None],
        inbox: vec![],
        roll: vec![
            RollSeat {
                member: 0,
                key8: [10; 8],
            },
            RollSeat {
                member: 5,
                key8: [11; 8],
            },
        ],
    }
}

fn sample_meta() -> WorldMeta {
    WorldMeta {
        season_id: 1_790_000_000_123,
        preset: 0,
        civs: 6,
        tick_seconds: 30,
        deadline: 1_790_000_123,
        finished: false,
        market: true,
        frozen: true,
        vrf: [13; 32],
        input_chunks: 2,
        input_logged: 1,
        revealing: true,
        undelegated: 0,
        input_hash: [30; 32],
        rand_state: 2,
        rand_tick: 17,
        rand_pre: [31; 32],
        rand_out: [32; 32],
        frozen_at: 1_790_000_118,
        rand_requested_at: 1_790_000_119,
        rand_requests: 1,
        usdc_broken: false,
    }
}

/// Chunk 0's header as the program writes it: magic, body length, `WorldMeta` padded.
fn world_header(meta: &WorldMeta, len: u32) -> Vec<u8> {
    let mut out = vec![0u8; WORLD_HEADER];
    out[..8].copy_from_slice(&WORLD_MAGIC);
    out[8..12].copy_from_slice(&len.to_le_bytes());
    let m = borsh::to_vec(meta).unwrap();
    out[12..12 + m.len()].copy_from_slice(&m);
    out
}

fn status_name(s: SeasonStatus) -> String {
    format!("{s:?}")
}

/// `Season::aborted_from` as the JS decoder names it (a `SeasonStatus`).
fn status_of(tag: u8) -> String {
    status_name(SeasonStatus::try_from_slice(&[tag]).unwrap())
}

/// A season's decoded JSON, as `decodeSeason` returns it.
fn season_json(s: &Season) -> Value {
    json!({
        "seasonId": s.season_id.to_string(), "bump": s.bump, "vaultBump": s.vault_bump, "admin": hex(&s.admin), "crank": hex(&s.crank),
        "usdcMint": hex(&s.usdc_mint), "usdcDecimals": s.usdc_decimals, "preset": s.preset, "nations": s.nations,
        "entryFee": s.entry_fee.to_string(), "tickSeconds": s.tick_seconds, "market": s.market, "status": status_name(s.status),
        "worldSeed": hex(&s.world_seed), "seasonSeed": hex(&s.season_seed), "memberCount": s.member_count,
        "nationMembers": s.nation_members, "seated": s.seated, "pool": s.pool.to_string(), "ops": s.ops.to_string(),
        "opsWithdrawn": s.ops_withdrawn, "treasury": u64s(&s.treasury), "treasuryFinal": u64s(&s.treasury_final),
        "payouts": u64s(&s.payouts), "finalRoot": hex(&s.final_root),
        "prevSeasonId": s.prev_season_id.to_string(), "prevHistoryRoot": hex(&s.prev_history_root), "historyRoot": hex(&s.history_root),
        "aiCount": s.ai_count, "rosterCommit": hex(&s.roster_commit), "bountyEach": s.bounty_each.to_string(), "bond": s.bond.to_string(),
        "rosterAcc": hex(&s.roster_acc), "rosterRevealed": s.roster_revealed,
        "rosterOutcome": (["none", "revealed", "forfeited"][s.roster_outcome as usize]), "bountyPaid": u64s(&s.bounty_paid),
        "delegated": s.delegated, "rosterBlind": hex(&s.roster_blind), "refundBase": u64s(&s.refund_base),
        "refundInPayout": hex(&s.refund_in_payout), "seedState": s.seed_state, "seedOracle": hex(&s.seed_oracle),
        "seedRequestedAt": s.seed_requested_at, "seedRequests": s.seed_requests, "deposit": s.deposit.to_string(),
        "outstanding": s.outstanding.to_string(), "voided": s.voided, "startBy": s.start_by, "stageAt": s.stage_at,
        "rolledBack": s.rolled_back, "abortedFrom": status_of(s.aborted_from), "validator": hex(&s.validator),
        "rulesVersion": s.rules_version, "rulesHash": hex(&s.rules_hash), "logicVersion": s.logic_version,
        "createdSlot": s.created_slot.to_string(), "legacy": s.magic == LEGACY_SEASON_MAGIC,
    })
}

/// A `PSSEASN7` account as the previous program left it: the fields up to
/// `bounty_paid`, then zeros (read by `load_season_compat`; every v8 tail
/// field decodes as zero / empty from them).
fn legacy_season() -> Season {
    let s = sample_season();
    Season {
        magic: LEGACY_SEASON_MAGIC,
        delegated: 0,
        roster_blind: [0; 32],
        refund_base: vec![],
        refund_in_payout: vec![],
        seed_state: 0,
        seed_oracle: [0; 32],
        seed_requested_at: 0,
        seed_requests: 0,
        deposit: 0,
        outstanding: 0,
        voided: false,
        start_by: 0,
        stage_at: 0,
        rolled_back: 0,
        aborted_from: 0,
        validator: [0; 32],
        rules_version: 0,
        rules_hash: [0; 32],
        logic_version: 0,
        created_slot: 0,
        ..s
    }
}

fn account_vectors() -> Value {
    let s = sample_season();
    let legacy = legacy_season();
    let ro = sample_roster();
    let m = sample_member();
    let n = sample_nation();
    let meta = sample_meta();
    json!({
        "season": { "hex": hex(&borsh::to_vec(&s).unwrap()), "decoded": season_json(&s) },
        "legacySeason": { "hex": hex(&borsh::to_vec(&legacy).unwrap()), "decoded": season_json(&legacy) },
        "roster": {
            "hex": hex(&borsh::to_vec(&ro).unwrap()),
            "decoded": {
                "seasonId": ro.season_id.to_string(), "bump": ro.bump,
                "entries": ro.entries.iter().map(|e| json!({"member": e.member, "civ": e.civ, "salt": hex(&e.salt), "shares": e.shares.to_string()})).collect::<Vec<_>>(),
            },
        },
        "member": {
            "hex": hex(&borsh::to_vec(&m).unwrap()),
            "decoded": {
                "seasonId": m.season_id.to_string(), "bump": m.bump, "index": m.index, "civ": m.civ, "wallet": hex(&m.wallet),
                "session": hex(&m.session), "kind": m.kind, "name": m.name, "attestation": hex(&m.attestation), "stand": m.stand,
                "votes": m.votes, "shares": m.shares.to_string(), "claimed": m.claimed, "tag": hex(&m.tag),
            },
        },
        "nation": {
            "hex": hex(&borsh::to_vec(&n).unwrap()),
            "decoded": {
                "seasonId": n.season_id.to_string(), "civ": n.civ, "bump": n.bump, "preset": n.preset, "market": n.market,
                "crank": hex(&n.crank), "openTick": n.open_tick, "officers": n.officers,
                "keys": n.keys.iter().map(|k| hex(k)).collect::<Vec<_>>(), "spendable": n.spendable, "submitted": n.submitted,
                "frozen": n.frozen, "revealing": n.revealing, "deadline": n.deadline, "committed": n.committed,
                "commits": n.commits.iter().map(|k| hex(k)).collect::<Vec<_>>(),
                "salts": n.salts.iter().map(|k| hex(k)).collect::<Vec<_>>(), "govQuota": n.gov_quota,
            },
            // `decodeNationHeader` reads the fixed front (`NationHead`,
            // `NATION_HEAD_LEN` bytes); the roll comes after the batches and
            // the inbox.
            "headLen": NATION_HEAD_LEN,
        },
        "worldHeader": {
            "hex": hex(&world_header(&meta, 23_456)),
            "decoded": {
                "magic": text(&WORLD_MAGIC), "len": 23_456, "bodyOffset": WORLD_HEADER,
                "meta": {
                    "seasonId": meta.season_id.to_string(), "preset": meta.preset, "civs": meta.civs, "tickSeconds": meta.tick_seconds,
                    "deadline": meta.deadline, "finished": meta.finished, "market": meta.market, "frozen": meta.frozen,
                    "vrf": hex(&meta.vrf), "inputChunks": meta.input_chunks, "inputLogged": meta.input_logged,
                    "revealing": meta.revealing, "undelegated": meta.undelegated, "inputHash": hex(&meta.input_hash),
                    "randState": meta.rand_state, "randTick": meta.rand_tick, "randPre": hex(&meta.rand_pre),
                    "randOut": hex(&meta.rand_out), "frozenAt": meta.frozen_at, "randRequestedAt": meta.rand_requested_at,
                    "randRequests": meta.rand_requests, "usdcBroken": meta.usdc_broken,
                },
            },
        },
    })
}

fn constant_vectors() -> Value {
    // Every `SeasonStatus`, in tag order: decode tags until one is refused.
    let statuses: Vec<String> = (0u8..=255)
        .map_while(|t| SeasonStatus::try_from_slice(&[t]).ok())
        .map(status_name)
        .collect();
    json!({
        "WORLD_CHUNKS": WORLD_CHUNKS, "CHUNK": CHUNK, "WORLD_HEADER": WORLD_HEADER, "NATION_TARGET": NATION_TARGET,
        "INPUT_CHUNK": INPUT_CHUNK, "MAX_NATIONS": MAX_NATIONS, "MAX_NAME": MAX_NAME, "MAX_MEMBERS": MAX_MEMBERS,
        "MAX_GOV_PER_SIGNER": MAX_GOV_PER_SIGNER, "MAX_POLICY": MAX_POLICY, "MAX_RATIONALE": MAX_RATIONALE, "BATCH_BYTES": BATCH_BYTES,
        "MAX_AI": MAX_AI, "ROSTER_GRACE_SECONDS": ROSTER_GRACE_SECONDS,
        "WORLD_META_SPACE": WORLD_META_SPACE, "WORLD_BODY_MAX": WORLD_BODY_MAX, "REVEAL_ROOM": REVEAL_ROOM, "MAX_REVEAL_BYTES": MAX_REVEAL_BYTES,
        "MAX_GOV_ACTION_BYTES": MAX_GOV_ACTION_BYTES, "GOV_SLOT_BYTES": GOV_SLOT_BYTES, "GOV_SLOTS_PER_TICK": GOV_SLOTS_PER_TICK,
        "GOV_QUOTA_MIN": GOV_QUOTA_MIN, "GOV_QUOTA_MAX": GOV_QUOTA_MAX, "SEASON_MEMBER_CAP": SEASON_MEMBER_CAP, "NATION_MEMBER_CAP": NATION_MEMBER_CAP,
        "MAX_GENESIS_WORK": MAX_GENESIS_WORK, "USDC_DECIMALS": USDC_DECIMALS, "MAX_ENTRY_FEE": MAX_ENTRY_FEE.to_string(),
        "MAX_DEPOSIT": MAX_DEPOSIT.to_string(), "MAX_BOUNTY": MAX_BOUNTY.to_string(), "MAX_BOND": MAX_BOND.to_string(),
        "ABORT_GRACE_SECONDS": ABORT_GRACE_SECONDS, "FINISH_GRACE_SECONDS": FINISH_GRACE_SECONDS,
        "MAX_REGISTRATION_SECONDS": MAX_REGISTRATION_SECONDS, "MAX_TICK_SECONDS": MAX_TICK_SECONDS,
        "TICK_OVERHEAD_SECONDS": TICK_OVERHEAD_SECONDS, "TAKEOVER_SECONDS": TAKEOVER_SECONDS, "TICK0_GRACE_SECONDS": TICK0_GRACE_SECONDS,
        "DEGRADED": DEGRADED, "VRF_RETRY_SECONDS": VRF_RETRY_SECONDS, "VRF_GIVEUP_SECONDS": VRF_GIVEUP_SECONDS,
        "SEED_RETRY_SECONDS": SEED_RETRY_SECONDS, "MAX_NATIONS_PER_INTENT": MAX_NATIONS_PER_INTENT,
        "PRESET_BLITZ": PRESET_BLITZ, "MIN_NATIONS": MIN_NATIONS, "NATION_HEAD_LEN": NATION_HEAD_LEN,
        "NATION_BASE_LEN": NationAccount::base_len(0), "MAX_BATCH_ORDERS": MAX_BATCH_ORDERS, "MAX_FREE_ORDERS": MAX_FREE_ORDERS,
        "TICKS_PER_SEASON": Ruleset::new(Preset::Blitz).ticks_per_season,
        "EXCHANGE_MAX_PRICE": Ruleset::new(Preset::Blitz).exchange_max_price.to_string(),
        "MAX_TRADE_AMOUNT": Ruleset::new(Preset::Blitz).max_trade_amount, "MAX_ORDER_COORD": MAX_ORDER_COORD,
        "RAND": { "none": RAND_NONE, "pending": RAND_PENDING, "vrf": RAND_VRF, "fallback": RAND_FALLBACK, "dev": RAND_DEV },
        "SEED": { "none": SEED_NONE, "pending": SEED_PENDING, "vrf": SEED_VRF, "dev": SEED_DEV },
        "VRF_PROGRAM_ID": VRF_PROGRAM_ID.to_string(), "VRF_QUEUE_BASE": VRF_QUEUE_BASE.to_string(),
        "VRF_QUEUE_ER": VRF_QUEUE_ER.to_string(), "CANONICAL_PROGRAM_ID": CANONICAL_PROGRAM_ID,
        "degradeAfter": ([1u32, 30, 300, 301, 600, 14_400]).iter().map(|t| json!([t, degrade_after(*t)])).collect::<Vec<_>>(),
        "SEEDS": { "season": text(SEASON_SEED), "world": text(WORLD_SEED), "nation": text(NATION_SEED), "member": text(MEMBER_SEED), "vault": text(VAULT_SEED), "roster": text(ROSTER_SEED) },
        "MAGIC": { "season": text(&SEASON_MAGIC), "legacySeason": text(&LEGACY_SEASON_MAGIC), "member": text(&MEMBER_MAGIC), "nation": text(&NATION_MAGIC), "world": text(&WORLD_MAGIC), "genesis": text(&GENESIS_MAGIC), "roster": text(&ROSTER_MAGIC) },
        "NATIONS": NATIONS,
        "ROLES": Role::ALL.iter().map(|r| format!("{r:?}")).collect::<Vec<_>>(),
        "SEASON_STATUS": statuses,
    })
}

/// Operator AI roster tags and their chain (V5 §18.2), as the gateway computes them.
fn roster_vectors() -> Value {
    use permutation_rules::roster::{roster_chain, roster_commit, roster_tag};
    let cases: Vec<(u64, [u8; 32], [u8; 32])> = vec![
        (1_790_000_000_123, [7; 32], [17; 32]),
        (1_790_000_000_123, [8; 32], [18; 32]),
        (42, [1; 32], [0; 32]),
    ];
    let tags: Vec<[u8; 32]> = cases.iter().map(|(s, w, x)| roster_tag(*s, w, x)).collect();
    json!({
        "tags": cases.iter().zip(&tags).map(|((s, w, x), t)| json!({
            "seasonId": s.to_string(), "wallet": hex(w), "salt": hex(x), "tag": hex(t),
        })).collect::<Vec<_>>(),
        "chain": hex(&roster_chain(&tags)),
        // WP09's blinded commitment (`roster_commit`, WP09 §4.1), which
        // CreateSeason stores and the completing RevealRoster opens.
        "commits": ([[0u8; 32], [3; 32], [0xa5; 32]]).iter().map(|blind| {
            let chain = roster_chain(&tags);
            json!({"blind": hex(blind), "chain": hex(&chain),
                   "commit": hex(&roster_commit(blind, &chain))})
        }).collect::<Vec<_>>(),
    })
}

/// The season inputs of `claim_amount` and the lifecycle helpers, as the JS
/// side takes them (a subset of `decodeSeason`).
fn money_json(s: &Season) -> Value {
    json!({
        "status": status_name(s.status), "abortedFrom": status_of(s.aborted_from), "entryFee": s.entry_fee.to_string(),
        "memberCount": s.member_count, "aiCount": s.ai_count, "bountyEach": s.bounty_each.to_string(), "bond": s.bond.to_string(),
        "payouts": u64s(&s.payouts), "treasury": u64s(&s.treasury), "treasuryFinal": u64s(&s.treasury_final),
        "refundBase": u64s(&s.refund_base), "refundInPayout": hex(&s.refund_in_payout), "legacy": s.magic == LEGACY_SEASON_MAGIC,
    })
}

/// (payouts, treasury, treasury_final, member index, civ, shares)
type ClaimCase = (Vec<u64>, Vec<u64>, Vec<u64>, u32, u16, u64);

/// Whether this build's `claim_amount` has the v9 rules (WP10 refund flags
/// and base, WP14 refunds after an abort). The JS client implements them;
/// its test holds the cases that need them to the vectors only once the
/// program does (the vectors are regenerated when it lands).
fn claim_rule_v9() -> bool {
    let flagged = Season {
        payouts: vec![7],
        treasury: vec![10],
        treasury_final: vec![10],
        refund_base: vec![10],
        refund_in_payout: vec![1],
        ..sample_season()
    };
    let m = MemberAccount {
        index: 0,
        civ: 0,
        shares: 10,
        ..sample_member()
    };
    let aborted = Season {
        status: SeasonStatus::Aborted,
        aborted_from: SeasonStatus::Seating as u8,
        ..sample_season()
    };
    claim_amount(&flagged, &m) == 7 && claim_amount(&aborted, &m) == refund_amount(&aborted, &m)
}

fn claim_vectors() -> Value {
    let base = Season {
        refund_base: vec![],
        refund_in_payout: vec![],
        ..sample_season()
    };
    let cases: Vec<ClaimCase> = vec![
        (
            vec![12_000_000, 0, 7],
            vec![5_000_000, 0],
            vec![4_000_000, 0],
            0,
            0,
            2_500_000,
        ),
        (
            vec![12_000_000, 0, 7],
            vec![5_000_000, 0],
            vec![4_000_000, 0],
            1,
            1,
            0,
        ),
        (
            vec![12_000_000, 0, 7],
            vec![5_000_000, 0],
            vec![4_000_000, 0],
            2,
            0,
            1,
        ),
        (vec![1], vec![3], vec![2], 9, 5, 1),
        (vec![], vec![0, 10], vec![0, 10], 0, 1, 10),
        // shares × left overflows u64: the program multiplies in u128.
        (
            vec![5],
            vec![u64::MAX / 2],
            vec![u64::MAX / 4],
            0,
            0,
            u64::MAX / 3,
        ),
    ];
    // Legacy rule (empty `refund_base`: the refund is over `treasury`), for
    // PSSEASN7 seasons and every season before FinishSeason.
    let mut out: Vec<(Season, MemberAccount, bool)> = cases
        .into_iter()
        .map(|(payouts, treasury, treasury_final, index, civ, shares)| {
            let season = Season {
                payouts,
                treasury,
                treasury_final,
                ..base.clone()
            };
            (
                season,
                MemberAccount {
                    index,
                    civ,
                    shares,
                    ..sample_member()
                },
                false,
            )
        })
        .collect();
    let finalized = Season {
        payouts: vec![12_000_000, 3_000_000, 1_000_000, 9, 4],
        treasury: vec![5_000_000, 2_000_000],
        treasury_final: vec![4_000_000, 3_000_000],
        refund_base: vec![4_000_000, 2_000_000],
        refund_in_payout: vec![0b0000_0110],
        ..base.clone()
    };
    let member = |index, civ, shares| MemberAccount {
        index,
        civ,
        shares,
        ..sample_member()
    };
    // WP10: refund over `refund_base`; flagged members (revealed AIs) get their payout alone.
    out.push((finalized.clone(), member(0, 0, 2_500_000), true));
    out.push((finalized.clone(), member(1, 0, 1_000_000), true));
    out.push((finalized.clone(), member(2, 1, 2_000_000), true));
    out.push((finalized.clone(), member(3, 1, 2_000_000), true));
    out.push((
        Season {
            refund_base: vec![0, 2_000_000],
            ..finalized.clone()
        },
        member(4, 0, 5),
        true,
    ));
    out.push((
        Season {
            payouts: vec![u64::MAX],
            ..finalized.clone()
        },
        member(0, 0, 4_000_000),
        true,
    ));
    // A legacy PSSEASN7 season: its tail is zero, so the legacy rule.
    let legacy = legacy_season();
    out.push((legacy.clone(), member(0, 0, 2_500_000), false));
    // WP14: after an abort, what Register took (plus a forfeited escrow share).
    for from in [
        SeasonStatus::Registering,
        SeasonStatus::Seeding,
        SeasonStatus::Seating,
        SeasonStatus::Running,
    ] {
        let s = Season {
            status: SeasonStatus::Aborted,
            aborted_from: from as u8,
            member_count: 7,
            ai_count: 3,
            bounty_each: 1_000_003,
            bond: 60_000_001,
            ..finalized.clone()
        };
        out.push((s, member(1, 0, 2_000_000), true));
    }
    // Until the program has the v9 rules, their cases carry no amount (the
    // HEAD rule would even overflow on the saturating one).
    let rule_v9 = claim_rule_v9();
    Value::Array(
        out.into_iter()
            .map(|(season, m, v9)| {
                let amount = (rule_v9 || !v9).then(|| claim_amount(&season, &m).to_string());
                json!({
                    "season": money_json(&season), "index": m.index, "civ": m.civ, "shares": m.shares.to_string(),
                    "amount": amount, "v9": v9,
                })
            })
            .collect(),
    )
}

/// `gov_slots` of every governance vector plus proposals at and past the
/// size limit (`None` → null).
fn gov_slot_vectors(govs: &[Value]) -> Value {
    let step = |i: i32| json!([i, -i]);
    let path: Vec<Value> = (0..12).map(step).collect();
    let long: Vec<Value> = (0..40).map(step).collect();
    let mv = |p: &Vec<Value>| json!({"type": "MoveUnit", "unit": 7, "path": p});
    let mut all: Vec<Value> = govs.to_vec();
    all.push(json!({"type": "Propose", "role": "General", "orders": [mv(&path), mv(&path), mv(&path), mv(&path)]}));
    all.push(json!({"type": "Propose", "role": "Steward", "orders": (0..5).map(|c| json!({"type": "Purchase", "city": c, "gold": 9})).collect::<Vec<_>>()}));
    all.push(json!({"type": "Propose", "role": "General", "orders": [mv(&long), mv(&long)]}));
    all.push(json!({"type": "Propose", "role": "Science", "orders": []}));
    Value::Array(
        all.into_iter()
            .map(|dto| {
                let action = serde_json::from_value::<GovDto>(dto.clone()).unwrap().to_action().unwrap();
                json!({"dto": dto, "bytes": borsh::object_length(&action).unwrap(), "slots": gov_slots(&action)})
            })
            .collect(),
    )
}

/// `gov_quota(members, here)` over the season and nation sizes a Blitz season can have.
fn gov_quota_vectors() -> Value {
    let mut out = Vec::new();
    for members in [1u32, 2, 5, 8, 12, 17, 24, 36, 48, 96, 256] {
        for here in [1u32, 2, 4, 8, 12, 16, 24] {
            if here <= members {
                out.push(json!([members, here, gov_quota(members, here)]));
            }
        }
    }
    Value::Array(out)
}

/// `forfeit_penalty` and `bond_floor` (WP09) for seasons around the caps.
fn bond_vectors() -> Value {
    let fee = 10_000_000u64;
    let mut out = Vec::new();
    for (n, a, bounty, deposits) in [
        (0u32, 0u16, 0u64, 0u64),
        (5, 0, 5_000_000, 0),
        (3, 3, 5_000_000, 0),
        (4, 3, 5_000_000, 0),
        (12, 3, 5_000_000, 2_000_000),
        (48, 47, 1_000_000_000, 1_000_000_000),
        (48, 40, 1_000_000, 0),
        (2, 1, 0, 0),
        (48, 12, 1_000_000_000, 48_000_000_000),
    ] {
        let s = Season {
            entry_fee: fee,
            member_count: n,
            ai_count: a,
            bounty_each: bounty,
            pool: n as u64 * fee * 8 / 10,
            treasury: vec![deposits, 0, 3, 0, 0, 0],
            ..sample_season()
        };
        out.push(json!({
            "entryFee": s.entry_fee.to_string(), "memberCount": n, "aiCount": a, "bountyEach": bounty.to_string(),
            "pool": s.pool.to_string(), "treasury": u64s(&s.treasury),
            "forfeitPenalty": forfeit_penalty(&s).to_string(), "bondFloor": bond_floor(&s).to_string(),
        }));
    }
    let max = Season {
        entry_fee: u64::MAX,
        member_count: 48,
        ai_count: 47,
        pool: u64::MAX,
        bounty_each: u64::MAX,
        treasury: vec![u64::MAX; 6],
        ..sample_season()
    };
    out.push(json!({
        "entryFee": max.entry_fee.to_string(), "memberCount": 48, "aiCount": 47, "bountyEach": max.bounty_each.to_string(),
        "pool": max.pool.to_string(), "treasury": u64s(&max.treasury),
        "forfeitPenalty": forfeit_penalty(&max).to_string(), "bondFloor": bond_floor(&max).to_string(),
    }));
    Value::Array(out)
}

/// `lifecycle`: running deadlines, `check_abort` verdicts, and the refunds after an abort.
fn lifecycle_vectors() -> Value {
    let base = Season {
        start_by: 1_000,
        stage_at: 500,
        tick_seconds: 30,
        delegated: 0,
        member_count: 12,
        ..sample_season()
    };
    let ticks = Ruleset::new(Preset::Blitz).ticks_per_season;
    let home = |finished: bool, precheck_ok: bool| WorldView {
        all_home: true,
        chunk0_home: true,
        is_world: true,
        finished,
        deadline: 530,
        precheck_ok,
    };
    let away = WorldView {
        all_home: false,
        chunk0_home: false,
        is_world: false,
        finished: false,
        deadline: 530,
        precheck_ok: false,
    };
    let running = Season {
        status: SeasonStatus::Running,
        ..base.clone()
    };
    let delegated = Season {
        delegated: all_targets(6),
        ..running.clone()
    };
    let rd = running_deadline(&delegated, ticks);
    let day = ABORT_GRACE_SECONDS;
    let with = |status| Season {
        status,
        ..base.clone()
    };
    let mut cases: Vec<(Season, Option<WorldView>, bool, i64)> = vec![
        (with(SeasonStatus::Registering), None, true, 0),
        (
            with(SeasonStatus::Registering),
            None,
            false,
            1_000 + day - 1,
        ),
        (with(SeasonStatus::Registering), None, false, 1_000 + day),
        (with(SeasonStatus::Seeding), None, true, 500 + day - 1),
        (with(SeasonStatus::Seeding), None, false, 500 + day),
        (with(SeasonStatus::Genesis), None, true, 500 + day - 1),
        (with(SeasonStatus::Seating), None, false, 500 + day),
        (
            running.clone(),
            Some(home(false, false)),
            false,
            530 + day - 1,
        ),
        (running.clone(), Some(home(false, false)), false, 530 + day),
        (
            running.clone(),
            Some(WorldView {
                deadline: 530 + day,
                ..home(false, false)
            }),
            false,
            530 + day,
        ),
        (running.clone(), None, false, rd),
        (delegated.clone(), Some(away), false, 530 + day),
        (delegated.clone(), Some(away), false, rd - 1),
        (delegated.clone(), Some(away), false, rd),
        (delegated.clone(), Some(home(true, true)), false, rd),
        (
            delegated.clone(),
            Some(home(true, true)),
            false,
            rd + FINISH_GRACE_SECONDS - 1,
        ),
        (
            delegated.clone(),
            Some(home(true, true)),
            false,
            rd + FINISH_GRACE_SECONDS,
        ),
        (delegated.clone(), Some(home(true, false)), false, rd),
        (with(SeasonStatus::Finalized), None, true, i64::MAX),
        (with(SeasonStatus::Aborted), None, true, i64::MAX),
    ];
    let long = Season {
        stage_at: i64::MAX - 10,
        ..delegated.clone()
    };
    cases.push((long, Some(away), false, i64::MAX));
    let view_json = |w: &WorldView| {
        json!({"allHome": w.all_home, "chunk0Home": w.chunk0_home, "isWorld": w.is_world, "finished": w.finished,
               "deadline": w.deadline, "precheckOk": w.precheck_ok})
    };
    let season_json = |s: &Season| {
        json!({"status": status_name(s.status), "startBy": s.start_by, "stageAt": s.stage_at, "tickSeconds": s.tick_seconds,
               "delegated": s.delegated})
    };
    let abort: Vec<Value> = cases
        .iter()
        .map(|(s, w, operator, now)| {
            json!({
                "season": season_json(s), "view": w.as_ref().map(view_json), "operator": operator, "now": now.to_string(),
                "runningDeadline": running_deadline(s, ticks).to_string(),
                "result": check_abort(s, ticks, w.as_ref(), *operator, *now).err().map(|e| e.name()),
            })
        })
        .collect();
    let mut refunds = Vec::new();
    for (from, n, ai, bounty, bond, shares) in [
        (
            SeasonStatus::Registering,
            12u32,
            3u16,
            1_000_000u64,
            6_000_000u64,
            2_000_000u64,
        ),
        (
            SeasonStatus::Seating,
            12,
            3,
            1_000_000,
            6_000_000,
            2_000_000,
        ),
        (SeasonStatus::Seeding, 7, 2, 333_333, 1, 0),
        (SeasonStatus::Running, 48, 47, 1_000_003, 777_777_777, 0),
        (SeasonStatus::Genesis, 0, 0, 0, 5, 0),
        (SeasonStatus::Running, 5, 2, u64::MAX, u64::MAX, u64::MAX),
    ] {
        let s = Season {
            status: SeasonStatus::Aborted,
            aborted_from: from as u8,
            member_count: n,
            ai_count: ai,
            bounty_each: bounty,
            bond,
            ..base.clone()
        };
        let m = MemberAccount {
            shares,
            ..sample_member()
        };
        refunds.push(json!({
            "season": money_json(&s), "shares": shares.to_string(), "escrow": escrow(&s).to_string(),
            "refund": refund_amount(&s, &m).to_string(), "opsAfterAbort": ops_after_abort(&s).to_string(),
        }));
    }
    json!({ "ticksPerSeason": ticks, "abort": abort, "refunds": refunds })
}

/// The program's identity PDA (it signs VRF requests) and the VRF's scoped
/// identity (it signs the callbacks), for the canonical program id.
fn vrf_vectors() -> Value {
    // The chain's `Pubkey` type, without naming its crate (not a dependency here).
    fn parse_as<T: std::str::FromStr>(_: &T, s: &str) -> T {
        s.parse().ok().unwrap()
    }
    let program = parse_as(&VRF_PROGRAM_ID, CANONICAL_PROGRAM_ID);
    json!({
        "programId": CANONICAL_PROGRAM_ID,
        "programIdentity": randomness::program_identity(&program).0.to_string(),
        "vrfIdentity": randomness::scoped_vrf_identity(&program).to_string(),
    })
}

#[test]
fn vectors_match_the_js_encoder_fixture() {
    let orders: Vec<Value> = vec![
        json!({"type":"MoveUnit","unit":7,"path":[[1,-2],[2,-2],[-3,4]]}),
        json!({"type":"Attack","army":3,"target":{"kind":"Unit","id":9}}),
        json!({"type":"Attack","army":3,"target":{"kind":"City","id":2}}),
        json!({"type":"Attack","army":3,"target":{"kind":"CityState","id":1}}),
        json!({"type":"FoundCity","settler":12}),
        json!({"type":"SetQueue","city":0,"items":[{"kind":"Building","building":"Granary"},{"kind":"Troops","unit":"Archer","n":5},{"kind":"Scout"},{"kind":"Settler"}]}),
        json!({"type":"SetFocus","city":1,"focus":"Science"}),
        json!({"type":"Purchase","city":1,"gold":60}),
        json!({"type":"SetResearch","techs":["Agriculture","CelestialMechanics","Writing"]}),
        json!({"type":"DeclareWar","civ":4}),
        json!({"type":"ProposePeace","civ":4}),
        json!({"type":"AcceptPeace","civ":4}),
        json!({"type":"ProposeNap","civ":2,"bond":30}),
        json!({"type":"AcceptNap","civ":2,"bond":30}),
        json!({"type":"BreakNap","civ":2}),
        json!({"type":"ProposeAlliance","civ":5}),
        json!({"type":"AcceptAlliance","civ":5}),
        json!({"type":"LeaveAlliance"}),
        json!({"type":"SendEnvoy","cityState":1,"influence":20}),
        json!({"type":"Transfer","civ":3,"good":{"kind":"Food","city":4},"amount":10}),
        json!({"type":"MarketTrade","good":{"kind":"Iron"},"side":"Buy","amount":5,"limitGold":200}),
        json!({"type":"ExchangeOrder","good":{"kind":"Production","city":2},"side":"Sell","amount":3,"price":1500000}),
        json!({"type":"Raze","city":6}),
        json!({"type":"SetStanding","target":{"kind":"Unit","id":1},"rule":{"kind":"AutoDefend","radius":2}}),
        json!({"type":"SetStanding","target":{"kind":"Unit","id":1},"rule":{"kind":"Retreat","ratioBps":15000}}),
        json!({"type":"SetStanding","target":{"kind":"Unit","id":0},"rule":{"kind":"Patrol","route":[[1,1],[2,0]]}}),
        json!({"type":"SetStanding","target":{"kind":"City","id":0},"rule":{"kind":"QueueRepeat","on":false}}),
        json!({"type":"SetStanding","target":{"kind":"City","id":0},"rule":{"kind":"AutoPurchase","maxGold":40}}),
        json!({"type":"SetStanding","target":{"kind":"Unit","id":0},"rule":{"kind":"Clear"}}),
        json!({"type":"RevealRationale","tick":3,"policy":"bot/warlord@1","salt":"00112233445566778899aabbccddeeff","text":"攻撃は最大の防御"}),
        json!({"type":"ConsentWar","civ":2}),
        json!({"type":"ConsentSpend","usdc":25000000}),
        json!({"type":"OfferContract","to":3,"term":{"kind":"Peace"},"usdc":4000000,"deadline":60}),
        json!({"type":"OfferContract","to":2,"term":{"kind":"LeaveAlliance","with":5},"usdc":2500000,"deadline":40}),
        json!({"type":"OfferContract","to":1,"term":{"kind":"KeepNap","every":5,"installments":4},"usdc":8000000,"deadline":30}),
        json!({"type":"OfferContract","term":{"kind":"Capture","city":11},"usdc":3000000,"deadline":90}),
        json!({"type":"AcceptContract","id":7}),
        json!({"type":"CancelContract","id":8}),
    ];
    let govs: Vec<Value> = vec![
        json!({"type":"Stand","roles":["General","Diplomat"]}),
        json!({"type":"Vote","role":"Steward","candidate":7}),
        json!({"type":"Propose","role":"Science","orders":[{"type":"SetResearch","techs":["Writing"]}]}),
        json!({"type":"Support","proposal":12}),
        json!({"type":"Recall","role":"General"}),
    ];
    let mut gov_vectors = Vec::new();
    for g in &govs {
        let dto: GovDto = serde_json::from_value(g.clone()).expect("gov dto parses");
        let action = dto.to_action().expect("gov dto converts");
        assert_eq!(
            GovDto::from_action(&action).to_action().unwrap(),
            action,
            "round trip {g}"
        );
        gov_vectors.push(json!({"dto": g, "hex": hex(&borsh::to_vec(&action).unwrap())}));
    }
    let mut order_vectors = Vec::new();
    for o in &orders {
        let dto: OrderDto = serde_json::from_value(o.clone()).expect("dto parses");
        let order = dto.to_order().expect("dto converts");
        assert_eq!(
            OrderDto::from_order(&order).to_order().unwrap(),
            order,
            "round trip {o}"
        );
        order_vectors.push(json!({"dto": o, "hex": hex(&borsh::to_vec(&order).unwrap())}));
    }
    let k = |b: u8| [b; 32];
    let all: Vec<_> = orders
        .iter()
        .map(|o| {
            serde_json::from_value::<OrderDto>(o.clone())
                .unwrap()
                .to_order()
                .unwrap()
        })
        .collect();
    let gov_action = serde_json::from_value::<GovDto>(govs[2].clone())
        .unwrap()
        .to_action()
        .unwrap();
    let ixs = vec![
        (
            "createSeason",
            ChainInstruction::CreateSeason {
                season_id: 42,
                preset: 0,
                nations: 6,
                entry_fee: 10_000_000,
                tick_seconds: 30,
                world_seed: k(7),
                crank: k(9),
                market: true,
                prev_season_id: 41,
                ai_count: 3,
                roster_commit: k(12),
                bounty_each: 5_000_000,
                bond: 60_000_000,
                deposit: 1_000_000,
                start_by: 1_790_003_600,
                validator: k(17),
            },
        ),
        ("allocWorld", ChainInstruction::AllocWorld { chunk: 3 }),
        (
            "register",
            ChainInstruction::Register {
                civ: 2,
                name: "アステル".into(),
                kind: 1,
                session: k(1),
                attestation: k(0),
                stand: 5,
                votes: [0, u32::MAX, 3, u32::MAX],
                deposit: 5_000_000,
                tag: k(13),
            },
        ),
        ("startSeason", ChainInstruction::StartSeason),
        ("genesisStep", ChainInstruction::GenesisStep { work: 50 }),
        ("delegate", ChainInstruction::Delegate { target: 1003 }),
        (
            "submitOrders",
            ChainInstruction::SubmitOrders {
                role: Role::Steward,
                tick: 17,
                decision_digest: k(5),
                orders: all[..6].to_vec(),
                adopt: vec![4, 9],
            },
        ),
        ("resolveTick", ChainInstruction::ResolveTick { to: 12 }),
        ("commit", ChainInstruction::Commit),
        ("commitAndUndelegate", ChainInstruction::CommitAndUndelegate),
        ("finishSeason", ChainInstruction::FinishSeason),
        ("claim", ChainInstruction::Claim),
        (
            "undelegatePart",
            ChainInstruction::UndelegatePart {
                targets: vec![3, 1002, 0],
            },
        ),
        (
            "updateMember",
            ChainInstruction::UpdateMember {
                stand: 3,
                votes: [1, 1, u32::MAX, 2],
            },
        ),
        ("allocNation", ChainInstruction::AllocNation { civ: 5 }),
        ("seatMembers", ChainInstruction::SeatMembers),
        ("openGovernment", ChainInstruction::OpenGovernment),
        (
            "submitGov",
            ChainInstruction::SubmitGov {
                member: 11,
                action: gov_action,
            },
        ),
        ("withdrawOps", ChainInstruction::WithdrawOps),
        ("logTickInput", ChainInstruction::LogTickInput { chunk: 2 }),
        (
            "commitPart",
            ChainInstruction::CommitPart {
                targets: vec![1000, 1001, 7],
            },
        ),
        ("closeCommits", ChainInstruction::CloseCommits),
        (
            "commitOrders",
            ChainInstruction::CommitOrders {
                role: Role::Science,
                tick: 17,
                commitment: k(6),
            },
        ),
        (
            "revealOrders",
            ChainInstruction::RevealOrders {
                role: Role::Steward,
                tick: 17,
                decision_digest: k(5),
                orders: all[..6].to_vec(),
                adopt: vec![4, 9],
                salt: k(7),
            },
        ),
        (
            "revealRoster",
            ChainInstruction::RevealRoster {
                from: 1,
                salts: vec![k(14), k(15)],
                blind: k(18),
            },
        ),
        (
            "anchorTalk",
            ChainInstruction::AnchorTalk {
                tick: 33,
                count: 12,
                root: k(16),
            },
        ),
        ("startClock", ChainInstruction::StartClock),
        ("postBond", ChainInstruction::PostBond { amount: 7_000_000 }),
        ("freezeTick", ChainInstruction::FreezeTick),
        (
            "consumeTickRandomness",
            ChainInstruction::ConsumeTickRandomness {
                randomness: k(19),
                season_id: 42,
                tick: 17,
            },
        ),
        ("retryTickRandomness", ChainInstruction::RetryTickRandomness),
        (
            "consumeSeasonSeed",
            ChainInstruction::ConsumeSeasonSeed {
                randomness: k(20),
                season_id: 42,
            },
        ),
        ("retrySeasonSeed", ChainInstruction::RetrySeasonSeed),
        ("abort", ChainInstruction::Abort),
        (
            "requestUndelegation",
            ChainInstruction::RequestUndelegation { target: 1002 },
        ),
        (
            "rollbackUndelegation",
            ChainInstruction::RollbackUndelegation { target: 7 },
        ),
        (
            "closeSeasonAccounts",
            ChainInstruction::CloseSeasonAccounts {
                targets: vec![1, 2, 1000],
            },
        ),
    ];
    // Sealed orders (commit–reveal): the commitment of a batch and a salt,
    // which the JS client computes before `CommitOrders`.
    let sealed = permutation_rules::orders::OrderBatch {
        civ: 3,
        tick: 17,
        role: Role::Steward,
        member: 9,
        decision_digest: k(5),
        orders: all[..6].to_vec(),
        adopt: vec![4, 9],
    };
    let commitment = json!({
        "civ": sealed.civ, "tick": sealed.tick, "role": "Steward", "member": sealed.member,
        "decisionDigest": hex(&sealed.decision_digest), "orders": orders[..6].to_vec(), "adopt": sealed.adopt,
        "salt": hex(&k(7)),
        "batchHex": hex(&borsh::to_vec(&sealed).unwrap()),
        "commitment": hex(&permutation_rules::orders::order_commitment(&sealed, &k(7))),
    });
    let ix_vectors: Vec<Value> = ixs
        .iter()
        .map(|(name, ix)| json!({"name": name, "hex": hex(&borsh::to_vec(ix).unwrap())}))
        .collect();
    let office_vectors: Vec<Value> = orders
        .iter()
        .zip(&all)
        .map(|(dto, order)| {
            let roles: Vec<String> = Role::ALL
                .iter()
                .filter(|r| role_allows_static(**r, order))
                .map(|r| format!("{r:?}"))
                .collect();
            json!({"dto": dto, "offices": roles, "treasury": order.is_treasury_order(), "free": order.cost() == 0})
        })
        .collect();
    let errors: Vec<Value> = ChainError::ALL
        .iter()
        .map(|e| json!({"code": *e as u32, "name": e.name()}))
        .collect();
    let doc = json!({
        "orders": order_vectors, "gov": gov_vectors, "instructions": ix_vectors,
        "accounts": account_vectors(), "errors": errors, "constants": constant_vectors(),
        "offices": office_vectors, "claims": claim_vectors(), "claimRuleV9": claim_rule_v9(), "commitment": commitment,
        "roster": roster_vectors(), "govSlots": gov_slot_vectors(&govs), "govQuota": gov_quota_vectors(), "bond": bond_vectors(),
        "lifecycle": lifecycle_vectors(), "vrf": vrf_vectors(),
    });
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../permutation-gateway/test/vectors.json");
    let text = serde_json::to_string_pretty(&doc).unwrap() + "\n";
    if std::env::var("UPDATE_VECTORS").is_ok() {
        std::fs::write(&path, &text).unwrap();
    } else {
        let current = std::fs::read_to_string(&path)
            .expect("vectors.json exists (run with UPDATE_VECTORS=1)");
        assert_eq!(
            current, text,
            "Rust encoding changed: regenerate vectors and fix the JS encoder"
        );
    }
}
