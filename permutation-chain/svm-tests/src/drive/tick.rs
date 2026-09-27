//! A tick as the crank drives it: past the deadline, CloseCommits, (the
//! reveals), `[FreezeTick, RetryTickRandomness]` in one transaction, the
//! oracle's answer (`vrf::fulfil`), LogTickInput for every chunk, then
//! ResolveTick in the crank's parts.

use permutation_chain::randomness::{tick_callback_args, RAND_VRF};
use permutation_rules::gov::Role;
use sha2::{Digest, Sha256};
use solana_signer::Signer;

use crate::budget::resolve_parts;
use crate::chain::{Chain, Landed};
use crate::records::records;
use crate::season::SeasonFx;
use crate::state::NationAccount;
use crate::vrf;

/// What one driven tick cost.
#[derive(Debug, Default)]
pub struct TickStats {
    /// The largest CU of any of its transactions.
    pub worst_cu: u64,
    /// The tick input as published (the PS_INPUT chunks, in order).
    pub input: Vec<u8>,
}

/// The oracle output the stand-in answers tick `tick` of season `id` with
/// (deterministic, so runs reproduce).
pub fn oracle_e(id: u64, tick: u16) -> [u8; 32] {
    Sha256::new()
        .chain_update(b"svm-tests/vrf")
        .chain_update(id.to_le_bytes())
        .chain_update(tick.to_le_bytes())
        .finalize()
        .into()
}

impl SeasonFx {
    /// Moves the clock to the open tick's deadline (if it is not past it)
    /// and sends CloseCommits.
    pub fn close(&self, c: &mut Chain) -> u64 {
        let crank = self.crank.insecure_clone();
        c.set_time(self.meta(c).deadline.max(c.now));
        c.send(vec![self.close_ix()], &[&crank])
            .expect("CloseCommits")
            .cu
    }

    /// Whether an office committed to the open tick and has not revealed.
    pub fn reveals_missing(&self, c: &Chain) -> bool {
        (0..self.p.nations as usize).any(|civ| {
            let n = self.nation(c, civ);
            Role::ALL.iter().any(|r| {
                let i = r.index();
                n.committed[i] == n.open_tick && n.submitted[i] != n.open_tick
            })
        })
    }

    /// The crank's `[FreezeTick, RetryTickRandomness]` (the freeze and the
    /// first VRF request) in one transaction, on the queue of the season's
    /// play mode. Waits for the reveal deadline if a reveal is missing.
    pub fn freeze_and_request(&self, c: &mut Chain) -> Landed {
        let crank = self.crank.insecure_clone();
        if self.reveals_missing(c) {
            c.set_time(self.meta(c).deadline.max(c.now));
        }
        let q = self.vrf_queue(c);
        let cp = crank.pubkey();
        c.send(
            vec![self.freeze_ix(&cp, &q), self.retry_ix(&cp, &q)],
            &[&crank],
        )
        .expect("FreezeTick + RetryTickRandomness")
    }

    /// The latest request the stand-in received for the frozen tick.
    pub fn tick_request(&self, c: &Chain) -> vrf::Request {
        let args = tick_callback_args(self.p.id, self.meta(c).rand_tick).to_vec();
        vrf::requests()
            .into_iter()
            .rev()
            .map(|r| r.request)
            .find(|r| r.callback_args == args && r.callback_program_id == self.program.to_bytes())
            .expect("a VRF request for the frozen tick")
    }

    /// The oracle answers the frozen tick's latest request with `e`.
    pub fn fulfil_tick(&self, c: &mut Chain, e: [u8; 32]) -> Landed {
        let request = self.tick_request(c);
        vrf::fulfil(c, &request, e).expect("the oracle's callback")
    }

    /// Freezes the tick, requests its randomness and has the oracle answer
    /// with `oracle_e`; returns the largest CU.
    pub fn freeze(&self, c: &mut Chain) -> u64 {
        let freeze = self.freeze_and_request(c).cu;
        let tick = self.meta(c).rand_tick;
        let answer = self.fulfil_tick(c, oracle_e(self.p.id, tick)).cu;
        assert_eq!(
            self.meta(c).rand_state,
            RAND_VRF,
            "the tick's randomness is drawn"
        );
        freeze.max(answer)
    }

    /// LogTickInput for every chunk of the open tick's input (freezing it
    /// first if it is not frozen yet); returns the input bytes and the
    /// largest CU.
    pub fn log_input(&self, c: &mut Chain) -> (Vec<u8>, u64) {
        let crank = self.crank.insecure_clone();
        let mut worst = 0;
        if !self.meta(c).frozen {
            worst = self.freeze(c);
        }
        let mut input = vec![];
        let mut chunk = 0;
        loop {
            let l = c
                .send(vec![self.log_ix(chunk)], &[&crank])
                .expect("LogTickInput");
            worst = worst.max(l.cu);
            let r = records(&l.logs, b"PS_INPUT");
            assert_eq!(r.len(), 1, "PS_INPUT fits the log");
            input.extend_from_slice(&r[0][5]);
            chunk += 1;
            let m = self.meta(c);
            if m.input_logged >= m.input_chunks {
                return (input, worst);
            }
        }
    }

    /// One tick with no orders, as the crank drives it (ResolveTick in
    /// `crank_stops()` parts).
    pub fn play_tick(&self, c: &mut Chain) -> TickStats {
        let crank = self.crank.insecure_clone();
        let mut worst = self.close(c);
        let (input, log) = self.log_input(c);
        worst = worst.max(log);
        for p in resolve_parts(c, self, &crank, false) {
            worst = worst.max(p.cu);
        }
        TickStats {
            worst_cu: worst,
            input,
        }
    }

    /// Every nation's roll and governance quota as `OpenGovernment` sets
    /// them (`NationAccount::seat_roll`, WP03), written where a nation's
    /// roll is still empty: a program whose OpenGovernment does not fill
    /// them yet (unit P2) is driven as if it did. Once it does, this
    /// changes nothing.
    pub fn ensure_rolls(&self, c: &mut Chain) {
        let state = self.world(c);
        for k in &self.nations {
            c.edit::<NationAccount>(k, |n| {
                if n.roll.is_empty() {
                    n.seat_roll(&state);
                }
            });
        }
    }

    /// Sets the world's tick (phase cursor 0) through `Chunks`: a synthetic
    /// world at that point of the season.
    pub fn set_tick(&self, c: &mut Chain, tick: u16) {
        let chunks = self.chunks.clone();
        c.with_world(&chunks, |w| {
            let mut s = w.read_world().unwrap();
            s.tick = tick;
            s.phase_cursor = 0;
            w.write_world(&s).unwrap();
        });
        for k in &self.nations {
            c.edit::<NationAccount>(k, |n| n.open_tick = tick);
        }
    }

    /// Jumps the world to its last tick, finished (a synthetic final world),
    /// with the deadline now.
    pub fn fast_forward_to_end(&self, c: &mut Chain) {
        let ticks = self.rules().ticks_per_season;
        let now = c.now;
        let chunks = self.chunks.clone();
        c.with_world(&chunks, |w| {
            let mut s = w.read_world().unwrap();
            s.tick = ticks;
            s.phase_cursor = 0;
            w.write_world(&s).unwrap();
            let mut m = w.meta().unwrap();
            m.finished = true;
            m.deadline = now;
            w.set_meta(&m).unwrap();
        });
    }
}
