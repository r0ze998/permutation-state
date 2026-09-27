//! Crash safety for `frontier-localnet` (M1 contract §8.7): a transaction
//! write-ahead log (the ledger) plus account snapshots.
//!
//! **Files** (in `--data-dir`):
//!
//! - `ledger.wal`: a header, then framed records in chain order. A record
//!   is `len u32 ‖ kind u8 ‖ payload ‖ sha256(kind ‖ payload)[..8]`, where
//!   `len` counts kind, payload and checksum. Kinds: [`REC_BLOCK`] (slot,
//!   the game clock in ms, the scale, and every executed transaction with
//!   its full recorded outcome, in execution order, empty blocks included),
//!   [`REC_AIRDROP`], [`REC_DEPLOY`], [`REC_SET_ACCOUNT`]. The record of a
//!   block is written (and flushed, and by default `fsync`ed) before any of
//!   its results become visible over RPC, so a client never sees a status
//!   the node forgets after a crash.
//! - `snap-<slot>.bin`: every account, the slot, the game clock, the scale,
//!   the blockhash window, the ledger offset the state includes and the
//!   number of ledger entries before it; a sha256 trailer.
//!
//! **Restore** = the newest valid snapshot, then the ledger: records the
//! snapshot already includes only rebuild the history (statuses, feed,
//! block times); later records are **re-executed** with their recorded
//! clock values, and each re-execution must reproduce the recorded outcome
//! (error, units, logs, return data, post-state), else the restore fails.
//! A torn last record (a crash mid-write) is cut off. Without a snapshot
//! the whole ledger is re-executed from the empty chain the header names.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use solana_address::Address;
use solana_hash::Hash;
use solana_signature::Signature;

use fclient::ports::Account;

pub const WAL_MAGIC: &[u8; 8] = b"PSFLWAL1";
pub const SNAP_MAGIC: &[u8; 8] = b"PSFLSNP1";
pub const WAL_FILE: &str = "ledger.wal";

pub const REC_BLOCK: u8 = 1;
pub const REC_AIRDROP: u8 = 2;
pub const REC_DEPLOY: u8 = 3;
pub const REC_SET_ACCOUNT: u8 = 4;

/// Bytes of the WAL header: magic, run id, g0, slot0, scale.
pub const WAL_HEADER: u64 = 8 + 16 + 8 + 8 + 8;

// ---------------------------------------------------------------- codec

/// Little-endian writer.
#[derive(Default)]
pub struct W(pub Vec<u8>);

impl W {
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn i64(&mut self, v: i64) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn f64(&mut self, v: f64) -> &mut Self {
        self.u64(v.to_bits())
    }
    pub fn raw(&mut self, b: &[u8]) -> &mut Self {
        self.0.extend_from_slice(b);
        self
    }
    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.u32(b.len() as u32).raw(b)
    }
    pub fn str(&mut self, s: &str) -> &mut Self {
        self.bytes(s.as_bytes())
    }
    pub fn opt_str(&mut self, s: &Option<String>) -> &mut Self {
        match s {
            None => self.u8(0),
            Some(s) => self.u8(1).str(s),
        }
    }
    pub fn key(&mut self, k: &Address) -> &mut Self {
        self.raw(k.as_ref())
    }
    pub fn account(&mut self, a: &Option<Account>) -> &mut Self {
        match a {
            None => self.u8(0),
            Some(a) => self
                .u8(1)
                .u64(a.lamports)
                .key(&a.owner)
                .u8(a.executable as u8)
                .bytes(&a.data),
        }
    }
}

/// Little-endian reader; every read is bounds-checked.
pub struct R<'a> {
    pub b: &'a [u8],
    pub at: usize,
}

pub type Res<T> = Result<T, String>;

