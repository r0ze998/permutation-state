//! The stack configuration (`frontier-node/configs/*.toml`) and the
//! command-line flags that override it (M1 contract §10.3, §12, §13.4).
//!
//! Every port is `base_port + offset` (§10.3: localnet 10/11, drand-replay
//! 20, relay 30/33, herald 40, keepers 50/51, bots 70, viewers 75); the
//! nightly stack is the same table at base 41500. Paths are relative to
//! the repository root unless absolute.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::toml;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Mode A: `frontier-localnet` (LiteSVM) with a scaled Clock (I-25, I-54).
    Accel,
    /// Mode R: a real-time validator (needs Agave ≥ 4.0, O-M1-12 item 4).
    Realtime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Beacon {
    /// `drand-replay --test-key` and the `test-beacon` program (I-53).
    TestKey,
    /// `drand-replay --archive DIR` (real historical quicknet rounds) and
    /// the release program.
    Archive,
}

impl Beacon {
    pub fn name(self) -> &'static str {
        match self {
            Beacon::TestKey => "test-key",
            Beacon::Archive => "archive",
        }
    }
}

/// Port offsets from `base_port` (§10.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Offsets {
    pub localnet: u16,
    pub localnet_ws: u16,
    pub drand: u16,
    pub relay_operator: u16,
    pub relay_public: u16,
    pub herald: u16,
    pub keeper_a: u16,
    pub keeper_b: u16,
    pub bots: u16,
    pub viewers: u16,
}

impl Default for Offsets {
    fn default() -> Self {
        Offsets {
            localnet: 10,
            localnet_ws: 11,
            drand: 20,
            relay_operator: 30,
            relay_public: 33,
            herald: 40,
            keeper_a: 50,
            keeper_b: 51,
            bots: 70,
            viewers: 75,
        }
    }
}

/// The ports of one stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ports {
    pub localnet: u16,
    pub localnet_ws: u16,
    pub drand: u16,
    pub relay_operator: u16,
    pub relay_public: u16,
    pub herald: u16,
    pub keeper_a: u16,
    pub keeper_b: u16,
    pub bots: u16,
    pub viewers: u16,
}

impl Ports {
    pub fn all(&self) -> Vec<(&'static str, u16)> {
        vec![
            ("localnet", self.localnet),
            ("localnet-ws", self.localnet_ws),
            ("drand-replay", self.drand),
            ("relay-operator", self.relay_operator),
            ("relay-public", self.relay_public),
            ("herald", self.herald),
            ("keeper-a", self.keeper_a),
            ("keeper-b", self.keeper_b),
            ("bots", self.bots),
            ("viewers", self.viewers),
        ]
    }
    pub fn to_json(&self) -> Value {
        Value::Object(
            self.all()
                .into_iter()
                .map(|(k, v)| (k.to_string(), json!(v)))
                .collect(),
        )
    }
    pub fn rpc(&self) -> String {
        format!("http://127.0.0.1:{}", self.localnet)
    }
    pub fn drand_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.drand)
    }
    pub fn herald_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.herald)
    }
    pub fn relay_public_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.relay_public)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StackConfig {
    /// The config file this came from (report only).
    pub source: Option<PathBuf>,
    pub run_id: String,
    pub mode: Mode,
    pub beacon: Beacon,
    /// Game seconds per real second after the pre-season (20× exit, 100×
    /// nightly, 2× latency run).
    pub scale: f64,
    /// The pre-season scale: AnnounceSeason's 24-h lead in ≈ 43 s (I-54).
    pub preseason_scale: f64,
    /// The scale of the keeper-only drain after play (W6-A). `None`: the
    /// run's scale, but at least 20× (the exit run's scale), so a scale-2
    /// latency run does not spend 2.2 h of wall time on its 26-bell drain.
    /// Latencies are judged over play only.
    pub drain_scale: Option<f64>,
    /// Play length in game days (fractional allowed), unless `game_hours`.
    pub days: f64,
    pub game_hours: Option<f64>,
    /// Keeper-only bells after play (idle provinces are skipped in 24-bell
    /// batches, so 26 covers one batch and its close; integ-W4 item 9).
    pub drain_bells: u32,
    pub bots: usize,
    pub bot_seed: u64,
    /// `frontier-bots --personas` (`default`, `off`, or a count).
    pub personas: String,
    /// Extra `frontier-bots` flags, passed through as given.
    pub bots_args: Vec<String>,
    /// Pass `--day0-share 1 --eager-personas` to `frontier-bots` when it
    /// supports them (W6-A; the in-process day's pacing, W5-B F2). `None`:
    /// on for runs shorter than one game day (the 6-game-hour latency run
    /// otherwise saw 43 of 300 bots join and no march in 3.5 game hours),
    /// off otherwise.
    pub eager_bots: Option<bool>,
    pub season_id: u64,
    pub base_port: u16,
    pub offsets: Offsets,
    /// The game clock origin (`frontier-localnet --g0`); with `archive` it
    /// must be the archive's G0.
    pub g0: i64,
    pub so_test_key: PathBuf,
    pub so_release: PathBuf,
    /// Overrides the mode's default `.so`.
    pub so: Option<PathBuf>,
    /// The release build's recorded sha256 (`scripts/build-frontier.sh`'s
    /// `file_sha256`): the run refuses a `.so` with another hash, and V2
    /// checks against this independent pin instead of the deployed file's
    /// own hash (wave-5 review of W5-B). Required for an exit-grade run.
    pub expect_so_sha256: Option<String>,
    pub archive_dir: Option<PathBuf>,
    pub drand_delay_ms: i64,
    pub web_dir: PathBuf,
    pub relay_script: PathBuf,
    pub keeper_b: bool,
    pub reveal_pool: usize,
    pub delay_pool: usize,
    pub funders: usize,
    pub relay_pool: usize,
    pub chaos: bool,
    pub chaos_min_hours: f64,
    pub chaos_max_hours: f64,
    pub chaos_restart_max_secs: f64,
    pub chaos_seed: u64,
    /// Components chaos may kill (empty = every one, `chaos::TARGETS`).
    pub chaos_targets: Vec<String>,
    pub adversary: bool,
    /// Viewers of the in-run window (0 = none; `load` runs one later).
    pub viewers: usize,
    pub viewer_start_hours: f64,
    pub viewer_window_hours: f64,
    /// Pause the chain when the run is complete so verify, tamper and the
    /// report read a fixed state (`--keep-running` turns it off).
    pub pause_at_end: bool,
    /// Send EndSeason once `end_bell` is over (a run that reaches it).
    pub end_season: bool,
    /// Where run directories live (default `frontier-node/.local/frontier`).
    pub runs_dir: Option<PathBuf>,
    /// W6T-4: CreateSeason gets `end_bell` = the play bells (and
    /// `join_close_bell` scaled below it), so a 1- or 2-day run reaches
    /// `end_bell`, EndSeason and the drain (`--season-end-at-play-end`).
    pub season_end_at_play_end: bool,
    /// Keeper B's `backup_delay_slots` (W6T-2: settles wait this long and
    /// re-read before sending); written only to a keeper that knows the key.
    pub keeper_b_backup_delay_slots: u32,
    /// The viewers' think time (`frontier-viewers --think-ms`).
    pub viewer_think_ms: u64,
    /// The viewers follow the live bell from the herald's `/h/status`
    /// (`--follow-status`).
    pub viewer_follow_status: bool,
    /// The viewers' recovery budget (`--retry-budget-ms`); `None`: the
    /// chaos restart maximum at the run's scale plus 2 s.
    pub viewer_retry_budget_ms: Option<u64>,
    /// Forced chaos kills `(component, game hours after the viewer
    /// window starts)` (`--chaos-force herald:2`, repeatable; applied even
    /// without `--chaos`).
    pub chaos_force: Vec<(String, f64)>,
}

