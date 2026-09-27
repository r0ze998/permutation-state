//! The bots' [`DirectPort`] over the in-process chain: the personas whose
//! transactions the relay never sponsors (a Reveal with their own
//! beneficiary, a zero tip, a forged key, a pre-funding transfer, a
//! `frontier_hold`) send them here, paid by their own airdropped key.

use fclient::ports::{ChainPort, PortError, SimResult};
use fclient::{Address, Hash};
use frontier_bots::ports::{DirectPort, PortResult};
use localnet::InProcess;

pub struct InProcDirect {
    pub ip: InProcess,
}

impl DirectPort for InProcDirect {
    async fn blockhash(&self) -> PortResult<Hash> {
        Ok(self.ip.blockhash().await?.0)
    }
    async fn simulate(&self, wire: &[u8]) -> PortResult<SimResult> {
        self.ip.simulate(wire).await
    }
    async fn send(&self, wire: &[u8]) -> PortResult<String> {
        Ok(self.ip.send(wire).await?.to_string())
    }
    async fn airdrop(&self, key: &Address, lamports: u64) -> PortResult<()> {
        self.ip
            .lock()
            .airdrop(key, lamports)
            .map(|_| ())
            .map_err(PortError::Rejected)
    }
    async fn balance(&self, key: &Address) -> PortResult<u64> {
        Ok(self.ip.lock().balance(key))
    }
    async fn hold(&self, keys: &[Address], priority_milli: u32, slots: u64) -> PortResult<()> {
        self.ip
            .lock()
            .hold(keys.to_vec(), priority_milli as u64, slots)
            .map(|_| ())
            .map_err(PortError::Rejected)
    }
}
