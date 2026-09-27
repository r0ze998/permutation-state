//! The drand round archive (M1 contract §8.7, I-53).
//!
//! **Layout** of an archive directory:
//!
//! - `info.json`: the chain's drand `/info` (the chain the rounds belong
//!   to; checked against the pinned chain at load, never trusted alone);
//! - `manifest.json`: `{"format": "psf-drand-archive-v1", "chain_hash",
//!   "segments": [{"file", "first", "count", "sha256"}]}`;
//! - one file per contiguous run of rounds, `r<first>-<last>.bin`:
//!   `"PSFDRND1" ‖ chain_hash[32] ‖ le64(first) ‖ le64(count) ‖ count × sig48`
//!   (round `first + i` at offset `48 + 48 i`). The exit's ≈ 250,000
//!   contiguous rounds are one ≈ 12 MB segment.
//!
//! **Verification.** Every round is verified with blstrs against the
//! pinned public key when it is fetched ([`crate::prefetch`]) or packed.
//! Loading checks each segment's sha256 against the manifest and its chain
//! hash, and re-verifies a sample of rounds (the first, the last and
//! [`SAMPLE`] more per segment); [`Archive::verify_all`] re-verifies every
//! round on all cores (`drand-replay verify --all`).
//!
//! A directory without `manifest.json` is read as the SP-V2 fixture layout
//! (one `{round}.json` per round, each verified on load).

use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

use fclient::beacon;
use fclient::ports::{Beacon, ChainInfo};

pub const MAGIC: &[u8; 8] = b"PSFDRND1";
pub const FORMAT: &str = "psf-drand-archive-v1";
pub const HEADER: usize = 8 + 32 + 8 + 8;
/// Rounds re-verified per segment at load, beside the first and the last.
pub const SAMPLE: usize = 16;

/// One contiguous run of rounds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub first: u64,
    pub sigs: Vec<[u8; 48]>,
}

impl Segment {
    pub fn last(&self) -> u64 {
        self.first + self.sigs.len() as u64 - 1
    }

    pub fn file_name(&self) -> String {
        format!("r{}-{}.bin", self.first, self.last())
    }

    pub fn encode(&self, chain_hash: &[u8; 32]) -> Vec<u8> {
        let mut b = Vec::with_capacity(HEADER + 48 * self.sigs.len());
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(chain_hash);
        b.extend_from_slice(&self.first.to_le_bytes());
        b.extend_from_slice(&(self.sigs.len() as u64).to_le_bytes());
        for s in &self.sigs {
            b.extend_from_slice(s);
        }
        b
    }

    pub fn decode(b: &[u8], chain_hash: &[u8; 32]) -> Result<Segment, String> {
        if b.len() < HEADER || &b[..8] != MAGIC {
            return Err("not a drand archive segment".into());
        }
        if &b[8..40] != chain_hash {
            return Err("segment of another drand chain".into());
        }
        let first = u64::from_le_bytes(b[40..48].try_into().expect("8"));
        let count = u64::from_le_bytes(b[48..56].try_into().expect("8")) as usize;
        if first == 0 || count == 0 || b.len() != HEADER + 48 * count {
            return Err("segment length does not match its header".into());
        }
        let sigs = b[HEADER..]
            .chunks_exact(48)
            .map(|c| <[u8; 48]>::try_from(c).expect("48"))
            .collect();
        Ok(Segment { first, sigs })
    }

    fn get(&self, r: u64) -> Option<[u8; 48]> {
        r.checked_sub(self.first)
            .and_then(|i| self.sigs.get(i as usize))
            .copied()
    }
}

/// A loaded, verified archive.
#[derive(Clone, Debug)]
pub struct Archive {
    pub info: ChainInfo,
    /// Sorted by `first`, non-overlapping.
    pub segments: Vec<Segment>,
}

fn sha_hex(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}

/// Deterministic sample of indices in `0..n` (first, last and `k` spread).
fn sample(n: usize, k: usize) -> Vec<usize> {
    let mut v = vec![0, n - 1];
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15 ^ n as u64;
    for _ in 0..k.min(n) {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        v.push((x % n as u64) as usize);
    }
    v.sort_unstable();
    v.dedup();
    v
}

impl Archive {
    /// Builds an archive from rounds already verified by the caller,
    /// splitting them into contiguous segments.
    pub fn from_rounds(info: ChainInfo, rounds: &BTreeMap<u64, [u8; 48]>) -> Archive {
        let mut segments: Vec<Segment> = vec![];
        for (&r, s) in rounds {
            match segments.last_mut() {
                Some(seg) if seg.last() + 1 == r => seg.sigs.push(*s),
                _ => segments.push(Segment {
                    first: r,
                    sigs: vec![*s],
                }),
            }
        }
        Archive { info, segments }
    }

