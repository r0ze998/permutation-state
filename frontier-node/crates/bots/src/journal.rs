//! The bots' marchbook (M1 contract §8.6: "plaintext and salt journalled";
//! §9.1's rule for people: written **before** Depart is signed). One JSON
//! line per event, appended and `fsync`ed, one file per process: a
//! `sealed` line with everything a reveal or a settlement needs, then
//! state lines (`sent`, `failed`, `revealed`, `settled`, …). On restart the
//! file is folded back into each bot's [`Memory`] (the last state wins), so
//! a crash between sealing and sending loses nothing a reveal needs.
//! `k` (the seal key) is never stored; the salt is `salt_of(k)`.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fclient::ix::HoldingRef;
use frontier_agents::obs::{b64, b64_encode};
use frontier_agents::policy::{MarchMemo, SealKind};
use serde_json::{json, Value};

use crate::seal::SealedMarch;

pub struct Journal {
    path: PathBuf,
    file: Mutex<File>,
}

fn kind_name(k: SealKind) -> &'static str {
    match k {
        SealKind::Honest => "honest",
        SealKind::Garbage => "garbage",
        SealKind::BadPlaintext => "bad_plaintext",
    }
}

fn kind_of(s: &str) -> SealKind {
    match s {
        "garbage" => SealKind::Garbage,
        "bad_plaintext" => SealKind::BadPlaintext,
        _ => SealKind::Honest,
    }
}

impl Journal {
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Journal> {
        let path = path.as_ref().to_path_buf();
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Journal {
            path,
            file: Mutex::new(file),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn append(&self, v: &Value) -> std::io::Result<()> {
        let mut line = serde_json::to_vec(v).map_err(std::io::Error::other)?;
        line.push(b'\n');
        let mut f = self.file.lock().expect("journal lock");
        f.write_all(&line)?;
        f.sync_data()
    }

    /// The sealed march, before its Depart is signed.
    #[allow(clippy::too_many_arguments)]
    pub fn sealed(&self, bot: u32, m: &MarchMemo, s: &SealedMarch) -> std::io::Result<()> {
        self.append(&json!({
            "ev": "sealed",
            "bot": bot,
            "host": m.key.0.to_string(),
            "depart_bell": m.key.1,
            "holding": [m.h.p, m.h.q, m.h.site],
            "transit_slot": m.transit_slot,
            "arrive_bell": m.arrive_bell,
            "dest": [m.dest.0, m.dest.1],
            "path_others": m.path_others.iter().map(|p| json!([p.0, p.1])).collect::<Vec<_>>(),
            "round": s.round,
            "plain_b64": b64_encode(&s.plain),
            "salt_b64": b64_encode(&s.salt),
            "commit_hex": hex::encode(s.commit),
            "seal_b64": b64_encode(&s.seal),
            "ct_hash_hex": hex::encode(s.ct_hash),
            "tip": m.tip.to_string(),
            "kind": kind_name(m.kind),
        }))
    }

    /// A state change of a march (`sent`, `failed`, `revealed`, `reveal_try`,
    /// `late_done`, `settled`, `redeparted`).
    pub fn state(&self, bot: u32, key: (u64, u32), state: &str) -> std::io::Result<()> {
        self.append(&json!({
            "ev": state,
            "bot": bot,
            "host": key.0.to_string(),
            "depart_bell": key.1,
        }))
    }

    /// Folds the journal back into per-bot marches.
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<BTreeMap<u32, Vec<MarchMemo>>> {
        let mut out: BTreeMap<u32, Vec<MarchMemo>> = BTreeMap::new();
        let f = match File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e),
        };
        for line in BufReader::new(f).lines() {
            let line = line?;
            // A torn last line (crash mid-write) is skipped.
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            // A state line for an unknown march (its sealed line torn) is
            // ignored.
            let _ = apply(&mut out, &v);
        }
        Ok(out)
    }
}

fn get_u64(v: &Value, k: &str) -> Option<u64> {
    let x = v.get(k)?;
    x.as_u64().or_else(|| x.as_str()?.parse().ok())
}

fn arr<const N: usize>(v: &Value, k: &str, hexed: bool) -> Option<[u8; N]> {
    let s = v.get(k)?.as_str()?;
    let b = if hexed {
        hex::decode(s).ok()?
    } else {
        b64(s).ok()?
    };
    b.try_into().ok()
}