/// 2026-08-01T00:00:00Z, `frontier-localnet`'s default origin (a past date,
/// so every test-key round the season needs exists at once).
pub const G0_TEST_KEY: i64 = 1_785_542_400;
/// The G0 of the contiguous quicknet archive the main session fetches for
/// O-M1-12 item 3 (rounds 32,065,012..=32,311,012; 2026-09-10T00:00:00Z).
pub const G0_ARCHIVE: i64 = 1_788_998_400;
/// That archive's directory, relative to a worktree root under
/// `.claude/worktrees/` (outside the repository; never committed).
pub const DEFAULT_ARCHIVE: &str = "../../data/drand-archive-quicknet-g0-1788998400";

impl Default for StackConfig {
    fn default() -> Self {
        StackConfig {
            source: None,
            run_id: "w5-smoke".into(),
            mode: Mode::Accel,
            beacon: Beacon::TestKey,
            scale: 100.0,
            preseason_scale: 2_000.0,
            drain_scale: None,
            days: 1.0,
            game_hours: None,
            drain_bells: 26,
            bots: 100,
            bot_seed: 1,
            personas: "default".into(),
            bots_args: vec![],
            eager_bots: None,
            season_id: 7,
            base_port: 41_000,
            offsets: Offsets::default(),
            g0: G0_TEST_KEY,
            so_test_key: "permutation-frontier/target/deploy-test-beacon/permutation_frontier.so"
                .into(),
            so_release: "permutation-frontier/target/deploy/permutation_frontier.so".into(),
            so: None,
            expect_so_sha256: None,
            archive_dir: None,
            drand_delay_ms: 1_000,
            web_dir: "permutation-server/web".into(),
            relay_script: "permutation-gateway/src/frontier/server.mjs".into(),
            keeper_b: true,
            reveal_pool: 150,
            delay_pool: 32,
            funders: 4,
            relay_pool: 150,
            chaos: false,
            chaos_min_hours: 2.0,
            chaos_max_hours: 6.0,
            chaos_restart_max_secs: 60.0,
            chaos_seed: 1,
            chaos_targets: vec![],
            adversary: false,
            viewers: 0,
            viewer_start_hours: 1.0,
            viewer_window_hours: 24.0,
            pause_at_end: true,
            end_season: true,
            runs_dir: None,
            season_end_at_play_end: false,
            keeper_b_backup_delay_slots: 8,
            viewer_think_ms: 5_000,
            viewer_follow_status: true,
            viewer_retry_budget_ms: None,
            chaos_force: vec![],
        }
    }
}

