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
//! concurrently (the unit tests). [`Fleet::step_due`] runs the bots that
//! are due at a clock the caller moves, with the same pacing as
//! [`Fleet::run`] (W4-F's in-process day, on virtual time).
//! `Config::eager_personas` gives persona bots a session every bell,
//! 90–150 game seconds in (off by default).

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
    /// Pacing per bot index ([`Fleet::step_due`]).
    pace: std::collections::BTreeMap<u32, Pace>,
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
        Fleet {
            shared,
            bots,
            pace: Default::default(),
        }
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
        let Fleet { shared, bots, .. } = self;
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

/// Game times of the season a pace is planned in.
#[derive(Clone, Copy, Debug)]
struct Times {
    genesis: i64,
    bell_secs: i64,
    day_secs: i64,
}

impl Times {
    fn of(season: &frontier_agents::obs::SeasonView) -> Times {
        let bell_secs = season.bell_secs.max(1) as i64;
        Times {
            genesis: season.genesis_ts,
            bell_secs,
            day_secs: bell_secs * BELLS_PER_DAY as i64,
        }
    }
}

/// One bot's pacing (the rules in the module note), shared by
/// [`Fleet::run`] (wall clock) and [`Fleet::step_due`] (a clock the caller
/// moves, e.g. an in-process chain on virtual time).
/// Game seconds between a settle racer's polls while a march of its is
/// between its arrival and the keepers' SettleTransit (integ-W6t review).
pub const RACER_POLL_SECS: i64 = 8;

/// Whether a settle racer is in its race (integ-W6t review): a march past
/// its arrival bell (up to `RACE_BELLS` on), not yet re-departed or settled.
/// Its window is the few slots between the destination's resolve of the
/// arrival bell (`seed_margin` after the close, ≈ 60–100 game seconds into
/// the next bell) and the keepers' SettleTransit; duties 0–20 s into each
/// bell never land there at 20× (the rehearsal `integ-w6t-rv-3d`: 5 racers,
/// no re-depart in 3 game days, criterion 5 n.a.), so the racer polls every
/// [`RACER_POLL_SECS`] until it has re-departed.
pub fn settle_racer_polls(bot: &Bot, bell: u32) -> bool {
    bot.spec.persona == Some(frontier_agents::Persona::SettleRacer)
        && bot.mem.marches.iter().any(|m| {
            !m.settled
                && !m.redeparted
                && bell >= m.arrive_bell
                && bell <= m.arrive_bell + frontier_agents::policy::RACE_BELLS
        })
}

struct Pace {
    sched: Schedule,
    jitter: Rng,
    next_session: i64,
    last_duty_bell: Option<u32>,
    /// The next game time this bot wants to run.
    wake: i64,
}

impl Pace {
    fn new(seed: u64, index: u32) -> Pace {
        Pace {
            sched: Schedule::new(seed, index),
            jitter: Rng::fork(seed, 0x7177_0000 ^ index as u64),
            next_session: i64::MIN,
            last_duty_bell: None,
            wake: i64::MIN,
        }
    }

    fn join_at(bot: &Bot, t: &Times) -> i64 {
        t.genesis + bot.spec.join_bell as i64 * t.bell_secs
    }

    /// Whether `bot` runs at `now`: `Some(session)` (economy and war
    /// allowed) or `None`.
    fn plan(&mut self, bot: &Bot, t: &Times, now: i64, eager: bool) -> Option<bool> {
        let join_at = Self::join_at(bot, t);
        if self.next_session == i64::MIN || self.next_session < now - t.day_secs {
            self.next_session =
                self.sched
                    .next_session(&bot.spec, t.genesis, t.day_secs, now.max(join_at));
        }
        let bell = ((now - t.genesis).max(0) / t.bell_secs) as u32;
        let pre_join = now < join_at;
        // The first two game days after joining: join, ticket, cohort,
        // first holding (polled every bell).
        let onboarding = now < join_at + 2 * t.day_secs;
        let busy = bot.mem.marches.iter().any(|m| !m.settled);
        let eager = eager && bot.spec.persona.is_some();
        let new_bell = self.last_duty_bell != Some(bell);
        let session_due = !pre_join && (now >= self.next_session || (eager && new_bell));
        let duty_due = !pre_join
            && ((busy || onboarding || eager) && new_bell || settle_racer_polls(bot, bell));
        (session_due || duty_due).then_some(session_due)
    }

