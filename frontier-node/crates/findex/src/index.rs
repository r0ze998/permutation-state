//! The SQLite entity index over the archive (offchain design §4.3).
//!
//! | table | one row per |
//! |---|---|
//! | `tx` | archived transaction (failed ones included): slot, signature, error, CU, fee, fee payer |
//! | `record` | PS2 record of a **successful** transaction (a failed transaction's state is rolled back, so its logged records are not events) |
//! | `link` | chain link of a record's tail, with the entity's address |
//! | `entity` | chained entity: its latest `(seq, head)` from the logs |
//! | `snapshot` | post-transaction state of a program-owned (or closed) account the transaction wrote |
//! | `account` | latest snapshot per address |
//! | `event` | PS2 record of a successful transaction, numbered 1, 2, … in archive order (`ev`): the herald's `/h/events?after=` pages (W3-D) |
//! | `meta` | `last_seq` (the archive sequence indexed through), `schema` |
//!
//! A link's address comes from the transaction's post-state (the written
//! account whose chained header carries exactly the link's `seq` and
//! `head`); without post-state (a public RPC) it comes from the record's
//! key and payload (`frontier_abi::log::chains_of`), and stays NULL when
//! neither determines it. The index is a cache: [`Index::rebuild_from`]
//! re-creates it from the archive.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use solana_address::Address;

use fclient::log::bodies_from_logs;
use fclient::ports::TxRecord;
use frontier_abi::addr::AddrCtx;
use frontier_abi::layout::AccountKind;
use frontier_abi::log as plog;

/// Index schema: 2 adds the `event` numbering (W3-D).
pub const SCHEMA: i64 = 2;

pub struct Index {
    pub conn: Connection,
    /// Program and season, for addresses the logs imply.
    pub ctx: Option<AddrCtx>,
    pub program: Address,
}

/// One decoded PS2 record as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredRecord {
    pub seq: u64,
    pub idx: u32,
    pub kind: u8,
    pub bell: u32,
    pub key: Vec<u8>,
    pub payload: Vec<u8>,
    pub body: Vec<u8>,
}

/// A snapshot row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub address: Address,
    pub seq: u64,
    pub slot: u64,
    /// `None` = the transaction closed (or never created) the account.
    pub lamports: Option<u64>,
    pub owner: Option<Address>,
    pub kind: Option<u8>,
    pub data: Vec<u8>,
}

fn sql<T>(r: rusqlite::Result<T>) -> Result<T, String> {
    r.map_err(|e| e.to_string())
}

