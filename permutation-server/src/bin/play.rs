//! Playable server for Game Design V5: nations, members, offices. See
//! `permutation_server::play`.
//!
//!     cargo run --release --bin play -- --port 4185 [--tick-seconds 30] [--ai-members 2] [--autostart]
//!     cargo run --release --bin play -- --port 4185 --chain http://127.0.0.1:4191 \
//!         [--gateway-proxy http://127.0.0.1:4194] [--operator-token-file F]
//!
//! `--host` (default 127.0.0.1) is the address to listen on; a public
//! deployment puts an HTTPS reverse proxy or tunnel in front of it.
//!
//! In chain mode the gateway's operator token (PS_OPERATOR_TOKEN, else the
//! file, by default the gateway's `.local/operator-token`) lets this server
//! act for the AI members the gateway hosts (V5 §18.2). `--gateway-proxy`
//! serves the gateway's public listener under `/gw` on this origin (never
//! with the operator token).

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
        host: arg("--host").unwrap_or_else(|| "127.0.0.1".into()),
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
        operator_token: std::env::var("PS_OPERATOR_TOKEN").ok().or_else(|| {
            let file = arg("--operator-token-file")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../permutation-gateway/.local/operator-token")
                });
            std::fs::read_to_string(file)
                .ok()
                .map(|t| t.trim().to_string())
        }),
        chain: arg("--chain"),
        gateway_proxy: arg("--gateway-proxy"),
    };
    if let Err(e) = serve(cfg) {
        eprintln!("play: {e}");
        std::process::exit(1);
    }
}