impl<'a> R<'a> {
    pub fn new(b: &'a [u8]) -> R<'a> {
        R { b, at: 0 }
    }
    pub fn take(&mut self, n: usize) -> Res<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .filter(|e| *e <= self.b.len())
            .ok_or_else(|| format!("truncated at {} (+{n})", self.at))?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }
    pub fn u8(&mut self) -> Res<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn u32(&mut self) -> Res<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("4")))
    }
    pub fn u64(&mut self) -> Res<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("8")))
    }
    pub fn i64(&mut self) -> Res<i64> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().expect("8")))
    }
    pub fn f64(&mut self) -> Res<f64> {
        Ok(f64::from_bits(self.u64()?))
    }
    pub fn bytes(&mut self) -> Res<&'a [u8]> {
        let n = self.u32()? as usize;
        self.take(n)
    }
    pub fn str(&mut self) -> Res<String> {
        String::from_utf8(self.bytes()?.to_vec()).map_err(|e| e.to_string())
    }
    pub fn opt_str(&mut self) -> Res<Option<String>> {
        Ok(match self.u8()? {
            0 => None,
            _ => Some(self.str()?),
        })
    }
    pub fn key(&mut self) -> Res<Address> {
        Ok(Address::new_from_array(
            self.take(32)?.try_into().expect("32"),
        ))
    }
    pub fn account(&mut self) -> Res<Option<Account>> {
        Ok(match self.u8()? {
            0 => None,
            _ => Some(Account {
                lamports: self.u64()?,
                owner: self.key()?,
                executable: self.u8()? != 0,
                data: self.bytes()?.to_vec(),
            }),
        })
    }
    pub fn done(&self) -> bool {
        self.at == self.b.len()
    }
}

pub fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

// ---------------------------------------------------------------- records

/// One executed transaction as the ledger keeps it (everything the RPC
/// serves about it, so history before a snapshot needs no re-execution).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxEntry {
    pub signature: Signature,
    pub wire: Vec<u8>,
    pub logs: Vec<String>,
    /// Debug form of the error (`fclient` parses custom codes out of it).
    pub err: Option<String>,
    /// JSON form of the error, as a JSON-RPC server serves it.
    pub err_json: Option<String>,
    pub code: Option<u32>,
    pub units: u64,
    pub fee: u64,
    pub post: Vec<(Address, Option<Account>)>,
    pub return_data: Vec<u8>,
    pub pre_balances: Vec<u64>,
    pub post_balances: Vec<u64>,
}

impl TxEntry {
    pub fn encode(&self, w: &mut W) {
        w.raw(self.signature.as_ref()).bytes(&self.wire);
        w.u32(self.logs.len() as u32);
        for l in &self.logs {
            w.str(l);
        }
        w.opt_str(&self.err).opt_str(&self.err_json);
        match self.code {
            None => w.u8(0),
            Some(c) => w.u8(1).u32(c),
        };
        w.u64(self.units).u64(self.fee);
        w.u32(self.post.len() as u32);
        for (k, a) in &self.post {
            w.key(k).account(a);
        }
        w.bytes(&self.return_data);
        w.u32(self.pre_balances.len() as u32);
        for b in &self.pre_balances {
            w.u64(*b);
        }
        w.u32(self.post_balances.len() as u32);
        for b in &self.post_balances {
            w.u64(*b);
        }
    }

    pub fn decode(r: &mut R) -> Res<TxEntry> {
        let signature = Signature::from(<[u8; 64]>::try_from(r.take(64)?).expect("64"));
        let wire = r.bytes()?.to_vec();
        let n = r.u32()? as usize;
        let mut logs = Vec::with_capacity(n.min(4_096));
        for _ in 0..n {
            logs.push(r.str()?);
        }
        let err = r.opt_str()?;
        let err_json = r.opt_str()?;
        let code = match r.u8()? {
            0 => None,
            _ => Some(r.u32()?),
        };
        let units = r.u64()?;
        let fee = r.u64()?;
        let n = r.u32()? as usize;
        let mut post = Vec::with_capacity(n.min(256));
        for _ in 0..n {
            post.push((r.key()?, r.account()?));
        }
        let return_data = r.bytes()?.to_vec();
        let n = r.u32()? as usize;
        let mut pre_balances = Vec::with_capacity(n.min(256));
        for _ in 0..n {
            pre_balances.push(r.u64()?);
        }
        let n = r.u32()? as usize;
        let mut post_balances = Vec::with_capacity(n.min(256));
        for _ in 0..n {
            post_balances.push(r.u64()?);
        }
        Ok(TxEntry {
            signature,
            wire,
            logs,
            err,
            err_json,
            code,
            units,
            fee,
            post,
            return_data,
            pre_balances,
            post_balances,
        })
    }
}