fn pos(v: f64, k: &str) -> Result<f64, String> {
    if v > 0.0 && v.is_finite() {
        Ok(v)
    } else {
        Err(format!("`{k}` must be > 0"))
    }
}

impl StackConfig {
    /// Reads a config file over the defaults. Unknown keys are refused, so
    /// a typo never silently falls back to a default.
    pub fn from_file(path: &Path) -> Result<StackConfig, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut c =
            StackConfig::from_toml(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        c.source = Some(path.to_path_buf());
        Ok(c)
    }

    pub fn from_toml(text: &str) -> Result<StackConfig, String> {
        let mut m = toml::parse(text)?;
        let mut c = StackConfig::default();
        let mut take = |k: &str| m.remove(k);
        macro_rules! s {
            ($k:literal) => {
                take($k)
                    .map(|v| {
                        v.as_str()
                            .map(String::from)
                            .ok_or(format!("`{}` must be a string", $k))
                    })
                    .transpose()?
            };
        }
        macro_rules! f {
            ($k:literal) => {
                take($k)
                    .map(|v| v.as_f64().ok_or(format!("`{}` must be a number", $k)))
                    .transpose()?
            };
        }
        macro_rules! i {
            ($k:literal) => {
                take($k)
                    .map(|v| {
                        v.as_i64()
                            .filter(|x| *x >= 0)
                            .ok_or(format!("`{}` must be a non-negative integer", $k))
                    })
                    .transpose()?
            };
        }
        macro_rules! b {
            ($k:literal) => {
                take($k)
                    .map(|v| v.as_bool().ok_or(format!("`{}` must be true or false", $k)))
                    .transpose()?
            };
        }
        if let Some(v) = s!("run_id") {
            c.run_id = v;
        }
        if let Some(v) = s!("mode") {
            c.mode = parse_mode(&v)?;
        }
        if let Some(v) = s!("beacon") {
            c.beacon = parse_beacon(&v)?;
        }
        if let Some(v) = f!("scale") {
            c.scale = pos(v, "scale")?;
        }
        if let Some(v) = f!("preseason_scale") {
            c.preseason_scale = pos(v, "preseason_scale")?;
        }
        if let Some(v) = f!("drain_scale") {
            c.drain_scale = Some(pos(v, "drain_scale")?);
        }
        if let Some(v) = f!("days") {
            c.days = pos(v, "days")?;
        }
        if let Some(v) = f!("game_hours") {
            c.game_hours = Some(pos(v, "game_hours")?);
        }
        if let Some(v) = i!("drain_bells") {
            c.drain_bells = v as u32;
        }
        if let Some(v) = i!("bots") {
            c.bots = v as usize;
        }
        if let Some(v) = i!("bot_seed") {
            c.bot_seed = v as u64;
        }
        if let Some(v) = take("personas") {
            c.personas = match v {
                toml::Value::Str(s) => s,
                toml::Value::Int(n) if n >= 0 => n.to_string(),
                _ => return Err("`personas` must be \"default\", \"off\" or a count".into()),
            };
        }
        if let Some(v) = s!("bots_args") {
            c.bots_args = v.split_whitespace().map(String::from).collect();
        }
        if let Some(v) = b!("eager_bots") {
            c.eager_bots = Some(v);
        }
        if let Some(v) = i!("season_id") {
            c.season_id = v as u64;
        }
        if let Some(v) = i!("base_port") {
            c.base_port = u16::try_from(v).map_err(|_| "`base_port` is not a port")?;
        }
        if let Some(v) = i!("g0") {
            c.g0 = v;
        }
        macro_rules! off {
            ($k:literal, $f:ident) => {
                if let Some(v) = i!($k) {
                    c.offsets.$f = u16::try_from(v).map_err(|_| format!("`{}` too large", $k))?;
                }
            };
        }
        off!("ports.localnet", localnet);
        off!("ports.localnet_ws", localnet_ws);
        off!("ports.drand", drand);
        off!("ports.relay_operator", relay_operator);
        off!("ports.relay_public", relay_public);
        off!("ports.herald", herald);
        off!("ports.keeper_a", keeper_a);
        off!("ports.keeper_b", keeper_b);
        off!("ports.bots", bots);
        off!("ports.viewers", viewers);
        if let Some(v) = s!("paths.so_test_key") {
            c.so_test_key = v.into();
        }
        if let Some(v) = s!("paths.so_release") {
            c.so_release = v.into();
        }
        if let Some(v) = s!("paths.so") {
            c.so = Some(v.into());
        }
        if let Some(v) = s!("paths.archive") {
            c.archive_dir = (!v.is_empty()).then(|| v.into());
        }
        if let Some(v) = s!("paths.so_sha256") {
            c.expect_so_sha256 = (!v.is_empty()).then(|| v.to_ascii_lowercase());
        }
        if let Some(v) = s!("paths.web") {
            c.web_dir = v.into();
        }
        if let Some(v) = s!("paths.relay") {
            c.relay_script = v.into();
        }
        if let Some(v) = s!("paths.runs") {
            c.runs_dir = Some(v.into());
        }
        if let Some(v) = i!("drand_delay_ms") {
            c.drand_delay_ms = v;
        }
        if let Some(v) = b!("keeper_b") {
            c.keeper_b = v;
        }
        if let Some(v) = i!("pools.reveal") {
            c.reveal_pool = v as usize;
        }
        if let Some(v) = i!("pools.delay") {
            c.delay_pool = v as usize;
        }
        if let Some(v) = i!("pools.funders") {
            c.funders = v as usize;
        }
        if let Some(v) = i!("pools.relay") {
            c.relay_pool = v as usize;
        }
        if let Some(v) = b!("chaos.enabled") {
            c.chaos = v;
        }
        if let Some(v) = f!("chaos.min_hours") {
            c.chaos_min_hours = pos(v, "chaos.min_hours")?;
        }
        if let Some(v) = f!("chaos.max_hours") {
            c.chaos_max_hours = pos(v, "chaos.max_hours")?;
        }
        if let Some(v) = f!("chaos.restart_max_secs") {
            c.chaos_restart_max_secs = v.max(0.0);
        }
        if let Some(v) = i!("chaos.seed") {
            c.chaos_seed = v as u64;
        }
        if let Some(v) = s!("chaos.targets") {
            c.chaos_targets = v
                .split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect();
        }
        if let Some(v) = s!("chaos.force") {
            c.chaos_force = v
                .split(',')
                .map(str::trim)
                .filter(|x| !x.is_empty())
                .map(parse_force)
                .collect::<Result<_, _>>()?;
        }
        if let Some(v) = b!("adversary.enabled") {
            c.adversary = v;
        }
        if let Some(v) = i!("viewers.count") {
            c.viewers = v as usize;
        }
        if let Some(v) = f!("viewers.start_hours") {
            c.viewer_start_hours = v.max(0.0);
        }
        if let Some(v) = f!("viewers.window_hours") {
            c.viewer_window_hours = pos(v, "viewers.window_hours")?;
        }
        if let Some(v) = i!("viewers.think_ms") {
            c.viewer_think_ms = v as u64;
        }
        if let Some(v) = b!("viewers.follow_status") {
            c.viewer_follow_status = v;
        }
        if let Some(v) = i!("viewers.retry_budget_ms") {
            c.viewer_retry_budget_ms = Some(v as u64);
        }
        if let Some(v) = b!("season_end_at_play_end") {
            c.season_end_at_play_end = v;
        }
        if let Some(v) = i!("keeper_b_backup_delay_slots") {
            c.keeper_b_backup_delay_slots =
                u32::try_from(v).map_err(|_| "`keeper_b_backup_delay_slots` too large")?;
        }
        if let Some(v) = b!("pause_at_end") {
            c.pause_at_end = v;
        }
        if let Some(v) = b!("end_season") {
            c.end_season = v;
        }
        let _ = &mut take;
        if let Some(k) = m.keys().next() {
            return Err(format!("unknown key `{k}`"));
        }
        c.finalize();
        c.check()?;
        Ok(c)
    }