    pub fn rounds(&self) -> u64 {
        self.segments.iter().map(|s| s.sigs.len() as u64).sum()
    }

    pub fn range(&self) -> Option<(u64, u64)> {
        Some((self.segments.first()?.first, self.segments.last()?.last()))
    }

    pub fn get(&self, r: u64) -> Option<Beacon> {
        let i = self.segments.partition_point(|s| s.first <= r);
        let seg = self.segments.get(i.checked_sub(1)?)?;
        seg.get(r).map(|sig48| Beacon { round: r, sig48 })
    }

    /// The highest archived round ≤ `max`.
    pub fn latest_upto(&self, max: u64) -> Option<Beacon> {
        let i = self.segments.partition_point(|s| s.first <= max);
        let seg = self.segments.get(i.checked_sub(1)?)?;
        let r = max.min(seg.last());
        seg.get(r).map(|sig48| Beacon { round: r, sig48 })
    }

    /// Writes `info.json`, the segments and `manifest.json` (last, so a
    /// half-written archive has no manifest and is not loaded as packed).
    pub fn write(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let _ = std::fs::remove_file(dir.join("manifest.json"));
        let put = |name: &str, b: &[u8]| -> Result<(), String> {
            let p = dir.join(name);
            let tmp = dir.join(format!("{name}.tmp"));
            std::fs::write(&tmp, b).map_err(|e| format!("{}: {e}", tmp.display()))?;
            std::fs::rename(&tmp, &p).map_err(|e| format!("{}: {e}", p.display()))
        };
        put(
            "info.json",
            serde_json::to_string_pretty(&beacon::info_json(&self.info))
                .expect("json")
                .as_bytes(),
        )?;
        let mut segs = vec![];
        for s in &self.segments {
            let b = s.encode(&self.info.chain_hash);
            put(&s.file_name(), &b)?;
            segs.push(serde_json::json!({"file": s.file_name(), "first": s.first, "count": s.sigs.len(), "sha256": sha_hex(&b)}));
        }
        let m = serde_json::json!({"format": FORMAT, "chain_hash": hex::encode(self.info.chain_hash), "segments": segs});
        put(
            "manifest.json",
            serde_json::to_string_pretty(&m).expect("json").as_bytes(),
        )
    }

    /// Whether `dir` holds a packed archive.
    pub fn is_packed(dir: &Path) -> bool {
        dir.join("manifest.json").exists()
    }

