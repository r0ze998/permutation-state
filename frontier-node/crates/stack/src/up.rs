//! `frontier-stack up`: the start order, then the supervisor.
//!
//! Start order (offchain design §11.4, contract §8.7):
//! 1. `frontier-localnet` with the `.so` deployed at `--max-len =
//!    round_up(1.25 × .so, 4 KiB)` and the operator key as its upgrade
//!    authority, at the run's scale;
//! 2. `drand-replay` (`--test-key`, or `--archive DIR` of real quicknet
//!    rounds), gated by the chain's Clock;
//! 3. the operator: AnnounceSeason (at the run's scale) → the 24-h lead at scale 2,000 → the
//!    target scale → CreateSeason, InitBeaconLogs, InitShards × 6
//!    (ConsumeGenesisSeed is keeper A's);
//! 4. the relay pool and both keepers' delay pools and funders funded
//!    (test SOL airdrops), keeper A (every role) and keeper B (the public
//!    profile: reveal, settle-departure, settle, claims), then the relay;
//! 5. the herald;
//! 6. the bots;
//! 7. the in-run viewer window, if asked.
//!
//! The supervisor then runs chaos (kill -9 and restart), the adversary
//! schedule (`frontier_hold`), EndSeason when due, per-bell keeper and
//! herald samples, and restarts any component that dies unexpectedly (a
//! "crash" event). When play and the drain are over it pauses the chain
//! (so verify, tamper and the report read one state), records the phase
//! `complete` and exits 0, **leaving the services up**: `down` stops them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fclient::addr::Addresses;
use fclient::{Address, Keypair, Signer};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::adversary;
use crate::chain::{self, Chain};
use crate::chaos;
use crate::config::{Beacon, Mode, Ports, StackConfig};
use crate::procs::{Proc, Spec};
use crate::run::{self, RunDir};
use crate::setup;

/// Exit code for an item blocked on an owner decision (O-M1-12; Mode R,
/// Agave >= 4.0 not approved).
pub const EXIT_PENDING_OWNER: i32 = 3;
/// Exit code while the approved round archive is still being fetched
/// (O-M1-12 item 3 is approved: this is PENDING, not PENDING-OWNER).
pub const EXIT_PENDING_FETCH: i32 = 4;

pub const SOL: u64 = 1_000_000_000;
/// Each reveal payer at start: above the default floor (≈ 0.215 SOL from
/// R99, §8.2) and near the band's middle, so payer care neither tops up
/// nor sweeps at once.
pub const REVEAL_PAYER_LAMPORTS: u64 = 350_000_000;

/// The program id of a run (any 32 bytes deploy on localnet).
pub fn program_id(run_id: &str) -> Address {
    let h = Sha256::digest([b"PSF-STACK-PROGRAM-v1".as_slice(), run_id.as_bytes()].concat());
    Address::new_from_array(h.into())
}

/// Why a run cannot start, before anything is spawned.
#[derive(Debug)]
pub enum Refusal {
    /// Needs an owner decision (exit 3, `PENDING-OWNER`).
    PendingOwner(String),
    /// Waits for the approved archive fetch (exit 4, `PENDING`).
    PendingFetch(String),
    Bad(String),
}

/// The `.so` checks: present; a test-key run needs the test-beacon marker,
/// an archive run needs a build without it (I-53: the release binary).
pub fn check_so(bytes: &[u8], beacon: Beacon) -> Result<(), String> {
    let has = |m: &[u8]| bytes.windows(m.len()).any(|w| w == m);
    let tb = has(b"PSF_TEST_BEACON_BUILD");
    match beacon {
        Beacon::TestKey if !tb => {
            Err("--beacon test-key needs a test-beacon build (marker PSF_TEST_BEACON_BUILD absent): scripts/build-frontier.sh --features test-beacon".into())
        }
        Beacon::Archive if tb => {
            Err("--beacon archive runs the release .so; this is a test-beacon build".into())
        }
        _ if has(b"PSF_ORACLE_BUILD") || has(b"PSF_TRACE_BUILD") => {
            Err("an oracle or trace build is never deployed by the stack".into())
        }
        _ => Ok(()),
    }
}

/// The round range of a packed archive: `(first, last)` rounds and the
/// time of the first (`genesis_time + (first − 1) × period`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveRange {
    pub first: u64,
    pub last: u64,
    pub first_time: i64,
    pub last_time: i64,
}

/// The archive directory must hold a finished archive (`manifest.json`);
/// a directory the prefetch is still filling is PENDING (the fetch is
/// approved, O-M1-12 item 3; exit 4), not PENDING-OWNER. A finished archive
/// is read (wave-5 review of W5-B): its first round's time must be the
/// run's G0 (the clock origin the archive was fetched for), and its last
/// round must reach `until` (the pre-season lead, play, drain and an hour),
/// else the run is refused (exit 2) before anything starts.
pub fn check_archive(dir: &Path, g0: i64, until: i64) -> Result<ArchiveRange, Refusal> {
    if dir.join("manifest.json").is_file() {
        let range = archive_range(dir).map_err(Refusal::Bad)?;
        if range.first_time != g0 {
            return Err(Refusal::Bad(format!(
                "{}: the archive starts at round {} (time {}), not at the run's G0 {g0}",
                dir.display(),
                range.first,
                range.first_time
            )));
        }
        if range.last_time < until {
            return Err(Refusal::Bad(format!(
                "{}: the archive ends at round {} (time {}), before the run needs ({until})",
                dir.display(),
                range.last,
                range.last_time
            )));
        }
        return Ok(range);
    }
    if dir.is_dir() {
        let partial = std::fs::read_dir(dir)
            .map(|r| {
                r.filter_map(|e| e.ok())
                    .any(|e| e.file_name().to_string_lossy().starts_with("partial-"))
            })
            .unwrap_or(false);
        let why = if partial {
            "the round archive is still being fetched (a partial-*.bin, no manifest.json yet)"
        } else {
            "no manifest.json"
        };
        return Err(Refusal::PendingFetch(format!(
            "real rounds (O-M1-12 item 3, approved): {}: {why}",
            dir.display()
        )));
    }
    Err(Refusal::PendingFetch(format!(
        "real rounds (O-M1-12 item 3, approved): {} does not exist",
        dir.display()
    )))
}

/// The rounds a packed archive holds, from its `manifest.json` and
/// `info.json` (drand-replay's layout; the segments are checked by
/// drand-replay itself when it loads them).
pub fn archive_range(dir: &Path) -> Result<ArchiveRange, String> {
    let read = |n: &str| -> Result<Value, String> {
        let t = std::fs::read_to_string(dir.join(n)).map_err(|e| format!("{n}: {e}"))?;
        serde_json::from_str(&t).map_err(|e| format!("{n}: {e}"))
    };
    let m = read("manifest.json")?;
    let info = read("info.json")?;
    let genesis = info["genesis_time"]
        .as_i64()
        .ok_or("info.json: genesis_time")?;
    let period = info["period"].as_i64().ok_or("info.json: period")?;
    let mut first = u64::MAX;
    let mut last = 0u64;
    for s in m["segments"].as_array().ok_or("manifest.json: segments")? {
        let f = s["first"].as_u64().ok_or("manifest.json: first")?;
        let c = s["count"].as_u64().ok_or("manifest.json: count")?;
        if c == 0 {
            continue;
        }
        first = first.min(f);
        last = last.max(f + c - 1);
    }
    if first == u64::MAX {
        return Err("manifest.json: no rounds".into());
    }
    let t = |r: u64| genesis + (r as i64 - 1) * period;
    Ok(ArchiveRange {
        first,
        last,
        first_time: t(first),
        last_time: t(last),
    })
}

pub struct Keys {
    pub authority: Keypair,
    pub keeper_a_seed: [u8; 32],
    pub keeper_b_seed: [u8; 32],
    pub relay_seed: [u8; 32],
    pub keeper_a_beneficiary: Keypair,
    pub keeper_b_beneficiary: Keypair,
    pub keeper_a_token: String,
    pub keeper_b_token: String,
    pub operator_token: String,
}

