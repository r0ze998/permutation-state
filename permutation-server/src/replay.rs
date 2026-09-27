//! Chain-first replay (the verifier's core, shared with the server): the
//! program's log records, read from any set of transactions and bound to
//! their season, and the chain of state roots they form.
//!
//! * `program_records` / `tx_records`: the records a transaction's logs carry
//!   that the season program itself emitted (the innermost invocation), each
//!   with the account list of the instruction that emitted it, so a record
//!   can be tied to the account the program read its season from.
//! * `ChainData`: records by kind. `PS_TICK`, `PS_INPUT` and `PS_SALTS` are
//!   bound to a season by value (roots and hashes); `PS_COMMITS` carries no
//!   binding, so it counts only when the instruction that logged it names
//!   this season's world chunk 0 where the program reads it (`accounts[0]`).
//! * `follow`: from a root, take THE advancing `PS_TICK` whose `pre_root` is
//!   the current root, replay it with the record's own tick and stop, and
//!   compare the post-state root. The gateway's index is never a source.
//! * `check_seals`: the sealed orders of a tick (commitments, salts, batches
//!   and the randomness), on its first part.
//!
//! A contradiction is a `fail`; data that could not be found is `missing`
//! (the caller scans for it, and reports what is still missing as
//! incomplete, never as a failure).

use std::collections::{BTreeMap, HashMap, HashSet};

use borsh::BorshDeserialize;
use permutation_rules::gov::NOBODY;
use permutation_rules::orders::order_commitment;
use permutation_rules::rng::Salt;
use permutation_rules::state::WorldState;
use permutation_rules::tick::{run_phase, TickInput, PHASE_COUNT};
use permutation_rules::Ruleset;
use serde_json::Value;

use crate::codec::{base64, hex};

/// A log record's fields, tag first (sol_log_data, base64-decoded).
pub type Record = Vec<Vec<u8>>;

/// One sealed commitment as `PS_COMMITS` logs it: (civ, role, member, hash).
pub type Commitment = (u16, u8, u32, [u8; 32]);

/// Where a record was emitted: the top-level instruction (`top`) and, inside
/// a CPI, which of that instruction's inner instructions (`inner`, in
/// execution order). `truncated`: the logs were cut before the record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub top: usize,
    pub inner: Option<usize>,
    pub truncated: bool,
}

/// One program record and the accounts of the instruction that emitted it
/// (`None`: not attributable).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Emitted {
    pub record: Record,
    pub accounts: Option<Vec<String>>,
}

/// The program's records in one transaction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TxRecords {
    pub sig: String,
    pub slot: u64,
    pub emitted: Vec<Emitted>,
}

/// The `PS_TICK` bit that marks a degraded step (v9 records).
pub const DEGRADED: u8 = 0x80;

/// The stop a `PS_TICK`'s `to` byte means (contract §1.5): a v8 record (6
/// fields) runs `min(to, 12)`; a v9 record (9 fields) carries the degraded
/// flag in bit 7 and runs `min(to & 0x7f, 12)`. Returns (stop, degraded).
pub fn effective_to(raw: u8, fields: usize) -> (u8, bool) {
    if fields >= 9 {
        ((raw & !DEGRADED).min(PHASE_COUNT), raw & DEGRADED != 0)
    } else {
        (raw.min(PHASE_COUNT), false)
    }
}

/// The records `program` emitted in `logs` (a transaction's log messages),
/// each with its frame. Only `Program data:` lines logged while `program`
/// is the innermost invocation count, so no other program can forge one; and
/// a line is an invoke/success/failed frame line only if its program id has
/// no `:` (a program can only print `Program log:`, `Program data:` and
/// `Program return:` lines, so it cannot push or pop a fake frame).
pub fn program_records(logs: &[&str], program: &str) -> Result<Vec<(Record, Frame)>, String> {
    walk(logs, Some(program))
}

/// `program_records`, or every record when `program` is `None`.
pub(crate) fn walk(logs: &[&str], program: Option<&str>) -> Result<Vec<(Record, Frame)>, String> {
    Ok(frames(logs, program)?.0)
}

/// `walk`, and the program id of every top-level frame (`invoke [1]`) seen
/// before the logs were cut, in order.
fn frames<'a>(
    logs: &[&'a str],
    program: Option<&str>,
) -> Result<(Vec<(Record, Frame)>, Vec<&'a str>), String> {
    // The invocation stack: program id and, inside a CPI, its inner index.
    let mut stack: Vec<(&str, Option<usize>)> = Vec::new();
    let mut tops: Vec<&str> = Vec::new();
    let mut top: Option<usize> = None;
    let mut inner = 0usize; // inner instructions of the current top-level one so far
    let mut truncated = false;
    let mut out = Vec::new();
    for l in logs {
        if *l == "Log truncated" {
            truncated = true;
            continue;
        }
        if let Some(rest) = l.strip_prefix("Program ") {
            let mut words = rest.split(' ');
            let (id, what) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
            if !id.contains(':') {
                if what == "invoke" {
                    let depth = words
                        .next()
                        .and_then(|d| d.strip_prefix('[')?.strip_suffix(']')?.parse().ok())
                        .unwrap_or(stack.len() + 1);
                    if depth <= 1 {
                        if !truncated {
                            tops.push(id);
                        }
                        top = Some(top.map_or(0, |t| t + 1));
                        inner = 0;
                        stack.clear();
                        stack.push((id, None));
                    } else {
                        stack.truncate(depth - 1);
                        stack.push((id, Some(inner)));
                        inner += 1;
                    }
                } else if (what == "success" || what == "failed:")
                    && stack.last().map(|s| s.0) == Some(id)
                {
                    stack.pop();
                }
                continue;
            }
        }
        let Some(data) = l.strip_prefix("Program data: ") else {
            continue;
        };
        let cpi = match (stack.last(), program) {
            (Some(&(id, cpi)), Some(p)) if id == p => cpi,
            (_, Some(_)) => continue,
            (last, None) => last.and_then(|s| s.1),
        };
        let record = data.split(' ').map(base64).collect::<Result<Vec<_>, _>>()?;
        out.push((
            record,
            Frame {
                top: top.unwrap_or(0),
                inner: cpi,
                truncated,
            },
        ));
    }
    Ok((out, tops))
}

/// The precompiles (Ed25519, secp256k1, secp256r1): the runtime runs them
/// without an `invoke [1]` line (Agave's `InvokeContext::process_precompile`
/// never logs one), so they have no top-level frame.
const PRECOMPILES: [&str; 3] = [
    "Ed25519SigVerify111111111111111111111111111",
    "KeccakSecp256k11111111111111111111111111111",
    "Secp256r1SigVerify1111111111111111111111111",
];

/// The message instruction each top-level frame ran: the instructions in
/// order with the precompiles skipped, and each frame's program must be its
/// instruction's. `None` when they do not line up (then nothing in the
/// transaction is attributable, so a record cannot be tied to another
/// instruction's accounts).
fn top_instructions(tx: &Value, keys: &[String], tops: &[&str], cut: bool) -> Option<Vec<usize>> {
    let program = |ix: &Value| keys.get(ix["programIdIndex"].as_u64()? as usize);
    let run: Vec<(usize, &String)> = tx["transaction"]["message"]["instructions"]
        .as_array()?
        .iter()
        .enumerate()
        .map(|(i, ix)| Some((i, program(ix)?)))
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .filter(|(_, id)| !PRECOMPILES.contains(&id.as_str()))
        .collect();
    let lined_up = if cut {
        tops.len() <= run.len()
    } else {
        tops.len() == run.len()
    };
    if !lined_up || tops.iter().zip(&run).any(|(t, (_, id))| *t != id.as_str()) {
        return None;
    }
    Some(run[..tops.len()].iter().map(|(i, _)| *i).collect())
}

/// The account keys of a `getTransaction` result (encoding `json`, or
/// `jsonParsed` objects), with a v0 transaction's loaded addresses.
fn account_keys(tx: &Value) -> Vec<String> {
    let key = |k: &Value| {
        k.as_str()
            .or_else(|| k["pubkey"].as_str())
            .unwrap_or("")
            .to_string()
    };
    let mut keys: Vec<String> = tx["transaction"]["message"]["accountKeys"]
        .as_array()
        .map(|a| a.iter().map(key).collect())
        .unwrap_or_default();
    for kind in ["writable", "readonly"] {
        if let Some(a) = tx["meta"]["loadedAddresses"][kind].as_array() {
            keys.extend(a.iter().map(key));
        }
    }
    keys
}

