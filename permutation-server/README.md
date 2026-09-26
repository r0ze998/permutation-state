# permutation-server

The game server, the hosted AI members, the web client and the tools around a season. The information model is perfect information (`fog`): every account is public on chain, so every view, preview and AI decision, for people, hosted AI members and bots alike, is made from the full world. Vision is kept only as a display-only "sight". What stays hidden is an officer's sealed batch until it is revealed.

## Layout

| Module | Contents |
|---|---|
| `play/` | the playable server (`bin/play`): `game` (members and who runs them, the lobby, resolving ticks), `chain` (chain mode: waiting for the on-chain world, following the chain, sending the AI members' batches through the gateway), `roster` (the operator's AI members and their salts, known only to this server; announces an AI whose home city fell), `talk` (members' messages, kept locally or read from the gateway, and the AI members' answers), `views` (`/api/state`, `/api/lobby`), `routes` (the JSON API, including `GET`/`POST /api/talk` and `GET /api/roster`), `proxy` (`/gw/*`: the gateway's public listener on this origin), `http` (with the security headers every answer carries) |
| `api/` | the JSON the clients and agents read: `dto` (orders and governance in), `blocked` (reasons), `previews` (options, forecasts, preflight), `world` (the per-viewer world view), `gov` (members, offices, achievements, payouts) |
| `driver` | the hosted AI members (`Planner`) and `AiSeason`, the all-AI season loop the tools share. Vacant offices are not driven here: the rules' caretaker (`gov::caretaker`) fills them |
| `bots/` | the scripted personas behind the planner: `plan` (one nation's orders, step by step in a fixed order), `geo` (paths and sites), `contracts` (treasury contracts), `rationale` (the sealed reasons) |
| `fog`, `ledger` | the information model (`Fog::belief` borrows the full state; sight is display-only); observations, sealed decisions and their reveals |
| `chainlink`, `codec` | the minimal HTTP client for the gateway and JSON-RPC; hex and base64 |
| `events` | chronicle lines from the difference between two worlds |

| Binary | Does |
|---|---|
| `play` | the server: `[--host 127.0.0.1] --port 4185 [--tick-seconds 30] [--ai-members 2] [--autostart]`, or `--chain http://127.0.0.1:4191 [--gateway-proxy http://127.0.0.1:4194] [--operator-token-file F]`. In chain mode the gateway's operator token (`PS_OPERATOR_TOKEN`, else the file, by default `../permutation-gateway/.local/operator-token`) lets it act for the operator's AI members and read the AI roster (V5 §18.2); `--gateway-proxy` serves the gateway's public listener under `/gw` (see below). A public deployment puts an HTTPS reverse proxy or tunnel (Caddy, Cloudflare Tunnel) in front of `--host`/`--port` and exposes nothing else ([README](../README.md#5-public-deployment)) |
| `verify` | replays an on-chain season from public data and checks every root and payout, every revealed batch against `PS_COMMITS`, every tick's randomness against `PS_SALTS`, and the history chain (`PS_HISTORY`). With operator AI members (rules version 7) it checks each revealed salt against the member's registration tag and the tags against the committed roster chain, and recomputes the settlement with `permutation_chain::finalize`, the function `FinishSeason` runs. Modules (`bin/verify/`): `setup` (season, genesis, seating, first election), `ticks`, `settlement`, `chain` (log records), `report`. A season verifies only with a build of its rules version (v8 from `4a28f58`, v7 `73e99eb`, v6 `9ab5311`, v5 `a02862f` or earlier) |
| `sim` | many AI-only seasons, with the balance numbers of V5 §6.5, non-exclusive path pairs, era timing, lead changes, wars and captures, and points per start slot. `SIM_SET` overrides rule numbers; `SIM_AI` (default 1) makes the first members of each nation operator AI members with `SIM_BOUNTY` (default 5 USDC) each, and `SIM_TREASURY` gives every nation a starting treasury (contracts, V5 §18.12); `SIM_ROTATE` and `SIM_EQUIV` are rotation diagnostics. Every variable is listed at the top of `bin/sim/main.rs`; `env` reads them, `season` plays one season, `totals` prints the summary, `equiv` is the rotation check |
| `mapstat` | measures generated maps |
| `replay` | one AI season as a JSON replay for the viewer |
| `ticklog` | one AI season as tick inputs and roots, for on-chain replay |

The web client (`web/`) is plain ES modules with no build step. `app.mjs` boots the client and applies each view; `sync.mjs` holds the single-flight `poll()` the other modules call. `rules.mjs` holds the rule numbers: most come from the server (`season.rules`), and the rest are marked as kept in step by hand.

## Chain mode

Every member other than the operator's AI members signs its own transactions: a person with their own wallet and a session key in the browser, or an outside agent through the SDK. Both register through the gateway's x402 route. The server holds no member tokens in chain mode and acts for nobody but the AI members.

- **Before the world exists** the server already listens. It serves the web client, and `GET /api/lobby` answers `{phase: "registering", mode: "chain", gateway, entryFee, chain: {seasonId, programId, cluster, accounts}}` (from the gateway's `/season`, once it answers). Once the world exists but the government has not opened, the phase is `starting`. Every other `/api/*` answers `503 {ok: false, error: "registering"}`. When the government opens, the full game is served without a restart.
- **`/gw/*`** (with `--gateway-proxy URL`) is the gateway's public listener on this origin, so one HTTPS hostname serves both (no CORS, no mixed content). The proxy has its own HTTP client. It never sends the operator token, and of the browser's headers it passes on only `Content-Type` and `X-PAYMENT` (the x402 payment). It adds the browser's address as `X-Forwarded-For`: the socket's peer, or, behind a reverse proxy or tunnel on this machine, the last address that proxy forwarded. The gateway honours that header on its public listener (`--trust-proxy`, on by default, only from a loopback peer), so its limits apply per browser; do not run it with `--no-trust-proxy` behind `/gw`. Bodies over 64 KiB get 413. What comes back is the gateway's status, content type, body and `X-PAYMENT-RESPONSE`. `view.chain.gateway` and `lobby.gateway` are `/gw` with the proxy, else the `--chain` URL.
- **Acting routes are refused, the same way for everyone**: `POST /api/orders`, `/api/control`, `/api/gov`, `/api/talk` and `/api/lobby` answer `400 {ok: false, error: "chain mode: sign in the browser"}` before the request is looked at, and `POST /api/start` is refused too. `/api/claim`, `/api/seats`, `X-Seat-Token` and `?token=` no longer exist, in either mode.
- **`?member=M`** (both modes) gives nation M's view plus M's public `member` block: merit, offices, `projectedPayout`, `standingFor`, activity, civ and name. It has the same fields for every member and never includes `committed`, `ready` or `host`.
- **`POST /api/validate {member, orders, adopt: {Role: [ids]}}`** splits the drafts by the offices M holds and checks each as its batch (`validate_batch`, any digest). It answers `{ok, tick, offices: [{role, orders, adopt, cost, spendable, error}], warnings, refused: [{order, error}]}`, where `refused` lists orders of offices M does not hold ("propose it instead") and `ok` covers the held offices. Without `member` it answers as before.
- **The operator AI count**: `/api/state` carries it as `aiRoster` (`{aiCount, bountyEach, revealed, fallen, homeTick}`), and its `roster` is the viewed nation's member list. `/api/lobby` keeps the AI-roster object under `roster`.
- **The tick's phase**: the view carries `chainPhase` (`commit|reveal|frozen|finished`) and `phaseSecondsLeft` (the reveal window during `reveal`). `secondsLeft` counts down to the commit deadline and is 0 once commitments close. In local mode `chainPhase` is null.
- **Nothing public tells the AI members apart** (V5 §18.2):
  - Member lists have no `ready`. In a season with AI members they have no `kind`/`attested` either, and every kind reads "undeclared" elsewhere.
  - Decisions carry no `external` flag.
  - `waiting` is local-mode only.
  - Every officer commits with one policy id, `officer@2` (`ledger::DEFAULT_POLICY`): the AI members here, people in local mode, and the web client.
- **The ledger follows what landed.** When a tick resolves, a decision on chain replaces a record sealed here with another digest, and a record whose batch never landed is dropped. Only decisions this server sealed are ever revealed from here.

## Concurrency

One lock guards the game. Routes run under it and return a `Response`; the socket is written after the lock is released. A route that must call the gateway (the history layer) returns an `Outcome`, which runs without the lock, and `/gw/*` never takes it. The chain follower fetches from the gateway first and then applies what it got under the lock. In chain mode the game is installed into the `Site` once the on-chain world exists; until then only the lobby answers. A handler that panics does not poison the server: `play::lock` recovers the game.

## Tests

```sh
cargo test --release
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

- `tests/golden.rs` plays four AI seasons and pins every tick's root, the payouts and the views. The engine is deterministic, so any behaviour change fails it. A deliberate change regenerates it with `UPDATE_GOLDEN=1 cargo test --release --test golden`, and the diff is reviewed. (The neutral policy id changed only the roots, through the decision digests and reveal events; payouts and views stayed the same.)
- `tests/codec_vectors.rs` writes the vectors the gateway's JavaScript codec is tested against.
- `tests/views.rs` checks what each kind of viewer may see, and that the ledger follows what landed on chain.
- `tests/symmetry.rs` replays a season in the world turned by 60° with turned orders and checks that the scores are identical (the maps are six-fold rotationally symmetric and the rules rotation-equivariant).
- Unit tests in `play/` cover:
  - request parsing, the security headers (on the wire too) and a person's turn from joining to the next tick;
  - chain mode: the pre-world lobby and its 503s; action refusals identical for every member; the `?member=` block with one shape for AI and other members; `/api/validate` with `member` and `adopt`; the tick phase;
  - the `/gw` proxy, against a fake gateway on a random port: it forwards no credentials (`Authorization`, `X-Member-Token`, cookies) and refuses odd paths, methods, headers and bodies.