    /// Loads a packed archive for the pinned chain `pinned`: its
    /// `info.json` must be that chain, every segment must match its sha256
    /// and chain hash, and the sampled rounds must verify.
    pub fn load(dir: &Path, pinned: &ChainInfo) -> Result<Archive, String> {
        let read = |name: &str| -> Result<Vec<u8>, String> {
            std::fs::read(dir.join(name)).map_err(|e| format!("{}: {e}", dir.join(name).display()))
        };
        let info_v: serde_json::Value =
            serde_json::from_slice(&read("info.json")?).map_err(|e| format!("info.json: {e}"))?;
        let info = beacon::parse_info_json(&info_v).ok_or("info.json: not a drand chain info")?;
        if info.public_key != pinned.public_key
            || info.chain_hash != pinned.chain_hash
            || info.period != pinned.period
            || info.genesis_time != pinned.genesis_time
        {
            return Err("the archive is of another drand chain than the pinned one".into());
        }
        let m: serde_json::Value = serde_json::from_slice(&read("manifest.json")?)
            .map_err(|e| format!("manifest.json: {e}"))?;
        if m.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
            return Err("manifest.json: unknown format".into());
        }
        if m.get("chain_hash").and_then(|f| f.as_str()) != Some(&hex::encode(info.chain_hash)) {
            return Err("manifest.json: chain hash differs from info.json".into());
        }
        let mut segments = vec![];
        for s in m
            .get("segments")
            .and_then(|s| s.as_array())
            .ok_or("manifest.json: no segments")?
        {
            let file = s
                .get("file")
                .and_then(|f| f.as_str())
                .ok_or("manifest.json: segment file")?;
            if file.contains('/') || file.contains("..") {
                return Err(format!("manifest.json: bad segment name {file}"));
            }
            let b = read(file)?;
            if Some(sha_hex(&b).as_str()) != s.get("sha256").and_then(|x| x.as_str()) {
                return Err(format!("{file}: sha256 does not match the manifest"));
            }
            let seg = Segment::decode(&b, &info.chain_hash).map_err(|e| format!("{file}: {e}"))?;
            if s.get("first").and_then(|x| x.as_u64()) != Some(seg.first)
                || s.get("count").and_then(|x| x.as_u64()) != Some(seg.sigs.len() as u64)
            {
                return Err(format!("{file}: range does not match the manifest"));
            }
            for i in sample(seg.sigs.len(), SAMPLE) {
                let r = seg.first + i as u64;
                if !beacon::verify(r, &seg.sigs[i], &info.public_key) {
                    return Err(format!("{file}: round {r} does not verify"));
                }
            }
            segments.push(seg);
        }
        segments.sort_by_key(|s| s.first);
        if segments.windows(2).any(|w| w[0].last() >= w[1].first) {
            return Err("manifest.json: overlapping segments".into());
        }
        Ok(Archive {
            info: pinned.clone(),
            segments,
        })
    }

    /// Re-verifies every round with blstrs on `threads` threads; returns
    /// the rounds that fail (empty = the archive is sound).
    pub fn verify_all(&self, threads: usize) -> Vec<u64> {
        let all: Vec<(u64, [u8; 48])> = self
            .segments
            .iter()
            .flat_map(|s| {
                s.sigs
                    .iter()
                    .enumerate()
                    .map(move |(i, sig)| (s.first + i as u64, *sig))
            })
            .collect();
        let threads = threads.max(1);
        let chunk = all.len().div_ceil(threads).max(1);
        let pk = self.info.public_key;
        let mut bad: Vec<u64> = std::thread::scope(|sc| {
            let hs: Vec<_> = all
                .chunks(chunk)
                .map(|c| {
                    sc.spawn(move || {
                        c.iter()
                            .filter(|(r, s)| !beacon::verify(*r, s, &pk))
                            .map(|(r, _)| *r)
                            .collect::<Vec<u64>>()
                    })
                })
                .collect();
            hs.into_iter()
                .flat_map(|h| h.join().unwrap_or_default())
                .collect()
        });
        bad.sort_unstable();
        bad
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fclient::beacon::TestKey;

    fn rounds(k: &TestKey, rs: impl Iterator<Item = u64>) -> BTreeMap<u64, [u8; 48]> {
        rs.map(|r| (r, k.sign(r))).collect()
    }

    #[test]
    fn segments_split_on_gaps_and_look_up_by_round() {
        let k = TestKey::new();
        let a = Archive::from_rounds(k.info(), &rounds(&k, (10..20).chain(30..35)));
        assert_eq!(a.segments.len(), 2);
        assert_eq!(a.rounds(), 15);
        assert_eq!(a.range(), Some((10, 34)));
        assert_eq!(a.get(15).unwrap().sig48, k.sign(15));
        assert!(a.get(9).is_none() && a.get(25).is_none() && a.get(35).is_none());
        assert_eq!(a.latest_upto(25).unwrap().round, 19);
        assert_eq!(a.latest_upto(1_000).unwrap().round, 34);
        assert!(a.latest_upto(9).is_none());
        let s = &a.segments[0];
        assert_eq!(
            Segment::decode(&s.encode(&k.info().chain_hash), &k.info().chain_hash).unwrap(),
            *s
        );
        assert!(Segment::decode(&s.encode(&[0; 32]), &k.info().chain_hash).is_err());
    }

    #[test]
    fn load_checks_the_chain_the_hashes_and_a_sample() {
        let k = TestKey::new();
        let dir = std::env::temp_dir().join(format!("psf-drand-arch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = Archive::from_rounds(k.info(), &rounds(&k, 1_000..1_100));
        a.write(&dir).unwrap();
        assert!(Archive::is_packed(&dir));
        let b = Archive::load(&dir, &k.info()).unwrap();
        assert_eq!(b.segments, a.segments);
        assert!(b.verify_all(4).is_empty());
        // Pinned to quicknet: refused.
        assert!(Archive::load(&dir, &beacon::quicknet_info()).is_err());
        // A flipped byte: the sha256 check refuses it.
        let f = dir.join(a.segments[0].file_name());
        let mut bytes = std::fs::read(&f).unwrap();
        bytes[HEADER + 48 * 50 + 3] ^= 1;
        std::fs::write(&f, &bytes).unwrap();
        let e = Archive::load(&dir, &k.info()).unwrap_err();
        assert!(e.contains("sha256"), "{e}");
        // A wrong signature under a rewritten manifest: sampled rounds or
        // `verify_all` catch it.
        let mut bad = a.clone();
        bad.segments[0].sigs[50] = k.sign(9_999);
        bad.write(&dir).unwrap();
        let loaded = Archive::load(&dir, &k.info());
        match loaded {
            Err(e) => assert!(e.contains("does not verify"), "{e}"),
            Ok(l) => assert_eq!(l.verify_all(2), vec![1_050]),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
