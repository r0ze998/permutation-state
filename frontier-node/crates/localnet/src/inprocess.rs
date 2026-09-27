//! `ChainPort::InProcess`: the chain in the same process, on virtual time.
//! `send` queues; the caller produces blocks with [`InProcess::step`] (the
//! in-process gates advance time slot by slot, so a game day of 144 bells
//! at 20× is 10,800 steps and no waiting).

use std::sync::{Arc, Mutex, MutexGuard};

use solana_address::Address;
use solana_hash::Hash;

use fclient::ports::{
    Account, ChainPort, ClockSysvar, Cursor, PortError, PortResult, Signature, SimResult, Status,
    TxRecord,
};

use crate::chain::{BlockReport, Chain, Config};

#[derive(Clone)]
pub struct InProcess {
    pub chain: Arc<Mutex<Chain>>,
    /// Feed filter (the Frontier program), or every transaction.
    pub program: Option<Address>,
}

impl InProcess {
    pub fn new(cfg: Config, program: Option<Address>) -> InProcess {
        InProcess {
            chain: Arc::new(Mutex::new(Chain::new(cfg))),
            program,
        }
    }

    pub fn from_chain(chain: Arc<Mutex<Chain>>, program: Option<Address>) -> InProcess {
        InProcess { chain, program }
    }

    pub fn lock(&self) -> MutexGuard<'_, Chain> {
        self.chain.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Produces `n` blocks.
    pub fn step(&self, n: u64) -> Vec<BlockReport> {
        let mut c = self.lock();
        (0..n).map(|_| c.produce_block()).collect()
    }
}

impl ChainPort for InProcess {
    async fn clock(&self) -> PortResult<ClockSysvar> {
        Ok(self.lock().clock())
    }

    async fn accounts(&self, keys: &[Address], min_slot: u64) -> PortResult<Vec<Option<Account>>> {
        let c = self.lock();
        if c.slot() < min_slot {
            return Err(PortError::Rpc {
                code: -32016,
                message: format!("minimum context slot {min_slot} not reached ({})", c.slot()),
            });
        }
        Ok(keys.iter().map(|k| c.account(k)).collect())
    }

    async fn simulate(&self, tx: &[u8]) -> PortResult<SimResult> {
        self.lock()
            .simulate(tx, true)
            .map_err(|e| PortError::Rejected(e.to_string()))
    }

    async fn send(&self, tx: &[u8]) -> PortResult<Signature> {
        self.lock()
            .submit(tx)
            .map_err(|e| PortError::Rejected(e.to_string()))
    }

    async fn statuses(&self, sigs: &[Signature]) -> PortResult<Vec<Option<Status>>> {
        let c = self.lock();
        Ok(sigs.iter().map(|s| c.status(s)).collect())
    }

    async fn feed(&self, after: Cursor) -> PortResult<Vec<TxRecord>> {
        let c = self.lock();
        Ok(c.feed(after.0, 10_000, self.program.as_ref())
            .into_iter()
            .map(|l| l.record())
            .collect())
    }

    async fn blockhash(&self) -> PortResult<(Hash, u64)> {
        Ok(self.lock().latest_blockhash())
    }
}