    /// Defaults that depend on the beacon: an archive run without an
    /// explicit origin runs at the archive's G0, and without a directory
    /// reads `FRONTIER_DRAND_ARCHIVE` or the main session's archive
    /// (`../../data/...` from a worktree under `.claude/worktrees/`).
    pub fn finalize(&mut self) {
        if self.beacon == Beacon::Archive {
            if self.g0 == G0_TEST_KEY {
                self.g0 = G0_ARCHIVE;
            }
            if self.archive_dir.is_none() {
                self.archive_dir = Some(
                    std::env::var("FRONTIER_DRAND_ARCHIVE")
                        .ok()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| DEFAULT_ARCHIVE.to_string())
                        .into(),
                );
            }
        }
    }

    /// Consistency rules every config and flag set must meet.
    pub fn check(&self) -> Result<(), String> {
        if self.run_id.is_empty()
            || !self
                .run_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(format!(
                "run id `{}`: letters, digits, `-` and `_` only",
                self.run_id
            ));
        }
        if !(self.scale > 0.0 && self.scale <= 100_000.0) {
            return Err("scale in (0, 100000]".into());
        }
        if !(self.preseason_scale > 0.0 && self.preseason_scale <= 100_000.0) {
            return Err("preseason_scale in (0, 100000]".into());
        }
        if !(self.drain_scale() > 0.0 && self.drain_scale() <= 100_000.0) {
            return Err("drain_scale in (0, 100000]".into());
        }
        if let Some(t) = self
            .chaos_targets
            .iter()
            .find(|t| !crate::chaos::TARGETS.contains(&t.as_str()))
        {
            return Err(format!(
                "chaos target `{t}`: one of {:?}",
                crate::chaos::TARGETS
            ));
        }
        if self.chaos_min_hours > self.chaos_max_hours {
            return Err("chaos.min_hours > chaos.max_hours".into());
        }
        if !(800..=2_000).contains(&self.drand_delay_ms) {
            return Err("drand_delay_ms in 800..=2000 (quicknet's publication latency)".into());
        }
        if self.season_end_at_play_end {
            let b = self.play_bells();
            if !(2..=4_032).contains(&b) {
                return Err(format!(
                    "--season-end-at-play-end: {b} play bells; end_bell must be in 2..=4032 (§5.7)"
                ));
            }
        }
        if self.viewer_think_ms == 0 {
            return Err("viewers.think_ms must be > 0".into());
        }
        if self.beacon == Beacon::Archive && self.archive_dir.is_none() {
            return Err(
                "--beacon archive needs an archive directory (`paths.archive` or --archive)".into(),
            );
        }
        Ok(())
    }

    pub fn ports(&self) -> Result<Ports, String> {
        let o = &self.offsets;
        let at = |off: u16| {
            self.base_port
                .checked_add(off)
                .ok_or(format!("base port {} + {off} overflows", self.base_port))
        };
        Ok(Ports {
            localnet: at(o.localnet)?,
            localnet_ws: at(o.localnet_ws)?,
            drand: at(o.drand)?,
            relay_operator: at(o.relay_operator)?,
            relay_public: at(o.relay_public)?,
            herald: at(o.herald)?,
            keeper_a: at(o.keeper_a)?,
            keeper_b: at(o.keeper_b)?,
            bots: at(o.bots)?,
            viewers: at(o.viewers)?,
        })
    }

    /// Play length in game seconds.
    /// Whether the fleet is asked for the in-process day's pacing.
    pub fn eager_bots(&self) -> bool {
        self.eager_bots.unwrap_or(self.play_secs() < 86_400)
    }

    /// The drain's scale: `drain_scale`, else the run's scale but at
    /// least 20×.
    pub fn drain_scale(&self) -> f64 {
        self.drain_scale.unwrap_or(self.scale.max(20.0))
    }

    /// Play length in bells (600-s bells, rounded up).
    pub fn play_bells(&self) -> u32 {
        ((self.play_secs().max(0) + 599) / 600) as u32
    }

    /// The `end_bell` CreateSeason gets: the play bells with
    /// `season_end_at_play_end`, else the preset's (`None`).
    pub fn season_end_bell(&self) -> Option<u32> {
        self.season_end_at_play_end.then(|| self.play_bells())
    }

    /// The viewers' recovery budget: the configured one, else the chaos
    /// restart maximum in wall time at the run's scale plus 2 s (a herald
    /// kill -9 is back within it; §13.4 A3).
    pub fn viewer_retry_budget_ms(&self) -> u64 {
        self.viewer_retry_budget_ms.unwrap_or_else(|| {
            (self.chaos_restart_max_secs / self.scale * 1_000.0).ceil() as u64 + 2_000
        })
    }

    pub fn play_secs(&self) -> i64 {
        match self.game_hours {
            Some(h) => (h * 3_600.0).round() as i64,
            None => (self.days * 86_400.0).round() as i64,
        }
    }

    /// The `.so` this mode deploys (relative to the repo root).
    pub fn so_path(&self) -> &Path {
        match (&self.so, self.beacon) {
            (Some(p), _) => p,
            (None, Beacon::TestKey) => &self.so_test_key,
            (None, Beacon::Archive) => &self.so_release,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "source": self.source.as_ref().map(|p| p.display().to_string()),
            "run_id": self.run_id,
            "mode": match self.mode { Mode::Accel => "accel", Mode::Realtime => "realtime" },
            "beacon": self.beacon.name(),
            "scale": self.scale,
            "preseason_scale": self.preseason_scale,
            "drain_scale": self.drain_scale(),
            "days": self.days,
            "game_hours": self.game_hours,
            "drain_bells": self.drain_bells,
            "bots": self.bots,
            "bot_seed": self.bot_seed,
            "personas": self.personas,
            "bots_args": self.bots_args,
            "eager_bots": self.eager_bots(),
            "season_id": self.season_id,
            "base_port": self.base_port,
            "g0": self.g0,
            "so": self.so_path().display().to_string(),
            "expect_so_sha256": self.expect_so_sha256,
            "archive": self.archive_dir.as_ref().map(|p| p.display().to_string()),
            "drand_delay_ms": self.drand_delay_ms,
            "keeper_b": self.keeper_b,
            "pools": {"reveal": self.reveal_pool, "delay": self.delay_pool, "funders": self.funders, "relay": self.relay_pool},
            "chaos": {"enabled": self.chaos, "min_hours": self.chaos_min_hours, "max_hours": self.chaos_max_hours,
                      "restart_max_secs": self.chaos_restart_max_secs, "seed": self.chaos_seed, "targets": self.chaos_targets},
            "adversary": self.adversary,
            "viewers": {"count": self.viewers, "start_hours": self.viewer_start_hours, "window_hours": self.viewer_window_hours},
            "pause_at_end": self.pause_at_end,
            "end_season": self.end_season,
            "season_end_at_play_end": self.season_end_at_play_end,
            "season_end_bell": self.season_end_bell(),
            "keeper_b_backup_delay_slots": self.keeper_b_backup_delay_slots,
            "viewer_flags": {"think_ms": self.viewer_think_ms, "follow_status": self.viewer_follow_status,
                             "retry_budget_ms": self.viewer_retry_budget_ms()},
            "chaos_force": self.chaos_force.iter().map(|(c, h)| json!({"component": c, "hours_after_viewer_start": h})).collect::<Vec<_>>(),
        })
    }
}