    /// After a run (`ran = Some(session)`) or a skip at `now`: the next
    /// wake-up — the join bell; else the next bell (0–20 s in) while
    /// onboarding or a march is open; else the next session.
    fn after(&mut self, bot: &Bot, t: &Times, now: i64, ran: Option<bool>, eager: bool) -> i64 {
        let bell = ((now - t.genesis).max(0) / t.bell_secs) as u32;
        // A session that held a resident action back and nudged its
        // province runs again 45–75 s later, when the keeper has usually
        // skipped it through b − 2 (integ-W4 review, W4-F), at most
        // `NUDGE_RETRIES` times.
        if ran == Some(true) && bot.nudged && bot.nudge_retries < crate::bot::NUDGE_RETRIES {
            self.wake = now + 45 + self.jitter.below(31) as i64;
            return self.wake;
        }
        if let Some(session) = ran {
            self.last_duty_bell = Some(bell);
            if session {
                self.next_session =
                    self.sched
                        .next_session(&bot.spec, t.genesis, t.day_secs, now + 1);
            }
        }
        let join_at = Self::join_at(bot, t);
        let pre_join = now < join_at;
        let onboarding = now < join_at + 2 * t.day_secs;
        let eager = eager && bot.spec.persona.is_some();
        let busy = bot.mem.marches.iter().any(|m| !m.settled) || eager;
        // Duties 0–20 s into the bell (§9.1); an eager persona's session
        // 90–150 s in, once its province has usually resolved bell − 2
        // (resident actions need it: at 0–20 s nearly every Muster was
        // `NotResident`, W4-F's in-process day).
        let offset = if eager {
            90 + self.jitter.below(61) as i64
        } else {
            self.jitter.below(21) as i64
        };
        let next_bell = t.genesis + (bell as i64 + 1) * t.bell_secs + offset;
        self.wake = if pre_join {
            join_at + self.jitter.below(21) as i64
        } else if busy || onboarding {
            next_bell.min(self.next_session)
        } else {
            self.next_session
        };
        if settle_racer_polls(bot, bell) {
            self.wake = self.wake.min(now + RACER_POLL_SECS);
        }
        self.wake
    }
}

impl<H, R, D> Fleet<H, R, D>
where
    H: HeraldPort + 'static,
    R: RelayPort + 'static,
    D: DirectPort + 'static,
{
    /// Runs every bot that is due at the shared clock's `now` (its session
    /// or its per-bell duty, by the same pacing as [`Fleet::run`]),
    /// `concurrency` bots at a time; the caller moves the clock
    /// (`ClockSource::set`) between calls. Returns (bots run, actions sent).
    pub async fn step_due(&mut self, concurrency: usize) -> (usize, usize) {
        let Ok(season) = self.shared.season().await else {
            return (0, 0);
        };
        let Some(now) = self.shared.now() else {
            return (0, 0);
        };
        let t = Times::of(&season);
        let eager = self.shared.cfg.eager_personas;
        let mut due = vec![];
        let mut rest = vec![];
        for b in std::mem::take(&mut self.bots) {
            let p = self
                .pace
                .entry(b.spec.index)
                .or_insert_with(|| Pace::new(self.shared.cfg.seed, b.spec.index));
            if now < p.wake {
                rest.push(b);
                continue;
            }
            match p.plan(&b, &t, now, eager) {
                Some(session) => due.push((b, session)),
                None => {
                    p.after(&b, &t, now, None, eager);
                    rest.push(b);
                }
            }
        }
        let ran = due.len();
        let mut sent = 0;
        let mut it = due.into_iter();
        loop {
            let mut set = tokio::task::JoinSet::new();
            for (mut b, session) in it.by_ref().take(concurrency.max(1)) {
                let sh = self.shared.clone();
                set.spawn(async move {
                    let n = b.step(&sh, session).await;
                    (b, session, n)
                });
            }
            if set.is_empty() {
                break;
            }
            while let Some(r) = set.join_next().await {
                let (mut b, session, n) = r.expect("bot task");
                sent += n;
                if let Some(p) = self.pace.get_mut(&b.spec.index) {
                    p.after(&b, &t, now, Some(session), eager);
                }
                b.nudge_retries = if session && b.nudged {
                    b.nudge_retries.saturating_add(1)
                } else {
                    0
                };
                rest.push(b);
            }
        }
        rest.sort_by_key(|b| b.spec.index);
        self.bots = rest;
        (ran, sent)
    }
}

async fn run_bot<H: HeraldPort, R: RelayPort, D: DirectPort>(
    sh: Arc<Shared<H, R, D>>,
    mut bot: Bot,
    until: i64,
    stop: Arc<AtomicBool>,
) {
    let mut pace = Pace::new(sh.cfg.seed, bot.spec.index);
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
    let t = Times::of(&season);
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
            // W6-C: end on the herald's own time, never on an extrapolation
            // alone (a bad rate estimate once ran the fleet's clock ≈ 40
            // bells ahead and every bot stopped at bell ≈ 102 of 144).
            let _ = sh.season().await;
            if sh.clock.observed().is_none_or(|u| u >= until) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1_000)).await;
            continue;
        }
        let eager = sh.cfg.eager_personas;
        let ran = pace.plan(&bot, &t, now, eager);
        if let Some(session) = ran {
            let _ = sh.season().await;
            bot.step(&sh, session).await;
        }
        let wake = pace.after(&bot, &t, now, ran, eager).min(until);
        bot.nudge_retries = if ran == Some(true) && bot.nudged {
            bot.nudge_retries.saturating_add(1)
        } else {
            0
        };
        // W6-C: a long sleep is computed from the rate estimate of one
        // moment; nap at most `MAX_NAP` and plan again, so a bot whose wake
        // is far off (a late join bell, the next session) follows the rate
        // as it is refined. Before, a late joiner slept on an early,
        // low estimate past the end of play (the W6-C stack runs with
        // eager personas: every bot action stopped at bell 102, then 124,
        // of 144).
        let d = sh.clock.wall_until(wake);
        tokio::time::sleep(d.clamp(std::time::Duration::from_millis(50), MAX_NAP)).await;
    }
}

/// The longest a bot sleeps before it re-plans on the game clock.
pub const MAX_NAP: std::time::Duration = std::time::Duration::from_secs(5);
