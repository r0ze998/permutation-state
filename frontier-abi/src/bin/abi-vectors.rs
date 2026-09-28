//! `abi-vectors`: writes `frontier-abi/vectors/*.json` from the ABI tables
//! (the only producer of these files, M1 contract §3.5).
//!
//! ```text
//! cargo run -p frontier-abi --bin abi-vectors              # (re)write
//! cargo run -p frontier-abi --bin abi-vectors -- --check   # exit 1 if stale
//! ```
//!
//! Consumers: `permutation-gateway/client/src/frontier/` (JS codec,
//! addresses, fees, budgets), `web-frontier-codec.test.mjs`,
//! `frontier-node` (`fclient`), the verifier.

use frontier_abi::addr::{self, AddrCtx, Seed};
use frontier_abi::budgets;
use frontier_abi::entry::{Entry, EntryOp};
use frontier_abi::error::FrontierError;
use frontier_abi::ix;
use frontier_abi::layout::{self, AccountKind, Field};
use frontier_abi::log::{self, Kind};
use frontier_abi::presets::{self, SeasonParams};
use frontier_abi::prologue::{self, Acc, Wr};
use frontier_abi::tags::{self, Ix};
use permutation_rules::hash::sha256;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

// ------------------------------------------------------------ tiny JSON

fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(2 * b.len());
    for x in b {
        let _ = write!(s, "{x:02x}");
    }
    s
}

