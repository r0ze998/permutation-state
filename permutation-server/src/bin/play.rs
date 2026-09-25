//! Playable server for Game Design V5: nations, members, offices. See
//! `permutation_server::play`.
//!
//!     cargo run --release --bin play -- --port 4185 [--tick-seconds 30] [--ai-members 2] [--autostart]
//!     cargo run --release --bin play -- --port 4185 --chain http://127.0.0.1:4191

use permutation_server::play::{serve, Config};
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let cfg = Config {
        port: arg("--port").and_then(|p| p.parse().ok()).unwrap_or(4185),
        tick_seconds: arg("--tick-seconds")
            .and_then(|p| p.parse().ok())
            .unwrap_or(30),
        ai_members: arg("--ai-members")
            .and_then(|p| p.parse().ok())
            .unwrap_or(2),
        autostart: args.iter().any(|a| a == "--autostart"),
        web: arg("--web")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("web")),
        chain: arg("--chain"),
    };
    if let Err(e) = serve(cfg) {
        eprintln!("play: {e}");
        std::process::exit(1);
    }
}