pub fn parse_mode(s: &str) -> Result<Mode, String> {
    match s {
        "accel" => Ok(Mode::Accel),
        "realtime" => Ok(Mode::Realtime),
        _ => Err(format!("mode `{s}`: accel or realtime")),
    }
}

/// `component:hours` of `--chaos-force` (hours after the viewer window
/// starts).
pub fn parse_force(s: &str) -> Result<(String, f64), String> {
    let (c, h) = s
        .split_once(':')
        .ok_or(format!("chaos force `{s}`: component:hours"))?;
    let c = c.trim();
    if !crate::chaos::TARGETS.contains(&c) {
        return Err(format!(
            "chaos force `{s}`: one of {:?}",
            crate::chaos::TARGETS
        ));
    }
    let h: f64 = h
        .trim()
        .parse()
        .map_err(|_| format!("chaos force `{s}`: hours is not a number"))?;
    if !(h >= 0.0 && h.is_finite()) {
        return Err(format!("chaos force `{s}`: hours ≥ 0"));
    }
    Ok((c.to_string(), h))
}

pub fn parse_beacon(s: &str) -> Result<Beacon, String> {
    match s {
        "test-key" => Ok(Beacon::TestKey),
        "archive" => Ok(Beacon::Archive),
        _ => Err(format!("beacon `{s}`: test-key or archive")),
    }
}

