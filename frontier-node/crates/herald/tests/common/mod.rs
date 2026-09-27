//! Shared test helpers: a source over a fixed list of transactions and
//! temporary directories.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use fclient::ports::{PortResult, TxRecord};
use serde_json::{json, Value};

/// A `findex::Source` over fixture transactions, `batch` per pull.
pub struct VecSource {
    pub txs: Vec<TxRecord>,
    pub pos: usize,
    pub batch: usize,
}

impl VecSource {
    pub fn new(txs: Vec<TxRecord>, batch: usize) -> VecSource {
        VecSource { txs, pos: 0, batch }
    }
}

impl findex::Source for VecSource {
    async fn pull(&mut self) -> PortResult<Vec<TxRecord>> {
        let end = (self.pos + self.batch).min(self.txs.len());
        let out = self.txs[self.pos..end].to_vec();
        self.pos = end;
        Ok(out)
    }
    fn cursor(&self) -> Value {
        json!({"pos": self.pos})
    }
    fn restore(&mut self, v: &Value) {
        if let Some(p) = v.get("pos").and_then(|x| x.as_u64()) {
            self.pos = p as usize;
        }
    }
}

pub fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "herald-it-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// Every file under `root` with its bytes, sorted by path.
pub fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    let out = herald_fold::files::Out::new(root);
    out.list()
        .unwrap()
        .into_iter()
        .filter(|p| !p.contains("/.") && !p.starts_with('.'))
        .map(|p| {
            let b = std::fs::read(root.join(&p)).unwrap();
            (p, b)
        })
        .collect()
}