impl Keys {
    pub fn load_or_create(r: &RunDir) -> Result<Keys, String> {
        let kp = |p: &str| run::secret(&r.path(p)).map(Keypair::new_from_array);
        Ok(Keys {
            authority: kp("keys/operator.seed")?,
            keeper_a_seed: run::secret(&r.path("keeper-a/keeper.seed"))?,
            keeper_b_seed: run::secret(&r.path("keeper-b/keeper.seed"))?,
            relay_seed: run::secret(&r.path("relay/relay-master.seed"))?,
            keeper_a_beneficiary: kp("keeper-a/beneficiary.key")?,
            keeper_b_beneficiary: kp("keeper-b/beneficiary.key")?,
            keeper_a_token: run::token(&r.path("keeper-a/keeper.token"))?,
            keeper_b_token: run::token(&r.path("keeper-b/keeper.token"))?,
            operator_token: run::token(&r.path("relay/operator.token"))?,
        })
    }
}

/// Keeper roles: A runs every duty (the operator's keeper), B the public
/// profile (§10.3 "reveal, prove, settle"; proving is settlement, I-44).
/// The pacing flags of `eager_bots` that the given `frontier-bots` lists in
/// its usage (`--help`); empty when it has neither (an unknown flag would
/// crash-loop the fleet).
pub fn eager_bot_flags(bots: &Path) -> Vec<(&'static str, Option<&'static str>)> {
    let usage = std::process::Command::new(bots)
        .arg("--help")
        .output()
        .map(|o| {
            let mut t = String::from_utf8_lossy(&o.stdout).into_owned();
            t.push_str(&String::from_utf8_lossy(&o.stderr));
            t
        })
        .unwrap_or_default();
    eager_flags_in(&usage)
}

/// The pacing flags a usage text offers.
pub fn eager_flags_in(usage: &str) -> Vec<(&'static str, Option<&'static str>)> {
    [("--day0-share", Some("1")), ("--eager-personas", None)]
        .into_iter()
        .filter(|(f, _)| {
            usage
                .split(|c: char| c.is_whitespace() || c == '[' || c == ']')
                .any(|w| w == *f)
        })
        .collect()
}

pub fn keeper_roles(which: char) -> Vec<&'static str> {
    match which {
        'a' => keeper_core_roles(),
        _ => vec!["reveal", "settle-departure", "settle", "claims"],
    }
}

/// `keeper::config::ROLES` (copied: the stack does not link the keeper;
/// a test pins the copy to the keeper's `ROLES` list by parsing its source).
pub fn keeper_core_roles() -> Vec<&'static str> {
    vec![
        "beacon",
        "reveal",
        "settle-departure",
        "gather",
        "resolve",
        "skip",
        "settle",
        "tickets",
        "explore",
        "archive",
        "close",
        "fold",
        "rings",
        "dormancy",
        "claims",
        "sweep",
    ]
}

/// `keeper.toml` for keeper `which` ('a' or 'b').
#[allow(clippy::too_many_arguments)]
pub fn keeper_toml(
    which: char,
    cfg: &StackConfig,
    ports: &Ports,
    program: &Address,
    beneficiary: &Address,
    dir: &Path,
) -> String {
    let roles: Vec<String> = keeper_roles(which)
        .iter()
        .map(|r| format!("\"{r}\""))
        .collect();
    let api = if which == 'a' {
        ports.keeper_a
    } else {
        ports.keeper_b
    };
    format!(
        "# written by frontier-stack for run {run}\n\
         program = \"{program}\"\n\
         season = {season}\n\
         rpc = [\"{rpc}\"]\n\
         drand = [\"{drand}\"]\n\
         roles = [{roles}]\n\
         reveal_pool = {rp}\n\
         delay_pool = {dp}\n\
         funders = {f}\n\
         beneficiary = \"{beneficiary}\"\n\
         api = \"127.0.0.1:{api}\"\n\
         token_file = \"{d}/keeper.token\"\n\
         master_seed_file = \"{d}/keeper.seed\"\n\
         journal = \"{d}/keeper.journal.sqlite\"\n\
         beneficiary_key_file = \"{d}/beneficiary.key\"\n\
         race_jitter_slots = {jitter}\n",
        run = cfg.run_id,
        season = cfg.season_id,
        rpc = ports.rpc(),
        drand = ports.drand_url(),
        roles = roles.join(", "),
        rp = cfg.reveal_pool,
        dp = cfg.delay_pool,
        f = cfg.funders,
        d = dir.display(),
        jitter = if which == 'a' { 0 } else { 2 },
    )
}

pub struct Stack {
    pub cfg: StackConfig,
    pub repo: PathBuf,
    pub run: RunDir,
    pub bin: PathBuf,
    pub ports: Ports,
    pub program: Address,
    pub addrs: Addresses,
    pub keys: Keys,
    pub chain: Chain,
    pub procs: BTreeMap<String, Proc>,
    pub state: Value,
    pub so: PathBuf,
    pub so_sha256: String,
    pub so_len: usize,
}

fn log_line(msg: &str) {
    eprintln!("frontier-stack: {msg}");
}

impl Stack {
    /// Checks everything that can be checked before a process starts.
    pub fn prepare(cfg: StackConfig) -> Result<Stack, Refusal> {
        let bad = Refusal::Bad;
        if cfg.mode == Mode::Realtime {
            return Err(Refusal::PendingOwner(
                "Mode R needs Agave >= 4.0 (O-M1-12 item 4, not approved); Mode A is the exit run (I-25)".into(),
            ));
        }
        let repo = run::repo_root().map_err(bad)?;
        let runs = cfg
            .runs_dir
            .as_ref()
            .map(|p| run::resolve(&repo, p))
            .unwrap_or_else(|| RunDir::default_runs(&repo));
        let rd = RunDir::new(&runs, &cfg.run_id);
        let ports = cfg.ports().map_err(bad)?;
        let probs = crate::ports::problems(&ports, true);
        if !probs.is_empty() {
            return Err(Refusal::Bad(format!("ports: {}", probs.join("; "))));
        }
        if cfg.beacon == Beacon::Archive {
            let dir = run::resolve(&repo, cfg.archive_dir.as_deref().unwrap_or(Path::new("")));
            let until =
                cfg.g0 + setup::LEAD_SECS + cfg.play_secs() + cfg.drain_bells as i64 * 600 + 3_600;
            check_archive(&dir, cfg.g0, until)?;
        }
        let so = run::resolve(&repo, cfg.so_path());
        let bytes = std::fs::read(&so).map_err(|e| {
            Refusal::Bad(format!(
                "{}: {e} (build it with scripts/build-frontier.sh)",
                so.display()
            ))
        })?;
        check_so(&bytes, cfg.beacon).map_err(bad)?;
        let bin = run::bin_dir().map_err(bad)?;
        for b in [
            "frontier-localnet",
            "drand-replay",
            "frontier-keeper",
            "frontier-herald",
            "frontier-bots",
            "frontier-viewers",
        ] {
            if !bin.join(b).is_file() {
                return Err(Refusal::Bad(format!(
                    "{}: missing (cargo build --release --workspace in frontier-node)",
                    bin.join(b).display()
                )));
            }
        }
        let relay = run::resolve(&repo, &cfg.relay_script);
        if !relay.is_file() {
            return Err(Refusal::Bad(format!("{}: missing", relay.display())));
        }
        let gw = relay
            .ancestors()
            .find(|p| p.join("package.json").is_file())
            .map(Path::to_path_buf)
            .unwrap_or_default();
        if !gw.join("node_modules/@solana/web3.js").is_dir() {
            return Err(Refusal::Bad(format!(
                "{}: node_modules missing (cd {} && npm ci --ignore-scripts)",
                gw.display(),
                gw.display()
            )));
        }
        // A previous run with the same id: refuse while any of its
        // components is alive, else start from a clean directory.
        if rd.exists() {
            if let Ok(st) = rd.load_state() {
                let alive: Vec<String> = live_components(&st);
                if !alive.is_empty() {
                    return Err(Refusal::Bad(format!(
                        "run {} is still up ({}): frontier-stack down --run-id {} first",
                        cfg.run_id,
                        alive.join(", "),
                        cfg.run_id
                    )));
                }
            }
            std::fs::remove_dir_all(&rd.root)
                .map_err(|e| Refusal::Bad(format!("{}: {e}", rd.root.display())))?;
        }
        rd.create().map_err(bad)?;
        let keys = Keys::load_or_create(&rd).map_err(bad)?;
        let program = program_id(&cfg.run_id);
        let addrs = Addresses::new(program, cfg.season_id);
        let chain = Chain::new(&ports.rpc(), program);
        let so_sha256 = hex::encode(Sha256::digest(&bytes));
        if let Some(want) = &cfg.expect_so_sha256 {
            if *want != so_sha256 {
                return Err(Refusal::Bad(format!(
                    "{}: sha256 {so_sha256}, not the pinned release build {want} (--expect-so-sha256)",
                    so.display()
                )));
            }
        }
        Ok(Stack {
            repo,
            run: rd,
            bin,
            ports,
            program,
            addrs,
            keys,
            chain,
            procs: BTreeMap::new(),
            state: json!({}),
            so,
            so_sha256,
            so_len: bytes.len(),
            cfg,
        })
    }