/// A ledger record.
#[derive(Clone, Debug, PartialEq)]
pub enum Record {
    Block {
        slot: u64,
        game_ms: i64,
        scale: f64,
        txs: Vec<TxEntry>,
    },
    /// A faucet credit; `wire` is its synthetic signed transfer (what
    /// `getTransaction` serves).
    Airdrop {
        slot: u64,
        key: Address,
        lamports: u64,
        signature: Signature,
        wire: Vec<u8>,
    },
    Deploy {
        program: Address,
        max_len: u64,
        authority: Option<Address>,
        so: Vec<u8>,
    },
    SetAccount {
        key: Address,
        account: Option<Account>,
    },
}

impl Record {
    pub fn encode(&self) -> (u8, Vec<u8>) {
        let mut w = W::default();
        let kind = match self {
            Record::Block {
                slot,
                game_ms,
                scale,
                txs,
            } => {
                w.u64(*slot).i64(*game_ms).f64(*scale).u32(txs.len() as u32);
                for t in txs {
                    t.encode(&mut w);
                }
                REC_BLOCK
            }
            Record::Airdrop {
                slot,
                key,
                lamports,
                signature,
                wire,
            } => {
                w.u64(*slot)
                    .key(key)
                    .u64(*lamports)
                    .raw(signature.as_ref())
                    .bytes(wire);
                REC_AIRDROP
            }
            Record::Deploy {
                program,
                max_len,
                authority,
                so,
            } => {
                w.key(program).u64(*max_len);
                match authority {
                    None => w.u8(0),
                    Some(a) => w.u8(1).key(a),
                };
                w.bytes(so);
                REC_DEPLOY
            }
            Record::SetAccount { key, account } => {
                w.key(key).account(account);
                REC_SET_ACCOUNT
            }
        };
        (kind, w.0)
    }

    pub fn decode(kind: u8, payload: &[u8]) -> Res<Record> {
        let mut r = R::new(payload);
        let rec = match kind {
            REC_BLOCK => {
                let slot = r.u64()?;
                let game_ms = r.i64()?;
                let scale = r.f64()?;
                let n = r.u32()? as usize;
                let mut txs = Vec::with_capacity(n.min(65_536));
                for _ in 0..n {
                    txs.push(TxEntry::decode(&mut r)?);
                }
                Record::Block {
                    slot,
                    game_ms,
                    scale,
                    txs,
                }
            }
            REC_AIRDROP => Record::Airdrop {
                slot: r.u64()?,
                key: r.key()?,
                lamports: r.u64()?,
                signature: Signature::from(<[u8; 64]>::try_from(r.take(64)?).expect("64")),
                wire: r.bytes()?.to_vec(),
            },
            REC_DEPLOY => {
                let program = r.key()?;
                let max_len = r.u64()?;
                let authority = match r.u8()? {
                    0 => None,
                    _ => Some(r.key()?),
                };
                Record::Deploy {
                    program,
                    max_len,
                    authority,
                    so: r.bytes()?.to_vec(),
                }
            }
            REC_SET_ACCOUNT => Record::SetAccount {
                key: r.key()?,
                account: r.account()?,
            },
            k => return Err(format!("unknown record kind {k}")),
        };
        if !r.done() {
            return Err(format!("record kind {kind}: trailing bytes"));
        }
        Ok(rec)
    }

    /// The framed bytes.
    pub fn frame(&self) -> Vec<u8> {
        let (kind, payload) = self.encode();
        let sum = sha256(&[&[kind], &payload]);
        let mut out = Vec::with_capacity(payload.len() + 13);
        out.extend_from_slice(&((1 + payload.len() + 8) as u32).to_le_bytes());
        out.push(kind);
        out.extend_from_slice(&payload);
        out.extend_from_slice(&sum[..8]);
        out
    }
}

// ---------------------------------------------------------------- the WAL

