//! The playable server for Game Design V5 (`bin/play`): nations, members, offices.
//!
//! Serves the web client from `web/`, `llms.txt` for agents, and a JSON API.
//! Every view, preview and AI decision is made from the full state (perfect
//! information, `fog`): everything is public on chain anyway.
//!
//! People and agents are **members** of one of the season's nations (V5 §4).
//! Members elect four officers; an officer orders within its office, every
//! member proposes, supports, votes and recalls (V5 §5).
//!
//! * AI member — a reference AI hosted by this server (`driver`).
//! * local mode: a person joins in the browser (`POST /api/join` in the
//!   lobby), which returns a member token (`X-Member-Token`).
//! * chain mode: every other member — a person with their own wallet, or an
//!   outside agent (both register through x402) — signs its own
//!   transactions in the browser or the SDK. This server holds no tokens for
//!   them: it serves any member's public view (`?member=M`, the same for
//!   every member), dry-runs its drafts (`/api/validate` with `member`), and
//!   refuses every action route the same way for everyone.
//!
//! Vacant offices are filled by the rules' caretaker (`gov::caretaker`):
//! the members' top proposal for the office, else a minimal default.
//!
//! Two modes:
//! * local (default): the engine runs in this process. The season opens in
//!   a lobby; it starts when a member presses start (or at once with
//!   `--autostart`). Entry fees are simulated test USDC.
//! * `--chain http://127.0.0.1:4191`: the world is the on-chain program's
//!   (read through permutation-gateway), ticks resolve on the MagicBlock ER,
//!   and members act on chain with their own session keys. The server
//!   listens at once: until the government opens, `/api/lobby` says
//!   `registering` (no world yet) or `starting`, and the rest of `/api/*`
//!   answers 503; then the full game is served, without a restart.
//!   `--gateway-proxy URL` serves the gateway's public listener under `/gw`
//!   (`proxy`), so the page and the gateway share one origin.
//!
//! | Module | Contents |
//! |---|---|
//! | `game` | the game: members and who runs them, the lobby, resolving ticks |
//! | `chain` | chain mode: following the chain, sending batches through the gateway |
//! | `roster` | the operator's AI members: salts, home cities, announcements (V5 §18.2) |
//! | `talk` | members' messages and the AI members' answers (V5 §18.7) |
//! | `views` | `/api/state` and `/api/lobby` |
//! | `routes` | the JSON API |
//! | `proxy` | `/gw/*`: the gateway's public listener on this origin |
//! | `http` | parsing requests, writing responses (with the security headers) |
//!
//! One lock guards the game. No route writes to a client or calls the
//! gateway while holding it.

mod chain;
mod game;
mod http;
mod proxy;
mod roster;
mod routes;
mod talk;
mod views;

pub use game::{Game, Host, Member, Phase, Viewer};
pub use http::{Request, Response};
pub use proxy::GatewayProxy;

use serde_json::Value;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use crate::chainlink::ChainLink;

pub type Shared = Arc<Mutex<Game>>;

/// What the server answers from: the web client, the `/gw` proxy, and the
/// game once it exists. In chain mode the server listens before the
/// on-chain world exists (people register meanwhile) and takes the game
/// over when it appears (`install`), without a restart.
pub struct Site {
    pub web: PathBuf,
    pub proxy: Option<GatewayProxy>,
    /// Chain mode: the gateway as the browser reaches it (`/gw` behind the
    /// proxy, else the `--chain` URL).
    pub gateway: Option<String>,
    game: OnceLock<Shared>,
    /// Chain mode, before the game exists: the gateway's last `/season`.
    season: Mutex<Option<Value>>,
}

impl Site {
    /// `chain`: the `--chain` URL (chain mode).
    pub fn new(web: PathBuf, proxy: Option<GatewayProxy>, chain: Option<&str>) -> Site {
        let gateway = chain.map(|url| match &proxy {
            Some(_) => proxy::PREFIX.to_string(),
            None => url.trim_end_matches('/').to_string(),
        });
        Site {
            web,
            proxy,
            gateway,
            game: OnceLock::new(),
            season: Mutex::new(None),
        }
    }

    /// A site serving `game` at once.
    pub fn with_game(web: PathBuf, game: Shared) -> Site {
        let site = Site::new(web, None, None);
        site.install(game);
        site
    }

    /// The game is ready to be served (once; later calls are ignored).
    pub fn install(&self, game: Shared) {
        let _ = self.game.set(game);
    }

