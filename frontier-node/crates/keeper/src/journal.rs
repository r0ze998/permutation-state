//! The keeper journal (M1 contract §8.2 "Crash safety"; offchain design §6.7).
//!
//! **The chain is the state**: every duty is idempotent and succeeds once
//! per object, so a restart re-plans from chain reads. The journal keeps the
//! evidence (every version sent: payer, bid, CU limit, slots, outcome) and
//! lets a restart close its books: in-flight attempts are reconciled with
//! `getSignatureStatuses` before anything new is sent, and the engine adopts
//! the ones still valid so they are not duplicated blindly.
//!
//! SQLite in WAL mode with `synchronous = FULL` (the attempts table must
//! survive a power cut). Tables: `attempts`, `plaintexts` (W4-C's reveal
//! cache), `cursor`, `claims` (W4), `payers`, `alerts`. One process per
//! keeper identity: [`lock`] takes an exclusive `flock`-style lock on
//! `<journal>.lock`.

use std::fs::{File, OpenOptions};
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

/// One transaction version as sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attempt {
    pub sig: String,
    pub kind: String,
    pub object_key: String,
    pub bell: Option<u32>,
    pub region: Option<u8>,
    pub class: String,
    pub payer: String,
    pub bid_milli: u64,
    pub cu_limit: u32,
    pub heap: Option<u32>,
    pub first_valid_slot: u64,
    pub sent_slot: u64,
    pub landed_slot: Option<u64>,
    /// `sent`, `landed`, `failed`, `expired`, `superseded`.
    pub status: String,
    pub code: Option<u32>,
}

pub struct Journal {
    pub conn: Connection,
    /// Crash injection (tests, offchain design §6.7): panics at a chosen
    /// journal write, as a `kill -9` at that point would stop the process.
    pub crash: Option<CrashAt>,
}

/// Where [`Journal::crash`] fires: the `nth` write at `point`
/// (`"attempt"`, `"status"` or `"plaintext"`) whose attempt kind is one of
/// `kinds` (`"*"` = any). The write itself does not happen.
#[derive(Debug)]
pub struct CrashAt {
    pub point: &'static str,
    pub kinds: Vec<&'static str>,
    pub nth: u32,
    pub seen: std::cell::Cell<u32>,
}

impl CrashAt {
    pub fn new(point: &'static str, kinds: &[&'static str], nth: u32) -> CrashAt {
        CrashAt {
            point,
            kinds: kinds.to_vec(),
            nth,
            seen: std::cell::Cell::new(0),
        }
    }
}

/// A journalled `(plaintext, salt)`.
pub type PlainSalt = (Vec<u8>, Vec<u8>);

/// The panic message of an injected crash.
pub const INJECTED_CRASH: &str = "injected crash";

fn sql<T>(r: rusqlite::Result<T>) -> Result<T, String> {
    r.map_err(|e| e.to_string())
}

/// Takes the process lock of a keeper identity (`<path>.lock`).
pub fn lock(journal: &Path) -> Result<File, String> {
    let p = journal.with_extension("lock");
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&p)
        .map_err(|e| format!("{}: {e}", p.display()))?;
    match f.try_lock() {
        Ok(()) => Ok(f),
        Err(std::fs::TryLockError::WouldBlock) => {
            Err(format!("{} is held by another keeper process", p.display()))
        }
        Err(std::fs::TryLockError::Error(e)) => Err(format!("{}: {e}", p.display())),
    }
}