impl Index {
    /// Opens (or creates) the index at `path` (`":memory:"` for tests).
    pub fn open(path: &Path, program: Address, ctx: Option<AddrCtx>) -> Result<Index, String> {
        let conn = sql(Connection::open(path))?;
        sql(conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS meta(k TEXT PRIMARY KEY, v INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS tx(seq INTEGER PRIMARY KEY, slot INTEGER NOT NULL, sig TEXT NOT NULL,
                 block_time INTEGER, err TEXT, code INTEGER, units INTEGER, fee INTEGER, fee_payer TEXT);
             CREATE INDEX IF NOT EXISTS tx_sig ON tx(sig);
             CREATE TABLE IF NOT EXISTS record(seq INTEGER NOT NULL, idx INTEGER NOT NULL, kind INTEGER NOT NULL,
                 bell INTEGER NOT NULL, key BLOB NOT NULL, payload BLOB NOT NULL, body BLOB NOT NULL,
                 PRIMARY KEY(seq, idx));
             CREATE INDEX IF NOT EXISTS record_kind ON record(kind, bell);
             CREATE TABLE IF NOT EXISTS link(seq INTEGER NOT NULL, idx INTEGER NOT NULL, n INTEGER NOT NULL,
                 entity_kind INTEGER NOT NULL, address TEXT, eseq INTEGER NOT NULL, head BLOB NOT NULL,
                 PRIMARY KEY(seq, idx, n));
             CREATE INDEX IF NOT EXISTS link_addr ON link(address, eseq);
             CREATE TABLE IF NOT EXISTS entity(address TEXT PRIMARY KEY, entity_kind INTEGER NOT NULL,
                 eseq INTEGER NOT NULL, head BLOB NOT NULL, tx_seq INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS snapshot(address TEXT NOT NULL, seq INTEGER NOT NULL, slot INTEGER NOT NULL,
                 lamports INTEGER, owner TEXT, kind INTEGER, data BLOB NOT NULL, PRIMARY KEY(address, seq));
             CREATE TABLE IF NOT EXISTS account(address TEXT PRIMARY KEY, seq INTEGER NOT NULL, slot INTEGER NOT NULL,
                 kind INTEGER, present INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS event(ev INTEGER PRIMARY KEY, seq INTEGER NOT NULL, idx INTEGER NOT NULL);",
        ))?;
        // An index written before the `event` table (schema 1) is emptied;
        // `Findex::open` then rebuilds it from the archive.
        let schema: Option<i64> = sql(conn
            .query_row("SELECT v FROM meta WHERE k = 'schema'", [], |r| r.get(0))
            .optional())?;
        if schema.unwrap_or(1) < SCHEMA {
            sql(conn.execute_batch(
                "DELETE FROM meta; DELETE FROM tx; DELETE FROM record; DELETE FROM link;
                 DELETE FROM entity; DELETE FROM snapshot; DELETE FROM account; DELETE FROM event;",
            ))?;
            sql(conn.execute(
                "INSERT INTO meta(k, v) VALUES('schema', ?1)",
                params![SCHEMA],
            ))?;
        }
        Ok(Index { conn, ctx, program })
    }

    /// The archive sequence indexed through.
    pub fn last_seq(&self) -> Result<u64, String> {
        Ok(sql(self
            .conn
            .query_row("SELECT v FROM meta WHERE k = 'last_seq'", [], |r| {
                r.get::<_, i64>(0)
            })
            .optional())?
        .unwrap_or(0) as u64)
    }

    /// Indexes archived records (in order, `seq` > [`Index::last_seq`]).
    pub fn add(&mut self, recs: &[TxRecord]) -> Result<usize, String> {
        let last = self.last_seq()?;
        let program = self.program;
        let ctx = self.ctx;
        let t = sql(self.conn.transaction())?;
        let mut n = 0;
        for r in recs.iter().filter(|r| r.seq > last) {
            index_one(&t, r, &program, ctx.as_ref())?;
            sql(t.execute(
                "INSERT INTO meta(k, v) VALUES('last_seq', ?1) ON CONFLICT(k) DO UPDATE SET v = ?1",
                params![r.seq as i64],
            ))?;
            n += 1;
        }
        sql(t.commit())?;
        Ok(n)
    }

    /// Drops every row and indexes `recs` again.
    pub fn rebuild_from(&mut self, recs: &[TxRecord]) -> Result<usize, String> {
        sql(self.conn.execute_batch(
            "DELETE FROM meta; DELETE FROM tx; DELETE FROM record; DELETE FROM link;
             DELETE FROM entity; DELETE FROM snapshot; DELETE FROM account; DELETE FROM event;",
        ))?;
        sql(self.conn.execute(
            "INSERT INTO meta(k, v) VALUES('schema', ?1)",
            params![SCHEMA],
        ))?;
        self.add(recs)
    }

    pub fn tx_count(&self) -> Result<(u64, u64), String> {
        sql(self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(err IS NOT NULL), 0) FROM tx",
            [],
            |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
        ))
    }

    /// Records of `kind` with `bell` in `[from, to]`, in feed order.
    pub fn records(&self, kind: u8, from: u32, to: u32) -> Result<Vec<StoredRecord>, String> {
        let mut st = sql(self.conn.prepare(
            "SELECT seq, idx, kind, bell, key, payload, body FROM record
             WHERE kind = ?1 AND bell BETWEEN ?2 AND ?3 ORDER BY seq, idx",
        ))?;
        let rows = sql(st.query_map(params![kind, from as i64, to as i64], |r| {
            Ok(StoredRecord {
                seq: r.get::<_, i64>(0)? as u64,
                idx: r.get::<_, i64>(1)? as u32,
                kind: r.get::<_, i64>(2)? as u8,
                bell: r.get::<_, i64>(3)? as u32,
                key: r.get(4)?,
                payload: r.get(5)?,
                body: r.get(6)?,
            })
        }))?;
        sql(rows.collect())
    }

    /// Latest `(entity seq, head)` of a chained entity from the logs.
    pub fn entity_head(&self, a: &Address) -> Result<Option<(u64, [u8; 32])>, String> {
        sql(self
            .conn
            .query_row(
                "SELECT eseq, head FROM entity WHERE address = ?1",
                params![a.to_string()],
                |r| {
                    let h: Vec<u8> = r.get(1)?;
                    Ok((
                        r.get::<_, i64>(0)? as u64,
                        h.try_into().unwrap_or([0u8; 32]),
                    ))
                },
            )
            .optional())
    }