/// The accounts of the instruction that emitted a record in frame `f`, if
/// that instruction is `program`'s and can be found (`tops`: the message
/// instruction of each top-level frame; a CPI needs `innerInstructions`).
fn instruction_accounts(
    tx: &Value,
    keys: &[String],
    tops: &[usize],
    f: Frame,
    program: &str,
) -> Option<Vec<String>> {
    if f.truncated {
        return None;
    }
    let top = *tops.get(f.top)?;
    let ix = match f.inner {
        None => &tx["transaction"]["message"]["instructions"][top],
        Some(n) => tx["meta"]["innerInstructions"]
            .as_array()?
            .iter()
            .find(|e| e["index"].as_u64() == Some(top as u64))?["instructions"]
            .get(n)?,
    };
    let id = keys.get(ix["programIdIndex"].as_u64()? as usize)?;
    if id != program {
        return None;
    }
    ix["accounts"]
        .as_array()?
        .iter()
        .map(|a| keys.get(a.as_u64()? as usize).cloned())
        .collect()
}

/// The program's records in `tx` (a `getTransaction` result, encoding
/// `json`), each with the account list of the instruction that emitted it.
/// A failed transaction has none.
pub fn tx_records(sig: &str, tx: &Value, program: &str) -> Result<TxRecords, String> {
    if tx.is_null() {
        return Err(format!("transaction {} not found", short(sig)));
    }
    let mut out = TxRecords {
        sig: sig.to_string(),
        slot: tx["slot"].as_u64().unwrap_or(0),
        emitted: Vec::new(),
    };
    if !tx["meta"]["err"].is_null() {
        return Ok(out);
    }
    let logs: Vec<&str> = tx["meta"]["logMessages"]
        .as_array()
        .ok_or_else(|| format!("transaction {}: no logs", short(sig)))?
        .iter()
        .filter_map(|l| l.as_str())
        .collect();
    let keys = account_keys(tx);
    let (records, top_ids) = frames(&logs, Some(program))?;
    let cut = logs.contains(&"Log truncated");
    let tops = top_instructions(tx, &keys, &top_ids, cut);
    out.emitted = records
        .into_iter()
        .map(|(record, f)| Emitted {
            accounts: tops
                .as_ref()
                .and_then(|t| instruction_accounts(tx, &keys, t, f, program)),
            record,
        })
        .collect();
    Ok(out)
}

/// The first characters of a signature or a hex root, for messages.
pub fn short(s: &str) -> &str {
    &s[..s.len().min(12)]
}

fn hex8(b: &[u8]) -> String {
    hex(&b[..b.len().min(8)])
}

/// One `PS_TICK` record and where it was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickRec {
    /// The transaction, and the record's position among its `PS_TICK`s.
    pub sig: String,
    pub slot: u64,
    pub pos: usize,
    pub tick: u16,
    /// `to` as logged (raw), and what it means (`effective_to`).
    pub to: u8,
    pub stop: u8,
    pub degraded: bool,
    pub pre: [u8; 32],
    pub post: [u8; 32],
    pub input_hash: [u8; 32],
    /// Every field (v9 records carry the randomness in fields 6..8).
    pub fields: Record,
    /// Logged by an instruction on this season's world chunk 0.
    pub bound: bool,
}

/// A tick input's `PS_INPUT` chunks (they share one hash).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputParts {
    pub tick: u16,
    pub total: u16,
    pub chunks: BTreeMap<u16, Vec<u8>>,
    pub sigs: Vec<String>,
}

/// One `PS_SALTS` record: tick, its third field raw (v8: the pre-state root),
/// the salts and the transaction.
pub type SaltsRec = (u16, [u8; 32], Vec<Salt>, String);

/// The program's records by kind, from any set of transactions.
#[derive(Clone, Debug, Default)]
pub struct ChainData {
    /// This season's world chunk 0 (base58).
    pub anchor: String,
    /// Every transaction added: signature → slot.
    pub fetched: HashMap<String, u64>,
    pub ticks: Vec<TickRec>,
    /// `PS_INPUT` by the input's hash.
    pub inputs: HashMap<[u8; 32], InputParts>,
    /// `PS_COMMITS` logged on this season's chunk 0, by tick: transaction and list.
    pub commits: HashMap<u16, Vec<(String, Vec<Commitment>)>>,
    pub salts: Vec<SaltsRec>,
    /// Every other record (with its attribution), by transaction.
    pub other: Vec<(String, Emitted)>,
    /// (tick, slot) of every tick record logged on this season's chunk 0
    /// (`PS_TICK`, `PS_COMMITS`, `PS_INPUT`, `PS_SALTS`): bounds scan windows.
    pub bound_at: Vec<(u16, u64)>,
    /// `PS_COMMITS` records dropped as not bound to this season.
    pub foreign: usize,
    /// Records that did not decode, or chunks that disagree.
    pub malformed: Vec<String>,
}

fn arr32(f: &[u8]) -> Option<[u8; 32]> {
    f.try_into().ok()
}

fn u16_of(f: &[u8]) -> Option<u16> {
    Some(u16::from_le_bytes(f.try_into().ok()?))
}

impl ChainData {
    pub fn new(anchor: &str) -> ChainData {
        ChainData {
            anchor: anchor.to_string(),
            ..Default::default()
        }
    }

    /// Add a transaction's records (once per signature).
    pub fn add_tx(&mut self, tx: &TxRecords) {
        if self.fetched.contains_key(&tx.sig) {
            return;
        }
        self.fetched.insert(tx.sig.clone(), tx.slot);
        let mut pos = 0;
        for e in &tx.emitted {
            let bound = e.accounts.as_ref().and_then(|a| a.first()) == Some(&self.anchor);
            let tag = e.record.first().map(|t| t.as_slice()).unwrap_or_default();
            let added = match tag {
                b"PS_TICK" => self.add_tick(tx, e, bound, &mut pos),
                b"PS_INPUT" => self.add_input(tx, e),
                b"PS_COMMITS" => self.add_commits(tx, e, bound),
                b"PS_SALTS" => self.add_salts(tx, e),
                _ => {
                    self.other.push((tx.sig.clone(), e.clone()));
                    Ok(None)
                }
            };
            match added {
                Ok(Some(tick)) if bound => self.bound_at.push((tick, tx.slot)),
                Ok(_) => {}
                Err(why) => self.malformed.push(format!(
                    "{} in {}: {why}",
                    String::from_utf8_lossy(tag),
                    short(&tx.sig)
                )),
            }
        }
    }

    fn add_tick(
        &mut self,
        tx: &TxRecords,
        e: &Emitted,
        bound: bool,
        pos: &mut usize,
    ) -> Result<Option<u16>, String> {
        let f = &e.record;
        let bad = || "fields too short".to_string();
        if f.len() < 6 || f[2].len() != 1 {
            return Err(bad());
        }
        let tick = u16_of(&f[1]).ok_or_else(bad)?;
        let (stop, degraded) = effective_to(f[2][0], f.len());
        self.ticks.push(TickRec {
            sig: tx.sig.clone(),
            slot: tx.slot,
            pos: *pos,
            tick,
            to: f[2][0],
            stop,
            degraded,
            pre: arr32(&f[3]).ok_or_else(bad)?,
            post: arr32(&f[4]).ok_or_else(bad)?,
            input_hash: arr32(&f[5]).ok_or_else(bad)?,
            fields: f.clone(),
            bound,
        });
        *pos += 1;
        Ok(Some(tick))
    }

