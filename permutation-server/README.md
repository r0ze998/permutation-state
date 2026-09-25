# permutation-server

The game server, the hosted AI members, the web client and the tools around a season. Every view, preview and AI decision is made from a nation's fogged belief (`fog`); only validation and resolution see the full state.

## Layout

| Module | Contents |
|---|---|
| `play/` | the playable server (`bin/play`): `game` (members and who runs them, the lobby, resolving ticks), `chain` (chain mode: following the chain, sending batches through the gateway), `views` (`/api/state`, `/api/lobby`), `routes` (the JSON API), `http` |
| `api/` | the JSON the clients and agents read: `dto` (orders and governance in), `blocked` (reasons), `previews` (options, forecasts, preflight), `world` (the per-viewer world view), `gov` (members, offices, achievements, payouts) |
| `driver` | the acting officials and the hosted AI members (`Planner`), and `AiSeason`, the all-AI season loop the tools share |
| `bots` | the scripted personas behind the planner |
| `fog`, `ledger` | per-nation vision and memory; observations, sealed decisions and their reveals |
| `chainlink`, `codec` | the minimal HTTP client for the gateway and JSON-RPC; hex and base64 |
| `events` | chronicle lines from the difference between two worlds |

| Binary | Does |
|---|---|
| `play` | the server: `--port 4185 [--tick-seconds 30] [--ai-members 2] [--autostart]`, or `--chain http://127.0.0.1:4191` |
| `verify` | replays an on-chain season from public data and checks every root and payout |
| `sim` | many AI-only seasons, with the balance numbers of V5 §6.5 |
| `replay` | one AI season as a JSON replay for the viewer |
| `ticklog` | one AI season as tick inputs and roots, for on-chain replay |

The web client (`web/`) is plain ES modules with no build step. `app.mjs` polls the view and renders. `rules.mjs` holds the rule numbers: most come from the server (`season.rules`), and the rest are marked as kept in step by hand.

## Concurrency

One lock guards the game. Routes run under it and return a `Response`; the socket is written after the lock is released. A route that must call the gateway returns an `Outcome`, which runs without the lock. The chain follower fetches from the gateway first and then applies what it got under the lock. A handler that panics does not poison the server: `play::lock` recovers the game.

## Tests

```sh
cargo test --release
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

- `tests/golden.rs` plays four AI seasons and pins every tick's root, the payouts and the fogged views. The engine is deterministic, so any behaviour change fails it. A deliberate change regenerates it with `UPDATE_GOLDEN=1 cargo test --release --test golden`, and the diff is reviewed.
- `tests/codec_vectors.rs` writes the vectors the gateway's JavaScript codec is tested against.
- `tests/views.rs` checks what each kind of viewer may see.
- Unit tests in `play/` cover request parsing, the routes, and a person's turn from joining to the next tick.
