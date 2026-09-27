//! `findex`: ingest and index (M1 contract §8.1, offchain design §4.3).
//!
//! **W1 skeleton.** W2-F builds the ingest backends (RpcPoll over
//! `getSignaturesForAddress`, LocalnetFeed over `frontier_feed`), the
//! append-only archive with its manifest, the SQLite entity index and the
//! account snapshots. What exists now is the part every later unit needs
//! first: pulling the ordered feed through any `ChainPort` and splitting
//! each transaction into its PS2 records.

use fclient::log::{bodies_from_logs, LogError, Record};
use fclient::ports::{ChainPort, Cursor, PortResult, TxRecord};

/// The PS2 records of one transaction, in log order.
pub fn records(tx: &TxRecord) -> Result<Vec<Record>, LogError> {
    bodies_from_logs(&tx.logs)?
        .iter()
        .map(|b| Record::decode(b))
        .collect()
}

/// An in-memory archive of the feed (W2-F replaces it with segment files).
#[derive(Default)]
pub struct MemArchive {
    pub cursor: Cursor,
    pub txs: Vec<TxRecord>,
}

impl MemArchive {
    /// Pulls everything after the cursor; returns how many were added.
    pub async fn pull<P: ChainPort>(&mut self, port: &P) -> PortResult<usize> {
        let got = port.feed(self.cursor).await?;
        if let Some(last) = got.last() {
            self.cursor = Cursor(last.seq);
        }
        let n = got.len();
        self.txs.extend(got);
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fclient::log;

    #[test]
    fn splits_ps2_records_out_of_logs() {
        let r = log::Record {
            ver: 1,
            kind: fclient::abi::kind::ANCHOR,
            bell: 7,
            key_payload: (0..60u8)
                .map(|i| i.wrapping_mul(7).wrapping_add(100))
                .collect(),
            links: vec![],
        };
        let tx = TxRecord {
            seq: 1,
            slot: 2,
            signature: Default::default(),
            block_time: 0,
            tx: vec![],
            logs: vec!["Program log: hi".into(), log::log_line(&r.encode())],
            err: None,
            code: None,
            units: 0,
            fee: 0,
            post: vec![],
        };
        assert_eq!(records(&tx).unwrap(), vec![r]);
    }
}