    fn add_input(&mut self, tx: &TxRecords, e: &Emitted) -> Result<Option<u16>, String> {
        let f = &e.record;
        let bad = || "fields too short".to_string();
        let (tick, chunk, total) = (
            u16_of(f.get(1).ok_or_else(bad)?).ok_or_else(bad)?,
            u16_of(f.get(2).ok_or_else(bad)?).ok_or_else(bad)?,
            u16_of(f.get(3).ok_or_else(bad)?).ok_or_else(bad)?,
        );
        let hash = arr32(f.get(4).ok_or_else(bad)?).ok_or_else(bad)?;
        let bytes = f.get(5).cloned().unwrap_or_default();
        if chunk >= total {
            return Err(format!("chunk {chunk} of {total}"));
        }
        let p = self.inputs.entry(hash).or_insert_with(|| InputParts {
            tick,
            total,
            ..Default::default()
        });
        if p.tick != tick || p.total != total {
            return Err(format!(
                "two PS_INPUT records of input {} disagree on its tick or size",
                hex8(&hash)
            ));
        }
        match p.chunks.get(&chunk) {
            Some(b) if *b != bytes => {
                return Err(format!(
                    "two PS_INPUT chunks {chunk} of input {} disagree",
                    hex8(&hash)
                ))
            }
            Some(_) => {}
            None => {
                p.chunks.insert(chunk, bytes);
            }
        }
        if !p.sigs.contains(&tx.sig) {
            p.sigs.push(tx.sig.clone());
        }
        Ok(Some(tick))
    }

    fn add_commits(
        &mut self,
        tx: &TxRecords,
        e: &Emitted,
        bound: bool,
    ) -> Result<Option<u16>, String> {
        let f = &e.record;
        let tick = f.get(1).and_then(|t| u16_of(t)).ok_or("fields too short")?;
        let list: Vec<Commitment> =
            Vec::try_from_slice(f.get(2).ok_or("fields too short")?).map_err(|e| e.to_string())?;
        if !bound {
            self.foreign += 1;
            return Ok(None);
        }
        self.commits
            .entry(tick)
            .or_default()
            .push((tx.sig.clone(), list));
        Ok(Some(tick))
    }

    fn add_salts(&mut self, tx: &TxRecords, e: &Emitted) -> Result<Option<u16>, String> {
        let f = &e.record;
        let bad = || "fields too short".to_string();
        let tick = u16_of(f.get(1).ok_or_else(bad)?).ok_or_else(bad)?;
        let key = arr32(f.get(2).ok_or_else(bad)?).ok_or_else(bad)?;
        let salts: Vec<Salt> =
            Vec::try_from_slice(f.get(3).ok_or_else(bad)?).map_err(|e| e.to_string())?;
        self.salts.push((tick, key, salts, tx.sig.clone()));
        Ok(Some(tick))
    }

    /// The input with this hash, if every chunk is here and the whole hashes to it.
    pub fn input(&self, hash: &[u8; 32]) -> Option<Vec<u8>> {
        let p = self.inputs.get(hash)?;
        if p.chunks.len() != p.total as usize {
            return None;
        }
        let bytes: Vec<u8> = p.chunks.values().flatten().copied().collect();
        (permutation_rules::hash::sha256(&[&bytes]) == *hash).then_some(bytes)
    }

    /// Why `input(hash)` has nothing.
    pub fn input_gap(&self, hash: &[u8; 32]) -> String {
        match self.inputs.get(hash) {
            None => format!("no PS_INPUT record of input {}", hex8(hash)),
            Some(p) => match (0..p.total).find(|c| !p.chunks.contains_key(c)) {
                Some(c) => format!("PS_INPUT chunk {} of {} missing", c + 1, p.total),
                None => "its PS_INPUT chunks do not hash to the input's hash".into(),
            },
        }
    }
}

/// Data the replay needs and could not find (the caller scans for it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Missing {
    /// No record advances from `root` (at `tick`).
    Successor { tick: u16, root: [u8; 32] },
    /// The tick's input is not fully published (`hash`).
    Input { tick: u16, hash: [u8; 32] },
    /// No `PS_COMMITS` of this season for the tick.
    Commits { tick: u16 },
    /// No `PS_SALTS` for the tick on its key.
    Salts { tick: u16 },
}

impl Missing {
    pub fn tick(&self) -> u16 {
        match self {
            Missing::Successor { tick, .. }
            | Missing::Input { tick, .. }
            | Missing::Commits { tick }
            | Missing::Salts { tick } => *tick,
        }
    }
}

/// The sealed orders seen across the season.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SealStats {
    /// Ticks whose seals were checked.
    pub ticks: usize,
    pub batches: usize,
    pub commitments: usize,
    pub gov: usize,
    /// Commitments with no revealed batch: (tick, civ, role, member).
    pub unrevealed: Vec<(u16, u16, u8, u32)>,
}

/// What `follow` needs about the season besides its records.
#[derive(Clone, Debug)]
pub struct SealCtx {
    pub nations: u16,
    pub season_id: u64,
    /// This season's world chunk 0.
    pub anchor: String,
}

/// Where `follow` ended.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// Advancing records replayed.
    pub steps: usize,
    /// The start root and every post-state root reached.
    pub visited: HashSet<[u8; 32]>,
    pub end_root: [u8; 32],
    /// The transaction (and its slot) of the last record replayed.
    pub last_sig: Option<String>,
    pub last_slot: Option<u64>,
    /// A contradiction: no scan can fix it.
    pub fail: Option<String>,
    /// The contradiction is in a tick's sealed orders (`check_seals`).
    pub seal_fail: bool,
    /// Data not found (scan for it).
    pub missing: Option<Missing>,
    pub seals: SealStats,
}

/// Why `check_seals` did not pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SealError {
    Missing(Missing),
    Fail(String),
}

/// Whether two records are the same step (the same record can be listed in
/// several index lines or fetched twice).
fn same_step(a: &TickRec, b: &TickRec) -> bool {
    (a.tick, a.stop, a.degraded, a.post, a.input_hash)
        == (b.tick, b.stop, b.degraded, b.post, b.input_hash)
}

/// Follow the chain of state roots from `state` (a tick's start) over the
/// advancing `PS_TICK` records of `data`, replaying each with the record's
/// own tick and stop, until no record continues (`missing`, unless the root
/// is `target`), a contradiction (`fail`) or missing data (`missing`).
pub fn follow(
    state: &mut WorldState,
    rules: &Ruleset,
    data: &ChainData,
    season: &SealCtx,
    target: Option<&[u8; 32]>,
) -> Outcome {
    let mut by_pre: HashMap<[u8; 32], Vec<&TickRec>> = HashMap::new();
    for r in data.ticks.iter().filter(|r| r.pre != r.post) {
        let v = by_pre.entry(r.pre).or_default();
        if !v.iter().any(|x| same_step(x, r)) {
            v.push(r);
        }
    }
    let mut out = Outcome::default();
    // The input of the tick being replayed: its hash and bytes, decoded.
    let mut current: Option<([u8; 32], TickInput)> = None;
    loop {
        let root = match state.state_root() {
            Ok(r) => r,
            Err(e) => {
                out.fail = Some(format!(
                    "tick {}: the world does not encode: {e:?}",
                    state.tick
                ));
                break;
            }
        };
        out.visited.insert(root);
        out.end_root = root;
        let Some(c) = by_pre.get(&root) else {
            if target != Some(&root) {
                out.missing = Some(Missing::Successor {
                    tick: state.tick,
                    root,
                });
            }
            break;
        };
        let tick = state.tick;
        if c.len() > 1 {
            out.fail = Some(format!(
                "tick {tick}: two records advance from root {} (two histories)",
                hex8(&root)
            ));
            break;
        }
        let r = c[0];
        // The format, on every part (the seal check reads it on the first).
        if !(r.fields.len() == 6 || r.fields.len() >= 9) {
            out.fail = Some(format_error(r.tick, r.fields.len()));
            break;
        }
        if r.tick != tick {
            out.fail = Some(format!(
                "tick record says tick {}, the replay is at tick {tick}",
                r.tick
            ));
            break;
        }
        if r.degraded {
            out.fail = Some(format!(
                "tick {tick}: a degraded step (v9 PS_TICK); this verifier cannot replay it"
            ));
            break;
        }
        if state.phase_cursor == 0 {
            let Some(bytes) = data.input(&r.input_hash) else {
                out.missing = Some(Missing::Input {
                    tick,
                    hash: r.input_hash,
                });
                break;
            };
            if data.inputs[&r.input_hash].tick != tick {
                out.fail = Some(format!(
                    "tick {tick}: its input was published for tick {}",
                    data.inputs[&r.input_hash].tick
                ));
                break;
            }
            let input = match TickInput::try_from_slice(&bytes) {
                Ok(i) => i,
                Err(e) => {
                    out.fail = Some(format!("tick {tick}: its input does not decode: {e}"));
                    break;
                }
            };
            match check_seals(state, r, &input, data, season, &mut out.seals) {
                Ok(()) => {}
                Err(SealError::Missing(m)) => {
                    out.missing = Some(m);
                    break;
                }
                Err(SealError::Fail(why)) => {
                    out.fail = Some(why);
                    out.seal_fail = true;
                    break;
                }
            }
            current = Some((r.input_hash, input));
        } else if current.as_ref().map(|c| c.0) != Some(r.input_hash) {
            out.fail = Some(format!("tick {tick}: its parts resolved different inputs"));
            break;
        }
        let input = &current.as_ref().expect("set on the tick's first part").1;
        while state.phase_cursor < r.stop {
            let p = state.phase_cursor;
            if let Err(e) = run_phase(state, rules, input, p) {
                out.fail = Some(format!("tick {tick} phase {p}: the rules refuse it: {e:?}"));
                return out;
            }
            if state.phase_cursor == 0 {
                break;
            }
        }
        let post = state.state_root().unwrap_or_default();
        if post != r.post {
            out.fail = Some(format!(
                "tick {tick} (to {}): recomputed root {} ≠ on-chain {}",
                r.stop,
                hex8(&post),
                hex8(&r.post)
            ));
            break;
        }
        out.steps += 1;
        out.last_sig = Some(r.sig.clone());
        out.last_slot = Some(r.slot);
    }
    out
}