    fn g0(&self) -> i64 {
        self.cfg.g0
    }

    pub fn specs(&self) -> Vec<Spec> {
        let r = &self.run;
        let p = &self.ports;
        let bin = |n: &str| self.bin.join(n);
        let s = |x: &str| x.to_string();
        let mut out = vec![];
        let max_len = fclient::fees::deploy_max_len(self.so_len as u64);
        out.push(Spec {
            name: s("localnet"),
            program: bin("frontier-localnet"),
            args: vec![
                s("--port"),
                p.localnet.to_string(),
                s("--ws-port"),
                p.localnet_ws.to_string(),
                s("--scale"),
                self.cfg.scale.to_string(),
                s("--g0"),
                self.g0().to_string(),
                s("--data-dir"),
                r.path("localnet").display().to_string(),
                s("--program"),
                format!(
                    "{}={}:{max_len}@{}",
                    self.program,
                    self.so.display(),
                    self.keys.authority.pubkey()
                ),
            ],
            env: vec![],
            cwd: r.root.clone(),
            log: r.log("localnet"),
            finishes: false,
        });
        let mut dargs = vec![
            s("serve"),
            s("--port"),
            p.drand.to_string(),
            s("--clock"),
            format!("chain:{}", p.rpc()),
            s("--delay-ms"),
            self.cfg.drand_delay_ms.to_string(),
        ];
        match self.cfg.beacon {
            Beacon::TestKey => dargs.push(s("--test-key")),
            Beacon::Archive => {
                dargs.push(s("--archive"));
                dargs.push(
                    run::resolve(
                        &self.repo,
                        self.cfg.archive_dir.as_deref().unwrap_or(Path::new("")),
                    )
                    .display()
                    .to_string(),
                );
            }
        }
        out.push(Spec {
            name: s("drand-replay"),
            program: bin("drand-replay"),
            args: dargs,
            env: vec![],
            cwd: r.root.clone(),
            log: r.log("drand-replay"),
            finishes: false,
        });
        for w in ['a', 'b'] {
            if w == 'b' && !self.cfg.keeper_b {
                continue;
            }
            out.push(Spec {
                name: format!("keeper-{w}"),
                program: bin("frontier-keeper"),
                args: vec![
                    s("--config"),
                    r.path(&format!("keeper-{w}/keeper.toml"))
                        .display()
                        .to_string(),
                ],
                env: vec![],
                cwd: r.path(&format!("keeper-{w}")),
                log: r.log(&format!("keeper-{w}")),
                finishes: false,
            });
        }
        let relay = run::resolve(&self.repo, &self.cfg.relay_script);
        out.push(Spec {
            name: s("relay"),
            program: which("node").unwrap_or_else(|| PathBuf::from("node")),
            args: vec![
                relay.display().to_string(),
                s("--program"),
                self.program.to_string(),
                s("--season"),
                self.cfg.season_id.to_string(),
                s("--rpc"),
                p.rpc(),
                s("--port"),
                p.relay_operator.to_string(),
                s("--public-port"),
                p.relay_public.to_string(),
                s("--herald"),
                p.herald_url(),
                s("--keeper"),
                format!("http://127.0.0.1:{}", p.keeper_a),
                s("--keeper-token-file"),
                r.path("keeper-a/keeper.token").display().to_string(),
                s("--pool-size"),
                self.cfg.relay_pool.to_string(),
                s("--master-seed-file"),
                r.path("relay/relay-master.seed").display().to_string(),
                s("--invite-secret-file"),
                r.path("relay/invite.secret").display().to_string(),
                s("--state-file"),
                r.path("relay/relay-state.json").display().to_string(),
            ],
            env: vec![(
                s("FRONTIER_OPERATOR_TOKEN"),
                self.keys.operator_token.clone(),
            )],
            cwd: relay
                .ancestors()
                .find(|x| x.join("package.json").is_file())
                .map(Path::to_path_buf)
                .unwrap_or_else(|| self.repo.clone()),
            log: r.log("relay"),
            finishes: false,
        });
        let mut hargs = vec![
            s("--data"),
            r.path("herald").display().to_string(),
            s("--program"),
            self.program.to_string(),
            s("--season"),
            self.cfg.season_id.to_string(),
            s("--rpc"),
            p.rpc(),
            s("--listen"),
            format!("127.0.0.1:{}", p.herald),
            s("--relay"),
            format!("127.0.0.1:{}", p.relay_public),
            s("--web"),
            run::resolve(&self.repo, &self.cfg.web_dir)
                .display()
                .to_string(),
        ];
        if self.cfg.beacon == Beacon::TestKey {
            hargs.push(s("--test-key"));
        }
        out.push(Spec {
            name: s("herald"),
            program: bin("frontier-herald"),
            args: hargs,
            env: vec![],
            cwd: r.path("herald"),
            log: r.log("herald"),
            finishes: false,
        });
        out
    }

    /// The bots' spec (needs the season's genesis to size the play window).
    pub fn bots_spec(&self, now: i64, play_end: i64) -> Spec {
        let s = |x: &str| x.to_string();
        let p = &self.ports;
        let hours = ((play_end - now).max(600) as f64) / 3_600.0;
        let mut args = vec![
            s("--herald"),
            p.herald_url(),
            s("--relay"),
            p.relay_public_url(),
            s("--rpc"),
            p.rpc(),
            s("--seed"),
            self.cfg.bot_seed.to_string(),
            s("--bots"),
            self.cfg.bots.to_string(),
            s("--days"),
            (self.cfg.days.ceil() as u32).max(1).to_string(),
            s("--game-hours"),
            format!("{hours:.4}"),
            s("--scale"),
            self.cfg.scale.to_string(),
            s("--personas"),
            self.cfg.personas.clone(),
            s("--journal"),
            self.run.path("bots").display().to_string(),
            s("--report"),
            self.run.path("bots/report.json").display().to_string(),
            // The control / metrics listener on the port the config
            // reserves for it (wave-5 review: it was checked, never given).
            s("--control"),
            format!("127.0.0.1:{}", p.bots),
        ];
        // The in-process day's pacing (W6-A, `eager_bots`), only when this
        // `frontier-bots` has the flags and `bots_args` does not set them.
        if self.cfg.eager_bots() {
            for (flag, value) in eager_bot_flags(&self.bin.join("frontier-bots")) {
                if !self.cfg.bots_args.iter().any(|a| a == flag) {
                    args.push(s(flag));
                    if let Some(v) = value {
                        args.push(s(v));
                    }
                }
            }
        }
        // Pass-through flags (`bots_args`).
        args.extend(self.cfg.bots_args.iter().cloned());
        Spec {
            name: s("bots"),
            program: self.bin.join("frontier-bots"),
            args,
            env: vec![],
            cwd: self.run.path("bots"),
            log: self.run.log("bots"),
            finishes: true,
        }
    }

