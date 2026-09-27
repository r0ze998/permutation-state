//! The herald's output directory (contract §8.4 fold rules).
//!
//! Every file is written **atomically**: bytes to a temporary file in the
//! same directory, then `rename` over the target (readers never see a
//! partial file). No precompressed sibling is written in M1 wave 3: the
//! `flate2` dependency (W3-D request R3) is outside the contract's expected
//! dependency set and waits for the owner (integ-W3). A `.gz` sibling
//! placed next to a file by other means is still served by the server, and
//! a write that changes a file's bytes removes a stale sibling.
//!
//! **Durability is batched** ([`Out::new`]): writes are not synced one by
//! one (an `fsync` is ≈ 10 ms on this Mac's APFS — F_FULLFSYNC — so a bell
//! of a few hundred files would stall the fold); the written paths are
//! remembered and [`sync_paths`] syncs them, and their directories, before
//! the fold's checkpoint is saved. A crash can therefore only lose or tear
//! files written after the last checkpoint, and the restart re-folds
//! exactly those records and rewrites them. [`Out::synced`] syncs every
//! write instead. The same archive gives byte-identical files
//! (determinism test).
//!
//! Writing the same bytes again is a no-op ([`Written::Same`]): a restart
//! that re-folds records after its checkpoint rewrites nothing. Writing
//! different bytes over a file is [`Written::Changed`]; for a per-bell
//! (immutable) file that is an alarm the fold counts.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// What a write did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Written {
    New,
    Same,
    Changed,
}

/// The output root (`<data>/files`); relative paths use `/`.
#[derive(Clone, Debug)]
pub struct Out {
    pub root: PathBuf,
    /// Paths written since the last [`Out::take_dirty`] (batched
    /// durability); `None` = every write is synced.
    pub dirty: Option<Arc<Mutex<BTreeSet<PathBuf>>>>,
}

/// `path` written atomically (temp + fsync + rename + directory sync).
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_via_temp(path, bytes, true)
}

fn write_via_temp(path: &Path, bytes: &[u8], sync: bool) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| io::Error::other("no parent"))?;
    fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("no file name"))?
        .to_string_lossy();
    let tmp = dir.join(format!(".{name}.tmp"));
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        if sync {
            f.sync_all()?;
        }
    }
    fs::rename(&tmp, path)?;
    if sync {
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
    }
    Ok(())
}

