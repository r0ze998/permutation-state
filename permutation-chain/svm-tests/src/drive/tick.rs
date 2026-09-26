//! A tick as the crank drives it: past the deadline, CloseCommits, (the
//! reveals), LogTickInput for every chunk, ResolveTick in the crank's parts.

use crate::budget::resolve_parts;
use crate::chain::Chain;
use crate::records::records;
use crate::season::SeasonFx;

/// What one driven tick cost.
#[derive(Debug, Default)]
pub struct TickStats {
    /// The largest CU of any of its transactions.
    pub worst_cu: u64,
    /// The tick input as published (the PS_INPUT chunks, in order).
    pub input: Vec<u8>,
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

    /// LogTickInput for every chunk of the open tick's input; returns the
    /// input bytes and the largest CU.
    pub fn log_input(&self, c: &mut Chain) -> (Vec<u8>, u64) {
        let crank = self.crank.insecure_clone();
        let (mut input, mut worst) = (vec![], 0);
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