/// The tick randomness of a v8 program (`PS_TICK` with 6 fields): the
/// pre-state root and every revealed salt, in (civ, role) order. It was
/// `permutation_rules::rng::tick_vrf` until the rules stopped deriving it
/// (v9 draws it from the VRF, `permutation_chain::randomness::tick_vrf`);
/// kept here for the v8 arm.
pub fn tick_vrf_v8(pre_root: &[u8; 32], salts: &[Salt]) -> [u8; 32] {
    let parts: Vec<[u8; 35]> = salts
        .iter()
        .map(|(civ, role, salt)| {
            let mut p = [0u8; 35];
            p[..2].copy_from_slice(&civ.to_le_bytes());
            p[2] = *role;
            p[3..].copy_from_slice(salt);
            p
        })
        .collect();
    let mut all: Vec<&[u8]> = vec![b"permutation-rules/tick-vrf".as_slice(), pre_root];
    all.extend(parts.iter().map(|p| p.as_slice()));
    permutation_rules::hash::sha256(&all)
}

/// The key a tick's `PS_SALTS` is logged on, by `PS_TICK` format: the
/// pre-state root in v8 (6 fields). v9 records (9 fields) are WP11's arm.
fn salts_key(rec: &TickRec) -> Result<[u8; 32], SealError> {
    match rec.fields.len() {
        6 => Ok(rec.pre),
        n => Err(SealError::Fail(format_error(rec.tick, n))),
    }
}

/// The tick randomness, by `PS_TICK` format: v8 derives it from the
/// pre-state root and the revealed salts.
fn check_randomness(rec: &TickRec, salts: &[Salt], input: &TickInput) -> Result<(), SealError> {
    match rec.fields.len() {
        6 if tick_vrf_v8(&rec.pre, salts) == input.vrf => Ok(()),
        6 => Err(SealError::Fail(format!(
            "tick {}: the randomness is not derived from the revealed salts",
            rec.tick
        ))),
        n => Err(SealError::Fail(format_error(rec.tick, n))),
    }
}

fn format_error(tick: u16, fields: usize) -> String {
    if fields >= 9 {
        format!("tick {tick}: PS_TICK in the v9 format ({fields} fields); the v9 arm of the seal check is not implemented in this verifier")
    } else {
        format!("tick {tick}: unknown PS_TICK format ({fields} fields)")
    }
}

