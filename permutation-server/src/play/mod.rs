//! The playable server for Game Design V5 (`bin/play`): nations, members, offices.
//!
//! Serves the web client from `web/`, `llms.txt` for agents, and a JSON API.
//! Every view, preview and AI decision is made from a nation's fogged belief
//! state (`vision`, §7.4); only validation and resolution see the full state.
//!
//! People and agents are **members** of one of the season's nations (V5 §4).
//! Members elect four officers; an officer orders within its office, every
//! member proposes, supports, votes and recalls (V5 §5).
//!
//! * human member — joins in the browser (`POST /api/join` in the lobby, or
//!   `POST /api/claim` of a member the gateway registered in chain mode),
//!   which returns a member token (`X-Member-Token`).
//! * AI member — a reference AI hosted by this server (`driver`).
//! * external member — an outside agent (e.g. joined through x402). It reads
//!   its nation's view here (`?member=M` or `?civ=N`), dry-runs with
//!   `/api/validate` and signs its own transactions.
//!
//! Vacant offices are run by the acting official (V5 §8).
//!
//! Two modes:
//! * local (default): the engine runs in this process. The season opens in
//!   a lobby; it starts when a member presses start (or at once with
//!   `--autostart`). Entry fees are simulated test USDC.
//! * `--chain http://127.0.0.1:4191`: the world is the on-chain program's
//!   (read through permutation-gateway), ticks resolve on the MagicBlock ER,
//!   and members act on chain with their own session keys.

//!
//! | Module | Contents |
//! |---|---|
//! | `game` | the game: members and who runs them, the lobby, resolving ticks |
//! | `chain` | chain mode: following the chain, sending batches through the gateway |
//! | `views` | `/api/state` and `/api/lobby` |
//! | `routes` | the JSON API |
//! | `http` | parsing requests, writing responses |
//!
//! One lock guards the game. No route writes to a client or calls the
//! gateway while holding it.

mod chain;
mod game;
mod http;
mod routes;
mod views;

pub use game::{Game, Host, Member, Phase, Viewer};
pub use http::{Request, Response};

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::chainlink::ChainLink;

pub type Shared = Arc<Mutex<Game>>;

/// The game, even if a request handler panicked while holding it: the game
/// is left as that handler left it, and the server keeps serving.
pub fn lock(game: &Shared) -> MutexGuard<'_, Game> {
    game.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Config {
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
}

/// Run the server until the process ends.
pub fn serve(cfg: Config) -> Result<(), String> {
    let game: Shared = match &cfg.chain {
        Some(url) => {
            let link = Arc::new(ChainLink::new(url)?);
            let game = Arc::new(Mutex::new(Game::from_chain(link)));
            let follower = game.clone();
            std::thread::spawn(move || chain::follow(follower));
            game
        }
        None => {
            let mut g = Game::new(cfg.tick_seconds, cfg.ai_members);
            if cfg.autostart {
                g.start()?;
            }
            let game = Arc::new(Mutex::new(g));
            let clock = game.clone();
            std::thread::spawn(move || run_clock(clock));
            game
        }
    };
    let listener = TcpListener::bind(("127.0.0.1", cfg.port))
        .map_err(|e| format!("port {}: {e}", cfg.port))?;
    {
        let g = lock(&game);
        eprintln!(
            "PERMUTATION STATE (V5) on http://127.0.0.1:{}/  (web: {})",
            cfg.port,
            cfg.web.display()
        );
        eprintln!(
            "  {} nations, {} members, phase {:?}",
            g.state.civs.len(),
            g.members.len(),
            g.phase
        );
        eprintln!(
            "  spectate: http://127.0.0.1:{0}/?spectate   agents: http://127.0.0.1:{0}/llms.txt",
            cfg.port
        );
    }
    let web = Arc::new(cfg.web);
    for stream in listener.incoming().flatten() {
        let (game, web) = (game.clone(), web.clone());
        std::thread::spawn(move || routes::handle(stream, game, &web));
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
