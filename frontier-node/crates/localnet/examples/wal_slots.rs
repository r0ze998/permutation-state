//! Prints, per block of a ledger in `[FROM, TO]`, the slot, the game time,
//! the scale, the transactions it executed (landed / failed), their §10.1
//! cost and priority range (milli-lamports per CU), and every
//! non-block record in the range (W6T-3: the w6-s7 no-landing window,
//! slots 827–1028). Streams the ledger; stops after `TO`.
//!
//! `cargo run --release -p localnet --example wal_slots -- LEDGER FROM TO`

use std::io::{BufReader, Read};

use localnet::persist::{Record, WalHeader};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (from, to): (u64, u64) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let mut f = BufReader::new(std::fs::File::open(&a[1]).expect("ledger"));
    let mut head = vec![0u8; 8 + 16 + 8 + 8 + 8];
    f.read_exact(&mut head).expect("header");
    let h = WalHeader::decode(&head).expect("header");
    println!("# header g0 {} slot0 {} scale {}", h.g0, h.slot0, h.scale);
    let mut last_slot = 0u64;
    loop {
        let mut len = [0u8; 4];
        if f.read_exact(&mut len).is_err() {
            break;
        }
        let n = u32::from_le_bytes(len) as usize;
        let mut b = vec![0u8; n];
        if f.read_exact(&mut b).is_err() {
            break;
        }
        let (kind, payload) = (b[0], &b[1..n - 8]);
        let Ok(rec) = Record::decode(kind, payload) else {
            println!("# undecodable record kind {kind}");
            break;
        };
        match rec {
            Record::Block {
                slot,
                game_ms,
                scale,
                txs,
            } => {
                if slot > to {
                    break;
                }
                if slot >= from {
                    let failed = txs.iter().filter(|t| t.err.is_some()).count();
                    // §10.1 cost (the block builder's budget unit) and the
                    // priority range of what executed.
                    let (mut cost, mut pmin, mut pmax) = (0u64, u64::MAX, 0u64);
                    for t in &txs {
                        if let Ok(x) = fclient::tx::from_wire(&t.wire) {
                            let (p, c) = fclient::tx::priority(&x.message);
                            cost += c;
                            pmin = pmin.min(p);
                            pmax = pmax.max(p);
                        }
                    }
                    let gap = if last_slot != 0 && slot != last_slot + 1 {
                        format!(" GAP after {last_slot}")
                    } else {
                        String::new()
                    };
                    if std::env::var("WAL_TXS").is_ok() {
                        for t in &txs {
                            let Ok(x) = fclient::tx::from_wire(&t.wire) else {
                                continue;
                            };
                            let m = &x.message;
                            let tags: Vec<String> = m
                                .instructions
                                .iter()
                                .filter_map(|c| {
                                    let prog = m.account_keys.get(c.program_id_index as usize)?;
                                    let name = fclient::abi::ix_info(*c.data.first()?)
                                        .map_or("?", |r| r.name);
                                    (prog.to_string().starts_with("GS8U")).then(|| name.to_string())
                                })
                                .collect();
                            let (p, c) = fclient::tx::priority(m);
                            println!(
                                "#   {slot} payer {} prio {p} cost {c} ok {} {}",
                                m.account_keys[0],
                                t.err.is_none(),
                                tags.join(",")
                            );
                        }
                    }
                    println!(
                        "{slot}\t{}\t{scale}\t{}\t{}\t{cost}\t{}\t{pmax}{gap}",
                        game_ms / 1000,
                        txs.len() - failed,
                        failed,
                        if pmin == u64::MAX { 0 } else { pmin },
                    );
                }
                last_slot = slot;
            }
            Record::Airdrop {
                slot,
                key,
                lamports,
                ..
            } if slot >= from && slot <= to => {
                println!("# airdrop slot {slot} {key} {lamports}");
            }
            Record::SetAccount { key, .. } if last_slot >= from && last_slot <= to => {
                println!("# set_account after slot {last_slot}: {key}");
            }
            Record::Deploy { program, .. } if last_slot >= from && last_slot <= to => {
                println!("# deploy after slot {last_slot}: {program}");
            }
            _ => {}
        }
    }
}
