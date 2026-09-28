//! `findex`: ingest and index (M1 contract §8.1; offchain design §4.3).
//!
//! | module | what |
//! |---|---|
//! | [`ingest`] | sources: [`ingest::LocalnetFeed`] (any `ChainPort`'s ordered feed), [`ingest::RpcPoll`] (`getSignaturesForAddress` + `getTransaction`) and [`ingest::Enriched`] (post-state fetched for a source without it, before archiving) |
//! | [`archive`] | append-only segment files with a manifest (sha256 per sealed segment, the source cursor) |
//! | [`index`] | the SQLite entity index: transactions, PS2 records, chain links and heads, account snapshots, event numbering; [`index::Reader`] for a second (read-only) connection |
//!
//! [`Findex`] ties them together: every pull is archived first (with the
//! source's cursor), then indexed; on open the index catches up from the
//! archive, and the source resumes from the archived cursor. The herald,
//! keeper and verifier read the archive; only the verifier refuses to trust
//! it (it re-derives every head from on-chain accounts, §8.5).

pub mod archive;
pub mod index;
pub mod ingest;

use std::path::{Path, PathBuf};

use fclient::log::{bodies_from_logs, LogError, Record};
use fclient::ports::TxRecord;
use frontier_abi::addr::AddrCtx;
use solana_address::Address;

pub use archive::Archive;
pub use index::{EventRow, Index, Reader};
pub use ingest::{Enriched, IngestError, LocalnetFeed, RpcPoll, Source};

/// The PS2 records of one transaction, in log order, decoded with the
/// canonical per-kind lengths of `frontier-abi::log` (W1-F note 5: without
/// them a payload byte could impersonate a tail).
pub fn records(tx: &TxRecord, program: &Address) -> Result<Vec<Record>, LogError> {
    let lens = |k: u8| {
        frontier_abi::log::Kind::from_u8(k).map(|k| k.spec().key_len() + k.spec().payload_len())
    };
    bodies_from_logs(&tx.logs, program)?
        .iter()
        .map(|b| Record::decode_with(b, &lens))
        .collect()
}

/// The address context of a season (program id and Season PDA).
pub fn addr_ctx(program: &Address, season_id: u64) -> AddrCtx {
    let (season, _) = fclient::addr::season_pda(program, season_id);
    AddrCtx {
        season: season.to_bytes(),
        program: program.to_bytes(),
    }
}

/// Records indexed per SQLite transaction while catching up.
pub const CATCH_UP_BATCH: usize = 2_000;

/// Archive + index in one directory (`archive/`, `index.sqlite`).
pub struct Findex {
    pub dir: PathBuf,
    pub archive: Archive,
    pub index: Index,
}

impl Findex {
    /// Opens (or creates) the store; the index catches up from the archive.
    pub fn open(
        dir: &Path,
        program: Address,
        season_id: Option<u64>,
        segment_bytes: u64,
    ) -> Result<Findex, IngestError> {
        let st = |e: String| IngestError::Store(e);
        std::fs::create_dir_all(dir).map_err(|e| st(e.to_string()))?;
        let archive =
            Archive::open(&dir.join("archive"), segment_bytes).map_err(|e| st(e.to_string()))?;
        let ctx = season_id.map(|id| addr_ctx(&program, id));
        let mut index = Index::open(&dir.join("index.sqlite"), program, ctx).map_err(st)?;
        let mut behind = index.last_seq().map_err(st)?;
        if behind > archive.last_seq() {
            // The index is ahead of the durable archive: a crash inside the
            // archive's group commit (the index commits at once, the
            // archive about once a second). Roll the index back to the
            // archive instead of rebuilding the season (wave-5 review of
            // W5-C); the source then re-delivers the lost tail.
            index.truncate_to(archive.last_seq()).map_err(st)?;
            behind = archive.last_seq();
        }
        if behind < archive.last_seq() {
            // Catch up in batches (a season's archive does not fit in memory).
            let mut batch = Vec::with_capacity(CATCH_UP_BATCH);
            archive
                .for_each_after(behind, |r| {
                    batch.push(r);
                    if batch.len() >= CATCH_UP_BATCH {
                        index.add(&batch).map_err(std::io::Error::other)?;
                        batch.clear();
                    }
                    Ok(())
                })
                .map_err(|e| st(e.to_string()))?;
            index.add(&batch).map_err(st)?;
        }
        Ok(Findex {
            dir: dir.into(),
            archive,
            index,
        })
    }

    /// Makes every archived record durable (group commit, see
    /// [`archive`]); a no-op when nothing is pending.
    pub fn commit(&mut self) -> Result<(), IngestError> {
        self.archive
            .commit()
            .map_err(|e| IngestError::Store(e.to_string()))
    }

    /// Resumes `src` from the archived cursor.
    pub fn resume<S: Source>(&self, src: &mut S) {
        src.restore(self.archive.cursor());
    }

    /// One pull: archive, then index. Returns the archived records.
    pub async fn ingest<S: Source>(&mut self, src: &mut S) -> Result<Vec<TxRecord>, IngestError> {
        let got = src.pull().await.map_err(IngestError::Port)?;
        if got.is_empty() {
            return Ok(vec![]);
        }
        let stored = self
            .archive
            .append(&got, src.cursor())
            .map_err(|e| IngestError::Store(e.to_string()))?;
        self.index.add(&stored).map_err(IngestError::Store)?;
        Ok(stored)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fclient::log;

    #[test]
    fn splits_ps2_records_out_of_logs_with_the_abi_lengths() {
        let r = log::Record {
            ver: 1,
            kind: fclient::abi::kind::ANCHOR,
            bell: 7,
            key_payload: (0..61u8)
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
            logs: log::in_frame(
                &Address::new_from_array([3; 32]),
                ["Program log: hi".into(), log::log_line(&r.encode())],
            ),
            err: None,
            code: None,
            units: 0,
            fee: 0,
            post: vec![],
        };
        assert_eq!(
            records(&tx, &Address::new_from_array([3; 32])).unwrap(),
            vec![r]
        );
        // Another program's frame: no record.
        assert!(records(&tx, &Address::new_from_array([4; 32]))
            .unwrap()
            .is_empty());
        assert_eq!(
            frontier_abi::log::Kind::ANCHOR.spec().body_len(),
            6 + 61,
            "ANCHOR: 5-B key, 56-B payload"
        );
    }
}