/// Applies the `up` flags (they win over the file).
pub fn apply_flags(c: &mut StackConfig, flags: &[(String, Option<String>)]) -> Result<(), String> {
    let need = |k: &str, v: &Option<String>| v.clone().ok_or(format!("--{k} needs a value"));
    let num = |k: &str, v: &Option<String>| -> Result<f64, String> {
        need(k, v)?
            .parse::<f64>()
            .map_err(|_| format!("--{k}: not a number"))
    };
    // Counts, seeds and ids: whole and not negative (wave-5 review: `as`
    // turned --bots -5 into 0).
    let count = |k: &str, v: &Option<String>| -> Result<u64, String> {
        let x = num(k, v)?;
        if x < 0.0 || x.fract() != 0.0 || x > u64::MAX as f64 {
            return Err(format!("--{k}: a whole number ≥ 0"));
        }
        Ok(x as u64)
    };
    for (k, v) in flags {
        match k.as_str() {
            "run-id" => c.run_id = need(k, v)?,
            "mode" => c.mode = parse_mode(&need(k, v)?)?,
            "beacon" => c.beacon = parse_beacon(&need(k, v)?)?,
            "scale" => c.scale = pos(num(k, v)?, "--scale")?,
            "preseason-scale" => c.preseason_scale = pos(num(k, v)?, "--preseason-scale")?,
            "drain-scale" => c.drain_scale = Some(pos(num(k, v)?, "--drain-scale")?),
            "days" => {
                c.days = pos(num(k, v)?, "--days")?;
                c.game_hours = None;
            }
            "game-hours" => c.game_hours = Some(pos(num(k, v)?, "--game-hours")?),
            "drain-bells" => {
                c.drain_bells =
                    u32::try_from(count(k, v)?).map_err(|_| "--drain-bells too large")?
            }
            "bots" => c.bots = count(k, v)? as usize,
            "seed" => c.bot_seed = count(k, v)?,
            "personas" => c.personas = need(k, v)?,
            "bots-args" => c.bots_args = need(k, v)?.split_whitespace().map(String::from).collect(),
            "season" => c.season_id = count(k, v)?,
            "base-port" => {
                let p = num(k, v)?;
                if !(0.0..=65_535.0).contains(&p) {
                    return Err("--base-port is not a port".into());
                }
                c.base_port = p as u16;
            }
            "g0" => c.g0 = count(k, v)? as i64,
            "so" => c.so = Some(need(k, v)?.into()),
            "expect-so-sha256" => {
                let h = need(k, v)?.to_ascii_lowercase();
                if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err("--expect-so-sha256: 64 hex digits".into());
                }
                c.expect_so_sha256 = Some(h);
            }
            "archive" => c.archive_dir = Some(need(k, v)?.into()),
            "chaos" => c.chaos = true,
            "no-chaos" => c.chaos = false,
            "chaos-seed" => c.chaos_seed = count(k, v)?,
            "chaos-targets" => {
                c.chaos_targets = need(k, v)?
                    .split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect();
            }
            "chaos-min-hours" => c.chaos_min_hours = pos(num(k, v)?, "--chaos-min-hours")?,
            "chaos-max-hours" => c.chaos_max_hours = pos(num(k, v)?, "--chaos-max-hours")?,
            "adversary" => c.adversary = true,
            "no-adversary" => c.adversary = false,
            "viewers" => c.viewers = num(k, v)? as usize,
            "viewer-window-hours" => {
                c.viewer_window_hours = pos(num(k, v)?, "--viewer-window-hours")?
            }
            "viewer-start-hours" => c.viewer_start_hours = num(k, v)?.max(0.0),
            "keep-running" => c.pause_at_end = false,
            "eager-bots" => c.eager_bots = Some(true),
            "no-eager-bots" => c.eager_bots = Some(false),
            "no-keeper-b" => c.keeper_b = false,
            "runs-dir" => c.runs_dir = Some(need(k, v)?.into()),
            "season-end-at-play-end" => c.season_end_at_play_end = true,
            "no-season-end-at-play-end" => c.season_end_at_play_end = false,
            "chaos-force" => c.chaos_force.push(parse_force(&need(k, v)?)?),
            "viewer-think-ms" => c.viewer_think_ms = count(k, v)?,
            "viewer-retry-budget-ms" => c.viewer_retry_budget_ms = Some(count(k, v)?),
            "no-viewer-follow-status" => c.viewer_follow_status = false,
            "keeper-b-backup-delay-slots" => {
                c.keeper_b_backup_delay_slots = u32::try_from(count(k, v)?)
                    .map_err(|_| "--keeper-b-backup-delay-slots too large")?
            }
            other => return Err(format!("unknown flag --{other}")),
        }
    }
    c.finalize();
    c.check()
}