    /// Count of links whose address could not be resolved.
    pub fn unresolved_links(&self) -> Result<u64, String> {
        sql(self
            .conn
            .query_row("SELECT COUNT(*) FROM link WHERE address IS NULL", [], |r| {
                r.get::<_, i64>(0)
            })
            .map(|n| n as u64))
    }

    /// The latest snapshot of `a` at or before archive sequence `at`.
    pub fn snapshot_at(&self, a: &Address, at: u64) -> Result<Option<Snapshot>, String> {
        sql(self
            .conn
            .query_row(
                "SELECT seq, slot, lamports, owner, kind, data FROM snapshot
                 WHERE address = ?1 AND seq <= ?2 ORDER BY seq DESC LIMIT 1",
                params![a.to_string(), at as i64],
                |r| {
                    Ok(Snapshot {
                        address: *a,
                        seq: r.get::<_, i64>(0)? as u64,
                        slot: r.get::<_, i64>(1)? as u64,
                        lamports: r.get::<_, Option<i64>>(2)?.map(|x| x as u64),
                        owner: r.get::<_, Option<String>>(3)?.and_then(|s| s.parse().ok()),
                        kind: r.get::<_, Option<i64>>(4)?.map(|x| x as u8),
                        data: r.get(5)?,
                    })
                },
            )
            .optional())
    }

    /// The latest snapshot of `a`.
    pub fn latest(&self, a: &Address) -> Result<Option<Snapshot>, String> {
        self.snapshot_at(a, i64::MAX as u64)
    }

    /// Addresses of present accounts of `kind` (latest snapshot).
    pub fn present_of_kind(&self, kind: AccountKind) -> Result<Vec<Address>, String> {
        let mut st = sql(self.conn.prepare(
            "SELECT address FROM account WHERE kind = ?1 AND present = 1 ORDER BY address",
        ))?;
        let rows = sql(st.query_map(params![kind as u8], |r| r.get::<_, String>(0)))?;
        let v: Vec<String> = sql(rows.collect())?;
        Ok(v.into_iter().filter_map(|s| s.parse().ok()).collect())
    }
}

/// One `/h/events` row: a PS2 record with its number and transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventRow {
    /// 1, 2, … over the records of landed transactions, in archive order.
    pub ev: u64,
    /// Archive sequence of the transaction.
    pub seq: u64,
    pub slot: u64,
    pub sig: String,
    pub kind: u8,
    pub bell: u32,
    pub body: Vec<u8>,
}

/// A read-only connection to an index another process (or task) writes
/// (WAL mode lets readers run beside the writer).
pub struct Reader {
    pub conn: Connection,
}

impl Reader {
    pub fn open(path: &Path) -> Result<Reader, String> {
        let conn = sql(Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ))?;
        Ok(Reader { conn })
    }

    /// Up to `limit` events with `ev > after`, in order.
    pub fn events_after(&self, after: u64, limit: usize) -> Result<Vec<EventRow>, String> {
        events_after(&self.conn, after, limit)
    }

    /// The number of the last event (0 = none).
    pub fn last_event(&self) -> Result<u64, String> {
        last_event(&self.conn)
    }
}

fn events_after(conn: &Connection, after: u64, limit: usize) -> Result<Vec<EventRow>, String> {
    let mut st = sql(conn.prepare_cached(
        "SELECT e.ev, e.seq, t.slot, t.sig, r.kind, r.bell, r.body FROM event e
         JOIN record r ON r.seq = e.seq AND r.idx = e.idx JOIN tx t ON t.seq = e.seq
         WHERE e.ev > ?1 ORDER BY e.ev LIMIT ?2",
    ))?;
    let rows = sql(st.query_map(params![after as i64, limit as i64], |r| {
        Ok(EventRow {
            ev: r.get::<_, i64>(0)? as u64,
            seq: r.get::<_, i64>(1)? as u64,
            slot: r.get::<_, i64>(2)? as u64,
            sig: r.get(3)?,
            kind: r.get::<_, i64>(4)? as u8,
            bell: r.get::<_, i64>(5)? as u32,
            body: r.get(6)?,
        })
    }))?;
    sql(rows.collect())
}

