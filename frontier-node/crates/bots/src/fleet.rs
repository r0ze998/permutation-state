//! Many bots in one process (M1 contract §8.6: 1,000 per process; offchain
//! design §9: "a tokio task per bot plus a shared tlock worker pool").
//!
//! Each bot's task sleeps on the game clock between wake-ups:
//! - **sessions** by its profile: on a day it plays (`day_p`), `sessions`
//!   sessions spread evenly over the game day with a random offset each
//!   (the simulator's hourly bot sessions become 24 a day);
//! - **duties** every bell, 0–20 game seconds after the bell starts (§9.1's
//!   self-reveal timing), while it has something to do then: not joined
//!   yet (from its join bell), a ticket or a provisional holding, a march
//!   not yet settled.
//!
//! [`Fleet::step_all`] runs one decision of every bot at the current clock,
//! concurrently: the unit tests and W4-F's in-process day drive it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use frontier_agents::profile::{AgentSpec, BELLS_PER_DAY};
use frontier_agents::rng::Rng;

use crate::bot::{Bot, ClockSource, Shared};
use crate::journal::Journal;
use crate::ports::{DirectPort, HeraldPort, RelayPort};
use crate::report::Report;

pub struct Fleet<H, R, D> {
    pub shared: Arc<Shared<H, R, D>>,
    pub bots: Vec<Bot>,
}

/// When a bot next wants to run.
struct Schedule {
    rng: Rng,
    day: Option<u32>,
    sessions: Vec<i64>,
}

impl Schedule {
    fn new(seed: u64, index: u32) -> Schedule {
        Schedule {
            rng: Rng::fork(seed, 0x5E55_0000 ^ index as u64),
            day: None,
            sessions: vec![],
        }
    }

    /// The next session time ≥ `now` (planning day by day).
    fn next_session(&mut self, spec: &AgentSpec, genesis: i64, day_secs: i64, now: i64) -> i64 {
        let p = spec.profile();
        loop {
            if let Some(&t) = self.sessions.iter().find(|&&t| t >= now) {
                return t;
            }
            let d = match self.day {
                None => ((now - genesis).max(0) / day_secs) as u32,
                Some(d) => d + 1,
            };
            self.day = Some(d);
            self.sessions.clear();
            if p.sessions > 0 && self.rng.chance(p.day_p) {
                let gap = day_secs / p.sessions as i64;
                for k in 0..p.sessions as i64 {
                    let off = self.rng.below(gap.max(1) as u64) as i64;
                    self.sessions
                        .push(genesis + d as i64 * day_secs + k * gap + off);
                }
            }
            if d > 400 {
                return i64::MAX;
            }
        }
    }
}