    pub fn save(&mut self, phase: &str) {
        let comps: serde_json::Map<String, Value> = self
            .procs
            .iter()
            .map(|(k, p)| {
                (
                    k.clone(),
                    json!({"pid": p.pid, "starts": p.starts, "done": p.done, "last_exit": p.last_exit,
                           "spec": p.spec.to_json()}),
                )
            })
            .collect();
        self.state["phase"] = json!(phase);
        self.state["components"] = Value::Object(comps);
        self.state["updated_wall_ms"] = json!(run::wall_ms());
        let _ = self.run.save_state(&self.state);
    }

    fn start(&mut self, spec: Spec) -> Result<u32, String> {
        let name = spec.name.clone();
        let mut p = Proc::new(spec);
        let pid = p.start()?;
        self.run
            .event(None, "start", json!({"component": name, "pid": pid}));
        self.procs.insert(name, p);
        self.save("setup");
        Ok(pid)
    }

    pub fn stop_all(&mut self) {
        // Reverse start order: bots, herald, relay, keepers, drand, chain.
        for n in [
            "viewers",
            "bots",
            "herald",
            "relay",
            "keeper-b",
            "keeper-a",
            "drand-replay",
            "localnet",
        ] {
            if let Some(p) = self.procs.get_mut(n) {
                p.stop(Duration::from_secs(10));
            }
        }
    }

    async fn fund(&self) -> Result<Value, String> {
        let mut n = 0u64;
        let mut total = 0u64;
        let mut drop = |k: Address, l: u64| {
            n += 1;
            total += l;
            (k, l)
        };
        let mut list = vec![];
        for (seed, which) in [
            (self.keys.keeper_a_seed, 'a'),
            (self.keys.keeper_b_seed, 'b'),
        ] {
            if which == 'b' && !self.cfg.keeper_b {
                continue;
            }
            // Offchain design §11.4 step 4: the keepers' 150 reveal payers
            // are funded at start (the keeper's payer care runs only at rest,
            // every 150 slots: 10 bells at 100x, so a pool left to it is
            // empty for the first hours of a fast run).
            for i in 0..self.cfg.reveal_pool as u32 {
                list.push(drop(
                    fclient::payers::derive(&seed, fclient::payers::REVEAL_POOL, i).pubkey(),
                    REVEAL_PAYER_LAMPORTS,
                ));
            }
            for i in 0..self.cfg.delay_pool as u32 {
                list.push(drop(
                    fclient::payers::derive(&seed, fclient::payers::DELAY_POOL, i).pubkey(),
                    2 * SOL,
                ));
            }
            for i in 0..self.cfg.funders as u32 {
                list.push(drop(
                    fclient::payers::derive(&seed, fclient::payers::FUNDER_POOL, i).pubkey(),
                    500 * SOL,
                ));
            }
        }
        for i in 0..self.cfg.relay_pool as u32 {
            list.push(drop(
                fclient::payers::derive(&self.keys.relay_seed, fclient::payers::RELAY_POOL, i)
                    .pubkey(),
                10 * SOL,
            ));
        }
        for (k, l) in list {
            self.chain.airdrop(&k, l).await?;
        }
        Ok(json!({"airdrops": n, "lamports": total}))
    }
}

/// Components of a saved state whose pid is still that component.
pub fn live_components(st: &Value) -> Vec<String> {
    let mut out = vec![];
    if let Some(m) = st["components"].as_object() {
        for (k, c) in m {
            if let (Some(pid), Some(prog)) = (c["pid"].as_u64(), c["spec"]["program"].as_str()) {
                if crate::procs::is_component(pid as u32, Path::new(prog)) {
                    out.push(k.clone());
                }
            }
        }
    }
    out
}

pub fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").ok()?;
    path.split(':')
        .map(|d| Path::new(d).join(cmd))
        .find(|p| p.is_file())
}

/// `up`: returns the process exit code.
pub async fn up(cfg: StackConfig) -> i32 {
    let mut st = match Stack::prepare(cfg) {
        Ok(s) => s,
        Err(Refusal::PendingOwner(m)) => {
            println!("PENDING-OWNER: {m}");
            return EXIT_PENDING_OWNER;
        }
        Err(Refusal::PendingFetch(m)) => {
            println!("PENDING (fetch in progress): {m}");
            return EXIT_PENDING_FETCH;
        }
        Err(Refusal::Bad(m)) => {
            log_line(&m);
            return 2;
        }
    };
    st.state = json!({
        "format": "frontier-stack-run-v1",
        "run_id": st.cfg.run_id,
        "config": st.cfg.to_json(),
        "ports": st.ports.to_json(),
        "program": st.program.to_string(),
        "season_id": st.cfg.season_id,
        "authority": st.keys.authority.pubkey().to_string(),
        "so": {"path": st.so.display().to_string(), "sha256": st.so_sha256, "len": st.so_len,
               "expected_sha256": st.cfg.expect_so_sha256, "pin": if st.cfg.expect_so_sha256.is_some() { "release build record" } else { "the deployed file's own hash (not exit-grade)" },
               "max_len": fclient::fees::deploy_max_len(st.so_len as u64)},
        "beacon": st.cfg.beacon.name(),
        "g0": st.cfg.g0,
        "supervisor_pid": std::process::id(),
        "wall_start_ms": run::wall_ms(),
        "keepers": {
            "a": {"beneficiary": st.keys.keeper_a_beneficiary.pubkey().to_string(), "api": st.ports.keeper_a},
            "b": if st.cfg.keeper_b { json!({"beneficiary": st.keys.keeper_b_beneficiary.pubkey().to_string(), "api": st.ports.keeper_b}) } else { Value::Null },
        },
    });
    st.save("setup");
    st.run.event(None, "phase", json!("setup"));
    let code = tokio::select! {
        r = run_all(&mut st) => match r {
            Ok(()) => 0,
            Err(e) => {
                log_line(&format!("run failed: {e}"));
                st.run.event(None, "failed", json!(e));
                st.state["error"] = json!(e);
                st.stop_all();
                st.save("failed");
                1
            }
        },
        _ = tokio::signal::ctrl_c() => {
            log_line("interrupted: stopping every component");
            st.stop_all();
            st.save("interrupted");
            130
        }
    };
    code
}

