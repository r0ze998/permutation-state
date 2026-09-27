//! The bond, StartSeason, the season seed, genesis, seating and the first
//! election.

use solana_signer::Signer;

use crate::chain::{Chain, Landed};
use crate::season::SeasonFx;
use crate::state::{bond_floor, SeasonStatus};
use crate::vrf;

/// Members per SeatMembers transaction (`season.mjs` `SEAT_BATCH`).
pub const SEAT_BATCH: usize = 6;
/// `GenesisStep { work }` as the crank sends it.
pub const GENESIS_WORK: u32 = 50;

/// The oracle output the drivers answer season `id`'s seed request with.
pub fn seed_oracle(id: u64) -> [u8; 32] {
    let mut e = [0x5e; 32];
    e[..8].copy_from_slice(&id.to_le_bytes());
    e
}

impl SeasonFx {
    /// PostBond for what the bond lacks of `bond_floor`, from the admin's
    /// USDC account (topped up to it), as the gateway's `topUpBond` does
    /// before StartSeason. Nothing when the bond covers the floor.
    pub fn top_up_bond(&self, c: &mut Chain) -> u64 {
        let season = self.season(c);
        let short = bond_floor(&season).saturating_sub(season.bond);
        if short > 0 {
            let admin = self.admin.insecure_clone();
            let have = c.balance(&self.admin_token);
            c.set_balance(&self.admin_token, have + short);
            c.send(
                vec![self.post_bond_ix(&admin.pubkey(), &self.admin_token, short)],
                &[&admin],
            )
            .expect("PostBond");
        }
        short
    }

    /// The bond (with AIs), StartSeason (the crank), the season seed, then
    /// GenesisStep until Seating; returns each step's CU.
    pub fn genesis(&self, c: &mut Chain) -> Vec<u64> {
        if !self.ai.is_empty() {
            self.top_up_bond(c);
        }
        let crank = self.crank.insecure_clone();
        c.send(vec![self.start_ix(&crank.pubkey())], &[&crank])
            .expect("StartSeason");
        self.genesis_steps(c)
    }

    /// While Seeding: RetrySeasonSeed (the crank) and the oracle's answer
    /// (`seed_oracle`) through the VRF stand-in; the season is then in
    /// Genesis. Returns the callback's transaction.
    pub fn seed(&self, c: &mut Chain) -> Landed {
        let crank = self.crank.insecure_clone();
        vrf::requests();
        c.send(vec![self.retry_seed_ix(&crank.pubkey())], &[&crank])
            .expect("RetrySeasonSeed");
        let req = vrf::requests()
            .pop()
            .expect("RetrySeasonSeed files a request")
            .request;
        let landed =
            vrf::fulfil(c, &req, seed_oracle(self.p.id)).expect("the season seed is consumed");
        assert_eq!(self.season(c).status, SeasonStatus::Genesis);
        landed
    }

    /// From Seeding (the seed first) or Genesis: GenesisStep until Seating;
    /// returns each step's CU.
    pub fn genesis_steps(&self, c: &mut Chain) -> Vec<u64> {
        if self.season(c).status == SeasonStatus::Seeding {
            self.seed(c);
        }
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