/// Flags that take no value.
pub const SWITCHES: &[&str] = &[
    "chaos",
    "no-chaos",
    "adversary",
    "no-adversary",
    "keep-running",
    "no-keeper-b",
    "eager-bots",
    "no-eager-bots",
    "season-end-at-play-end",
    "no-season-end-at-play-end",
    "no-viewer-follow-status",
    "json",
    "strict",
    "force",
];

/// Splits `--k v` / `--switch` arguments.
pub fn split_flags(args: &[String]) -> Result<Vec<(String, Option<String>)>, String> {
    let mut out = vec![];
    let mut i = 0;
    while i < args.len() {
        let k = args[i]
            .strip_prefix("--")
            .ok_or(format!("unexpected argument `{}`", args[i]))?;
        if SWITCHES.contains(&k) {
            out.push((k.to_string(), None));
            i += 1;
        } else {
            let v = args
                .get(i + 1)
                .ok_or(format!("--{k} needs a value"))?
                .clone();
            out.push((k.to_string(), Some(v)));
            i += 2;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_gate_w5_smoke() {
        let c = StackConfig::default();
        let p = c.ports().unwrap();
        assert_eq!(
            (p.localnet, p.localnet_ws, p.drand),
            (41_010, 41_011, 41_020)
        );
        assert_eq!(
            (p.relay_operator, p.relay_public, p.herald),
            (41_030, 41_033, 41_040)
        );
        assert_eq!(
            (p.keeper_a, p.keeper_b, p.bots, p.viewers),
            (41_050, 41_051, 41_070, 41_075)
        );
        assert_eq!(c.play_secs(), 86_400);
        assert_eq!(c.beacon, Beacon::TestKey);
    }

    #[test]
    fn a_file_and_flags() {
        let mut c = StackConfig::from_toml(
            "run_id = \"n1\"\nscale = 20\nbase_port = 41500\n[chaos]\nenabled = true\n[ports]\nherald = 41\n",
        )
        .unwrap();
        assert_eq!(c.ports().unwrap().herald, 41_541);
        assert!(c.chaos);
        let f = split_flags(&[
            "--game-hours".into(),
            "6".into(),
            "--no-chaos".into(),
            "--base-port".into(),
            "41000".into(),
        ])
        .unwrap();
        apply_flags(&mut c, &f).unwrap();
        assert_eq!(c.play_secs(), 21_600);
        assert!(!c.chaos);
        assert_eq!(c.ports().unwrap().herald, 41_041);
    }

    /// W6-A: eager bots by default only below one game day.
    #[test]
    fn eager_bots_defaults() {
        let mut c = StackConfig::default();
        assert!(!c.eager_bots(), "a one-day run keeps the fleet's pacing");
        c.game_hours = Some(6.0);
        assert!(c.eager_bots());
        let f = split_flags(&["--no-eager-bots".into()]).unwrap();
        apply_flags(&mut c, &f).unwrap();
        assert!(!c.eager_bots());
        let t = StackConfig::from_toml("days = 7\neager_bots = true\n").unwrap();
        assert!(t.eager_bots());
    }

    /// W6-A: the drain of a slow run goes at 20x unless set; faster runs
    /// drain at their own scale.
    #[test]
    fn drain_scale_defaults() {
        let mut c = StackConfig::default();
        assert_eq!(c.drain_scale(), 100.0);
        c.scale = 2.0;
        assert_eq!(c.drain_scale(), 20.0);
        c.scale = 20.0;
        assert_eq!(c.drain_scale(), 20.0);
        let f = split_flags(&["--drain-scale".into(), "50".into()]).unwrap();
        apply_flags(&mut c, &f).unwrap();
        assert_eq!(c.drain_scale(), 50.0);
        let t = StackConfig::from_toml("scale = 2\ndrain_scale = 2\n").unwrap();
        assert_eq!(t.drain_scale(), 2.0);
        assert!(split_flags(&["--drain-scale".into(), "0".into()])
            .and_then(|f| apply_flags(&mut StackConfig::default(), &f))
            .is_err());
    }

    /// W6T-4: the season-end, viewer, keeper-B and forced-kill keys and flags.
    #[test]
    fn w6t4_keys_and_flags() {
        let c = StackConfig::from_toml(
            "season_end_at_play_end = true\nkeeper_b_backup_delay_slots = 12\n[viewers]\nthink_ms = 3000\nfollow_status = false\nretry_budget_ms = 7000\n[chaos]\nforce = \"herald:2, herald:7.5\"\n",
        )
        .unwrap();
        assert!(c.season_end_at_play_end);
        assert_eq!(c.season_end_bell(), Some(144));
        assert_eq!(c.keeper_b_backup_delay_slots, 12);
        assert_eq!((c.viewer_think_ms, c.viewer_follow_status), (3_000, false));
        assert_eq!(c.viewer_retry_budget_ms(), 7_000);
        assert_eq!(
            c.chaos_force,
            vec![("herald".to_string(), 2.0), ("herald".to_string(), 7.5)]
        );
        let mut d = StackConfig::default();
        let f = split_flags(&[
            "--season-end-at-play-end".into(),
            "--days".into(),
            "2".into(),
            "--chaos-force".into(),
            "herald:2".into(),
            "--chaos-force".into(),
            "herald:7".into(),
            "--viewer-think-ms".into(),
            "4000".into(),
            "--no-viewer-follow-status".into(),
        ])
        .unwrap();
        apply_flags(&mut d, &f).unwrap();
        assert_eq!(d.season_end_bell(), Some(288));
        assert_eq!(d.chaos_force.len(), 2);
        assert_eq!(d.viewer_think_ms, 4_000);
        assert!(!d.viewer_follow_status);
        assert_eq!(d.to_json()["season_end_bell"], 288);
        // Refusals: an unknown component, a negative hour, a season past
        // §5.7's 4,032 bells.
        for bad in [
            ["--chaos-force", "nobody:1"],
            ["--chaos-force", "herald:-1"],
            ["--days", "30"],
        ] {
            let mut e = StackConfig {
                season_end_at_play_end: true,
                ..Default::default()
            };
            let f = split_flags(&[bad[0].into(), bad[1].into()]).unwrap();
            assert!(apply_flags(&mut e, &f).is_err(), "{bad:?}");
        }
        assert!(StackConfig::from_toml("[viewers]\nthink_ms = 0\n").is_err());
    }

    #[test]
    fn refusals() {
        assert!(StackConfig::from_toml("typo = 1").is_err());
        let a = StackConfig::from_toml("beacon = \"archive\"").unwrap();
        assert_eq!(
            a.g0, G0_ARCHIVE,
            "an archive run defaults to the archive's G0"
        );
        assert!(a.archive_dir.is_some());
        assert!(StackConfig::from_toml("scale = 0").is_err());
        assert!(StackConfig::from_toml("run_id = \"a b\"").is_err());
        assert!(StackConfig::from_toml("drand_delay_ms = 100").is_err());
        let mut c = StackConfig::default();
        assert!(apply_flags(
            &mut c,
            &split_flags(&["--bogus".into(), "1".into()]).unwrap()
        )
        .is_err());
    }

    #[test]
    fn every_committed_config_parses_and_keeps_the_port_rule() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../configs");
        let mut n = 0;
        for e in std::fs::read_dir(&dir).expect("frontier-node/configs") {
            let p = e.unwrap().path();
            if p.extension().is_none_or(|x| x != "toml") {
                continue;
            }
            let c = StackConfig::from_file(&p).unwrap_or_else(|e| panic!("{e}"));
            let ports = c.ports().unwrap();
            let probs = crate::ports::problems(&ports, false);
            assert!(probs.is_empty(), "{}: {probs:?}", p.display());
            if c.beacon == Beacon::Archive {
                assert_eq!(c.g0, G0_ARCHIVE, "{}: the archive's G0", p.display());
            }
            n += 1;
        }
        assert!(n >= 5, "{n} configs");
        // The Gate W5 smoke flags equal the defaults and w5-smoke.toml.
        let w5 = StackConfig::from_file(&dir.join("w5-smoke.toml")).unwrap();
        let d = StackConfig::default();
        assert_eq!(
            (w5.scale, w5.days, w5.bots, w5.base_port, w5.beacon),
            (d.scale, d.days, d.bots, d.base_port, d.beacon)
        );
        let nightly = StackConfig::from_file(&dir.join("nightly.toml")).unwrap();
        assert_eq!(
            nightly.ports().unwrap().localnet,
            41_510,
            "nightly = base + 500"
        );
    }
}