async fn run_all(st: &mut Stack) -> Result<(), String> {
    // Keeper configs and seeds.
    for w in ['a', 'b'] {
        if w == 'b' && !st.cfg.keeper_b {
            continue;
        }
        let ben = if w == 'a' {
            st.keys.keeper_a_beneficiary.pubkey()
        } else {
            st.keys.keeper_b_beneficiary.pubkey()
        };
        let dir = st.run.path(&format!("keeper-{w}"));
        let t = keeper_toml(w, &st.cfg, &st.ports, &st.program, &ben, &dir);
        std::fs::write(dir.join("keeper.toml"), t).map_err(|e| e.to_string())?;
    }
    let specs: BTreeMap<String, Spec> = st
        .specs()
        .into_iter()
        .map(|s| (s.name.clone(), s))
        .collect();
    let t0 = Instant::now();
    // 1. The chain.
    st.start(specs["localnet"].clone())?;
    st.chain.wait_healthy(Duration::from_secs(90)).await?;
    st.chain
        .airdrop(&st.keys.authority.pubkey(), 1_000 * SOL)
        .await?;
    log_line(&format!(
        "localnet up on {} ({:.1} s)",
        st.ports.rpc(),
        t0.elapsed().as_secs_f64()
    ));
    // 2. Beacons.
    st.start(specs["drand-replay"].clone())?;
    chain::wait_http(
        &format!("{}/info", st.ports.drand_url()),
        Duration::from_secs(60),
    )
    .await?;
    // 3. The operator's season.
    let run = st.run.clone();
    let logf = move |k: &str, v: Value| run.event(None, k, v);
    let season = setup::run(
        &st.chain,
        &st.addrs,
        &st.keys.authority,
        st.cfg.beacon,
        st.cfg.preseason_scale,
        st.cfg.scale,
        &logf,
    )
    .await?;
    st.state["season"] = season.clone();
    let genesis_ts = season["genesis_ts"].as_i64().ok_or("genesis_ts")?;
    let end_bell = season["end_bell"].as_u64().unwrap_or(1_008) as i64;
    let bell_secs = season["bell_secs"].as_i64().unwrap_or(600);
    let play_end = genesis_ts + st.cfg.play_secs().min(end_bell * bell_secs);
    let end = play_end + st.cfg.drain_bells as i64 * bell_secs;
    st.state["play"] = json!({"genesis_ts": genesis_ts, "play_end": play_end, "end": end,
        "play_bells": (play_end - genesis_ts) / bell_secs, "drain_bells": st.cfg.drain_bells});
    log_line(&format!(
        "season {} created (genesis {genesis_ts}); play until {play_end}, drain until {end}",
        st.cfg.season_id
    ));
    // 4. Payers, keepers, relay.
    let funded = st.fund().await?;
    st.run.event(None, "funded", funded);
    st.start(specs["keeper-a"].clone())?;
    chain::wait_http(
        &format!("http://127.0.0.1:{}/v1/status", st.ports.keeper_a),
        Duration::from_secs(60),
    )
    .await?;
    if st.cfg.keeper_b {
        st.start(specs["keeper-b"].clone())?;
        chain::wait_http(
            &format!("http://127.0.0.1:{}/v1/status", st.ports.keeper_b),
            Duration::from_secs(60),
        )
        .await?;
    }
    st.start(specs["relay"].clone())?;
    chain::wait_tcp(st.ports.relay_public, Duration::from_secs(60)).await?;
    // 5. The herald, and its season file (the bots read the program id there).
    st.start(specs["herald"].clone())?;
    chain::wait_tcp(st.ports.herald, Duration::from_secs(60)).await?;
    wait_season_file(&st.ports.herald_url(), Duration::from_secs(300)).await?;
    // 6. Bots.
    let now = st.chain.status().await?.now;
    let bots = st.bots_spec(now, play_end);
    st.start(bots)?;
    st.state["wall_setup_secs"] = json!(t0.elapsed().as_secs_f64());
    st.save("running");
    st.run.event(Some(now), "phase", json!("running"));
    log_line(&format!(
        "running: {} bots, scale {}, {} ({:.0} s of setup)",
        st.cfg.bots,
        st.cfg.scale,
        st.cfg.beacon.name(),
        t0.elapsed().as_secs_f64()
    ));
    supervise(st, genesis_ts, play_end, end, bell_secs).await
}