/// The sealed orders of one tick, on its first part (`state` at the tick's
/// start, `rec` its first `PS_TICK`, `input` the published input):
///
/// * S1 one `PS_COMMITS` of this season for the tick;
/// * S2 commitments name real offices, each at most once;
/// * S3 each was made by the office holder when the tick opened;
/// * S4 a `PS_SALTS` for the tick on the format's key;
/// * S5 one salt per batch, aligned; S6 batches in (civ, role) order;
/// * S7 each batch is for this tick, by the committed member, and hashes
///   with its salt to the commitment;
/// * S8 the randomness follows from the salts;
/// * S9 commitments without a batch are listed (not a failure).
pub fn check_seals(
    state: &WorldState,
    rec: &TickRec,
    input: &TickInput,
    data: &ChainData,
    season: &SealCtx,
    stats: &mut SealStats,
) -> Result<(), SealError> {
    let tick = rec.tick;
    let fail = |why: String| Err(SealError::Fail(format!("tick {tick}: {why}")));
    let key = salts_key(rec)?;
    // S1
    let mut lists: Vec<&Vec<Commitment>> = Vec::new();
    for (_, l) in data.commits.get(&tick).into_iter().flatten() {
        if !lists.contains(&l) {
            lists.push(l);
        }
    }
    let commits = match lists[..] {
        [] => return Err(SealError::Missing(Missing::Commits { tick })),
        [one] => one,
        _ => return fail("two different PS_COMMITS records of this season".into()),
    };
    // S2, S3
    let mut seen = HashSet::new();
    for &(civ, role, member, _) in commits {
        if civ >= season.nations || role >= 4 {
            return fail(format!("a commitment names nation {civ} office {role}"));
        }
        if !seen.insert((civ, role)) {
            return fail(format!("two commitments for nation {civ} office {role}"));
        }
        let holder = state
            .nations
            .get(civ as usize)
            .map(|n| n.offices[role as usize]);
        if holder != Some(member) || member == NOBODY {
            return fail(format!(
                "nation {civ} office {role}: committed by member {member}, not the office holder when the tick opened"
            ));
        }
    }
    // S4
    let mut found: Vec<&Vec<Salt>> = Vec::new();
    for (t, k, s, _) in &data.salts {
        if *t == tick && *k == key && !found.contains(&s) {
            found.push(s);
        }
    }
    let salts = match found[..] {
        [] => return Err(SealError::Missing(Missing::Salts { tick })),
        [one] => one,
        _ => return fail("two different PS_SALTS records on its key".into()),
    };
    // S5, S6
    if salts.len() != input.batches.len() {
        return fail(format!(
            "{} salts for {} batches",
            salts.len(),
            input.batches.len()
        ));
    }
    let mut last: Option<(u16, u8)> = None;
    for (b, (civ, role, salt)) in input.batches.iter().zip(salts) {
        let at = (b.civ, b.role as u8);
        if at != (*civ, *role) {
            return fail("salts and batches out of order".into());
        }
        if last.is_some_and(|l| l >= at) {
            return fail(format!(
                "batches not in (nation, office) order at nation {} office {}",
                b.civ, at.1
            ));
        }
        last = Some(at);
        // S7
        if b.tick != tick {
            return fail(format!(
                "nation {} office {}: the revealed batch is for tick {}",
                b.civ, at.1, b.tick
            ));
        }
        let Some(&(_, _, member, hash)) = commits.iter().find(|c| (c.0, c.1) == at) else {
            return fail(format!(
                "nation {} office {}: a batch without a commitment",
                b.civ, at.1
            ));
        };
        if b.member != member || order_commitment(b, salt) != hash {
            return fail(format!(
                "nation {} office {}: revealed orders match no commitment",
                b.civ, at.1
            ));
        }
    }
    // S8
    check_randomness(rec, salts, input)?;
    // S9
    for &(civ, role, member, _) in commits {
        if !input
            .batches
            .iter()
            .any(|b| (b.civ, b.role as u8) == (civ, role))
        {
            stats.unrevealed.push((tick, civ, role, member));
        }
    }
    stats.ticks += 1;
    stats.batches += input.batches.len();
    stats.commitments += commits.len();
    stats.gov += input.gov.len();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::base64_encode;
    use permutation_rules::genesis::{nation_entries, new_season};
    use permutation_rules::gov::{self, GovAction, GovEntry, Role};
    use permutation_rules::orders::OrderBatch;
    use permutation_rules::Preset;
    use serde_json::json;

    const X: &str = "ChunkZeroOfSeasonX";
    const Y: &str = "ChunkZeroOfSeasonY";

    /// A 2-nation Blitz world after the first election, each office held by
    /// the member who stood for it alone (member `civ * 4 + role`).
    fn world() -> (Ruleset, WorldState) {
        let rules = Ruleset::new(Preset::Blitz);
        let mut s = new_season(&rules, &[1; 32], &[2; 32], &nation_entries(2)).unwrap();
        for civ in 0..2u16 {
            for role in Role::ALL {
                let key = [civ as u8 * 4 + role as u8 + 1; 32];
                let id = gov::join(&mut s, &rules, civ, key).unwrap();
                let stand = GovEntry {
                    member: id,
                    signer: key,
                    action: GovAction::Stand { roles: role.bit() },
                };
                gov::apply_pre_season(&mut s, &rules, &[stand]).unwrap();
            }
        }
        gov::first_election(&mut s, &rules).unwrap();
        (rules, s)
    }

    fn ctx() -> SealCtx {
        SealCtx {
            nations: 2,
            season_id: 7,
            anchor: X.into(),
        }
    }

    /// One sealed tick: its input, the commitments and the salts, for the
    /// offices in `offices` ((civ, role) with the holder's batch).
    struct Sealed {
        input: TickInput,
        commits: Vec<Commitment>,
        salts: Vec<Salt>,
    }

    fn sealed(s: &WorldState, offices: &[(u16, Role)]) -> Sealed {
        let (mut batches, mut commits, mut salts) = (Vec::new(), Vec::new(), Vec::new());
        for &(civ, role) in offices {
            let member = s.nations[civ as usize].offices[role as usize];
            let b = OrderBatch {
                civ,
                tick: s.tick,
                role,
                member,
                decision_digest: [9; 32],
                orders: Vec::new(),
                adopt: Vec::new(),
            };
            let salt = [civ as u8 * 4 + role as u8 + 1; 32];
            commits.push((civ, role as u8, member, order_commitment(&b, &salt)));
            salts.push((civ, role as u8, salt));
            batches.push(b);
        }
        let pre = s.state_root().unwrap();
        Sealed {
            input: TickInput {
                vrf: tick_vrf_v8(&pre, &salts),
                batches,
                ..Default::default()
            },
            commits,
            salts,
        }
    }

    fn rec(tag: &[u8], fields: &[&[u8]]) -> Record {
        std::iter::once(tag.to_vec())
            .chain(fields.iter().map(|f| f.to_vec()))
            .collect()
    }

    fn bound(record: Record, anchor: &str) -> Emitted {
        Emitted {
            record,
            accounts: Some(vec![anchor.to_string(), "other".into()]),
        }
    }

    /// A chain being played: the world, and transactions as the program
    /// logs them.
    struct Chain {
        rules: Ruleset,
        s: WorldState,
        slot: u64,
        txs: Vec<TxRecords>,
    }

    impl Chain {
        fn new() -> Chain {
            let (rules, s) = world();
            Chain {
                rules,
                s,
                slot: 10,
                txs: Vec::new(),
            }
        }

        fn tx(&mut self, sig: &str, emitted: Vec<Emitted>) {
            self.slot += 1;
            self.txs.push(TxRecords {
                sig: sig.into(),
                slot: self.slot,
                emitted,
            });
        }

        /// Close, publish (one chunk) and return the tick's sealed input,
        /// with the salts logged on the pre-state root.
        fn open(&mut self, sig: &str, offices: &[(u16, Role)]) -> TickInput {
            let t = self.s.tick;
            let sd = sealed(&self.s, offices);
            let list = borsh::to_vec(&sd.commits).unwrap();
            self.tx(
                &format!("{sig}-close"),
                vec![bound(rec(b"PS_COMMITS", &[&t.to_le_bytes(), &list]), X)],
            );
            let bytes = borsh::to_vec(&sd.input).unwrap();
            let hash = permutation_rules::hash::sha256(&[&bytes]);
            let pre = self.s.state_root().unwrap();
            let salts = borsh::to_vec(&sd.salts).unwrap();
            self.tx(
                &format!("{sig}-input"),
                vec![
                    bound(rec(b"PS_SALTS", &[&t.to_le_bytes(), &pre, &salts]), X),
                    bound(
                        rec(
                            b"PS_INPUT",
                            &[
                                &t.to_le_bytes(),
                                &0u16.to_le_bytes(),
                                &1u16.to_le_bytes(),
                                &hash,
                                &bytes,
                            ],
                        ),
                        X,
                    ),
                ],
            );
            sd.input
        }

        /// What the program logs for `ResolveTick { to }` (raw byte).
        fn resolve(&mut self, input: &TickInput, to: u8) -> Emitted {
            let pre = self.s.state_root().unwrap();
            let tick = self.s.tick;
            while self.s.phase_cursor < to.min(PHASE_COUNT) {
                let p = self.s.phase_cursor;
                run_phase(&mut self.s, &self.rules, input, p).unwrap();
                if self.s.phase_cursor == 0 {
                    break;
                }
            }
            let hash = permutation_rules::hash::sha256(&[&borsh::to_vec(input).unwrap()]);
            let post = self.s.state_root().unwrap();
            bound(
                rec(
                    b"PS_TICK",
                    &[&tick.to_le_bytes(), &[to], &pre, &post, &hash],
                ),
                X,
            )
        }

        fn data(&self, skip: &[&str]) -> ChainData {
            let mut d = ChainData::new(X);
            for t in self.txs.iter().filter(|t| !skip.contains(&t.sig.as_str())) {
                d.add_tx(t);
            }
            d
        }
    }

    const ALL: [(u16, Role); 3] = [(0, Role::General), (0, Role::Steward), (1, Role::Science)];

    /// Tick 0 whole; tick 1 as one transaction `[ResolveTick{2},
    /// ResolveTick{12}]`, with a no-op `ResolveTick{0}` first.
    fn season() -> (Chain, WorldState) {
        let mut c = Chain::new();
        let start = c.s.clone();
        let i0 = c.open("t0", &ALL);
        let r = c.resolve(&i0, 12);
        c.tx("t0-resolve", vec![r]);
        let i1 = c.open("t1", &ALL[..1]);
        let noop = c.resolve(&i1, 0);
        let a = c.resolve(&i1, 2);
        let b = c.resolve(&i1, 12);
        c.tx("tx1", vec![noop, a, b]);
        (c, start)
    }

    fn run(
        c: &Chain,
        start: &WorldState,
        data: &ChainData,
        target: Option<&[u8; 32]>,
    ) -> (Outcome, WorldState) {
        let mut s = start.clone();
        let out = follow(&mut s, &c.rules, data, &ctx(), target);
        (out, s)
    }

    #[test]
    fn several_tick_records_in_one_transaction() {
        let (c, start) = season();
        let live = c.s.state_root().unwrap();
        let d = c.data(&[]);
        assert_eq!(d.ticks.iter().filter(|t| t.sig == "tx1").count(), 3);
        let (out, s) = run(&c, &start, &d, Some(&live));
        assert_eq!((out.fail.clone(), out.missing.clone()), (None, None));
        assert_eq!(out.steps, 3);
        assert_eq!(out.end_root, live);
        assert_eq!(s.tick, 2);
        assert_eq!(out.last_sig.as_deref(), Some("tx1"));
        assert_eq!(out.seals.ticks, 2);
        // The old verifier took the first PS_TICK of a transaction for every
        // line: the no-op, whose roots are neither line's.
        let first = d.ticks.iter().find(|t| t.sig == "tx1").unwrap();
        assert_eq!(first.pre, first.post);
    }

    /// A part in neither format (7 or 8 fields) is a contradiction, also
    /// after a tick's first part.
    #[test]
    fn unknown_format_on_a_later_part_fails() {
        let mut c = Chain::new();
        let start = c.s.clone();
        let i0 = c.open("t0", &ALL);
        let a = c.resolve(&i0, 5);
        let mut b = c.resolve(&i0, 12);
        b.record.push(vec![0]);
        c.tx("a", vec![a]);
        c.tx("b", vec![b]);
        let live = c.s.state_root().unwrap();
        let (out, _) = run(&c, &start, &c.data(&[]), Some(&live));
        assert!(out
            .fail
            .unwrap()
            .contains("unknown PS_TICK format (7 fields)"));
    }

    #[test]
    fn noop_records_are_ignored() {
        let mut c = Chain::new();
        let start = c.s.clone();
        let i0 = c.open("t0", &ALL);
        let a = c.resolve(&i0, 5);
        let noop = c.resolve(&i0, 5); // to <= cursor: pre == post
        let b = c.resolve(&i0, 255); // an outsider's raw `to`: replays as 12
        assert_eq!(noop.record[3], noop.record[4]);
        c.tx("a", vec![a]);
        c.tx("noop", vec![noop]);
        c.tx("b", vec![b]);
        let live = c.s.state_root().unwrap();
        let (with, _) = run(&c, &start, &c.data(&[]), Some(&live));
        let (without, _) = run(&c, &start, &c.data(&["noop"]), Some(&live));
        assert_eq!((with.fail.clone(), with.missing.clone()), (None, None));
        assert_eq!(
            (with.steps, with.end_root),
            (without.steps, without.end_root)
        );
        assert_eq!(with.steps, 2);
        let d = c.data(&[]);
        let raw = d.ticks.iter().find(|t| t.sig == "b").unwrap();
        assert_eq!((raw.to, raw.stop, raw.degraded), (255, 12, false));
    }

    #[test]
    fn a_gap_is_missing_not_failed() {
        let (c, start) = season();
        let live = c.s.state_root().unwrap();
        let (out, s) = run(&c, &start, &c.data(&["tx1"]), Some(&live));
        assert_eq!(out.fail, None);
        assert!(
            matches!(out.missing, Some(Missing::Successor { tick: 1, root }) if root == s.state_root().unwrap())
        );
        assert!(!out.visited.contains(&live));
        assert_eq!(out.last_sig.as_deref(), Some("t0-resolve"));
    }

    #[test]
    fn two_histories_fail() {
        let (mut c, start) = season();
        // Another step from tick 0's start root, to another post root.
        let mut other = Chain::new();
        let i0 = other.open("o", &ALL);
        let r = other.resolve(&i0, 5);
        c.tx("other", vec![r]);
        let (out, _) = run(&c, &start, &c.data(&[]), None);
        assert!(out.fail.unwrap().contains("two records advance"));
    }

    #[test]
    fn record_tick_must_match() {
        let (c, start) = season();
        let mut d = c.data(&[]);
        // Tick 0's record rewritten as tick 5 (same roots).
        let t = d.ticks.iter_mut().find(|t| t.sig == "t0-resolve").unwrap();
        t.tick = 5;
        let (out, _) = run(&c, &start, &d, None);
        assert!(out.fail.unwrap().contains("says tick 5"));
    }

    #[test]
    fn parts_must_share_the_input() {
        let mut c = Chain::new();
        let start = c.s.clone();
        let i0 = c.open("t0", &ALL);
        let a = c.resolve(&i0, 2);
        let other = TickInput {
            vrf: [5; 32],
            ..Default::default()
        };
        // Phases 2..12 do not read the input: the roots chain, the hash differs.
        let b = c.resolve(&other, 12);
        c.tx("a", vec![a]);
        c.tx("b", vec![b]);
        let (out, _) = run(&c, &start, &c.data(&[]), None);
        assert!(out.fail.unwrap().contains("different inputs"));
    }

    #[test]
    fn commits_tick_field_checked() {
        let (c, start) = season();
        let live = c.s.state_root().unwrap();
        let t0 = c.txs.iter().position(|t| t.sig == "t0-close").unwrap();
        // Logged with tick + 1 only: missing, not failed.
        let mut shifted = c.txs.clone();
        shifted[t0].emitted[0].record[1] = 1u16.to_le_bytes().to_vec();
        shifted.retain(|t| t.sig != "t1-close");
        let mut d = ChainData::new(X);
        shifted.iter().for_each(|t| d.add_tx(t));
        let (out, _) = run(&c, &start, &d, Some(&live));
        assert_eq!(out.missing, Some(Missing::Commits { tick: 0 }));
        assert_eq!(out.fail, None);
        // Two different bound PS_COMMITS for the tick: a contradiction.
        let mut d = c.data(&[]);
        let mut dup = c.txs[t0].clone();
        dup.sig = "dup".into();
        let list = borsh::to_vec(&vec![(0u16, 0u8, 0u32, [0u8; 32])]).unwrap();
        dup.emitted[0].record[2] = list.clone();
        d.add_tx(&dup);
        let (out, _) = run(&c, &start, &d, Some(&live));
        assert!(out.fail.unwrap().contains("two different PS_COMMITS"));
        // Another season's close for tick 0 (its chunk 0 is Y's, the list
        // names non-officers): ignored next to the bound one...
        let foreign = TxRecords {
            sig: "foreign".into(),
            slot: 11,
            emitted: vec![bound(rec(b"PS_COMMITS", &[&0u16.to_le_bytes(), &list]), Y)],
        };
        let mut d = c.data(&[]);
        d.add_tx(&foreign);
        assert_eq!(d.foreign, 1);
        let (out, _) = run(&c, &start, &d, Some(&live));
        assert_eq!((out.fail, out.missing), (None, None));
        // ...and alone it is not a commitment of this season: missing.
        let mut d = c.data(&["t0-close"]);
        d.add_tx(&foreign);
        let (out, _) = run(&c, &start, &d, Some(&live));
        assert_eq!(
            (out.fail, out.missing),
            (None, Some(Missing::Commits { tick: 0 }))
        );
    }

    /// Tick 0 with its close replaced by `commits`.
    fn with_commits(commits: Vec<Commitment>) -> Outcome {
        let (c, start) = season();
        let mut d = c.data(&["t0-close"]);
        let list = borsh::to_vec(&commits).unwrap();
        d.add_tx(&TxRecords {
            sig: "close".into(),
            slot: 11,
            emitted: vec![bound(rec(b"PS_COMMITS", &[&0u16.to_le_bytes(), &list]), X)],
        });
        run(&c, &start, &d, None).0
    }

    fn tick0_commits() -> Vec<Commitment> {
        let (c, _) = season();
        let e = &c.txs.iter().find(|t| t.sig == "t0-close").unwrap().emitted[0];
        Vec::try_from_slice(&e.record[2]).unwrap()
    }

    #[test]
    fn commitment_by_a_non_officer_fails() {
        let mut commits = tick0_commits();
        commits[0].2 = 1; // nation 0's general is member 0
        let out = with_commits(commits);
        assert!(out.fail.unwrap().contains("not the office holder"));
        // A commitment for a vacant office or an unknown nation fails too.
        let mut commits = tick0_commits();
        commits[0].0 = 2;
        assert!(with_commits(commits)
            .fail
            .unwrap()
            .contains("names nation 2"));
    }

    #[test]
    fn duplicate_commitment_or_batch_fails() {
        let mut commits = tick0_commits();
        commits.push(commits[0]);
        assert!(with_commits(commits)
            .fail
            .unwrap()
            .contains("two commitments"));
        // A batch revealed twice (with its salt twice): out of order.
        let (mut c, start) = season();
        let s0 = start.clone();
        let mut sd = sealed(&s0, &ALL);
        sd.input.batches.insert(1, sd.input.batches[0].clone());
        sd.salts.insert(1, sd.salts[0]);
        sd.input.vrf = tick_vrf_v8(&s0.state_root().unwrap(), &sd.salts);
        let bytes = borsh::to_vec(&sd.input).unwrap();
        let hash = permutation_rules::hash::sha256(&[&bytes]);
        c.txs
            .retain(|t| !t.sig.starts_with("t0-") && t.sig != "tx1" && !t.sig.starts_with("t1-"));
        let list = borsh::to_vec(&sd.commits).unwrap();
        let salts = borsh::to_vec(&sd.salts).unwrap();
        let pre = s0.state_root().unwrap();
        c.tx(
            "dup",
            vec![
                bound(rec(b"PS_COMMITS", &[&0u16.to_le_bytes(), &list]), X),
                bound(rec(b"PS_SALTS", &[&0u16.to_le_bytes(), &pre, &salts]), X),
                bound(
                    rec(
                        b"PS_INPUT",
                        &[
                            &0u16.to_le_bytes(),
                            &0u16.to_le_bytes(),
                            &1u16.to_le_bytes(),
                            &hash,
                            &bytes,
                        ],
                    ),
                    X,
                ),
                bound(
                    rec(
                        b"PS_TICK",
                        &[&0u16.to_le_bytes(), &[12], &pre, &[3; 32], &hash],
                    ),
                    X,
                ),
            ],
        );
        let (out, _) = run(&c, &start, &c.data(&[]), None);
        assert!(out.fail.unwrap().contains("not in (nation, office) order"));
    }

    #[test]
    fn batch_for_another_tick_fails() {
        let (mut c, start) = season();
        c.txs.clear();
        c.s = start.clone();
        let mut sd = sealed(&start, &ALL[..1]);
        sd.input.batches[0].tick = 3;
        // Committed as revealed (so only the batch's tick is wrong).
        sd.commits[0].3 = order_commitment(&sd.input.batches[0], &sd.salts[0].2);
        let bytes = borsh::to_vec(&sd.input).unwrap();
        let hash = permutation_rules::hash::sha256(&[&bytes]);
        let pre = start.state_root().unwrap();
        c.tx(
            "t",
            vec![
                bound(
                    rec(
                        b"PS_COMMITS",
                        &[&0u16.to_le_bytes(), &borsh::to_vec(&sd.commits).unwrap()],
                    ),
                    X,
                ),
                bound(
                    rec(
                        b"PS_SALTS",
                        &[
                            &0u16.to_le_bytes(),
                            &pre,
                            &borsh::to_vec(&sd.salts).unwrap(),
                        ],
                    ),
                    X,
                ),
                bound(
                    rec(
                        b"PS_INPUT",
                        &[
                            &0u16.to_le_bytes(),
                            &0u16.to_le_bytes(),
                            &1u16.to_le_bytes(),
                            &hash,
                            &bytes,
                        ],
                    ),
                    X,
                ),
                bound(
                    rec(
                        b"PS_TICK",
                        &[&0u16.to_le_bytes(), &[12], &pre, &[3; 32], &hash],
                    ),
                    X,
                ),
            ],
        );
        let (out, _) = run(&c, &start, &c.data(&[]), None);
        assert!(out.fail.unwrap().contains("is for tick 3"));
    }

    #[test]
    fn salts_tick_and_root_checked() {
        let (c, start) = season();
        let live = c.s.state_root().unwrap();
        let t0 = c.txs.iter().position(|t| t.sig == "t0-input").unwrap();
        // The right root, another tick: missing.
        let mut txs = c.txs.clone();
        txs[t0].emitted[0].record[1] = 9u16.to_le_bytes().to_vec();
        let mut d = ChainData::new(X);
        txs.iter().for_each(|t| d.add_tx(t));
        let (out, _) = run(&c, &start, &d, Some(&live));
        assert_eq!(
            (out.fail, out.missing),
            (None, Some(Missing::Salts { tick: 0 }))
        );
        // The right tick on another season's root: ignored.
        let mut other = c.txs[t0].clone();
        other.sig = "other".into();
        other.emitted[0].record[2] = vec![7; 32];
        other.emitted[0].record[3] = borsh::to_vec(&Vec::<Salt>::new()).unwrap();
        let mut d = c.data(&[]);
        d.add_tx(&other);
        let (out, _) = run(&c, &start, &d, Some(&live));
        assert_eq!((out.fail, out.missing), (None, None));
    }

    #[test]
    fn randomness_from_salts() {
        let (c, start) = season();
        let t0 = c.txs.iter().position(|t| t.sig == "t0-input").unwrap();
        // Another input with a wrong vrf, and a resolve record over it.
        let mut sd = sealed(&start, &ALL);
        sd.input.vrf = [0; 32];
        let bytes = borsh::to_vec(&sd.input).unwrap();
        let hash = permutation_rules::hash::sha256(&[&bytes]);
        let mut txs = c.txs.clone();
        txs[t0].emitted[1].record[4] = hash.to_vec();
        txs[t0].emitted[1].record[5] = bytes;
        let r = txs.iter_mut().find(|t| t.sig == "t0-resolve").unwrap();
        r.emitted[0].record[5] = hash.to_vec();
        let mut d = ChainData::new(X);
        txs.iter().for_each(|t| d.add_tx(t));
        let (out, _) = run(&c, &start, &d, None);
        assert!(out.fail.unwrap().contains("randomness"));
        // A 7-field PS_TICK: unknown format; 9 fields: the v9 arm (not here).
        for (n, why) in [(7, "unknown PS_TICK format"), (9, "v9 arm")] {
            let mut d = c.data(&[]);
            let t = d.ticks.iter_mut().find(|t| t.sig == "t0-resolve").unwrap();
            t.fields.resize(n, vec![0; 32]);
            let (out, _) = run(&c, &start, &d, None);
            assert!(out.fail.unwrap().contains(why), "{n} fields");
        }
    }

    #[test]
    fn unrevealed_are_listed() {
        let (c, start) = season();
        let live = c.s.state_root().unwrap();
        // Tick 1 committed nation 0's general only; add a commitment of
        // nation 1's diplomat (member 7) that was never revealed.
        let mut txs = c.txs.clone();
        let close = txs.iter_mut().find(|t| t.sig == "t1-close").unwrap();
        let mut list: Vec<Commitment> = Vec::try_from_slice(&close.emitted[0].record[2]).unwrap();
        list.push((1, Role::Diplomat as u8, 7, [4; 32]));
        close.emitted[0].record[2] = borsh::to_vec(&list).unwrap();
        let mut d = ChainData::new(X);
        txs.iter().for_each(|t| d.add_tx(t));
        let (out, _) = run(&c, &start, &d, Some(&live));
        assert_eq!((out.fail.clone(), out.missing.clone()), (None, None));
        assert_eq!(out.seals.unrevealed, vec![(1, 1, Role::Diplomat as u8, 7)]);
        assert_eq!((out.seals.batches, out.seals.commitments), (4, 5));
    }

    #[test]
    fn input_chunks() {
        let bytes: Vec<u8> = (0..50u8).collect();
        let hash = permutation_rules::hash::sha256(&[&bytes]);
        let chunk = |c: u16, b: &[u8]| {
            bound(
                rec(
                    b"PS_INPUT",
                    &[
                        &4u16.to_le_bytes(),
                        &c.to_le_bytes(),
                        &2u16.to_le_bytes(),
                        &hash,
                        b,
                    ],
                ),
                X,
            )
        };
        let tx = |sig: &str, e: Emitted| TxRecords {
            sig: sig.into(),
            slot: 1,
            emitted: vec![e],
        };
        let mut d = ChainData::new(X);
        d.add_tx(&tx("a", chunk(0, &bytes[..30])));
        assert_eq!(d.input(&hash), None);
        assert_eq!(d.input_gap(&hash), "PS_INPUT chunk 2 of 2 missing");
        d.add_tx(&tx("b", chunk(1, &bytes[30..])));
        assert_eq!(d.input(&hash), Some(bytes.clone()));
        assert_eq!(d.inputs[&hash].sigs, vec!["a", "b"]);
        d.add_tx(&tx("c", chunk(1, &bytes[31..])));
        assert_eq!(d.malformed.len(), 1);
        assert!(d.malformed[0].contains("disagree"));
        assert_eq!(d.input(&hash), Some(bytes), "the first copy stays");
        // Missing in `follow`: the tick's input.
        let (c, start) = season();
        let (out, _) = run(&c, &start, &c.data(&["t0-input"]), None);
        assert!(matches!(out.missing, Some(Missing::Input { tick: 0, .. })));
    }

    fn line(id: &str, what: &str) -> String {
        format!("Program {id} {what}")
    }

    fn data_line(fields: &[&[u8]]) -> String {
        format!(
            "Program data: {}",
            fields
                .iter()
                .map(|f| base64_encode(f))
                .collect::<Vec<_>>()
                .join(" ")
        )
    }

    #[test]
    fn program_records_filter() {
        let ours = data_line(&[b"PS_TICK", &[1, 0]]);
        let logs = [
            line("Other111", "invoke [1]"),
            ours.clone(), // emitted under another program's frame: dropped
            line("Other111", "invoke [2]"), // a nested frame of Other
            line("Other111", "success"),
            line("Ours1111", "invoke [2]"), // a CPI into our program
            ours.clone(),
            line("Ours1111", "success"),
            line("Other111", "success"),
            line("Ours1111", "invoke [1]"),
            // Lines a program can print: neither pushes a frame.
            "Program data: invoke [1]".replace(" [1]", ""),
            "Program log: Program Other111 invoke [2]".to_string(),
            ours.clone(),
            line("Ours1111", "success"),
        ];
        let logs: Vec<&str> = logs.iter().map(|s| s.as_str()).collect();
        let r = program_records(&logs, "Ours1111").unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(
            r[0].1,
            Frame {
                top: 0,
                inner: Some(1),
                truncated: false
            }
        );
        // After `Program data: invoke`, the record is still our program's.
        assert_eq!(r[2].0[0], b"PS_TICK");
        assert_eq!(
            r[2].1,
            Frame {
                top: 1,
                inner: None,
                truncated: false
            }
        );
        // Without a program, every record counts.
        assert_eq!(walk(&logs, None).unwrap().len(), 4);
    }

    #[test]
    fn attribution() {
        let tick = data_line(&[b"PS_TICK", &[1, 0]]);
        let key = |k: &str| k.to_string();
        let tx = |logs: Vec<String>, inner: Value, loaded: Value| {
            json!({
                "slot": 42,
                "meta": { "err": null, "logMessages": logs, "innerInstructions": inner, "loadedAddresses": loaded },
                "transaction": { "message": {
                    "accountKeys": ["Payer", "Ours1111", "Wrap1111", "ChunkA", "ChunkB"],
                    "instructions": [
                        { "programIdIndex": 1, "accounts": [3, 0], "data": "" },
                        { "programIdIndex": 1, "accounts": [4, 0], "data": "" },
                        { "programIdIndex": 2, "accounts": [1, 5], "data": "" },
                    ],
                }},
            })
        };
        let top_level = vec![
            line("Ours1111", "invoke [1]"),
            line("Ours1111", "success"),
            line("Ours1111", "invoke [1]"),
            tick.clone(),
            line("Ours1111", "success"),
            line("Wrap1111", "invoke [1]"),
            line("Wrap1111", "success"),
        ];
        let r = tx_records(
            "s",
            &tx(top_level.clone(), json!([]), json!(null)),
            "Ours1111",
        )
        .unwrap();
        assert_eq!(r.slot, 42);
        assert_eq!(
            r.emitted[0].accounts,
            Some(vec![key("ChunkB"), key("Payer")])
        );
        // Frames that do not line up with the message (one missing, or a
        // frame of another program): nothing is attributable.
        let short_logs = top_level[..5].to_vec();
        let r = tx_records("s", &tx(short_logs, json!([]), json!(null)), "Ours1111").unwrap();
        assert_eq!(r.emitted[0].accounts, None);
        let mut other = top_level.clone();
        other[0] = line("Wrap1111", "invoke [1]");
        other[1] = line("Wrap1111", "success");
        let r = tx_records("s", &tx(other, json!([]), json!(null)), "Ours1111").unwrap();
        assert_eq!(r.emitted[0].accounts, None);
        // A CPI from the wrapper (instruction 2) into our program; a v0
        // transaction's loaded address is key 5.
        let cpi = vec![
            line("Ours1111", "invoke [1]"),
            line("Ours1111", "success"),
            line("Ours1111", "invoke [1]"),
            line("Ours1111", "success"),
            line("Wrap1111", "invoke [1]"),
            line("Ours1111", "invoke [2]"),
            tick.clone(),
            line("Ours1111", "success"),
            line("Wrap1111", "success"),
        ];
        let inner = json!([{ "index": 2, "instructions": [{ "programIdIndex": 1, "accounts": [5, 3], "data": "" }] }]);
        let loaded = json!({ "writable": ["Loaded1"], "readonly": [] });
        let r = tx_records("s", &tx(cpi.clone(), inner, loaded.clone()), "Ours1111").unwrap();
        assert_eq!(
            r.emitted[0].accounts,
            Some(vec![key("Loaded1"), key("ChunkA")])
        );
        // Without innerInstructions a CPI record is not attributable.
        let r = tx_records("s", &tx(cpi, json!(null), loaded), "Ours1111").unwrap();
        assert_eq!(r.emitted[0].accounts, None);
        // Records after `Log truncated` are not attributable.
        let cut = vec![
            line("Ours1111", "invoke [1]"),
            "Log truncated".to_string(),
            tick.clone(),
        ];
        let r = tx_records("s", &tx(cut, json!([]), json!(null)), "Ours1111").unwrap();
        assert_eq!(r.emitted.len(), 1);
        assert_eq!(r.emitted[0].accounts, None);
        // A failed transaction has no records; a missing one is an error.
        let mut failed = tx(vec![tick], json!([]), json!(null));
        failed["meta"]["err"] = json!({"InstructionError": [0, "Custom"]});
        assert!(tx_records("s", &failed, "Ours1111")
            .unwrap()
            .emitted
            .is_empty());
        assert!(tx_records("s", &Value::Null, "Ours1111").is_err());
    }

    /// A precompile instruction (Ed25519 here) runs without an `invoke [1]`
    /// line, so the frames skip it: a record is tied to its own instruction,
    /// never to the one before it.
    #[test]
    fn attribution_skips_precompiles() {
        const ED25519: &str = "Ed25519SigVerify111111111111111111111111111";
        let close = data_line(&[b"PS_COMMITS", &[1, 0]]);
        let tx = json!({
            "slot": 7,
            "meta": { "err": null, "innerInstructions": [], "logMessages": [
                line("Ours1111", "invoke [1]"),
                line("Ours1111", "success"),
                line("Ours1111", "invoke [1]"),
                close,
                line("Ours1111", "success"),
            ]},
            "transaction": { "message": {
                "accountKeys": ["Payer", "Ours1111", ED25519, "ChunkX", "ChunkY"],
                "instructions": [
                    { "programIdIndex": 2, "accounts": [], "data": "" },
                    { "programIdIndex": 1, "accounts": [3, 0], "data": "" },
                    { "programIdIndex": 1, "accounts": [4, 0], "data": "" },
                ],
            }},
        });
        let r = tx_records("s", &tx, "Ours1111").unwrap();
        assert_eq!(
            r.emitted[0].accounts,
            Some(vec!["ChunkY".to_string(), "Payer".to_string()])
        );
        // An unknown instruction without a frame (not a listed precompile):
        // nothing is attributable.
        let mut unknown = tx.clone();
        unknown["transaction"]["message"]["accountKeys"][2] = json!("Silent111");
        let r = tx_records("s", &unknown, "Ours1111").unwrap();
        assert_eq!(r.emitted[0].accounts, None);
    }

    /// The attack: an outsider's transaction `[Ed25519, X's instruction on
    /// X's chunk 0, a CloseCommits of season Y]`. Y's `PS_COMMITS` is Y's (or
    /// foreign to X); it never counts as X's, so X does not fail.
    #[test]
    fn foreign_close_after_a_precompile() {
        const ED25519: &str = "Ed25519SigVerify111111111111111111111111111";
        let (mut c, start) = season();
        let live = c.s.state_root().unwrap();
        let commits: Vec<Commitment> = vec![(0, 0, 0, [9; 32])];
        let list = borsh::to_vec(&commits).unwrap();
        let close = data_line(&[b"PS_COMMITS", &0u16.to_le_bytes(), &list]);
        let tx = json!({
            "slot": 11,
            "meta": { "err": null, "innerInstructions": [], "logMessages": [
                line("Ours1111", "invoke [1]"),
                line("Ours1111", "success"),
                line("Ours1111", "invoke [1]"),
                close,
                line("Ours1111", "success"),
            ]},
            "transaction": { "message": {
                "accountKeys": ["Outsider", "Ours1111", ED25519, X, Y],
                "instructions": [
                    { "programIdIndex": 2, "accounts": [], "data": "" },
                    { "programIdIndex": 1, "accounts": [3, 0], "data": "" },
                    { "programIdIndex": 1, "accounts": [4, 0], "data": "" },
                ],
            }},
        });
        let r = tx_records("spam", &tx, "Ours1111").unwrap();
        assert_eq!(r.emitted[0].accounts.as_ref().unwrap()[0], Y);
        c.txs.push(r);
        let d = c.data(&[]);
        assert_eq!(d.foreign, 1);
        let (out, _) = run(&c, &start, &d, Some(&live));
        assert_eq!((out.fail, out.missing), (None, None));
        assert_eq!(out.end_root, live);
    }

    #[test]
    fn effective_to_by_format() {
        assert_eq!(effective_to(255, 6), (12, false));
        assert_eq!(effective_to(5, 6), (5, false));
        assert_eq!(effective_to(0x80 | 7, 9), (7, true));
        assert_eq!(effective_to(0x7f, 9), (12, false));
    }
}