fn apply(out: &mut BTreeMap<u32, Vec<MarchMemo>>, v: &Value) -> Option<()> {
    let bot = get_u64(v, "bot")? as u32;
    let key = (get_u64(v, "host")?, get_u64(v, "depart_bell")? as u32);
    let ev = v.get("ev")?.as_str()?;
    let list = out.entry(bot).or_default();
    if ev == "sealed" {
        let h = v.get("holding")?.as_array()?;
        let d = v.get("dest")?.as_array()?;
        let seal = b64(v.get("seal_b64")?.as_str()?).ok()?;
        let memo = MarchMemo {
            key,
            h: HoldingRef {
                p: h.first()?.as_i64()? as i16,
                q: h.get(1)?.as_i64()? as i16,
                site: h.get(2)?.as_u64()? as u8,
            },
            transit_slot: get_u64(v, "transit_slot")? as u8,
            arrive_bell: get_u64(v, "arrive_bell")? as u32,
            dest: (d.first()?.as_i64()? as i32, d.get(1)?.as_i64()? as i32),
            path_others: v
                .get("path_others")?
                .as_array()?
                .iter()
                .filter_map(|p| Some((p.get(0)?.as_i64()? as i32, p.get(1)?.as_i64()? as i32)))
                .collect(),
            plain: arr(v, "plain_b64", false)?,
            salt: arr(v, "salt_b64", false)?,
            commit: arr(v, "commit_hex", true)?,
            seal,
            ct_hash: arr(v, "ct_hash_hex", true)?,
            tip: get_u64(v, "tip")?,
            kind: kind_of(v.get("kind")?.as_str()?),
            sent: false,
            reveal_tries: 0,
            revealed: false,
            late_done: false,
            settled: false,
            redeparted: false,
        };
        list.retain(|m| m.key != key);
        list.push(memo);
        return Some(());
    }
    let m = list.iter_mut().find(|m| m.key == key)?;
    match ev {
        "sent" => m.sent = true,
        "failed" => {
            list.retain(|m| m.key != key);
        }
        "reveal_try" => m.reveal_tries = m.reveal_tries.saturating_add(1),
        "revealed" => m.revealed = true,
        "late_done" => m.late_done = true,
        "settled" => m.settled = true,
        "redeparted" => m.redeparted = true,
        _ => {}
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memo() -> (MarchMemo, SealedMarch) {
        let s = SealedMarch {
            plain: [1; 37],
            salt: [2; 32],
            commit: [3; 32],
            seal: [4; 165],
            ct_hash: [5; 32],
            seal_root: [6; 32],
            round: 99,
        };
        let m = MarchMemo {
            key: (1 << 40 | 7, 40),
            h: HoldingRef {
                p: 2,
                q: -1,
                site: 3,
            },
            transit_slot: 1,
            arrive_bell: 44,
            dest: (3, -1),
            path_others: vec![(2, -1)],
            plain: s.plain,
            salt: s.salt,
            commit: s.commit,
            seal: s.seal.to_vec(),
            ct_hash: s.ct_hash,
            tip: 12_345,
            kind: SealKind::Garbage,
            sent: false,
            reveal_tries: 0,
            revealed: false,
            late_done: false,
            settled: false,
            redeparted: false,
        };
        (m, s)
    }

    #[test]
    fn round_trips_and_folds_states() {
        let dir = std::env::temp_dir().join(format!("w3e-journal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = dir.join("marchbook.jsonl");
        let j = Journal::open(&p).unwrap();
        let (m, s) = memo();
        j.sealed(5, &m, &s).unwrap();
        // Nothing but the sealed line: the march is known, not sent.
        let got = Journal::load(&p).unwrap();
        assert_eq!(got[&5], vec![m.clone()]);
        j.state(5, m.key, "sent").unwrap();
        j.state(5, m.key, "reveal_try").unwrap();
        j.state(5, m.key, "revealed").unwrap();
        // A second march that failed disappears.
        let mut m2 = m.clone();
        m2.key.1 = 41;
        j.sealed(5, &m2, &s).unwrap();
        j.state(5, m2.key, "failed").unwrap();
        // A torn line is skipped.
        std::fs::OpenOptions::new()
            .append(true)
            .open(&p)
            .unwrap()
            .write_all(b"{\"ev\":\"sen")
            .unwrap();
        let got = Journal::load(&p).unwrap();
        let want = MarchMemo {
            sent: true,
            reveal_tries: 1,
            revealed: true,
            ..m
        };
        assert_eq!(got[&5], vec![want]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