/// The chain parameters the WAL starts from (its header).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalHeader {
    pub run_id: [u8; 16],
    pub g0: i64,
    pub slot0: u64,
    pub scale: f64,
}

impl WalHeader {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.raw(WAL_MAGIC)
            .raw(&self.run_id)
            .i64(self.g0)
            .u64(self.slot0)
            .f64(self.scale);
        w.0
    }
    pub fn decode(b: &[u8]) -> Res<WalHeader> {
        let mut r = R::new(b);
        if r.take(8)? != WAL_MAGIC {
            return Err("not a frontier-localnet ledger".into());
        }
        Ok(WalHeader {
            run_id: r.take(16)?.try_into().expect("16"),
            g0: r.i64()?,
            slot0: r.u64()?,
            scale: r.f64()?,
        })
    }
}

/// The open ledger (append side).
pub struct Wal {
    pub path: PathBuf,
    file: BufWriter<File>,
    /// Bytes durably framed so far (the end of the last whole record).
    pub offset: u64,
    pub fsync: bool,
    pub header: WalHeader,
}

impl Wal {
    /// Creates a new ledger (refuses to overwrite one).
    pub fn create(dir: &Path, header: WalHeader, fsync: bool) -> Res<Wal> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(WAL_FILE);
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        f.write_all(&header.encode())
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(Wal {
            path,
            file: BufWriter::new(f),
            offset: WAL_HEADER,
            fsync,
            header,
        })
    }

    /// Opens an existing ledger for appending at `offset` (the end of the
    /// last valid record; anything after it is cut off first).
    pub fn reopen(path: &Path, header: WalHeader, offset: u64, fsync: bool) -> Res<Wal> {
        let f = OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        f.set_len(offset).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        let mut f = f;
        f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        Ok(Wal {
            path: path.to_path_buf(),
            file: BufWriter::new(f),
            offset,
            fsync,
            header,
        })
    }

    /// Appends one record and makes it durable (flushed to the OS, and
    /// `fsync`ed when configured) before returning.
    pub fn append(&mut self, rec: &Record) -> Res<()> {
        let b = rec.frame();
        self.file.write_all(&b).map_err(|e| e.to_string())?;
        self.file.flush().map_err(|e| e.to_string())?;
        if self.fsync {
            self.file.get_ref().sync_data().map_err(|e| e.to_string())?;
        }
        self.offset += b.len() as u64;
        Ok(())
    }

    /// Cuts the ledger back to `offset` (a rewind to a snapshot).
    pub fn truncate(&mut self, offset: u64) -> Res<()> {
        self.file.flush().map_err(|e| e.to_string())?;
        let f = self.file.get_mut();
        f.set_len(offset).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        self.offset = offset;
        Ok(())
    }
}

/// Reads a whole ledger: its header, the records with their end offsets,
/// and the end of the last valid record (a torn or corrupt tail stops the
/// read there; the caller cuts it off).
/// A read ledger: header, records with their end offsets, valid length.
pub type WalRead = (WalHeader, Vec<(u64, Record)>, u64);

pub fn read_wal(path: &Path) -> Res<WalRead> {
    let mut b = vec![];
    File::open(path)
        .and_then(|mut f| f.read_to_end(&mut b))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let header = WalHeader::decode(&b)?;
    let mut at = WAL_HEADER as usize;
    let mut out = vec![];
    loop {
        if at + 4 > b.len() {
            break;
        }
        let len = u32::from_le_bytes(b[at..at + 4].try_into().expect("4")) as usize;
        if len < 9 || at + 4 + len > b.len() {
            break;
        }
        let body = &b[at + 4..at + 4 + len];
        let (kp, sum) = body.split_at(len - 8);
        if sha256(&[kp])[..8] != *sum {
            break;
        }
        let Ok(rec) = Record::decode(kp[0], &kp[1..]) else {
            break;
        };
        at += 4 + len;
        out.push((at as u64, rec));
    }
    Ok((header, out, at as u64))
}

// ---------------------------------------------------------------- snapshots