/// Syncs files (those still present) and then their directories.
pub fn sync_paths(paths: &[PathBuf]) -> io::Result<usize> {
    let mut dirs = BTreeSet::new();
    let mut n = 0;
    for p in paths {
        match File::open(p) {
            Ok(f) => {
                f.sync_all()?;
                n += 1;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        if let Some(d) = p.parent() {
            dirs.insert(d.to_path_buf());
        }
    }
    for d in dirs {
        if let Ok(f) = File::open(&d) {
            let _ = f.sync_all();
        }
    }
    Ok(n)
}

impl Out {
    /// Batched durability ([`Out::take_dirty`] + [`sync_paths`]).
    pub fn new(root: impl Into<PathBuf>) -> Out {
        Out {
            root: root.into(),
            dirty: Some(Arc::new(Mutex::new(BTreeSet::new()))),
        }
    }

    /// Every write synced.
    pub fn synced(root: impl Into<PathBuf>) -> Out {
        Out {
            root: root.into(),
            dirty: None,
        }
    }

    /// The paths written since the last call.
    pub fn take_dirty(&self) -> Vec<PathBuf> {
        match &self.dirty {
            Some(d) => std::mem::take(&mut *d.lock().unwrap_or_else(|e| e.into_inner()))
                .into_iter()
                .collect(),
            None => vec![],
        }
    }

    fn put(&self, p: &Path, bytes: &[u8]) -> io::Result<()> {
        write_via_temp(p, bytes, self.dirty.is_none())?;
        if let Some(d) = &self.dirty {
            d.lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(p.to_path_buf());
        }
        Ok(())
    }

    /// The absolute path of a relative one (no `..`, no absolute paths).
    pub fn path(&self, rel: &str) -> Option<PathBuf> {
        let ok = !rel.is_empty()
            && !rel.starts_with('/')
            && rel
                .split('/')
                .all(|s| !s.is_empty() && s != "." && s != ".." && !s.starts_with('.'));
        ok.then(|| self.root.join(rel))
    }

    /// Writes `bytes` at `rel` (removing a stale `.gz` sibling).
    pub fn write(&self, rel: &str, bytes: &[u8]) -> io::Result<Written> {
        let p = self
            .path(rel)
            .ok_or_else(|| io::Error::other(format!("bad path {rel}")))?;
        let gzp = gz_path(&p);
        let old = match fs::read(&p) {
            Ok(b) => Some(b),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        if old.as_deref() == Some(bytes) {
            return Ok(Written::Same);
        }
        // A stale sibling must never be served with new bytes.
        if gzp.exists() {
            fs::remove_file(&gzp)?;
        }
        self.put(&p, bytes)?;
        Ok(if old.is_some() {
            Written::Changed
        } else {
            Written::New
        })
    }

    /// Reads a file (`None` if absent or the path is not allowed).
    pub fn read(&self, rel: &str) -> Option<Vec<u8>> {
        fs::read(self.path(rel)?).ok()
    }

    /// Every file under the root (relative paths, sorted), for the
    /// determinism tests.
    pub fn list(&self) -> io::Result<Vec<String>> {
        let mut out = vec![];
        walk(&self.root, &self.root, &mut out)?;
        out.sort();
        Ok(out)
    }
}

/// `x.json` → `x.json.gz`.
pub fn gz_path(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".gz");
    PathBuf::from(s)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> io::Result<()> {
    let rd = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for e in rd {
        let e = e?;
        let p = e.path();
        if e.file_type()?.is_dir() {
            walk(root, &p, out)?;
        } else if let Ok(r) = p.strip_prefix(root) {
            out.push(r.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "herald-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn writes_atomically_and_drops_a_stale_gz_sibling() {
        let d = tmp("files");
        let o = Out::synced(&d);
        assert!(o.take_dirty().is_empty());
        assert_eq!(o.write("h/a/1.json", b"{\"x\":1}").unwrap(), Written::New);
        assert_eq!(o.write("h/a/1.json", b"{\"x\":1}").unwrap(), Written::Same);
        assert!(!d.join("h/a/1.json.gz").exists(), "no sibling written");
        // A sibling placed by other means is removed when the bytes change.
        fs::write(d.join("h/a/1.json.gz"), b"stale").unwrap();
        assert_eq!(o.write("h/a/1.json", b"{\"x\":1}").unwrap(), Written::Same);
        assert!(d.join("h/a/1.json.gz").exists(), "same bytes keep it");
        assert_eq!(
            o.write("h/a/1.json", b"{\"x\":2}").unwrap(),
            Written::Changed
        );
        assert!(!d.join("h/a/1.json.gz").exists(), "stale sibling removed");
        assert_eq!(o.read("h/a/1.json").unwrap(), b"{\"x\":2}");
        assert_eq!(o.list().unwrap(), vec!["h/a/1.json".to_string()]);
        for bad in ["", "/etc/x", "h/../x", "h/.hidden", "h//x"] {
            assert!(o.path(bad).is_none(), "{bad}");
        }
        // Batched durability remembers what it wrote.
        let b = Out::new(&d);
        b.write("h/b/2.bin", &[1, 2, 3]).unwrap();
        let dirty = b.take_dirty();
        assert_eq!(dirty.len(), 1, "the file");
        assert_eq!(sync_paths(&dirty).unwrap(), 1);
        assert!(b.take_dirty().is_empty());
        let _ = fs::remove_dir_all(&d);
    }
}