    pub fn game(&self) -> Option<&Shared> {
        self.game.get()
    }

    /// The season as the gateway describes it while there is no game yet.
    pub fn set_season(&self, info: Value) {
        *self.season.lock().unwrap_or_else(PoisonError::into_inner) = Some(info);
    }

    pub fn season(&self) -> Option<Value> {
        self.season
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// The game, even if a request handler panicked while holding it: the game
/// is left as that handler left it, and the server keeps serving.
pub fn lock(game: &Shared) -> MutexGuard<'_, Game> {
    game.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Config {
    /// Address to listen on (`--host`, default 127.0.0.1).
    pub host: String,
    pub port: u16,
    pub tick_seconds: u64,
    /// Hosted AI members per nation (local mode).
    pub ai_members: usize,
    /// Start the local season at once instead of waiting in the lobby.
    pub autostart: bool,
    /// The web client's directory.
    pub web: PathBuf,
    /// The gateway (chain mode), e.g. `http://127.0.0.1:4191`.
    pub chain: Option<String>,
    /// The gateway's operator token (chain mode): lets this server act for
    /// the AI members the gateway hosts and read the AI roster (V5 §18.2).
    /// Only this server's own gateway client (`ChainLink`) sends it.
    pub operator_token: Option<String>,
    /// The gateway's public listener, served under `/gw` (`--gateway-proxy`).
    pub gateway_proxy: Option<String>,
}

/// Run the server until the process ends.
pub fn serve(cfg: Config) -> Result<(), String> {
    let proxy = cfg
        .gateway_proxy
        .as_deref()
        .map(GatewayProxy::new)
        .transpose()?;
    if let (Some(p), Some(chain)) = (&proxy, &cfg.chain) {
        // The operator listener behind a public path would be one mistake
        // away from exposing the operator routes.
        if crate::chainlink::parse_http_url(chain).ok()
            == crate::chainlink::parse_http_url(&p.url()).ok()
        {
            return Err("--gateway-proxy must be the gateway's public listener (--public-port), not the --chain operator listener".into());
        }
    }
    let site = Arc::new(Site::new(cfg.web, proxy, cfg.chain.as_deref()));
    match &cfg.chain {
        Some(url) => {
            let link = Arc::new(ChainLink::new(url)?.with_token(cfg.operator_token.clone()));
            if !link.has_token() {
                eprintln!("no operator token: hosted AI members cannot act (set PS_OPERATOR_TOKEN or --operator-token-file)");
            }
            let s = site.clone();
            // Waits for the on-chain world, installs the game, follows the chain.
            std::thread::spawn(move || chain::attach(s, link));
        }
        None => {
            let mut g = Game::new(cfg.tick_seconds, cfg.ai_members);
            if cfg.autostart {
                g.start()?;
            }
            let game = Arc::new(Mutex::new(g));
            site.install(game.clone());
            std::thread::spawn(move || run_clock(game));
        }
    }
    let listener = TcpListener::bind((cfg.host.as_str(), cfg.port))
        .map_err(|e| format!("{}:{}: {e}", cfg.host, cfg.port))?;
    let origin = format!("http://{}:{}", cfg.host, cfg.port);
    eprintln!(
        "PERMUTATION STATE (V5) on {origin}/  (web: {})",
        site.web.display()
    );
    match site.game() {
        Some(game) => {
            let g = lock(game);
            eprintln!(
                "  {} nations, {} members, phase {:?}",
                g.state.civs.len(),
                g.members.len(),
                g.phase
            );
        }
        None => eprintln!("  chain mode: registration lobby until the on-chain world exists"),
    }
    if let Some(p) = &site.proxy {
        eprintln!(
            "  gateway (public listener {}) at {origin}{}/",
            p.url(),
            proxy::PREFIX
        );
    }
    eprintln!("  spectate: {origin}/?spectate   agents: {origin}/llms.txt");
    for stream in listener.incoming().flatten() {
        let site = site.clone();
        std::thread::spawn(move || routes::handle(stream, &site));
    }
    Ok(())
}

/// Local mode: resolve the open tick when its time is up.
fn run_clock(game: Shared) {
    loop {
        std::thread::sleep(Duration::from_millis(150));
        let mut g = lock(&game);
        if g.phase == Phase::Playing && !g.paused && !g.over() && Instant::now() >= g.deadline {
            g.advance();
        }
    }
}