/// A snapshot's contents.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub run_id: [u8; 16],
    pub slot: u64,
    pub game_ms: i64,
    pub scale: f64,
    pub next_snapshot_ms: i64,
    pub blockhashes: Vec<(Hash, u64)>,
    /// Ledger bytes the state includes.
    pub wal_offset: u64,
    /// History entries (landed transactions and airdrops) before it.
    pub landed_len: u64,
    /// Signature of the last of those entries (zero if none): a rewind
    /// refuses a snapshot from another timeline.
    pub last_sig: Signature,
    pub accounts: Vec<(Address, Account)>,
}

impl Snapshot {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.raw(SNAP_MAGIC)
            .raw(&self.run_id)
            .u64(self.slot)
            .i64(self.game_ms)
            .f64(self.scale)
            .i64(self.next_snapshot_ms)
            .u32(self.blockhashes.len() as u32);
        for (h, s) in &self.blockhashes {
            w.raw(h.as_ref()).u64(*s);
        }
        w.u64(self.wal_offset)
            .u64(self.landed_len)
            .raw(self.last_sig.as_ref())
            .u64(self.accounts.len() as u64);
        for (k, a) in &self.accounts {
            w.key(k).account(&Some(a.clone()));
        }
        let sum = sha256(&[&w.0]);
        w.raw(&sum);
        w.0
    }

    pub fn decode(b: &[u8]) -> Res<Snapshot> {
        if b.len() < 40 {
            return Err("snapshot too short".into());
        }
        let (body, sum) = b.split_at(b.len() - 32);
        if sha256(&[body]) != *sum {
            return Err("snapshot checksum mismatch".into());
        }
        let mut r = R::new(body);
        if r.take(8)? != SNAP_MAGIC {
            return Err("not a frontier-localnet snapshot".into());
        }
        let run_id = r.take(16)?.try_into().expect("16");
        let slot = r.u64()?;
        let game_ms = r.i64()?;
        let scale = r.f64()?;
        let next_snapshot_ms = r.i64()?;
        let n = r.u32()? as usize;
        let mut blockhashes = Vec::with_capacity(n.min(1_024));
        for _ in 0..n {
            blockhashes.push((
                Hash::new_from_array(r.take(32)?.try_into().expect("32")),
                r.u64()?,
            ));
        }
        let wal_offset = r.u64()?;
        let landed_len = r.u64()?;
        let last_sig = Signature::from(<[u8; 64]>::try_from(r.take(64)?).expect("64"));
        let n = r.u64()? as usize;
        let mut accounts = Vec::with_capacity(n.min(1 << 20));
        for _ in 0..n {
            let k = r.key()?;
            let a = r.account()?.ok_or("snapshot: absent account")?;
            accounts.push((k, a));
        }
        if !r.done() {
            return Err("snapshot: trailing bytes".into());
        }
        Ok(Snapshot {
            run_id,
            slot,
            game_ms,
            scale,
            next_snapshot_ms,
            blockhashes,
            wal_offset,
            landed_len,
            last_sig,
            accounts,
        })
    }

    /// Writes atomically (temp file, fsync, rename).
    pub fn write(&self, path: &Path) -> Res<()> {
        let tmp = path.with_extension("tmp");
        {
            let mut f = File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
            f.write_all(&self.encode())
                .and_then(|_| f.sync_all())
                .map_err(|e| e.to_string())?;
        }
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn read(path: &Path) -> Res<Snapshot> {
        let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Snapshot::decode(&b).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// `snap-<slot>.bin` files in `dir`, newest first.
pub fn snapshots_in(dir: &Path) -> Vec<(u64, PathBuf)> {
    let mut v: Vec<(u64, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_str()?.to_string();
            let slot = name
                .strip_prefix("snap-")?
                .strip_suffix(".bin")?
                .parse()
                .ok()?;
            Some((slot, p))
        })
        .collect();
    v.sort_by_key(|b| std::cmp::Reverse(b.0));
    v
}

pub fn snapshot_path(dir: &Path, slot: u64) -> PathBuf {
    dir.join(format!("snap-{slot:012}.bin"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(n: u8) -> TxEntry {
        TxEntry {
            signature: Signature::from([n; 64]),
            wire: vec![n; 40],
            logs: vec!["Program log: a".into(), "b".into()],
            err: Some("InstructionError(3, Custom(12))".into()),
            err_json: Some(r#"{"InstructionError":[3,{"Custom":12}]}"#.into()),
            code: Some(12),
            units: 1_234,
            fee: 5_000,
            post: vec![
                (
                    Address::new_from_array([1; 32]),
                    Some(Account {
                        lamports: 9,
                        data: vec![1, 2, 3],
                        owner: Address::new_from_array([2; 32]),
                        executable: false,
                    }),
                ),
                (Address::new_from_array([3; 32]), None),
            ],
            return_data: vec![7; 5],
            pre_balances: vec![10, 20],
            post_balances: vec![5, 25],
        }
    }

    #[test]
    fn records_round_trip_and_torn_tails_stop_the_read() {
        let dir = std::env::temp_dir().join(format!("psf-wal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let h = WalHeader {
            run_id: [4; 16],
            g0: 1_785_542_400,
            slot0: 1,
            scale: 20.0,
        };
        let mut wal = Wal::create(&dir, h, false).unwrap();
        let recs = vec![
            Record::Block {
                slot: 2,
                game_ms: 1_785_542_408_000,
                scale: 20.0,
                txs: vec![entry(1), entry(2)],
            },
            Record::Airdrop {
                slot: 2,
                key: Address::new_from_array([5; 32]),
                lamports: 77,
                signature: Signature::from([6; 64]),
                wire: vec![1, 2, 3],
            },
            Record::Deploy {
                program: Address::new_from_array([7; 32]),
                max_len: 100,
                authority: Some(Address::new_from_array([8; 32])),
                so: vec![0x7f, b'E', b'L', b'F'],
            },
            Record::SetAccount {
                key: Address::new_from_array([9; 32]),
                account: None,
            },
            Record::Block {
                slot: 3,
                game_ms: 1_785_542_416_000,
                scale: 2.0,
                txs: vec![],
            },
        ];
        for r in &recs {
            wal.append(r).unwrap();
        }
        let end = wal.offset;
        drop(wal);
        let (hh, got, valid) = read_wal(&dir.join(WAL_FILE)).unwrap();
        assert_eq!(hh, h);
        assert_eq!(valid, end);
        assert_eq!(got.iter().map(|(_, r)| r.clone()).collect::<Vec<_>>(), recs);
        // A torn record (half written) and a flipped byte both stop the read.
        let mut f = OpenOptions::new()
            .append(true)
            .open(dir.join(WAL_FILE))
            .unwrap();
        let torn = recs[0].frame();
        f.write_all(&torn[..torn.len() / 2]).unwrap();
        drop(f);
        let (_, got, valid) = read_wal(&dir.join(WAL_FILE)).unwrap();
        assert_eq!((got.len(), valid), (recs.len(), end));
        let mut b = std::fs::read(dir.join(WAL_FILE)).unwrap();
        b[WAL_HEADER as usize + 10] ^= 1;
        std::fs::write(dir.join(WAL_FILE), &b).unwrap();
        let (_, got, valid) = read_wal(&dir.join(WAL_FILE)).unwrap();
        assert_eq!((got.len(), valid), (0, WAL_HEADER));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshots_round_trip_and_refuse_corruption() {
        let s = Snapshot {
            run_id: [1; 16],
            slot: 99,
            game_ms: -5,
            scale: 20.0,
            next_snapshot_ms: 1_000,
            blockhashes: vec![(Hash::new_from_array([2; 32]), 98)],
            wal_offset: 1_234,
            landed_len: 7,
            last_sig: Signature::from([9; 64]),
            accounts: vec![(
                Address::new_from_array([3; 32]),
                Account {
                    lamports: 1,
                    data: vec![4; 10],
                    owner: Address::new_from_array([5; 32]),
                    executable: true,
                },
            )],
        };
        let b = s.encode();
        assert_eq!(Snapshot::decode(&b).unwrap(), s);
        let mut bad = b.clone();
        bad[20] ^= 1;
        assert!(Snapshot::decode(&bad).is_err());
    }
}