fn b58(b: &[u8]) -> String {
    const A: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let zeros = b.iter().take_while(|x| **x == 0).count();
    let mut digits: Vec<u8> = Vec::new();
    for &byte in b {
        let mut carry = byte as u32;
        for d in digits.iter_mut() {
            carry += (*d as u32) << 8;
            *d = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let mut s = "1".repeat(zeros);
    for d in digits.iter().rev() {
        s.push(A[*d as usize] as char);
    }
    s
}

fn q(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// A JSON value with deterministic formatting.
enum J {
    N(i128),
    S(String),
    B(bool),
    A(Vec<J>),
    O(Vec<(String, J)>),
    Null,
}

fn n<T: Into<i128>>(x: T) -> J {
    J::N(x.into())
}
fn st(x: &str) -> J {
    J::S(x.to_string())
}
fn o(v: Vec<(&str, J)>) -> J {
    J::O(v.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

impl J {
    fn write(&self, out: &mut String, ind: usize) {
        let pad = |out: &mut String, i: usize| out.push_str(&"  ".repeat(i));
        match self {
            J::N(x) => {
                let _ = write!(out, "{x}");
            }
            J::S(s) => out.push_str(&q(s)),
            J::B(b) => out.push_str(if *b { "true" } else { "false" }),
            J::Null => out.push_str("null"),
            J::A(v) => {
                if v.iter().all(|x| matches!(x, J::N(_) | J::S(_) | J::B(_))) && v.len() <= 16 {
                    out.push('[');
                    for (i, x) in v.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        x.write(out, 0);
                    }
                    out.push(']');
                    return;
                }
                out.push_str("[\n");
                for (i, x) in v.iter().enumerate() {
                    pad(out, ind + 1);
                    x.write(out, ind + 1);
                    out.push_str(if i + 1 < v.len() { ",\n" } else { "\n" });
                }
                pad(out, ind);
                out.push(']');
            }
            J::O(v) => {
                if v.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push_str("{\n");
                for (i, (k, x)) in v.iter().enumerate() {
                    pad(out, ind + 1);
                    out.push_str(&q(k));
                    out.push_str(": ");
                    x.write(out, ind + 1);
                    out.push_str(if i + 1 < v.len() { ",\n" } else { "\n" });
                }
                pad(out, ind);
                out.push('}');
            }
        }
    }
    fn render(&self) -> String {
        let mut s = String::new();
        self.write(&mut s, 0);
        s.push('\n');
        s
    }
}

fn header(what: &str) -> Vec<(&'static str, J)> {
    vec![
        ("generator", st("frontier-abi abi-vectors")),
        ("abi_version", n(frontier_abi::ABI_VERSION)),
        ("contract", st("docs/frontier/m1/M1-CONTRACT.md v1.8")),
        ("content", st(what)),
    ]
}

fn fields(f: &[Field]) -> J {
    J::A(
        f.iter()
            .map(|x| {
                o(vec![
                    ("name", st(x.name)),
                    ("off", n(x.off as u64)),
                    ("len", n(x.len as u64)),
                    ("ty", st(x.ty)),
                ])
            })
            .collect(),
    )
}

// ------------------------------------------------------------ files

fn layouts() -> J {
    let mut v = header("account layouts (§4.3, §5.3), sub-records, SeasonParams");
    v.push((
        "accounts",
        J::A(
            AccountKind::ALL
                .iter()
                .map(|k| {
                    o(vec![
                        ("kind", st(k.name())),
                        ("code", n(*k as u8)),
                        ("magic", st(std::str::from_utf8(&k.magic()).unwrap_or("?"))),
                        ("size", n(k.size() as u64)),
                        ("rent", n(k.rent())),
                        ("chained", J::B(k.chained())),
                        (
                            "seed_tag",
                            addr::tag_of(*k)
                                .map(|t| st(std::str::from_utf8(&t).unwrap_or("?")))
                                .unwrap_or(J::Null),
                        ),
                        ("raw_key_len", n(addr::raw_len(*k) as u64)),
                        ("fields", fields(k.fields())),
                    ])
                })
                .collect(),
        ),
    ));
    v.push((
        "records",
        J::A(
            layout::RECORDS
                .iter()
                .map(|(name, size, f)| {
                    o(vec![
                        ("name", st(name)),
                        ("size", n(*size as u64)),
                        ("fields", fields(f)),
                    ])
                })
                .collect(),
        ),
    ));
    v.push((
        "season_params",
        o(vec![
            ("size", n(presets::SEASON_PARAMS_LEN as u64)),
            ("fields", fields(presets::layout::FIELDS)),
        ]),
    ));
    v.push(("rent_per_byte", n(layout::RENT_PER_BYTE)));
    v.push((
        "season_tombstone_size",
        n(layout::world::season::TOMBSTONE_SIZE as u64),
    ));
    v.push(("reserved_magic", st("PSF1SVRD")));
    J::O(v.into_iter().map(|(k, x)| (k.to_string(), x)).collect())
}

fn acc_name(a: Acc) -> String {
    match a {
        Acc::Kind(k) => k.name().to_string(),
        Acc::Either(a, b) => format!("{}|{}", a.name(), b.name()),
        Acc::Wallet => "wallet".into(),
        Acc::System => "system".into(),
        Acc::IxSysvar => "instructions_sysvar".into(),
        Acc::ProgramAccount => "program".into(),
        Acc::ProgramData => "programdata".into(),
        Acc::Incinerator => "incinerator".into(),
        Acc::Any => "any".into(),
    }
}

fn tags_file() -> J {
    let mut v = header("instruction tags, classes, data layouts and account lists (§5.5–§5.12)");
    v.push((
        "instructions",
        J::A(
            Ix::ALL
                .iter()
                .map(|i| {
                    let (lo, hi) = ix::data_len_range(*i);
                    let wire = ix::wire_of(*i)
                        .map(|w| {
                            J::A(
                                w.iter()
                                    .map(|(f, l)| o(vec![("name", st(f)), ("len", n(*l as u64))]))
                                    .collect(),
                            )
                        })
                        .unwrap_or(J::Null);
                    let groups = prologue::accounts_of(*i)
                        .iter()
                        .map(|g| {
                            o(vec![
                                ("min", n(g.min)),
                                ("max", n(g.max)),
                                (
                                    "accounts",
                                    J::A(
                                        g.specs
                                            .iter()
                                            .map(|s| {
                                                o(vec![
                                                    ("name", st(s.name)),
                                                    ("kind", J::S(acc_name(s.acc))),
                                                    ("signer", J::B(s.signer)),
                                                    (
                                                        "writable",
                                                        st(match s.wr {
                                                            Wr::R => "r",
                                                            Wr::W => "w",
                                                            Wr::Either => "r|w",
                                                        }),
                                                    ),
                                                ])
                                            })
                                            .collect(),
                                    ),
                                ),
                            ])
                        })
                        .collect();
                    o(vec![
                        ("name", st(i.name())),
                        ("tag", n(i.tag())),
                        ("class", st(i.class().name())),
                        ("top_level_only", J::B(i.top_level_only())),
                        ("relay_player_shape", J::B(tags::relay_player_shape(*i))),
                        ("relay_settle_shape", J::B(tags::relay_settle_shape(*i))),
                        ("data_len", J::A(vec![n(lo as u64), n(hi as u64)])),
                        ("data", wire),
                        ("account_groups", J::A(groups)),
                    ])
                })
                .collect(),
        ),
    ));
    v.push((
        "reserved_tags",
        J::A(
            (0u8..=255)
                .filter(|t| tags::is_reserved(*t))
                .map(n)
                .collect(),
        ),
    ));
    v.push((
        "constants",
        o(vec![
            ("sig48_len", n(ix::SIG48_LEN as u64)),
            ("hints_len", n(ix::HINTS_LEN as u64)),
            ("seal_len", n(ix::SEAL_LEN as u64)),
            ("plain_len", n(ix::PLAIN_LEN as u64)),
            ("multi_max_regions", n(budgets::MULTI_MAX_REGIONS as u64)),
        ]),
    ));
    J::O(v.into_iter().map(|(k, x)| (k.to_string(), x)).collect())
}

fn errors_file() -> J {
    let mut v = header("program error codes (§5.4), stable forever; keeper actions (offchain P8)");
    v.push((
        "errors",
        J::A(
            FrontierError::ALL
                .iter()
                .map(|e| {
                    o(vec![
                        ("code", n(e.code())),
                        ("name", st(e.name())),
                        ("program", J::B(e.program_code())),
                        (
                            "keeper",
                            st(match frontier_abi::error::keeper_action(*e) {
                                frontier_abi::error::KeeperAction::Success => "success",
                                frontier_abi::error::KeeperAction::RetrySlots => "retry-slots",
                                frontier_abi::error::KeeperAction::Stop => "stop",
                                frontier_abi::error::KeeperAction::Wait => "wait",
                                frontier_abi::error::KeeperAction::Refused => "refused",
                            }),
                        ),
                    ])
                })
                .collect(),
        ),
    ));
    J::O(v.into_iter().map(|(k, x)| (k.to_string(), x)).collect())
}

/// Deterministic sample bytes for one log field.
fn sample_field(kind: Kind, name: &str, width: usize, salt: u8) -> Vec<u8> {
    let host = addr::host_id(2, -1, 3, 1, 42).unwrap_or(0);
    match (name, width) {
        ("host_id" | "displaced_host", 8) => host.to_le_bytes().to_vec(),
        ("p" | "origin_p", 4) => 2i32.to_le_bytes().to_vec(),
        ("q" | "origin_q", 4) => (-1i32).to_le_bytes().to_vec(),
        ("site", 1) => vec![3],
        ("account_kind", 1) => vec![AccountKind::Province as u8],
        ("key", 15) if kind == Kind::CLOSE => {
            let mut k = vec![0u8; 15];
            k[..4].copy_from_slice(&2i32.to_le_bytes());
            k[4..8].copy_from_slice(&(-1i32).to_le_bytes());
            k
        }
        ("faction", 1) => vec![4],
        ("shard", 1) => vec![6],
        ("n", 1) if kind == Kind::TICKET => vec![2],
        ("sites", 15) => {
            let mut s = vec![0u8; 15];
            s[..2].copy_from_slice(&2i16.to_le_bytes());
            s[2..4].copy_from_slice(&(-1i16).to_le_bytes());
            s[4] = 3;
            s[5..7].copy_from_slice(&3i16.to_le_bytes());
            s[7..9].copy_from_slice(&(-2i16).to_le_bytes());
            s[9] = 0;
            s
        }
        ("outcome", 1) if kind == Kind::SETTLE => vec![log::settle_outcome::DISPLACE],
        ("outcome", 1) if kind == Kind::TRANSIT_SETTLED => vec![log::transit_outcome::STAYS],
        _ => (0..width)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(salt))
            .collect(),
    }
}

fn logs_file() -> J {
    let mut v = header("PS2 log records (§6): kinds, field widths, one encoded vector per kind with its chain heads");
    v.push(("prefix", st("PS2")));
    v.push(("version", n(log::VERSION)));
    v.push((
        "tail_order",
        st("ascending entity_kind; entities of one kind in instruction account order"),
    ));
    v.push((
        "entity_kinds",
        J::A(
            [1u8, 2, 3, 4, 5, 6, 7]
                .iter()
                .filter_map(|e| log::EntityKind::from_u8(*e))
                .map(|e| {
                    o(vec![
                        ("code", n(e as u8)),
                        ("account", st(e.account_kind().name())),
                    ])
                })
                .collect(),
        ),
    ));
    let mut kinds = Vec::new();
    for spec in log::SPECS {
        let mut key = Vec::new();
        for (f, w) in spec.key {
            key.extend(sample_field(spec.kind, f, *w, 0x11));
        }
        let mut payload = Vec::new();
        for (f, w) in spec.payload {
            payload.extend(sample_field(spec.kind, f, *w, 0x5a));
        }
        let bell = 1_000 + spec.kind as u32;
        let mut buf = vec![0u8; 1024];
        let len = log::write_body(spec.kind, bell, &key, &payload, &mut buf).unwrap_or(0);
        let bwt = buf[..len].to_vec();
        let chains = log::chains_of(spec.kind, &key, &payload);
        let mut links = Vec::new();
        let mut chain_json = Vec::new();
        if let Some(c) = chains {
            for (i, ce) in c.iter().filter(|c| !c.optional).enumerate() {
                let prev_head = sha256(&[b"abi-vectors prev", &[spec.kind as u8, i as u8]]);
                let prev_seq = 5 + i as u64;
                if let Some(l) = log::advance(ce.entity, prev_seq, &prev_head, &bwt) {
                    links.push(l);
                    chain_json.push(o(vec![
                        ("entity", n(ce.entity as u8)),
                        ("who", J::S(format!("{:?}", ce.who))),
                        ("prev_seq", n(prev_seq)),
                        ("prev_head", J::S(hex(&prev_head))),
                        ("seq", n(l.seq)),
                        ("head", J::S(hex(&l.head))),
                    ]));
                }
            }
            for ce in c.iter().filter(|c| c.optional) {
                chain_json.push(o(vec![
                    ("entity", n(ce.entity as u8)),
                    ("who", J::S(format!("{:?}", ce.who))),
                    ("optional", J::B(true)),
                ]));
            }
        }
        let end = log::write_tail(&links, &mut buf, len).unwrap_or(len);
        let body = buf[..end].to_vec();
        let decoded_ok = log::decode(&body).is_ok();
        kinds.push(o(vec![
            ("kind", n(spec.kind as u8)),
            ("name", st(spec.name)),
            (
                "key",
                J::A(
                    spec.key
                        .iter()
                        .map(|(f, w)| o(vec![("name", st(f)), ("len", n(*w as u64))]))
                        .collect(),
                ),
            ),
            (
                "payload",
                J::A(
                    spec.payload
                        .iter()
                        .map(|(f, w)| o(vec![("name", st(f)), ("len", n(*w as u64))]))
                        .collect(),
                ),
            ),
            ("body_without_tail_len", n(spec.body_len() as u64)),
            (
                "vector",
                o(vec![
                    ("bell", n(bell)),
                    ("body_without_tail", J::S(hex(&bwt))),
                    ("body", J::S(hex(&body))),
                    ("decodes", J::B(decoded_ok)),
                    ("chains", J::A(chain_json)),
                ]),
            ),
        ]));
    }
    v.push(("kinds", J::A(kinds)));
    let fates: [u8; 24] = core::array::from_fn(|k| (k % 6) as u8);
    v.push((
        "fates_vector",
        o(vec![
            ("fates", J::A(fates.iter().map(|x| n(*x)).collect())),
            ("packed", J::S(hex(&log::pack_fates(&fates)))),
        ]),
    ));
    J::O(v.into_iter().map(|(k, x)| (k.to_string(), x)).collect())
}

fn budgets_file() -> J {
    let mut v = header(
        "budgets (§5.5, §10.1, §10.2): wave-1 placeholders; L(kind) from the worst account sets",
    );
    v.push((
        "constants",
        o(vec![
            ("heap_gate", n(budgets::HEAP_GATE)),
            ("heap_frame", n(budgets::HEAP_FRAME)),
            ("cu_ladder_max", n(budgets::CU_LADDER_MAX)),
            ("locks_max", n(budgets::LOCKS_MAX)),
            ("tx_max", n(budgets::TX_MAX)),
            (
                "loaded_limit_working_default",
                n(budgets::LOADED_LIMIT_WORKING_DEFAULT),
            ),
            ("placeholder_so_len", n(budgets::PLACEHOLDER_SO_LEN)),
            (
                "placeholder_programdata_len",
                n(budgets::PLACEHOLDER_PROGRAMDATA_LEN),
            ),
            ("account_overhead", n(budgets::ACCOUNT_OVERHEAD)),
            ("programdata_meta", n(budgets::PROGRAMDATA_META)),
            ("multi_max_regions", n(budgets::MULTI_MAX_REGIONS as u64)),
        ]),
    ));
    v.push((
        "instructions",
        J::A(
            Ix::ALL
                .iter()
                .map(|i| {
                    let b = budgets::budget(*i);
                    let (lo, hi) = prologue::count_bounds(*i);
                    o(vec![
                        ("name", st(i.name())),
                        ("tag", n(i.tag())),
                        ("class", st(i.class().name())),
                        ("cu_budget", n(b.cu_budget)),
                        ("cu_per_unit", n(b.cu_per_unit)),
                        ("cu_limit", n(b.cu_limit)),
                        ("tx_contract", n(b.tx_contract)),
                        ("tx_ceiling", n(budgets::tx_ceiling(*i))),
                        ("tx_worst_estimate", n(budgets::tx_worst_estimate(*i))),
                        ("builder_limited", J::B(budgets::builder_limited(*i))),
                        ("accounts", J::A(vec![n(lo as u64), n(hi as u64)])),
                        ("write_locks_worst", n(budgets::write_locks(*i))),
                        (
                            "loaded_need_placeholder",
                            n(budgets::loaded_need(
                                *i,
                                budgets::PLACEHOLDER_PROGRAMDATA_LEN,
                            )),
                        ),
                        ("loaded_limit", n(budgets::loaded_limit(*i))),
                        ("heap", n(budgets::HEAP_GATE)),
                    ])
                })
                .collect(),
        ),
    ));
    J::O(v.into_iter().map(|(k, x)| (k.to_string(), x)).collect())
}

fn test_ctx() -> AddrCtx {
    AddrCtx {
        season: sha256(&[b"frontier-abi vectors: season pda"]),
        program: sha256(&[b"frontier-abi vectors: program id"]),
    }
}

fn addr_entry(ctx: &AddrCtx, kind: &str, key: J, seed: &Seed) -> J {
    let a = ctx.of(seed);
    o(vec![
        ("kind", st(kind)),
        ("key", key),
        (
            "seed",
            st(std::str::from_utf8(seed.as_bytes()).unwrap_or("?")),
        ),
        ("address_hex", J::S(hex(&a))),
        ("address", J::S(b58(&a))),
    ])
}

fn addresses_file() -> J {
    let ctx = test_ctx();
    let mut v = header("full with-seed addresses (§4.1) for a fixed season PDA and program id");
    v.push((
        "season_pda",
        o(vec![
            ("hex", J::S(hex(&ctx.season))),
            ("b58", J::S(b58(&ctx.season))),
        ]),
    ));
    v.push((
        "program_id",
        o(vec![
            ("hex", J::S(hex(&ctx.program))),
            ("b58", J::S(b58(&ctx.program))),
        ]),
    ));
    let wallet = sha256(&[b"frontier-abi vectors: wallet"]);
    let keeper = sha256(&[b"frontier-abi vectors: keeper"]);
    let mut e = Vec::new();
    e.push(addr_entry(
        &ctx,
        "Frontier",
        J::Null,
        &addr::frontier_seed(),
    ));
    e.push(addr_entry(
        &ctx,
        "DefencePool",
        J::Null,
        &addr::defence_pool_seed(),
    ));
    for d in [0u16, 3, u16::MAX] {
        e.push(addr_entry(
            &ctx,
            "RingSeed",
            o(vec![("d", n(d))]),
            &addr::ring_seed_seed(d),
        ));
    }
    for w in [0u8, 5] {
        e.push(addr_entry(
            &ctx,
            "ProvinceFund",
            o(vec![("w", n(w))]),
            &addr::province_fund_seed(w),
        ));
    }
    for (f, s) in [(0u8, 0u8), (5, 7)] {
        e.push(addr_entry(
            &ctx,
            "JoinShard",
            o(vec![("faction", n(f)), ("shard", n(s))]),
            &addr::join_shard_seed(f, s),
        ));
    }
    for r in [0u8, 15] {
        e.push(addr_entry(
            &ctx,
            "BeaconLog",
            o(vec![("region", n(r))]),
            &addr::beacon_log_seed(r),
        ));
    }
    let t15 = addr::citizen_tag15(&wallet);
    e.push(addr_entry(
        &ctx,
        "Citizen",
        o(vec![
            ("wallet", J::S(b58(&wallet))),
            ("tag15", J::S(hex(&t15))),
        ]),
        &addr::citizen_seed(&t15),
    ));
    let pqs = [(0i32, 0i32), (2, -1), (-128, 64), (i32::MIN, i32::MAX)];
    for (p, q) in pqs {
        e.push(addr_entry(
            &ctx,
            "Province",
            o(vec![("p", n(p)), ("q", n(q))]),
            &addr::province_seed(p, q),
        ));
        e.push(addr_entry(
            &ctx,
            "Holding",
            o(vec![("p", n(p)), ("q", n(q)), ("site", n(11u8))]),
            &addr::holding_seed(p, q, 11),
        ));
    }
    for (p, q, b) in [(2i32, -1i32, 0u32), (i32::MIN, i32::MAX, u32::MAX)] {
        e.push(addr_entry(
            &ctx,
            "ArrivalSlot",
            o(vec![
                ("p", n(p)),
                ("q", n(q)),
                ("bell", n(b)),
                ("faction", n(5u8)),
                ("i", n(3u8)),
            ]),
            &addr::arrival_slot_seed(p, q, b, 5, 3),
        ));
        e.push(addr_entry(
            &ctx,
            "ArrivalDay",
            o(vec![("p", n(p)), ("q", n(q)), ("day", n(b))]),
            &addr::arrival_day_seed(p, q, b),
        ));
        e.push(addr_entry(
            &ctx,
            "ClashInputs",
            o(vec![("p", n(p)), ("q", n(q)), ("bell", n(b))]),
            &addr::clash_inputs_seed(p, q, b),
        ));
        e.push(addr_entry(
            &ctx,
            "PosturePDA (reserved, M3)",
            o(vec![
                ("p", n(p)),
                ("q", n(q)),
                ("bell", n(b)),
                ("pos", n(59u8)),
            ]),
            &addr::posture_seed(p, q, b, 59),
        ));
    }
    for (b, r) in [(0u32, 0u8), (u32::MAX, 15)] {
        e.push(addr_entry(
            &ctx,
            "BellAnchor",
            o(vec![("bell", n(b)), ("region", n(r))]),
            &addr::bell_anchor_seed(b, r),
        ));
        e.push(addr_entry(
            &ctx,
            "SeedCache",
            o(vec![("bell", n(b)), ("region", n(r)), ("nonce", n(255u8))]),
            &addr::seed_cache_seed(b, r, 255),
        ));
        e.push(addr_entry(
            &ctx,
            "AnchorArchive",
            o(vec![("region", n(r)), ("part", n(b))]),
            &addr::anchor_archive_seed(r, b),
        ));
    }
    e.push(addr_entry(
        &ctx,
        "SealVerdict (reserved, removed v1.1)",
        o(vec![
            ("host_id", J::S(u64::MAX.to_string())),
            ("arrive_bell", n(7u32)),
        ]),
        &addr::seal_verdict_seed(u64::MAX, 7),
    ));
    let kt = addr::keeper_tag8(&keeper);
    e.push(addr_entry(
        &ctx,
        "DefenceClaim",
        o(vec![
            ("beneficiary", J::S(b58(&keeper))),
            ("keeper_tag8", J::S(hex(&kt))),
            ("day", n(6u32)),
        ]),
        &addr::defence_claim_seed(&kt, 6),
    ));
    v.push(("accounts", J::A(e)));
    // host ids (u64 as decimal strings: JS numbers are 53-bit)
    let hosts = [
        (0i32, 0i32, 0u8, 0u8, 0u32),
        (2, -1, 3, 1, 42),
        (128, 0, 11, 255, u32::MAX),
        (-64, -64, 5, 7, 12_345),
    ];
    v.push((
        "host_ids",
        J::A(
            hosts
                .iter()
                .filter_map(|&(p, q, site, gen, seq)| {
                    let id = addr::host_id(p, q, site, gen, seq)?;
                    Some(o(vec![
                        ("p", n(p)),
                        ("q", n(q)),
                        ("site", n(site)),
                        ("gen", n(gen)),
                        ("seq", n(seq)),
                        ("host_id", J::S(id.to_string())),
                        ("holding_address", J::S(b58(&ctx.holding(p, q, site)))),
                    ]))
                })
                .collect(),
        ),
    ));
    let cit = ctx.citizen(&wallet);
    v.push((
        "tags",
        o(vec![
            ("wallet", J::S(b58(&wallet))),
            ("citizen_address", J::S(b58(&cit))),
            ("citizen_tag_u64", J::S(addr::citizen_tag(&cit).to_string())),
            ("join_shard", n(addr::join_shard_of(&wallet))),
        ]),
    ));
    J::O(v.into_iter().map(|(k, x)| (k.to_string(), x)).collect())
}

fn params_json(p: &SeasonParams) -> J {
    let b = p.to_bytes();
    let mut fields_v = Vec::new();
    for f in presets::layout::FIELDS {
        if f.ty == "rsv" {
            continue;
        }
        let bytes = &b[f.off..f.off + f.len];
        let val = match f.ty {
            "u8" => n(bytes[0]),
            "u16" => n(u16::from_le_bytes([bytes[0], bytes[1]])),
            "u32" => n(u32::from_le_bytes(bytes.try_into().unwrap_or([0; 4]))),
            "u64" => J::S(u64::from_le_bytes(bytes.try_into().unwrap_or([0; 8])).to_string()),
            "i64" => J::S(i64::from_le_bytes(bytes.try_into().unwrap_or([0; 8])).to_string()),
            _ => J::S(hex(bytes)),
        };
        fields_v.push((f.name.to_ascii_lowercase(), val));
    }
    J::O(fields_v)
}

/// Borsh of `PayoutParams` (plain integers: LE in field order).
/// The borsh derive's encoding (integ-W1 review: no hand copy, so a field
/// added or retyped in PayoutParams moves the vector).
fn payout_borsh(p: &permutation_rules::frontier::payout::PayoutParams) -> Vec<u8> {
    p.to_borsh()
}

fn presets_file() -> J {
    let mut v = header("SeasonParams presets (§5.7) and the announced params hash");
    let payout = payout_borsh(&permutation_rules::frontier::payout::PayoutParams::REV3);
    v.push((
        "quicknet",
        o(vec![
            ("genesis", n(presets::QUICKNET_GENESIS)),
            ("period", n(presets::QUICKNET_PERIOD)),
            ("public_key", J::S(hex(&presets::QUICKNET_PUBLIC_KEY))),
            ("pk_hash", J::S(hex(&presets::QUICKNET_PK_HASH))),
        ]),
    ));
    v.push(("ruleset_hash", J::S(hex(&presets::RULESET_HASH))));
    v.push(("payout_params_rev3_borsh", J::S(hex(&payout))));
    let mut ps = Vec::new();
    for (name, p) in [
        ("M1_LOCAL_7D", presets::M1_LOCAL_7D),
        ("M1_PLAYTEST", presets::M1_PLAYTEST),
    ] {
        let b = p.to_bytes();
        ps.push(o(vec![
            ("name", st(name)),
            ("valid", J::B(p.validate().is_ok())),
            ("fields", params_json(&p)),
            ("bytes", J::S(hex(&b))),
            (
                "params_hash_with_rev3_payout",
                J::S(hex(&presets::params_hash(&b, &payout))),
            ),
        ]));
    }
    v.push(("presets", J::A(ps)));
    J::O(v.into_iter().map(|(k, x)| (k.to_string(), x)).collect())
}

fn ix_file() -> J {
    let mut v = header("one encoded sample per instruction (data with the tag, hex)");
    let b32 = |x: u8| [x; 32];
    let sig = [0xA1u8; 48];
    let hints = [0x3Cu8; ix::HINTS_LEN];
    let seal = [0x5Eu8; ix::SEAL_LEN];
    let mut out: Vec<(Ix, Vec<u8>)> = vec![
        (
            Ix::AnnounceSeason,
            ix::AnnounceSeason {
                id: 7,
                params_hash: b32(1),
                t_create_min: 1_800_000_000,
                bond: 1_000_000_000,
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::InitBeaconLogs,
            ix::InitBeaconLogs {}.to_bytes().to_vec(),
        ),
        (
            Ix::InitShards,
            ix::InitShards { faction: 5 }.to_bytes().to_vec(),
        ),
        (
            Ix::ConsumeGenesisSeed,
            ix::ConsumeGenesisSeed {
                round: 12_345_678,
                sig48: sig,
                hints,
            }
            .to_bytes()
            .to_vec(),
        ),
        (Ix::EndSeason, ix::EndSeason {}.to_bytes().to_vec()),
        (
            Ix::CloseSeason,
            ix::CloseSeason { part: 2 }.to_bytes().to_vec(),
        ),
        (Ix::AbortSeason, ix::AbortSeason {}.to_bytes().to_vec()),
        (
            Ix::SetWindowSchedule,
            ix::SetWindowSchedule {
                window: 900,
                from_bell: 300,
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::PostAnchor,
            ix::PostAnchor {
                region: 15,
                bell: 1_000,
                round: 99,
                sig48: sig,
                hints,
                beneficiary: b32(2),
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::PostAnchorMulti,
            ix::PostAnchorMulti {
                bell: 1_000,
                round: 99,
                sig48: sig,
                hints,
                mask: 0x007F,
                beneficiary: b32(2),
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::PostSeed,
            ix::PostSeed {
                region: 15,
                bell: 1_000,
                nonce: 3,
                round: 330,
                sig48: sig,
                hints,
                beneficiary: b32(2),
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::PostBeacon,
            ix::PostBeacon {
                region: 1,
                round: 400,
                sig48: sig,
                hints,
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::CloseSeedCache,
            ix::CloseSeedCache {
                bell: 1_000,
                region: 15,
                nonce: 3,
            }
            .to_bytes()
            .to_vec(),
        ),
        (Ix::OpenRing, ix::OpenRing { d: 4 }.to_bytes().to_vec()),
        (
            Ix::ConsumeRingSeed,
            ix::ConsumeRingSeed {
                d: 4,
                round: 500,
                sig48: sig,
                hints,
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::OpenProvince,
            ix::OpenProvince { p: 2, q: -1 }.to_bytes().to_vec(),
        ),
        (
            Ix::FoldOccupancy,
            ix::FoldOccupancy { part: 1 }.to_bytes().to_vec(),
        ),
        (
            Ix::CloseProvince,
            ix::CloseProvince { p: 2, q: -1 }.to_bytes().to_vec(),
        ),
        (
            Ix::Join,
            ix::Join {
                faction: 4,
                session: b32(9),
                session_expiry: 1_800_086_400,
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::SetSession,
            ix::SetSession {
                session: b32(9),
                expiry: 1_800_086_400,
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::SetVigil,
            ix::SetVigil { start_min: 1_380 }.to_bytes().to_vec(),
        ),
        (
            Ix::SettleTicket,
            ix::SettleTicket { k: 1 }.to_bytes().to_vec(),
        ),
        (
            Ix::ReleaseDormant,
            ix::ReleaseDormant {}.to_bytes().to_vec(),
        ),
        (Ix::CloseHolding, ix::CloseHolding {}.to_bytes().to_vec()),
        (Ix::CloseCitizen, ix::CloseCitizen {}.to_bytes().to_vec()),
        (Ix::Harvest, ix::Harvest {}.to_bytes().to_vec()),
        (Ix::Build, ix::Build { item: 2 }.to_bytes().to_vec()),
        (Ix::Train, ix::Train { unit: 5, n: 300 }.to_bytes().to_vec()),
        (
            Ix::Muster,
            ix::Muster {
                unit: 5,
                troops: 300,
                tile: 30,
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::Dissolve,
            ix::Dissolve {
                host_id: addr::host_id(2, -1, 3, 1, 42).unwrap_or(0),
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::Garrison,
            ix::Garrison { delta: -150 }.to_bytes().to_vec(),
        ),
        (
            Ix::Explore,
            ix::Explore {
                host_id: addr::host_id(2, -1, 3, 1, 43).unwrap_or(0),
                n: 1,
                tiles: [12, 0xFF],
            }
            .to_bytes()
            .to_vec(),
        ),
        (Ix::SettleExplore, ix::SettleExplore {}.to_bytes().to_vec()),
        (
            Ix::DisbandStranded,
            ix::DisbandStranded { entry: 55 }.to_bytes().to_vec(),
        ),
        (
            Ix::Depart,
            ix::Depart {
                host_id: addr::host_id(2, -1, 3, 1, 42).unwrap_or(0),
                commit: b32(3),
                seal,
                arrive_bell: 1_010,
                tip: 14_441,
                transit_slot: 3,
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::Reveal,
            ix::Reveal {
                transit_slot: 3,
                target_i: 2,
                plain: [0x2D; ix::PLAIN_LEN],
                salt: b32(4),
                ct_hash: b32(5),
                beneficiary: b32(2),
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::SettleDeparture,
            ix::SettleDeparture { transit_slot: 3 }.to_bytes().to_vec(),
        ),
        (
            Ix::SettleTransit,
            ix::SettleTransit {
                transit_slot: 3,
                commit: b32(3),
                seal,
                beneficiary: b32(2),
            }
            .to_bytes()
            .to_vec(),
        ),
        (Ix::SweepPoolOwed, ix::SweepPoolOwed {}.to_bytes().to_vec()),
        (
            Ix::GatherClash,
            ix::GatherClash {
                bell: 1_010,
                start: 0,
                n: 12,
                holdings_bitmap: 0x0000_0FFF,
                beneficiary: b32(2),
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::ResolveFromInputs,
            ix::ResolveFromInputs {
                bell: 1_010,
                beneficiary: b32(2),
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::ResolveClash,
            ix::ResolveClash {
                bell: 1_010,
                beneficiary: b32(2),
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::SkipQuiet,
            ix::SkipQuiet { b0: 1_010, n: 24 }.to_bytes().to_vec(),
        ),
        (
            Ix::CloseClashInputs,
            ix::CloseClashInputs {
                p: 2,
                q: -1,
                bell: 1_010,
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::CloseArrivalDay,
            ix::CloseArrivalDay {
                p: 2,
                q: -1,
                day: 7,
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::CloseArrivalSlot,
            ix::CloseArrivalSlot {
                p: 2,
                q: -1,
                bell: 1_010,
                faction: 4,
                i: 3,
            }
            .to_bytes()
            .to_vec(),
        ),
        (
            Ix::ClaimDefence,
            ix::ClaimDefence { day: 7, n: 6 }.to_bytes().to_vec(),
        ),
    ];
    let mut buf = [0u8; 512];
    let ft = ix::FileTicket {
        n: 2,
        sites: [
            ix::TicketSite {
                p: 2,
                q: -1,
                site: 3,
            },
            ix::TicketSite {
                p: 3,
                q: -2,
                site: 0,
            },
            ix::TicketSite::default(),
        ],
    };
    if let Some(len) = ft.encode(&mut buf) {
        out.push((Ix::FileTicket, buf[..len].to_vec()));
    }
    let aa = ix::ArchiveAnchors {
        region: 15,
        part: 14,
        n: 3,
        bells: [1_008, 1_009, 1_010, 0, 0, 0, 0, 0],
    };
    if let Some(len) = aa.encode(&mut buf) {
        out.push((Ix::ArchiveAnchors, buf[..len].to_vec()));
    }
    let payout = payout_borsh(&permutation_rules::frontier::payout::PayoutParams::REV3);
    let cs = ix::CreateSeason {
        params: presets::M1_LOCAL_7D,
        payout: &payout,
    };
    if let Some(len) = cs.encode(&mut buf) {
        out.push((Ix::CreateSeason, buf[..len].to_vec()));
    }
    out.sort_by_key(|(i, _)| i.tag());
    v.push((
        "samples",
        J::A(
            out.iter()
                .map(|(i, d)| {
                    o(vec![
                        ("name", st(i.name())),
                        ("tag", n(i.tag())),
                        ("len", n(d.len() as u64)),
                        ("data", J::S(hex(d))),
                    ])
                })
                .collect(),
        ),
    ));
    J::O(v.into_iter().map(|(k, x)| (k.to_string(), x)).collect())
}

fn entries_file() -> J {
    use permutation_rules::frontier::host::{Host, Pending, PendingOp, Stamina};
    use permutation_rules::units::UnitType;
    let mut v = header("Province entry (48 B) <-> kernel Host codec samples (§5.3, I-55)");
    let id = addr::host_id(2, -1, 3, 1, 42).unwrap_or(0);
    let key = addr::holding_key_of_host(id);
    let base = Host {
        id,
        owner: key,
        faction: 4,
        unit: UnitType::Knight,
        troops: 12_345_678,
        stamina: Stamina {
            value: 77,
            bell: 1_234,
        },
        ready_bell: 1_236,
        pending: None,
    };
    let cases: Vec<(&str, Option<PendingOp>, Option<EntryOp>)> = vec![
        ("none", None, None),
        ("spend", Some(PendingOp::Spend { cost: 120 }), None),
        (
            "split",
            Some(PendingOp::Split {
                troops: 5_000_000,
                of: 12_345_678,
                new_id: key | 99,
            }),
            None,
        ),
        ("absorb", Some(PendingOp::Absorb { from: key | 7 }), None),
        (
            "absorbed_into",
            Some(PendingOp::AbsorbedInto { into: key | 8 }),
            None,
        ),
        ("leave", None, Some(EntryOp::Leave)),
        ("forfeit", None, Some(EntryOp::Forfeit)),
    ];
    let mut rows = Vec::new();
    for (name, op, prog) in cases {
        let mut h = base;
        h.pending = op.map(|op| Pending { bell: 1_235, op });
        let mut e = Entry::from_host(&h, 30, layout::province::entry::STATE_ROSTER, 11_000, 901);
        if let Some(p) = prog {
            e.op = p;
            e.pend_bell = 1_235;
        }
        let mut b = [0u8; 48];
        let ok = e.write(&mut b).is_ok();
        rows.push(o(vec![
            ("case", st(name)),
            ("host_id", J::S(id.to_string())),
            ("encodes", J::B(ok)),
            ("entry", J::S(hex(&b))),
            ("pend_op", n(e.op.code())),
        ]));
    }
    v.push(("entries", J::A(rows)));
    J::O(v.into_iter().map(|(k, x)| (k.to_string(), x)).collect())
}

fn files() -> Vec<(&'static str, String)> {
    vec![
        ("layouts.json", layouts().render()),
        ("tags.json", tags_file().render()),
        ("errors.json", errors_file().render()),
        ("logs.json", logs_file().render()),
        ("budgets.json", budgets_file().render()),
        ("addresses.json", addresses_file().render()),
        ("presets.json", presets_file().render()),
        ("ix.json", ix_file().render()),
        ("entries.json", entries_file().render()),
    ]
}

fn vectors_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("vectors")
}

fn main() {
    let check = std::env::args().any(|a| a == "--check");
    let dir = vectors_dir();
    let mut stale = Vec::new();
    for (name, body) in files() {
        let path = dir.join(name);
        if check {
            match std::fs::read_to_string(&path) {
                Ok(cur) if cur == body => {}
                _ => stale.push(name),
            }
        } else if let Err(e) =
            std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, body))
        {
            eprintln!("abi-vectors: cannot write {}: {e}", path.display());
            std::process::exit(2);
        }
    }
    if check {
        if stale.is_empty() {
            println!("abi-vectors: {} files fresh", files().len());
        } else {
            eprintln!("abi-vectors: stale or missing: {} (run `cargo run -p frontier-abi --bin abi-vectors`)", stale.join(", "));
            std::process::exit(1);
        }
    } else {
        println!(
            "abi-vectors: wrote {} files to {}",
            files().len(),
            dir.display()
        );
    }
}