async fn wait_season_file(herald: &str, limit: Duration) -> Result<(), String> {
    let t0 = Instant::now();
    loop {
        if let Ok(Ok(r)) = tokio::time::timeout(
            Duration::from_secs(3),
            fclient::http::get(&format!("{herald}/h/season")),
        )
        .await
        {
            if r.status == 200 {
                return Ok(());
            }
        }
        if t0.elapsed() > limit {
            return Err(format!("{herald}/h/season not served after {limit:?}"));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

struct HoldState {
    plan: adversary::Planned,
    done: bool,
}

async fn supervise(
    st: &mut Stack,
    genesis_ts: i64,
    play_end: i64,
    end: i64,
    bell_secs: i64,
) -> Result<(), String> {
    let scale = st.cfg.scale;
    let wall_per_game = 1.0 / scale;
    // The chain's scale now: the run's through play, the drain's after it
    // (W6-A: a scale-2 run drains at 20x).
    let mut cur_scale = scale;
    let mut drain_scale_failures = 0u32;
    // Chaos plan over the play window.
    let targets: Vec<String> = chaos::TARGETS
        .iter()
        .filter(|t| st.procs.contains_key(**t) || **t == "bots")
        .filter(|t| {
            st.cfg.chaos_targets.is_empty() || st.cfg.chaos_targets.iter().any(|x| x == **t)
        })
        .map(|s| s.to_string())
        .collect();
    let mut kills: Vec<chaos::Kill> = if st.cfg.chaos {
        chaos::plan(
            st.cfg.chaos_seed,
            genesis_ts,
            play_end,
            st.cfg.chaos_min_hours,
            st.cfg.chaos_max_hours,
            st.cfg.chaos_restart_max_secs,
            &targets,
        )
    } else {
        vec![]
    };
    st.state["chaos_plan"] = json!(kills.iter().map(|k| k.to_json()).collect::<Vec<_>>());
    let mut holds: Vec<HoldState> = if st.cfg.adversary {
        adversary::plan(genesis_ts, play_end)
            .into_iter()
            .map(|plan| HoldState { plan, done: false })
            .collect()
    } else {
        vec![]
    };
    st.state["adversary_plan"] = json!(holds
        .iter()
        .map(|h| json!({"kind": h.plan.kind, "at": h.plan.at, "priority_milli": h.plan.priority_milli, "game_secs": h.plan.game_secs}))
        .collect::<Vec<_>>());
    st.save("running");
    let keeper_reveal: Vec<Address> = (0..20)
        .map(|i| {
            fclient::payers::derive(&st.keys.keeper_a_seed, fclient::payers::REVEAL_POOL, i)
                .pubkey()
        })
        .collect();
    let relay_payers: Vec<Address> = (0..20)
        .map(|i| {
            fclient::payers::derive(&st.keys.relay_seed, fclient::payers::RELAY_POOL, i).pubkey()
        })
        .collect();
    let mut last = chain::Status::default();
    let mut last_sample_bell: i64 = -1;
    let mut last_probe: Option<u32> = None;
    let mut last_due = 0usize;
    let mut end_season_sent = false;
    let mut end_season_task: Option<tokio::task::JoinHandle<Value>> = None;
    let mut viewers: Option<std::process::Child> = None;
    let mut viewers_started = false;
    // Wave-5 review: the in-run viewer window is judged — the fold lag is
    // sampled once a second while it runs, as `load` does.
    let mut inrun_lags: Vec<f64> = vec![];
    let mut inrun_judged = false;
    let mut last_lag = Instant::now();
    let mut crashes = 0u32;
    let mut chain_down_since: Option<Instant> = None;
    loop {
        tokio::time::sleep(Duration::from_millis(400)).await;
        // Exits (reap; unexpected ones are crashes and restart).
        let names: Vec<String> = st.procs.keys().cloned().collect();
        for n in &names {
            let p = st.procs.get_mut(n).expect("proc");
            if let Some(d) = p.poll_exit() {
                if p.done {
                    st.run.event(
                        Some(last.now),
                        "finished",
                        json!({"component": n, "exit": d}),
                    );
                } else if p.restart_at.is_none() {
                    crashes += 1;
                    st.run
                        .event(Some(last.now), "crash", json!({"component": n, "exit": d}));
                    log_line(&format!("{n} exited unexpectedly ({d}); restarting"));
                    p.restart_at = Some(Instant::now() + Duration::from_secs(2));
                }
            }
        }
        // Restarts due.
        for n in &names {
            let due = {
                let p = &st.procs[n];
                !p.running() && !p.done && p.restart_at.is_some_and(|t| Instant::now() >= t)
            };
            if due {
                if n == "bots" {
                    // A restarted fleet keeps the run's play end (its
                    // --game-hours counts from the herald's time at start),
                    // and is not restarted once play is over.
                    if last.now >= play_end {
                        let p = st.procs.get_mut(n).expect("proc");
                        p.done = true;
                        p.restart_at = None;
                        st.run.event(
                            Some(last.now),
                            "not-restarted",
                            json!({"component": n, "why": "play is over"}),
                        );
                        continue;
                    }
                    let spec = st.bots_spec(last.now, play_end);
                    st.procs.get_mut(n).expect("proc").spec = spec;
                }
                let p = st.procs.get_mut(n).expect("proc");
                match p.start() {
                    Ok(pid) => {
                        st.run.event(
                            Some(last.now),
                            "restart",
                            json!({"component": n, "pid": pid, "starts": p.starts}),
                        );
                    }
                    Err(e) => {
                        st.run.event(
                            Some(last.now),
                            "restart-failed",
                            json!({"component": n, "error": e}),
                        );
                        p.restart_at = Some(Instant::now() + Duration::from_secs(5));
                    }
                }
                if n == "localnet" {
                    // The recovered header's scale may be the pre-season's.
                    if st.chain.wait_healthy(Duration::from_secs(60)).await.is_ok() {
                        let _ = st.chain.set_scale(cur_scale).await;
                    }
                }
                st.save("running");
            }
        }
        // The clock.
        match st.chain.status().await {
            Ok(s) => {
                last = s;
                chain_down_since = None;
            }
            Err(_) => {
                let since = *chain_down_since.get_or_insert_with(Instant::now);
                let lnet_restarting = st.procs.get("localnet").is_some_and(|p| !p.running());
                if since.elapsed() > Duration::from_secs(120) && !lnet_restarting {
                    return Err("the chain has not answered for 120 s".into());
                }
                continue;
            }
        }
        let now = last.now;
        // Chaos.
        while let Some(k) = kills.first().cloned() {
            if now < k.at {
                break;
            }
            kills.remove(0);
            if let Some(p) = st.procs.get_mut(&k.component) {
                if p.running() {
                    let _ = p.kill9();
                    let wall = (k.restart_after * wall_per_game).max(0.0);
                    p.restart_at = Some(Instant::now() + Duration::from_secs_f64(wall));
                    st.run.event(Some(now), "chaos-kill", json!({"component": k.component, "restart_after_game_secs": k.restart_after, "restart_after_wall_secs": wall}));
                    log_line(&format!(
                        "chaos: kill -9 {} (restart in {:.2} s)",
                        k.component, wall
                    ));
                } else {
                    st.run.event(
                        Some(now),
                        "chaos-skip",
                        json!({"component": k.component, "why": "not running"}),
                    );
                }
            }
        }
        // Adversary holds.
        let probe_bell = ((now - genesis_ts).max(0) / bell_secs) as u32;
        let due_holds = holds.iter().filter(|h| !h.done && now >= h.plan.at).count();
        // Wave-5 review: a hold waiting for its situation probes the
        // Provinces once a bell (a new hold coming due probes at once), not
        // every 400 ms against the chain whose latencies are measured.
        if due_holds > 0 && (last_probe != Some(probe_bell) || due_holds > last_due) {
            last_probe = Some(probe_bell);
            last_due = due_holds;
            let bell_now = probe_bell;
            let mut provinces: Option<Vec<fclient::decode::Province>> = None;
            let mut pending: Option<adversary::Pending> = None;
            let mut transit_slots = 4u8;
            let mut season_now: Option<fclient::decode::Season> = None;
            // W6-C: the ring-opening and claim-grace holds read their
            // situation only while they wait.
            let want_ring = holds
                .iter()
                .any(|h| !h.done && now >= h.plan.at && h.plan.kind == "frontier-fund");
            let want_claims = holds
                .iter()
                .any(|h| !h.done && now >= h.plan.at && h.plan.kind == "defence-pool");
            for h in holds.iter_mut().filter(|h| !h.done && now >= h.plan.at) {
                if provinces.is_none() {
                    provinces = Some(
                        st.chain
                            .provinces(&st.program, st.cfg.season_id)
                            .await
                            .unwrap_or_default(),
                    );
                    if let Ok(Some(s)) = st.chain.season(&st.addrs).await {
                        transit_slots = s.transit_slots.max(1);
                        season_now = Some(s);
                    }
                }
                let ps = provinces.as_deref().unwrap_or(&[]);
                if pending.is_none() {
                    // Provinces with arrivals due today (their ArrivalDay).
                    let day = bell_now / 144;
                    let keys: Vec<Address> = ps
                        .iter()
                        .map(|p| st.addrs.arrival_day(p.p as i32, p.q as i32, day))
                        .collect();
                    let there = st.chain.accounts(&keys).await.unwrap_or_default();
                    let (ring_opening, open_claims) = match &season_now {
                        Some(season) if want_ring || want_claims => {
                            adversary::probe_land_and_claims(
                                &st.chain,
                                &st.addrs,
                                &st.program,
                                season,
                                ps,
                                now,
                                want_ring,
                                want_claims,
                            )
                            .await
                        }
                        _ => (None, vec![]),
                    };
                    pending = Some(adversary::Pending {
                        arrivals_today: ps
                            .iter()
                            .zip(there.iter())
                            .filter(|(_, a)| a.as_ref().is_some_and(|a| a.owner == st.program))
                            .map(|(p, _)| (p.p, p.q))
                            .collect(),
                        require: true,
                        now,
                        ring_opening,
                        open_claims,
                    });
                }
                match adversary::keys_for_pending(
                    h.plan.kind,
                    &st.addrs,
                    ps,
                    pending.as_ref().expect("pending"),
                    bell_now,
                    transit_slots,
                    &keeper_reveal,
                    &relay_payers,
                ) {
                    Some((keys, detail)) => {
                        // The ticket hold lasts through its cohort's bells.
                        let game_secs = match detail["until_bell"].as_i64() {
                            Some(u) => (genesis_ts + u * bell_secs - now).max(bell_secs),
                            None => h.plan.game_secs,
                        };
                        let slots = adversary::slots_for(game_secs, scale);
                        let r = st.chain.hold(&keys, h.plan.priority_milli, slots).await;
                        st.run.event(Some(now), "hold", json!({
                            "kind": h.plan.kind, "priority_milli": h.plan.priority_milli, "slots": slots,
                            "game_secs": game_secs, "bell": bell_now, "keys": keys.len(),
                            "above_keeper_cap": adversary::above_cap(h.plan.kind, h.plan.priority_milli),
                            "detail": detail, "result": r.as_ref().ok(), "error": r.as_ref().err(),
                        }));
                        h.done = true;
                    }
                    None if now > h.plan.deadline => {
                        st.run.event(
                            Some(now),
                            "hold-skipped",
                            json!({"kind": h.plan.kind, "why": "nothing to hold before the deadline", "deadline": h.plan.deadline}),
                        );
                        h.done = true;
                    }
                    None => {}
                }
            }
        }
        // Per-bell samples: keepers' status, the herald's fold lag.
        let bell = (now - genesis_ts).div_euclid(bell_secs);
        if bell != last_sample_bell {
            last_sample_bell = bell;
            sample(st, last, bell).await;
        }
        // The in-run viewer window.
        if st.cfg.viewers > 0
            && !viewers_started
            && now >= genesis_ts + (st.cfg.viewer_start_hours * 3_600.0) as i64
        {
            viewers_started = true;
            let game = (st.cfg.viewer_window_hours * 3_600.0).min((play_end - now).max(60) as f64);
            match crate::load::spawn_viewers(
                st,
                st.cfg.viewers,
                game / scale,
                bell.max(0) as u32,
                "in-run",
            ) {
                Ok(c) => {
                    st.run.event(
                        Some(now),
                        "viewers",
                        json!({"viewers": st.cfg.viewers, "game_secs": game, "pid": c.id()}),
                    );
                    viewers = Some(c);
                }
                Err(e) => st.run.event(Some(now), "viewers-failed", json!(e)),
            }
        }
        if viewers.is_some() && last_lag.elapsed() >= Duration::from_secs(1) {
            last_lag = Instant::now();
            if let Some(l) = crate::load::fold_lag(&st.chain, &st.program, st.ports.herald).await {
                inrun_lags.push(l);
            }
        }
        if let Some(c) = viewers.as_mut() {
            if let Ok(Some(s)) = c.try_wait() {
                st.run
                    .event(Some(now), "viewers-done", json!({"exit": s.code()}));
                viewers = None;
                if !inrun_judged {
                    inrun_judged = true;
                    judge_in_run(st, &inrun_lags, now, s.code());
                }
            }
        }
        // EndSeason once the last bell is over.
        if st.cfg.end_season && !end_season_sent {
            let end_bell = st.state["season"]["end_bell"]
                .as_i64()
                .unwrap_or(i64::MAX / 2);
            if now >= genesis_ts + end_bell * bell_secs {
                // Wave-5 review: retried until it lands or the program
                // answers AlreadyDone (bounded), from a task, so a slow or
                // killed chain does not stall the supervisor's loop.
                end_season_sent = true;
                let url = st.chain.url.clone();
                let program = st.program;
                let addrs = st.addrs.clone();
                let seed = run::secret(&st.run.path("keys/operator.seed"));
                end_season_task = Some(tokio::spawn(async move {
                    let Ok(seed) = seed else {
                        return json!({"ok": false, "error": "no operator key", "attempts": 0});
                    };
                    end_season_until_done(&url, program, &addrs, &Keypair::new_from_array(seed))
                        .await
                }));
            }
        }
        if end_season_task.as_ref().is_some_and(|t| t.is_finished()) {
            if let Some(t) = end_season_task.take() {
                let r = t
                    .await
                    .unwrap_or_else(|e| json!({"ok": false, "error": e.to_string()}));
                st.run.event(Some(now), "end-season", r);
            }
        }
        // Play is over: the fleet stops (it counts its own end from the
        // herald's clock; one bell of grace, then SIGINT, which writes its
        // report).
        if now >= play_end + bell_secs {
            if let Some(p) = st.procs.get_mut("bots") {
                if p.running() {
                    p.stop(Duration::from_secs(30));
                    p.done = true;
                    st.run.event(
                        Some(now),
                        "bots-stopped",
                        json!({"why": "play is over", "play_end": play_end}),
                    );
                    st.save("running");
                }
            }
        }
        // The drain runs at the drain's scale once the fleet is done (one
        // bell after play; latencies are judged over play only).
        let drain_scale = st.cfg.drain_scale();
        if now >= play_end + bell_secs
            && (drain_scale - cur_scale).abs() > 1e-9
            && drain_scale_failures < 5
        {
            match st.chain.set_scale(drain_scale).await {
                Ok(()) => {
                    cur_scale = drain_scale;
                    st.state["play"]["drain_scale"] =
                        json!({"scale": drain_scale, "from_slot": last.slot, "from_game": now});
                    st.run.event(
                        Some(now),
                        "drain-scale",
                        json!({"scale": drain_scale, "slot": last.slot}),
                    );
                    log_line(&format!("drain at {drain_scale}x from slot {}", last.slot));
                    st.save("running");
                }
                Err(e) => {
                    drain_scale_failures += 1;
                    st.run
                        .event(Some(now), "drain-scale-failed", json!({"error": e}));
                }
            }
        }
        // Done: play and drain over, bots finished.
        let bots_done = st
            .procs
            .get("bots")
            .is_none_or(|p| p.done || !p.running() && p.restart_at.is_none());
        if now >= end && (bots_done || now >= end + 6 * bell_secs) {
            if let Some(p) = st.procs.get_mut("bots") {
                if p.running() {
                    p.stop(Duration::from_secs(20));
                }
            }
            if let Some(mut c) = viewers.take() {
                let code = c.wait().ok().and_then(|s| s.code());
                if !inrun_judged {
                    judge_in_run(st, &inrun_lags, now, code);
                }
            }
            if let Some(t) = end_season_task.take() {
                let r = t
                    .await
                    .unwrap_or_else(|e| json!({"ok": false, "error": e.to_string()}));
                st.run.event(Some(now), "end-season", r);
            }
            if st.cfg.pause_at_end {
                st.chain.pause().await?;
            }
            let s = st.chain.status().await?;
            st.state["complete"] = json!({"slot": s.slot, "game": s.now, "paused": s.paused, "crashes": crashes,
                "wall_ms": run::wall_ms()});
            st.run.event(Some(s.now), "phase", json!("complete"));
            st.save("complete");
            log_line(&format!(
                "complete at slot {} (game {}); services left up{} — run verify/tamper/report, then down",
                s.slot,
                s.now,
                if s.paused { ", chain paused" } else { "" }
            ));
            return Ok(());
        }
    }
}

/// EndSeason, retried (every 5 s, at most 12 attempts of 30 s) until it
/// lands or the program answers `AlreadyDone` (the season already Ended).
async fn end_season_until_done(
    url: &str,
    program: Address,
    addrs: &fclient::addr::Addresses,
    payer: &Keypair,
) -> Value {
    let chain = chain::Chain::new(url, program);
    let done = format!(
        "code Some({})",
        frontier_abi::error::FrontierError::AlreadyDone.code()
    );
    let mut errors = vec![];
    for attempt in 1..=12u32 {
        match chain
            .send_op(
                &[fclient::ix::end_season(addrs, payer.pubkey())],
                &[payer],
                Duration::from_secs(30),
            )
            .await
        {
            Ok(()) => return json!({"ok": true, "attempts": attempt, "errors": errors}),
            Err(e) if e.contains(&done) => {
                return json!({"ok": true, "already_done": true, "attempts": attempt, "errors": errors})
            }
            Err(e) => {
                errors.push(e.lines().next().unwrap_or("").to_string());
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
    json!({"ok": false, "attempts": 12, "errors": errors})
}

/// The in-run viewer window's verdict (§13.4 criterion 6): the generator's
/// `load/in-run.json` judged with the fold lag sampled meanwhile, written
/// to `load/in-run.verdict.json` (the report's criterion-6 evidence).
fn judge_in_run(st: &Stack, lags: &[f64], now: i64, exit: Option<i32>) {
    let rep: Value = std::fs::read_to_string(st.run.path("load/in-run.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    let verdict = crate::load::judge(&rep, lags);
    let out = json!({"tag": "in-run", "window": "in-run", "criterion6_evidence": true,
        "viewers": st.cfg.viewers, "game_hours": st.cfg.viewer_window_hours, "scale": st.cfg.scale,
        "exit": exit, "generator": rep, "verdict": verdict});
    let _ = run::write_atomic(
        &st.run.path("load/in-run.verdict.json"),
        serde_json::to_string_pretty(&out)
            .unwrap_or_default()
            .as_bytes(),
    );
    st.run
        .event(Some(now), "viewers-verdict", out["verdict"].clone());
}

async fn sample(st: &Stack, s: chain::Status, bell: i64) {
    for (w, port, tok) in [
        ('a', st.ports.keeper_a, &st.keys.keeper_a_token),
        ('b', st.ports.keeper_b, &st.keys.keeper_b_token),
    ] {
        if w == 'b' && !st.cfg.keeper_b {
            continue;
        }
        let url = format!("http://127.0.0.1:{port}/v1/status");
        let auth = format!("Bearer {tok}");
        if let Ok(Ok(r)) = tokio::time::timeout(
            Duration::from_secs(5),
            fclient::http::get_with_headers(&url, &[("authorization", &auth)]),
        )
        .await
        {
            if r.status == 200 {
                if let Ok(v) = serde_json::from_slice::<Value>(&r.body) {
                    let line = json!({"bell": bell, "slot": s.slot, "game": s.now, "status": v});
                    append(&st.run.path(&format!("metrics/keeper-{w}.jsonl")), &line);
                }
            }
        }
    }
    let url = format!("{}/h/status", st.ports.herald_url());
    let newest = st
        .chain
        .latest_program_slot(&st.program)
        .await
        .ok()
        .flatten();
    if let Ok(Ok(r)) = tokio::time::timeout(Duration::from_secs(5), fclient::http::get(&url)).await
    {
        if let Ok(v) = serde_json::from_slice::<Value>(&r.body) {
            // Program activity not yet folded (slots), not the chain slot:
            // `lastSlot` is the newest folded transaction's slot.
            let lag = match (newest, v["lastSlot"].as_u64()) {
                (Some(n), Some(x)) => Some(n.saturating_sub(x)),
                _ => None,
            };
            append(
                &st.run.path("metrics/herald.jsonl"),
                &json!({"bell": bell, "slot": s.slot, "game": s.now, "lag_slots": lag, "status": v}),
            );
        }
    }
}

pub fn append(p: &Path, v: &Value) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
    {
        let _ = writeln!(f, "{v}");
    }
}

/// `down`: stops the supervisor (if still running) and every component.
pub fn down(rd: &RunDir) -> i32 {
    let Ok(mut stv) = rd.load_state() else {
        eprintln!("frontier-stack: no run at {}", rd.root.display());
        return 2;
    };
    if let Some(sp) = stv["supervisor_pid"].as_u64() {
        let sp = sp as u32;
        if sp != std::process::id() && crate::procs::is_component(sp, Path::new("frontier-stack")) {
            eprintln!("frontier-stack: stopping the supervisor (pid {sp})");
            crate::procs::stop_pid(sp, Duration::from_secs(60));
        }
    }
    // Re-read: the supervisor may have stopped everything itself.
    stv = rd.load_state().unwrap_or(stv);
    let order = [
        "viewers",
        "bots",
        "herald",
        "relay",
        "keeper-b",
        "keeper-a",
        "drand-replay",
        "localnet",
    ];
    let mut stopped = vec![];
    for n in order {
        let c = &stv["components"][n];
        if let (Some(pid), Some(prog)) = (c["pid"].as_u64(), c["spec"]["program"].as_str()) {
            let pid = pid as u32;
            if crate::procs::is_component(pid, Path::new(prog))
                && crate::procs::stop_pid(pid, Duration::from_secs(15))
            {
                stopped.push(n);
            }
        }
    }
    // The in-run or `load` viewer generators.
    if let Some(v) = stv["viewers_pids"].as_array() {
        for p in v.iter().filter_map(|x| x.as_u64()) {
            if crate::procs::is_component(p as u32, Path::new("frontier-viewers")) {
                crate::procs::stop_pid(p as u32, Duration::from_secs(5));
            }
        }
    }
    if let Some(m) = stv["components"].as_object_mut() {
        for c in m.values_mut() {
            c["pid"] = Value::Null;
        }
    }
    let prev = stv["phase"].as_str().unwrap_or("").to_string();
    stv["phase"] = json!("down");
    stv["phase_before_down"] = json!(prev);
    stv["down_wall_ms"] = json!(run::wall_ms());
    let _ = rd.save_state(&stv);
    rd.event(None, "phase", json!({"down": stopped}));
    println!(
        "down: stopped {}",
        if stopped.is_empty() {
            "nothing (already down)".to_string()
        } else {
            stopped.join(", ")
        }
    );
    let left = live_components(&stv);
    if left.is_empty() {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_rules() {
        let tb = b"xx PSF_TEST_BEACON_BUILD yy".to_vec();
        let rel = b"xx release yy".to_vec();
        assert!(check_so(&tb, Beacon::TestKey).is_ok());
        assert!(check_so(&rel, Beacon::TestKey).is_err());
        assert!(check_so(&rel, Beacon::Archive).is_ok());
        assert!(check_so(&tb, Beacon::Archive).is_err());
        assert!(check_so(b"PSF_ORACLE_BUILD", Beacon::Archive).is_err());
        assert!(check_so(b"PSF_TRACE_BUILD PSF_TEST_BEACON_BUILD", Beacon::TestKey).is_err());
    }

    #[test]
    fn archive_states() {
        let d = std::env::temp_dir().join(format!("psf-arch-{}", run::wall_ms()));
        assert!(matches!(
            check_archive(&d, 0, 0),
            Err(Refusal::PendingFetch(_))
        ));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("partial-1-2.bin"), b"x").unwrap();
        match check_archive(&d, 0, 0) {
            Err(Refusal::PendingFetch(m)) => assert!(m.contains("still being fetched"), "{m}"),
            _ => panic!(),
        }
        // A finished archive (quicknet: genesis 1692803367, period 3) of the
        // main session's plan: rounds 32065012..=32311012 start at G0.
        std::fs::write(
            d.join("info.json"),
            r#"{"genesis_time": 1692803367, "period": 3}"#,
        )
        .unwrap();
        std::fs::write(
            d.join("manifest.json"),
            r#"{"segments": [{"file": "r.bin", "first": 32065012, "count": 246001}]}"#,
        )
        .unwrap();
        let g0 = 1_788_998_400;
        let r = check_archive(&d, g0, g0 + 7 * 86_400).expect("covers 7 days");
        assert_eq!(r.first_time, g0);
        assert_eq!(r.last, 32_311_012);
        assert!(matches!(
            check_archive(&d, g0 + 600, g0 + 86_400),
            Err(Refusal::Bad(m)) if m.contains("not at the run's G0")
        ));
        assert!(matches!(
            check_archive(&d, g0, g0 + 9 * 86_400),
            Err(Refusal::Bad(m)) if m.contains("before the run needs")
        ));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// W6-A: the fleet gets the pacing flags only when it lists them.
    #[test]
    fn eager_flags_follow_the_usage() {
        let old = "usage: frontier-bots --herald URL --relay URL [--rpc URL] [--game-hours H] [--scale S] [--control 127.0.0.1:PORT]";
        assert!(eager_flags_in(old).is_empty());
        let new = "usage: frontier-bots --herald URL [--control 127.0.0.1:PORT] [--day0-share F] [--eager-personas]";
        assert_eq!(
            eager_flags_in(new),
            vec![("--day0-share", Some("1")), ("--eager-personas", None)]
        );
        assert!(eager_flags_in("[--day0-shared X] [--eager-personas-x]").is_empty());
        // The binary of this build answers without hanging.
        let bots = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/release/frontier-bots");
        if bots.exists() {
            let _ = eager_bot_flags(&bots);
        }
    }

    #[test]
    fn keeper_roles_match_the_keeper() {
        let listed: Vec<&str> = keeper_core::config::ROLES.to_vec();
        assert_eq!(listed, keeper_core_roles());
        for r in keeper_roles('b') {
            assert!(listed.contains(&r), "{r}");
        }
    }

    #[test]
    fn keeper_tomls_parse_as_the_keeper_reads_them() {
        let cfg = StackConfig::default();
        let p = cfg.ports().unwrap();
        let ben = Address::new_from_array([3; 32]);
        for w in ['a', 'b'] {
            let t = keeper_toml(w, &cfg, &p, &program_id("x"), &ben, Path::new("/tmp/k"));
            let k = keeper_core::config::KeeperConfig::from_toml(&t)
                .unwrap_or_else(|e| panic!("{e}\n{t}"));
            assert_eq!(k.program, program_id("x"));
            assert_eq!(k.season_id, cfg.season_id);
            assert_eq!(k.beneficiary, ben);
            assert_eq!(k.rpc, vec!["http://127.0.0.1:41010".to_string()]);
            assert_eq!(k.drand, vec!["http://127.0.0.1:41020".to_string()]);
            assert_eq!(k.reveal_pool, 150);
            assert_eq!(k.delay_pool, 32);
            assert_eq!(k.funders, 4);
            let want: Vec<String> = keeper_roles(w).iter().map(|r| r.to_string()).collect();
            assert_eq!(k.roles, want);
            let api = if w == 'a' { 41_050 } else { 41_051 };
            assert_eq!(k.api.map(|a| a.port()), Some(api));
            assert_eq!(k.race_jitter_slots, if w == 'a' { 0 } else { 2 });
            assert!(!k.dev, "the contract's pool minimums");
        }
    }

    #[test]
    fn program_ids_differ_by_run() {
        assert_ne!(program_id("a"), program_id("b"));
        assert_eq!(program_id("a"), program_id("a"));
    }
}
