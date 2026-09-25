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
//! * `claims`: inputs and outputs of the program's `claim_amount`.
//!
//! `UPDATE_VECTORS=1 cargo test --test codec_vectors` rewrites the file;
//! otherwise the file must match (so a Rust-side change fails loudly).

use borsh::BorshDeserialize;
use permutation_chain::error::ChainError;
use permutation_chain::instruction::ChainInstruction;
use permutation_chain::processor::{claim_amount, NATION_TARGET};
use permutation_chain::state::{
    MemberAccount, NationAccount, RosterAccount, RosterEntry, Season, SeasonStatus, WorldMeta,
    CHUNK, GENESIS_MAGIC, INPUT_CHUNK, MAX_AI, MAX_GOV_PER_SIGNER, MAX_MEMBERS, MAX_NAME,
    MAX_NATIONS, MEMBER_MAGIC, MEMBER_SEED, NATION_MAGIC, NATION_SEED, ROSTER_GRACE_SECONDS,
    ROSTER_MAGIC, ROSTER_SEED, SEASON_MAGIC, SEASON_SEED, VAULT_SEED, WORLD_CHUNKS, WORLD_HEADER,
    WORLD_MAGIC, WORLD_SEED,
};
use permutation_rules::decision::{MAX_POLICY, MAX_RATIONALE};
use permutation_rules::genesis::NATIONS;
use permutation_rules::gov::Role;
use permutation_rules::orders::role_allows_static;
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
        roster_chain: [16; 32],
        bounty_each: 5_000_000,
        bond: 60_000_000,
        roster_acc: [16; 32],
        roster_revealed: 3,
        roster_outcome: 1,
        bounty_paid: vec![0, 5_000_000, 0, 0, 0, 0],
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
            },
            RosterEntry {
                member: 2,
                civ: 1,
                salt: [18; 32],
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
        reveal_deadline: 1_790_000_140,
        committed: [17, u16::MAX, 17, 16],
        commits: [[20; 32], [0; 32], [21; 32], [22; 32]],
        salts: [[23; 32], [0; 32], [24; 32], [25; 32]],
        batches: [None, None, None, None],
        inbox: vec![],
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

fn account_vectors() -> Value {
    let s = sample_season();
    let ro = sample_roster();
    let m = sample_member();
    let n = sample_nation();
    let meta = sample_meta();
    json!({
        "season": {
            "hex": hex(&borsh::to_vec(&s).unwrap()),
            "decoded": {
                "seasonId": s.season_id.to_string(), "bump": s.bump, "vaultBump": s.vault_bump, "admin": hex(&s.admin), "crank": hex(&s.crank),
                "usdcMint": hex(&s.usdc_mint), "usdcDecimals": s.usdc_decimals, "preset": s.preset, "nations": s.nations,
                "entryFee": s.entry_fee.to_string(), "tickSeconds": s.tick_seconds, "market": s.market, "status": status_name(s.status),
                "worldSeed": hex(&s.world_seed), "seasonSeed": hex(&s.season_seed), "memberCount": s.member_count,
                "nationMembers": s.nation_members, "seated": s.seated, "pool": s.pool.to_string(), "ops": s.ops.to_string(),
                "opsWithdrawn": s.ops_withdrawn, "treasury": u64s(&s.treasury), "treasuryFinal": u64s(&s.treasury_final),
                "payouts": u64s(&s.payouts), "finalRoot": hex(&s.final_root),
                "prevSeasonId": s.prev_season_id.to_string(), "prevHistoryRoot": hex(&s.prev_history_root), "historyRoot": hex(&s.history_root),
                "aiCount": s.ai_count, "rosterChain": hex(&s.roster_chain), "bountyEach": s.bounty_each.to_string(), "bond": s.bond.to_string(),
                "rosterAcc": hex(&s.roster_acc), "rosterRevealed": s.roster_revealed, "rosterOutcome": "revealed", "bountyPaid": u64s(&s.bounty_paid),
            },
        },
        "roster": {
            "hex": hex(&borsh::to_vec(&ro).unwrap()),
            "decoded": {
                "seasonId": ro.season_id.to_string(), "bump": ro.bump,
                "entries": ro.entries.iter().map(|e| json!({"member": e.member, "civ": e.civ, "salt": hex(&e.salt)})).collect::<Vec<_>>(),
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
                "frozen": n.frozen, "revealing": n.revealing, "revealDeadline": n.reveal_deadline, "committed": n.committed,
                "commits": n.commits.iter().map(|k| hex(k)).collect::<Vec<_>>(),
                "salts": n.salts.iter().map(|k| hex(k)).collect::<Vec<_>>(),
            },
        },
        "worldHeader": {
            "hex": hex(&world_header(&meta, 23_456)),
            "decoded": {
                "magic": text(&WORLD_MAGIC), "len": 23_456, "bodyOffset": WORLD_HEADER,
                "meta": {
                    "seasonId": meta.season_id.to_string(), "preset": meta.preset, "civs": meta.civs, "tickSeconds": meta.tick_seconds,
                    "deadline": meta.deadline, "finished": meta.finished, "market": meta.market, "frozen": meta.frozen,
                    "vrf": hex(&meta.vrf), "inputChunks": meta.input_chunks, "inputLogged": meta.input_logged,
                    "revealing": meta.revealing,
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
        "SEEDS": { "season": text(SEASON_SEED), "world": text(WORLD_SEED), "nation": text(NATION_SEED), "member": text(MEMBER_SEED), "vault": text(VAULT_SEED), "roster": text(ROSTER_SEED) },
        "MAGIC": { "season": text(&SEASON_MAGIC), "member": text(&MEMBER_MAGIC), "nation": text(&NATION_MAGIC), "world": text(&WORLD_MAGIC), "genesis": text(&GENESIS_MAGIC), "roster": text(&ROSTER_MAGIC) },
        "NATIONS": NATIONS,
        "ROLES": Role::ALL.iter().map(|r| format!("{r:?}")).collect::<Vec<_>>(),
        "SEASON_STATUS": statuses,
    })
}

/// Operator AI roster tags and their chain (V5 §18.2), as the gateway computes them.
fn roster_vectors() -> Value {
    use permutation_rules::roster::{roster_chain, roster_tag};
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
    })
}

/// (payouts, treasury, treasury_final, member index, civ, shares)
type ClaimCase = (Vec<u64>, Vec<u64>, Vec<u64>, u32, u16, u64);

fn claim_vectors() -> Value {
    let base = sample_season();
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
    let out: Vec<Value> = cases
        .into_iter()
        .map(|(payouts, treasury, treasury_final, index, civ, shares)| {
            let season = Season { payouts: payouts.clone(), treasury: treasury.clone(), treasury_final: treasury_final.clone(), ..base.clone() };
            let member = MemberAccount { index, civ, shares, ..sample_member() };
            json!({
                "payouts": u64s(&payouts), "treasury": u64s(&treasury), "treasuryFinal": u64s(&treasury_final),
                "index": index, "civ": civ, "shares": shares.to_string(), "amount": claim_amount(&season, &member).to_string(),
            })
        })
        .collect();
    Value::Array(out)
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
                roster_chain: k(12),
                bounty_each: 5_000_000,
                bond: 60_000_000,
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
                salts: vec![k(14), k(15)],
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
            json!({"dto": dto, "offices": roles})
        })
        .collect();
    let errors: Vec<Value> = ChainError::ALL
        .iter()
        .map(|e| json!({"code": *e as u32, "name": e.name()}))
        .collect();
    let doc = json!({
        "orders": order_vectors, "gov": gov_vectors, "instructions": ix_vectors,
        "accounts": account_vectors(), "errors": errors, "constants": constant_vectors(),
        "offices": office_vectors, "claims": claim_vectors(), "commitment": commitment, "roster": roster_vectors(),
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