fn last_event(conn: &Connection) -> Result<u64, String> {
    sql(
        conn.query_row("SELECT COALESCE(MAX(ev), 0) FROM event", [], |r| {
            r.get::<_, i64>(0)
        }),
    )
    .map(|v| v as u64)
}

impl Index {
    /// Up to `limit` events with `ev > after`, in order.
    pub fn events_after(&self, after: u64, limit: usize) -> Result<Vec<EventRow>, String> {
        events_after(&self.conn, after, limit)
    }

    /// The number of the last event (0 = none).
    pub fn last_event(&self) -> Result<u64, String> {
        last_event(&self.conn)
    }
}

fn fee_payer(wire: &[u8]) -> Option<Address> {
    fclient::tx::from_wire(wire)
        .ok()
        .and_then(|t| t.message.account_keys.first().copied())
}

fn index_one(
    t: &rusqlite::Transaction<'_>,
    r: &TxRecord,
    program: &Address,
    ctx: Option<&AddrCtx>,
) -> Result<(), String> {
    sql(t.execute(
        "INSERT INTO tx(seq, slot, sig, block_time, err, code, units, fee, fee_payer)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            r.seq as i64,
            r.slot as i64,
            r.signature.to_string(),
            r.block_time,
            r.err,
            r.code.map(|c| c as i64),
            r.units as i64,
            r.fee as i64,
            fee_payer(&r.tx).map(|a| a.to_string()),
        ],
    ))?;
    if r.err.is_some() {
        return Ok(());
    }
    // Snapshots of the accounts the transaction wrote (program-owned or gone).
    for (k, a) in &r.post {
        let (lamports, owner, kind, data, present) = match a {
            Some(a) if a.owner == *program => {
                let kind = a
                    .data
                    .get(..8)
                    .and_then(|m| AccountKind::from_magic(m.try_into().ok()?))
                    .map(|k| k as u8);
                (
                    Some(a.lamports as i64),
                    Some(a.owner.to_string()),
                    kind,
                    a.data.clone(),
                    true,
                )
            }
            Some(a) if !fclient::decode::is_absent(&a.owner, &a.data) => continue,
            _ => (None, None, None, vec![], false),
        };
        sql(t.execute(
            "INSERT OR REPLACE INTO snapshot(address, seq, slot, lamports, owner, kind, data)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                k.to_string(),
                r.seq as i64,
                r.slot as i64,
                lamports,
                owner,
                kind,
                data
            ],
        ))?;
        // A closed account keeps the kind it had.
        let kind = match kind {
            Some(k) => Some(k),
            None => sql(t
                .query_row(
                    "SELECT kind FROM account WHERE address = ?1",
                    params![k.to_string()],
                    |x| x.get::<_, Option<i64>>(0),
                )
                .optional())?
            .flatten()
            .map(|x| x as u8),
        };
        if kind.is_none() && !present {
            continue;
        }
        sql(t.execute(
            "INSERT INTO account(address, seq, slot, kind, present) VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(address) DO UPDATE SET seq = ?2, slot = ?3, kind = ?4, present = ?5",
            params![
                k.to_string(),
                r.seq as i64,
                r.slot as i64,
                kind,
                present as i64
            ],
        ))?;
    }
    // PS2 records and their chain links.
    // Only the program's own frame (fclient::log); an undecodable body is
    // skipped, never an error that would wedge the index (wave-3 review).
    let bodies = bodies_from_logs(&r.logs, program).unwrap_or_default();
    let mut idx = 0usize;
    for body in bodies.iter() {
        let Ok(rec) = plog::decode(body) else {
            continue;
        };
        let this = idx;
        idx += 1;
        let idx = this;
        sql(t.execute(
            "INSERT INTO record(seq, idx, kind, bell, key, payload, body) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                r.seq as i64,
                idx as i64,
                rec.kind as u8 as i64,
                rec.bell as i64,
                rec.key,
                rec.payload,
                body
            ],
        ))?;
        sql(t.execute(
            "INSERT INTO event(seq, idx) VALUES(?1, ?2)",
            params![r.seq as i64, idx as i64],
        ))?;
        let expected = plog::chains_of(rec.kind, rec.key, rec.payload);
        for (n, link) in rec.links.iter().take(rec.n_links).flatten().enumerate() {
            let from_post = r.post.iter().find_map(|(k, a)| {
                let a = a.as_ref()?;
                let h = fclient::decode::chained_header(&a.data)?;
                (a.owner == *program && h.event_seq == link.seq && h.event_head == link.head)
                    .then_some(*k)
            });
            let from_logs = || {
                let e = expected?;
                let c = e.iter().filter(|c| c.entity == link.entity).nth(
                    rec.links[..n]
                        .iter()
                        .flatten()
                        .filter(|l| l.entity == link.entity)
                        .count(),
                )?;
                c.who.address(ctx?).map(Address::new_from_array)
            };
            let addr = from_post.or_else(from_logs);
            sql(t.execute(
                "INSERT INTO link(seq, idx, n, entity_kind, address, eseq, head) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    r.seq as i64,
                    idx as i64,
                    n as i64,
                    link.entity as u8 as i64,
                    addr.map(|a| a.to_string()),
                    link.seq as i64,
                    link.head.to_vec()
                ],
            ))?;
            if let Some(a) = addr {
                sql(t.execute(
                    "INSERT INTO entity(address, entity_kind, eseq, head, tx_seq) VALUES(?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(address) DO UPDATE SET eseq = ?3, head = ?4, tx_seq = ?5 WHERE ?3 > entity.eseq",
                    params![
                        a.to_string(),
                        link.entity as u8 as i64,
                        link.seq as i64,
                        link.head.to_vec(),
                        r.seq as i64
                    ],
                ))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fclient::ports::{Account, Signature};
    use frontier_abi::log::{advance, write_body, write_tail, EntityKind, Kind};

    fn body(kind: Kind, bell: u32, key: &[u8], payload: &[u8], links: &[plog::Link]) -> Vec<u8> {
        let mut out = vec![0u8; 512];
        let n = write_body(kind, bell, key, payload, &mut out).unwrap();
        let m = write_tail(links, &mut out, n).unwrap();
        out.truncate(m);
        out
    }

    fn tx(
        seq: u64,
        logs: Vec<String>,
        post: Vec<(Address, Option<Account>)>,
        err: bool,
    ) -> TxRecord {
        TxRecord {
            seq,
            slot: 100 + seq,
            signature: Signature::from([seq as u8; 64]),
            block_time: 0,
            tx: vec![],
            logs,
            err: err.then(|| "InstructionError(0, Custom(7))".into()),
            code: err.then_some(7),
            units: 0,
            fee: 5_000,
            post,
        }
    }

    #[test]
    fn records_links_heads_and_snapshots() {
        let program = Address::new_from_array([0xAB; 32]);
        let ctx = crate::addr_ctx(&program, 3);
        let season = Address::new_from_array(ctx.season);
        let mut ix = Index::open(Path::new(":memory:"), program, Some(ctx)).unwrap();

        // An unchained ANCHOR with its created account in the post-state.
        let anchor_key = [7u8, 0, 0, 0, 5];
        let anchor_payload = [1u8; 56];
        let anchor_addr = Address::new_from_array(ctx.bell_anchor(7, 5));
        let mut anchor_data = vec![0u8; 144];
        anchor_data[..8].copy_from_slice(b"PSF1ANCH");
        let a1 = body(Kind::ANCHOR, 7, &anchor_key, &anchor_payload, &[]);

        // GENESIS_SEED chained on the Season: once with the Season in the
        // post-state (address from the header), once without (from the key).
        let gs_key = 3u64.to_le_bytes();
        let gs_payload = [2u8; 40];
        let mut bwt = vec![0u8; 64];
        let n = write_body(Kind::GENESIS_SEED, u32::MAX, &gs_key, &gs_payload, &mut bwt).unwrap();
        let l1 = advance(EntityKind::Season, 4, &[9u8; 32], &bwt[..n]).unwrap();
        let g1 = body(Kind::GENESIS_SEED, u32::MAX, &gs_key, &gs_payload, &[l1]);
        let mut season_data = vec![0u8; 2_048];
        season_data[..8].copy_from_slice(b"PSF1SEAS");
        season_data[24..32].copy_from_slice(&l1.seq.to_le_bytes());
        season_data[32..64].copy_from_slice(&l1.head);
        let l2 = advance(EntityKind::Season, l1.seq, &l1.head, &bwt[..n]).unwrap();
        let g2 = body(Kind::GENESIS_SEED, u32::MAX, &gs_key, &gs_payload, &[l2]);

        let acct = |data: Vec<u8>| {
            Some(Account {
                lamports: 1_000,
                data,
                owner: program,
                executable: false,
            })
        };
        let recs = vec![
            tx(
                1,
                fclient::log::in_frame(&program, [fclient::log::log_line(&a1)]),
                vec![(anchor_addr, acct(anchor_data))],
                false,
            ),
            tx(
                2,
                fclient::log::in_frame(&program, [fclient::log::log_line(&g1)]),
                vec![(season, acct(season_data))],
                false,
            ),
            // A failed transaction's records are not events.
            tx(
                3,
                fclient::log::in_frame(&program, [fclient::log::log_line(&a1)]),
                vec![],
                true,
            ),
            tx(
                4,
                fclient::log::in_frame(&program, [fclient::log::log_line(&g2)]),
                vec![],
                false,
            ),
            // The anchor closes.
            tx(5, vec![], vec![(anchor_addr, None)], false),
        ];
        assert_eq!(ix.add(&recs).unwrap(), 5);
        assert_eq!(ix.add(&recs).unwrap(), 0, "idempotent by sequence");
        assert_eq!(ix.tx_count().unwrap(), (5, 1));
        assert_eq!(ix.records(Kind::ANCHOR as u8, 0, 10).unwrap().len(), 1);
        assert_eq!(
            ix.records(Kind::GENESIS_SEED as u8, 0, u32::MAX)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(ix.unresolved_links().unwrap(), 0);
        assert_eq!(ix.entity_head(&season).unwrap(), Some((l2.seq, l2.head)));
        let s1 = ix.snapshot_at(&anchor_addr, 4).unwrap().unwrap();
        assert_eq!((s1.seq, s1.kind), (1, Some(AccountKind::BellAnchor as u8)));
        let s5 = ix.latest(&anchor_addr).unwrap().unwrap();
        assert_eq!((s5.seq, s5.lamports), (5, None), "closed");
        assert_eq!(
            ix.present_of_kind(AccountKind::Season).unwrap(),
            vec![season]
        );
        assert!(ix
            .present_of_kind(AccountKind::BellAnchor)
            .unwrap()
            .is_empty());
        // Events: the records of landed transactions, numbered in order.
        let ev = ix.events_after(0, 10).unwrap();
        assert_eq!(
            ev.iter().map(|e| (e.ev, e.seq, e.kind)).collect::<Vec<_>>(),
            vec![
                (1, 1, Kind::ANCHOR as u8),
                (2, 2, Kind::GENESIS_SEED as u8),
                (3, 4, Kind::GENESIS_SEED as u8)
            ]
        );
        assert_eq!(ev[2].body, g2);
        assert_eq!(ix.events_after(2, 10).unwrap().len(), 1);
        assert_eq!(ix.events_after(0, 2).unwrap().len(), 2);
        assert_eq!(ix.last_event().unwrap(), 3);
        // Rebuild gives the same answers.
        assert_eq!(ix.rebuild_from(&recs).unwrap(), 5);
        assert_eq!(ix.entity_head(&season).unwrap(), Some((l2.seq, l2.head)));
        assert_eq!(ix.last_event().unwrap(), 3, "renumbered from 1");
    }

    /// Wave-3 review (W3-D): a garbage PS2 body in the program's frame and
    /// a well-formed one in another program's frame neither fail the batch
    /// nor take an event number.
    #[test]
    fn garbage_and_foreign_records_are_skipped() {
        let program = Address::new_from_array([0xAB; 32]);
        let other = Address::new_from_array([0xCD; 32]);
        let ctx = crate::addr_ctx(&program, 3);
        let mut ix = Index::open(Path::new(":memory:"), program, Some(ctx)).unwrap();
        let a1 = body(Kind::ANCHOR, 7, &[7u8, 0, 0, 0, 5], &[1u8; 56], &[]);
        let a2 = body(Kind::ANCHOR, 8, &[8u8, 0, 0, 0, 5], &[1u8; 56], &[]);
        let recs = vec![
            tx(
                1,
                fclient::log::in_frame(&program, [fclient::log::log_line(&[1, 2, 3])]),
                vec![],
                false,
            ),
            tx(
                2,
                fclient::log::in_frame(&other, [fclient::log::log_line(&a1)]),
                vec![],
                false,
            ),
            tx(
                3,
                fclient::log::in_frame(&program, [fclient::log::log_line(&a2)]),
                vec![],
                false,
            ),
        ];
        assert_eq!(ix.add(&recs).unwrap(), 3);
        let ev = ix.events_after(0, 10).unwrap();
        assert_eq!(
            ev.iter().map(|e| (e.ev, e.seq)).collect::<Vec<_>>(),
            vec![(1, 3)]
        );
    }
}
