//! StartSeason, genesis, seating and the first election.

use solana_signer::Signer;

use crate::chain::Chain;
use crate::season::SeasonFx;
use crate::state::SeasonStatus;

/// Members per SeatMembers transaction (`season.mjs` `SEAT_BATCH`).
pub const SEAT_BATCH: usize = 6;
/// `GenesisStep { work }` as the crank sends it.
pub const GENESIS_WORK: u32 = 50;

impl SeasonFx {
    /// StartSeason (the crank), then GenesisStep until Seating; returns each
    /// step's CU.
    pub fn genesis(&self, c: &mut Chain) -> Vec<u64> {
        let crank = self.crank.insecure_clone();
        c.send(vec![self.start_ix(&crank.pubkey())], &[&crank])
            .expect("StartSeason");
        self.genesis_steps(c)
    }

    /// GenesisStep until Seating (from Genesis); returns each step's CU.
    pub fn genesis_steps(&self, c: &mut Chain) -> Vec<u64> {
        let crank = self.crank.insecure_clone();
        let mut cus = vec![];
        for _ in 0..200 {
            let l = c
                .send(vec![self.genesis_step_ix(GENESIS_WORK)], &[&crank])
                .expect("GenesisStep");
            cus.push(l.cu);
            if self.season(c).status == SeasonStatus::Seating {
                return cus;
            }
        }
        panic!("genesis did not finish in 200 steps");
    }

    /// SeatMembers in the gateway's batches, in registration order.
    pub fn seat_all(&self, c: &mut Chain) {
        let crank = self.crank.insecure_clone();
        let seated = self.season(c).seated as usize;
        let all: Vec<usize> = (seated..self.members.len()).collect();
        for batch in all.chunks(SEAT_BATCH) {
            c.send(vec![self.seat_ix(&crank.pubkey(), batch)], &[&crank])
                .expect("SeatMembers");
        }
    }

    /// OpenGovernment (the crank).
    pub fn open(&self, c: &mut Chain) {
        let crank = self.crank.insecure_clone();
        c.send(vec![self.open_ix(&crank.pubkey())], &[&crank])
            .expect("OpenGovernment");
    }

    pub fn seat_and_open(&self, c: &mut Chain) {
        self.seat_all(c);
        self.open(c);
        assert_eq!(self.season(c).status, SeasonStatus::Running);
    }

    /// Registered, genesis, seated and open: a running season of `n`
    /// members spread over the nations in turn.
    pub fn running(c: &mut Chain, p: crate::season::Params, n: usize) -> SeasonFx {
        let nations = p.nations as usize;
        let mut s = SeasonFx::create(c, p);
        for i in 0..n {
            s.register(c, (i % nations) as u16, 0);
        }
        s.genesis(c);
        s.seat_and_open(c);
        s
    }
}
