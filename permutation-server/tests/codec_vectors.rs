//! Byte-level vectors for the JS encoder in permutation-gateway: every order
//! DTO variant and every program instruction, encoded by the Rust types.
//! `UPDATE_VECTORS=1 cargo test --test codec_vectors` rewrites the file;
//! otherwise the file must match (so a Rust-side change fails loudly).

use permutation_chain::instruction::ChainInstruction;
use permutation_server::api::OrderDto;
use serde_json::{json, Value};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
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
    ];
    let mut order_vectors = Vec::new();
    for o in &orders {
        let dto: OrderDto = serde_json::from_value(o.clone()).expect("dto parses");
        let order = dto.to_order().expect("dto converts");
        assert_eq!(OrderDto::from_order(&order).to_order().unwrap(), order, "round trip {o}");
        order_vectors.push(json!({"dto": o, "hex": hex(&borsh::to_vec(&order).unwrap())}));
    }
    let k = |b: u8| [b; 32];
    let all: Vec<_> = orders.iter().map(|o| serde_json::from_value::<OrderDto>(o.clone()).unwrap().to_order().unwrap()).collect();
    let ixs = vec![
        ("createSeason", ChainInstruction::CreateSeason { season_id: 42, preset: 0, max_civs: 6, entry_fee: 10_000_000, exchange_credit: 20_000_000, tick_seconds: 30, world_seed: k(7), crank: k(9) }),
        ("allocWorld", ChainInstruction::AllocWorld { chunk: 3 }),
        ("joinSeason", ChainInstruction::JoinSeason { name: "アステル".into(), kind: 1, session: k(1), payout: k(2) }),
        ("startSeason", ChainInstruction::StartSeason),
        ("genesisStep", ChainInstruction::GenesisStep { work: 50 }),
        ("delegate", ChainInstruction::Delegate { target: 3 }),
        ("submitOrders", ChainInstruction::SubmitOrders { tick: 17, decision_digest: k(5), orders: all[..6].to_vec() }),
        ("resolveTick", ChainInstruction::ResolveTick { to: 12 }),
        ("commit", ChainInstruction::Commit),
        ("commitAndUndelegate", ChainInstruction::CommitAndUndelegate),
        ("finishSeason", ChainInstruction::FinishSeason),
        ("claim", ChainInstruction::Claim { civ: 4 }),
        ("undelegatePart", ChainInstruction::UndelegatePart { targets: vec![3, 1002, 0] }),
    ];
    let ix_vectors: Vec<Value> = ixs.iter().map(|(name, ix)| json!({"name": name, "hex": hex(&borsh::to_vec(ix).unwrap())})).collect();
    let doc = json!({"orders": order_vectors, "instructions": ix_vectors});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../permutation-gateway/test/vectors.json");
    let text = serde_json::to_string_pretty(&doc).unwrap() + "\n";
    if std::env::var("UPDATE_VECTORS").is_ok() {
        std::fs::write(&path, &text).unwrap();
    } else {
        let current = std::fs::read_to_string(&path).expect("vectors.json exists (run with UPDATE_VECTORS=1)");
        assert_eq!(current, text, "Rust encoding changed: regenerate vectors and fix the JS encoder");
    }
}