impl Journal {
    /// Opens (or creates) the journal (`":memory:"` for tests).
    pub fn open(path: &Path) -> Result<Journal, String> {
        let conn = sql(Connection::open(path))?;
        sql(conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS attempts(sig TEXT PRIMARY KEY, kind TEXT NOT NULL, object_key TEXT NOT NULL,
                 bell INTEGER, region INTEGER, class TEXT NOT NULL, payer TEXT NOT NULL, bid_milli INTEGER NOT NULL,
                 cu_limit INTEGER NOT NULL, heap INTEGER, first_valid_slot INTEGER NOT NULL, sent_slot INTEGER NOT NULL,
                 landed_slot INTEGER, status TEXT NOT NULL, code INTEGER);
             CREATE INDEX IF NOT EXISTS attempts_status ON attempts(status);
             CREATE INDEX IF NOT EXISTS attempts_object ON attempts(object_key);
             CREATE TABLE IF NOT EXISTS plaintexts(host INTEGER NOT NULL, bell INTEGER NOT NULL, plain BLOB NOT NULL,
                 salt BLOB NOT NULL, PRIMARY KEY(host, bell));
             CREATE TABLE IF NOT EXISTS cursor(name TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS claims(slot_key TEXT PRIMARY KEY, bell INTEGER NOT NULL, region INTEGER NOT NULL,
                 ev_slot INTEGER, ev_price INTEGER, ev_limit INTEGER, state TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS payers(pool TEXT NOT NULL, i INTEGER NOT NULL, address TEXT NOT NULL,
                 balance_seen INTEGER NOT NULL, slot INTEGER NOT NULL, PRIMARY KEY(pool, i));
             CREATE TABLE IF NOT EXISTS alerts(id INTEGER PRIMARY KEY AUTOINCREMENT, slot INTEGER NOT NULL,
                 kind TEXT NOT NULL, detail TEXT NOT NULL);",
        ))?;
        Ok(Journal { conn, crash: None })
    }

    fn maybe_crash(&self, point: &str, kind: &str) {
        let Some(c) = &self.crash else { return };
        if c.point != point || !(c.kinds.contains(&"*") || c.kinds.contains(&kind)) {
            return;
        }
        let n = c.seen.get() + 1;
        c.seen.set(n);
        if n == c.nth {
            panic!("{INJECTED_CRASH} at journal {point} #{n} ({kind})");
        }
    }

    pub fn record_attempt(&self, a: &Attempt) -> Result<(), String> {
        self.maybe_crash("attempt", &a.kind);
        sql(self.conn.execute(
            "INSERT OR REPLACE INTO attempts(sig, kind, object_key, bell, region, class, payer, bid_milli, cu_limit,
                 heap, first_valid_slot, sent_slot, landed_slot, status, code)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                a.sig,
                a.kind,
                a.object_key,
                a.bell,
                a.region,
                a.class,
                a.payer,
                a.bid_milli as i64,
                a.cu_limit,
                a.heap,
                a.first_valid_slot as i64,
                a.sent_slot as i64,
                a.landed_slot.map(|s| s as i64),
                a.status,
                a.code
            ],
        ))
        .map(|_| ())
    }

    /// Sets an attempt's outcome.
    pub fn set_status(
        &self,
        sig: &str,
        status: &str,
        landed_slot: Option<u64>,
        code: Option<u32>,
    ) -> Result<(), String> {
        if self.crash.is_some() {
            let kind: Option<String> = self
                .conn
                .query_row(
                    "SELECT kind FROM attempts WHERE sig = ?1",
                    params![sig],
                    |r| r.get(0),
                )
                .optional()
                .unwrap_or(None);
            self.maybe_crash("status", kind.as_deref().unwrap_or(""));
        }
        sql(self.conn.execute(
            "UPDATE attempts SET status = ?2, landed_slot = ?3, code = ?4 WHERE sig = ?1",
            params![sig, status, landed_slot.map(|s| s as i64), code],
        ))
        .map(|_| ())
    }

    fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Attempt> {
        Ok(Attempt {
            sig: r.get(0)?,
            kind: r.get(1)?,
            object_key: r.get(2)?,
            bell: r.get(3)?,
            region: r.get(4)?,
            class: r.get(5)?,
            payer: r.get(6)?,
            bid_milli: r.get::<_, i64>(7)? as u64,
            cu_limit: r.get(8)?,
            heap: r.get(9)?,
            first_valid_slot: r.get::<_, i64>(10)? as u64,
            sent_slot: r.get::<_, i64>(11)? as u64,
            landed_slot: r.get::<_, Option<i64>>(12)?.map(|s| s as u64),
            status: r.get(13)?,
            code: r.get(14)?,
        })
    }

    const COLS: &'static str =
        "sig, kind, object_key, bell, region, class, payer, bid_milli, cu_limit, heap,
         first_valid_slot, sent_slot, landed_slot, status, code";

    /// Attempts still marked `sent` (in flight at a crash).
    pub fn in_flight(&self) -> Result<Vec<Attempt>, String> {
        let mut st = sql(self.conn.prepare(&format!(
            "SELECT {} FROM attempts WHERE status = 'sent' ORDER BY sent_slot",
            Self::COLS
        )))?;
        let rows = sql(st.query_map([], Self::row))?;
        sql(rows.collect())
    }

    /// Every attempt of an object.
    pub fn attempts_of(&self, object_key: &str) -> Result<Vec<Attempt>, String> {
        let mut st = sql(self.conn.prepare(&format!(
            "SELECT {} FROM attempts WHERE object_key = ?1 ORDER BY sent_slot",
            Self::COLS
        )))?;
        let rows = sql(st.query_map(params![object_key], Self::row))?;
        sql(rows.collect())
    }

    /// Distinct object keys starting with `prefix`.
    pub fn object_keys_with_prefix(&self, prefix: &str) -> Result<Vec<String>, String> {
        let mut st = sql(self.conn.prepare(
            "SELECT DISTINCT object_key FROM attempts WHERE substr(object_key, 1, length(?1)) = ?1",
        ))?;
        let rows = sql(st.query_map(params![prefix], |r| r.get::<_, String>(0)))?;
        sql(rows.collect())
    }

    /// `(status, count)` over all attempts.
    pub fn status_counts(&self) -> Result<Vec<(String, u64)>, String> {
        let mut st = sql(self
            .conn
            .prepare("SELECT status, COUNT(*) FROM attempts GROUP BY status ORDER BY status"))?;
        let rows = sql(st.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
        }))?;
        sql(rows.collect())
    }

    pub fn set_cursor(&self, name: &str, value: &str) -> Result<(), String> {
        sql(self.conn.execute(
            "INSERT INTO cursor(name, value) VALUES(?1, ?2) ON CONFLICT(name) DO UPDATE SET value = ?2",
            params![name, value],
        ))
        .map(|_| ())
    }

    pub fn cursor(&self, name: &str) -> Result<Option<String>, String> {
        sql(self
            .conn
            .query_row(
                "SELECT value FROM cursor WHERE name = ?1",
                params![name],
                |r| r.get(0),
            )
            .optional())
    }

    pub fn save_plaintext(
        &self,
        host: u64,
        bell: u32,
        plain: &[u8],
        salt: &[u8],
    ) -> Result<(), String> {
        self.maybe_crash("plaintext", "reveal");
        sql(self.conn.execute(
            "INSERT OR REPLACE INTO plaintexts(host, bell, plain, salt) VALUES(?1, ?2, ?3, ?4)",
            params![host as i64, bell, plain, salt],
        ))
        .map(|_| ())
    }

    /// A journalled plaintext and salt (W4-C's reveal cache).
    pub fn plaintext(&self, host: u64, bell: u32) -> Result<Option<PlainSalt>, String> {
        sql(self
            .conn
            .query_row(
                "SELECT plain, salt FROM plaintexts WHERE host = ?1 AND bell = ?2",
                params![host as i64, bell],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional())
    }

    pub fn note_payer(
        &self,
        pool: &str,
        i: u32,
        address: &str,
        balance: u64,
        slot: u64,
    ) -> Result<(), String> {
        sql(self.conn.execute(
            "INSERT INTO payers(pool, i, address, balance_seen, slot) VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(pool, i) DO UPDATE SET address = ?3, balance_seen = ?4, slot = ?5",
            params![pool, i, address, balance as i64, slot as i64],
        ))
        .map(|_| ())
    }

    pub fn alert(&self, slot: u64, kind: &str, detail: &str) -> Result<(), String> {
        sql(self.conn.execute(
            "INSERT INTO alerts(slot, kind, detail) VALUES(?1, ?2, ?3)",
            params![slot as i64, kind, detail],
        ))
        .map(|_| ())
    }

    pub fn alerts(&self, kind: &str) -> Result<Vec<(u64, String)>, String> {
        let mut st = sql(self
            .conn
            .prepare("SELECT slot, detail FROM alerts WHERE kind = ?1 ORDER BY id"))?;
        let rows = sql(st.query_map(params![kind], |r| {
            Ok((r.get::<_, i64>(0)? as u64, r.get::<_, String>(1)?))
        }))?;
        sql(rows.collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(sig: &str, status: &str) -> Attempt {
        Attempt {
            sig: sig.into(),
            kind: "anchor".into(),
            object_key: "anchor:5:3".into(),
            bell: Some(5),
            region: Some(3),
            class: "D".into(),
            payer: "P".into(),
            bid_milli: 100,
            cu_limit: 345_000,
            heap: None,
            first_valid_slot: 10,
            sent_slot: 10,
            landed_slot: None,
            status: status.into(),
            code: None,
        }
    }

    #[test]
    fn attempts_survive_reopen_and_the_lock_is_exclusive() {
        let dir = std::env::temp_dir().join(format!("keeper-journal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("j.sqlite");
        let l1 = lock(&p).unwrap();
        assert!(lock(&p).is_err(), "a second keeper is refused");
        {
            let j = Journal::open(&p).unwrap();
            j.record_attempt(&a("s1", "sent")).unwrap();
            j.record_attempt(&a("s2", "sent")).unwrap();
            j.set_status("s1", "landed", Some(11), None).unwrap();
            j.set_cursor("ingest", "42").unwrap();
            j.alert(11, "contested", "bell 5 region 3").unwrap();
        }
        let j = Journal::open(&p).unwrap();
        let f = j.in_flight().unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].sig, "s2");
        assert_eq!(j.attempts_of("anchor:5:3").unwrap().len(), 2);
        assert_eq!(j.cursor("ingest").unwrap().as_deref(), Some("42"));
        assert_eq!(
            j.status_counts().unwrap(),
            vec![("landed".into(), 1), ("sent".into(), 1)]
        );
        assert_eq!(j.alerts("contested").unwrap().len(), 1);
        drop(l1);
        assert!(lock(&p).is_ok(), "released with the file");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