impl<H, R, D> Fleet<H, R, D>
where
    H: HeraldPort + 'static,
    R: RelayPort + 'static,
    D: DirectPort + 'static,
{
    pub fn new(shared: Shared<H, R, D>, roster: &[AgentSpec]) -> Self {
        let seed = shared.cfg.seed;
        let bots = roster.iter().map(|s| Bot::new(*s, seed)).collect();
        let shared = Arc::new(shared);
        shared.report.lock().expect("report").bots = roster.len() as u64;
        Fleet { shared, bots }
    }

    /// Restores every bot's marches from the marchbook (after a restart).
    pub fn restore(&mut self, path: &std::path::Path) -> std::io::Result<usize> {
        let mut by_bot = Journal::load(path)?;
        let mut n = 0;
        for b in &mut self.bots {
            if let Some(ms) = by_bot.remove(&b.spec.index) {
                n += ms.len();
                b.mem.marches = ms;
            }
        }
        Ok(n)
    }

    /// One decision of every bot at the current clock, `concurrency` bots
    /// at a time. Returns the actions sent.
    pub async fn step_all(&mut self, session: bool, concurrency: usize) -> usize {
        let bots = std::mem::take(&mut self.bots);
        let mut out = Vec::with_capacity(bots.len());
        let mut sent = 0;
        let mut it = bots.into_iter();
        loop {
            let mut set = tokio::task::JoinSet::new();
            for mut b in it.by_ref().take(concurrency.max(1)) {
                let sh = self.shared.clone();
                set.spawn(async move {
                    let n = b.step(&sh, session).await;
                    (b, n)
                });
            }
            if set.is_empty() {
                break;
            }
            while let Some(r) = set.join_next().await {
                let (b, n) = r.expect("bot task");
                sent += n;
                out.push(b);
            }
        }
        out.sort_by_key(|b| b.spec.index);
        self.bots = out;
        sent
    }

    /// Runs every bot on its own schedule until game time `until` or
    /// `stop`. Needs a game clock (`ClockSource::Game`).
    pub async fn run(self, until: i64, stop: Arc<AtomicBool>) -> Report {
        let Fleet { shared, bots } = self;
        if matches!(shared.clock, ClockSource::Fixed(_)) {
            shared.error("run: a fixed clock (use step_all)");
            return shared.report.lock().expect("report").clone();
        }
        let mut tasks = tokio::task::JoinSet::new();
        for bot in bots {
            let sh = shared.clone();
            let stop = stop.clone();
            tasks.spawn(async move { run_bot(sh, bot, until, stop).await });
        }
        while tasks.join_next().await.is_some() {}
        let r = shared.report.lock().expect("report").clone();
        r
    }
}

async fn run_bot<H: HeraldPort, R: RelayPort, D: DirectPort>(
    sh: Arc<Shared<H, R, D>>,
    mut bot: Bot,
    until: i64,
    stop: Arc<AtomicBool>,
) {
    let mut sched = Schedule::new(sh.cfg.seed, bot.spec.index);
    let mut jitter = Rng::fork(sh.cfg.seed, 0x7177_0000 ^ bot.spec.index as u64);
    // Wait for the season file.
    let season = loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match sh.season().await {
            Ok(s) => break s,
            Err(_) => tokio::time::sleep(std::time::Duration::from_secs(1)).await,
        }
    };
    let bell_secs = season.bell_secs.max(1) as i64;
    let day_secs = bell_secs * BELLS_PER_DAY as i64;
    let genesis = season.genesis_ts;
    let join_at = genesis + bot.spec.join_bell as i64 * bell_secs;
    let mut next_session = i64::MIN;
    let mut last_duty_bell: Option<u32> = None;
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let Some(now) = sh.now() else {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let _ = sh.season().await;
            continue;
        };
        if now >= until {
            return;
        }
        if next_session == i64::MIN || next_session < now - day_secs {
            next_session = sched.next_session(&bot.spec, genesis, day_secs, now.max(join_at));
        }
        let bell = ((now - genesis).max(0) / bell_secs) as u32;
        let pre_join = now < join_at;
        // The first two game days after joining: join, ticket, cohort,
        // first holding (polled every bell).
        let onboarding = now < join_at + 2 * day_secs;
        let busy = bot.mem.marches.iter().any(|m| !m.settled);
        let session_due = !pre_join && now >= next_session;
        let duty_due = !pre_join && (busy || onboarding) && last_duty_bell != Some(bell);
        if session_due || duty_due {
            let _ = sh.season().await;
            bot.step(&sh, session_due).await;
            last_duty_bell = Some(bell);
            if session_due {
                next_session = sched.next_session(&bot.spec, genesis, day_secs, now + 1);
            }
        }
        // Next wake-up: the join bell; else the next bell (0–20 s in) while
        // onboarding or a march is open; else the next session.
        let next_bell = genesis + (bell as i64 + 1) * bell_secs + jitter.below(21) as i64;
        let wake = if pre_join {
            join_at + jitter.below(21) as i64
        } else if busy || onboarding {
            next_bell.min(next_session)
        } else {
            next_session
        };
        let wake = wake.min(until);
        let d = sh.clock.wall_until(wake);
        tokio::time::sleep(d.max(std::time::Duration::from_millis(50))).await;
    }
}
