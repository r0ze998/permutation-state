# permutation-server

The game server, the hosted AI members, the web client and the tools around a season. The information model is perfect information (`fog`): every account is public on chain, so every view, preview and AI decision, for people, hosted AI members and bots alike, is made from the full world. Vision is kept only as a display-only "sight". What stays hidden is an officer's sealed batch until it is revealed.

## Layout

| Module | Contents |
|---|---|
| `play/` | the playable server (`bin/play`): `game` (members and who runs them, the lobby, resolving ticks), `chain` (chain mode: following the chain, sending batches through the gateway), `roster` (the operator's AI members and their salts, known only to this server; announces an AI whose home city fell), `views` (`/api/state`, `/api/lobby`), `routes` (the JSON API, including `GET`/`POST /api/talk` and `GET /api/roster`), `http` |
| `api/` | the JSON the clients and agents read: `dto` (orders and governance in), `blocked` (reasons), `previews` (options, forecasts, preflight), `world` (the per-viewer world view), `gov` (members, offices, achievements, payouts) |
| `driver` | the hosted AI members (`Planner`) and `AiSeason`, the all-AI season loop the tools share. Vacant offices are not driven here: the rules' caretaker (`gov::caretaker`) fills them |
| `bots` | the scripted personas behind the planner |
| `fog`, `ledger` | the information model (full state, display-only sight); observations, sealed decisions and their reveals |
| `chainlink`, `codec` | the minimal HTTP client for the gateway and JSON-RPC; hex and base64 |
| `events` | chronicle lines from the difference between two worlds |

| Binary | Does |
|---|---|
| `play` | the server: `--port 4185 [--tick-seconds 30] [--ai-members 2] [--autostart]`, or `--chain http://127.0.0.1:4191 [--operator-token-file F]`. In chain mode the gateway's operator token (`PS_OPERATOR_TOKEN`, else the file, by default `../permutation-gateway/.local/operator-token`) lets it act for hosted members and read the AI roster (V5 §18.2) |
| `verify` | replays an on-chain season from public data and checks every root and payout, every revealed batch against `PS_COMMITS`, every tick's randomness against `PS_SALTS`, and the history chain (`PS_HISTORY`). With operator AI members (rules version 7) it checks each revealed salt against the member's registration tag and the tags against the committed roster chain, and recomputes the settlement with `permutation_chain::finalize`, the function `FinishSeason` runs. Seasons from the devnet program (rules version 5) verify with a build of commit `a02862f` or earlier |
| `sim` | many AI-only seasons, with the balance numbers of V5 §6.5, non-exclusive path pairs, era timing, lead changes, wars and captures, and points per start slot. `SIM_SET` overrides rule numbers; `SIM_AI` (default 1) makes the first members of each nation operator AI members with `SIM_BOUNTY` (default 5 USDC) each, and `SIM_TREASURY` gives every nation a starting treasury (contracts, V5 §18.12); `SIM_ROTATE` and `SIM_EQUIV` are rotation diagnostics |
| `mapstat` | measures generated maps |
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

- `tests/golden.rs` plays four AI seasons and pins every tick's root, the payouts and the views. The engine is deterministic, so any behaviour change fails it. A deliberate change regenerates it with `UPDATE_GOLDEN=1 cargo test --release --test golden`, and the diff is reviewed.
- `tests/codec_vectors.rs` writes the vectors the gateway's JavaScript codec is tested against.
- `tests/views.rs` checks what each kind of viewer may see.
- `tests/symmetry.rs` replays a season in the world turned by 60° with turned orders and checks that the scores are identical (the maps are six-fold rotationally symmetric and the rules rotation-equivariant).
- Unit tests in `play/` cover request parsing, the routes, and a person's turn from joining to the next tick.
