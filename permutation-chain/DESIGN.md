# permutation-chain — design

The on-chain half of PERMUTATION STATE. One Solana program runs the same `permutation-rules` crate as the server, the replay verifier and the agents, so a tick resolved on chain is byte-identical to one resolved anywhere else. Play runs on a MagicBlock Ephemeral Rollup (ER); entry, prizes and the final world live on the Solana base layer.

## Accounts

| Account | Seeds | Layer | Holds |
|---|---|---|---|
| Season | `["season", id]` | base | Entry fee, USDC mint, civ registry (wallet, session key, payout owner, name), status, pool, then payouts, claims and the final world root |
| Vault | `["vault", id]` | base | SPL token account for the pool; its owner is the Season PDA |
| World chunks | `["world", id, k]`, k = 0..8 | ER during play | One logical buffer split over 8 accounts of 10 KiB: header (magic, body length, `WorldMeta`: season id, preset, civ count, tick length, deadline, finished) + the borsh `WorldState` (a `GenesisJob` while genesis runs). 80 KiB in total |
| Orders | `["orders", id, civ]` | ER during play | One civ's batch for the open tick, the civ's session key, and the cached budget, so order submission never decodes the world |

## Season lifecycle

```
base  CreateSeason ─ AllocWorld ×8 ─ JoinSeason ×N (USDC → vault; via x402 or directly) ─ StartSeason
base  GenesisStep ×~20 (permissionless)  → tick 0 open
base  Delegate the 8 world chunks + every Orders PDA to the ER
ER    loop: SubmitOrders (session key) … ResolveTick (anyone, after the deadline or once all civs submitted)
ER    Commit (every 20 ticks)  …  UndelegatePart × 5 once the last tick resolved
base  FinishSeason (permissionless: payouts by §14.5 from the final world)  ─ Claim (each payout owner)
```

- **Commits and undelegation (measured on the local MagicBlock stack).**
  - MagicBlock sponsors 10 commits per delegated account when the payer is not delegated. The crank commits every 20 ticks and stops at 9, which keeps the 10th for the final undelegation. A delegated fee payer (an ephemeral balance) would lift the limit.
  - On base, the finalize step of one intent runs every undelegation in one transaction. For all 14 accounts, that exceeded Solana's instruction-trace limit (`MaxInstructionTraceLengthExceeded`).
  - `UndelegatePart` therefore sends groups of 3. World chunk 0 goes last, because it carries the "finished" header the program checks.
- **Why chunks.** Accounts that a program creates or grows through CPI are limited to 10 KiB per step (`MAX_PERMITTED_DATA_INCREASE`). That limit also applies to the delegation program's buffer and to undelegation, so a single 96 KiB world could not be delegated or brought back. Eight 10 KiB PDAs avoid it; the program reads and writes them as one buffer (`state::Chunks`).
- **Genesis is on chain.** Map generation is split into bounded steps (`map::MapJob`): terrain, start-candidate scanning in chunks, placing starts, balancing rounds, and sites. Running the steps back to back is exactly `generate`; a test checks equality for both presets.
- **The season seed** is the latest slot hash, mixed with the season id and every entrant, and taken when entry closes. Limitation: the slot leader could grind it. MagicBlock VRF is the planned source.
- **Tick randomness** is `sha256("PS/tick-vrf/v1" ‖ event_head ‖ slot ‖ unix time)`, taken at phase 0. Orders are fixed before this value exists. Same limitation and plan.
- **Session keys.** A civ's orders may be signed by its registered session key or its wallet. The session key can only submit orders; it cannot move USDC.
- **Order checks.** `SubmitOrders` runs the rules' `check_structure` (freezes, lengths, one order per unit, reveal timing) and checks the cached budget. Full validation happens again at resolution, inside the engine.
- **Decision commitments** travel in each batch (`decision_digest`) and are appended to the world's event chain at phase 0. Reveals ride along later as `RevealRationale` orders (spec §4.3, §7.5).
- **Replay record.** Every `ResolveTick` logs `PS_TICK ‖ tick ‖ to ‖ vrf ‖ pre_root ‖ post_root ‖ borsh(batches)` with `sol_log_data`, and genesis logs `PS_GENESIS ‖ root ‖ season_seed`. From these logs and the Season account, anyone can replay the season and check every root.

## Compute (item 1 measurement)

Measured on `solana-test-validator` (Agave 3.1.9) by replaying a full 6-civ Blitz bot match (180 ticks) whose orders and state roots were recorded natively (`permutation-server --bin ticklog`):

| Operation | CU | Transactions |
|---|---|---|
| One tick (decode + 12 phases + encode), peak | **862k** of 1.4M | 1 |
| One tick, average | 634k | 1 |
| Genesis, per step (50 candidates) | ≤ 1.16M | 18–19 in total, ~11.3M CU |
| All 180 on-chain state roots vs native | identical | — |

What it took to get there, with results byte-identical at every step:

1. SHA-256 through the `sol_sha256` syscall (the `hash` module).
2. Arithmetic tile indexing instead of binary search: the map is always a full hexagon.
3. Neighbourhood-only scans for territory, city yields and start checks.
4. Encoding the world into one buffer and copying it once, instead of writing field by field into the account.

With the 256 KiB heap and an upward bump allocator (`heap.rs`), one tick uses well under the heap limit.

Per-tick breakdown at peak: decode 125k · production 306k · movement 134k · encode 76k · scoring 50k · everything else below 60k each.

If a tick ever exceeds the budget (the Season preset has 16 civs and a larger map), `ResolveTick { to }` runs a prefix of the phases; the engine's `phase_cursor` resumes exactly where it stopped.

## Trust model (what the chain guarantees)

- **Rules.** Every rule is enforced by the program: the same crate, with no operator override. Anyone can resolve a tick after its deadline.
- **Money.** Entry fees go into a vault that only the program controls. Payouts are computed on chain from the final world; each payout owner claims their own share.
- **Fog.** Fog is enforced off chain. The world accounts are readable on the ER, so hidden information is hidden only by the clients: the game server serves each civ its fogged view (`?civ=N`) to anyone, because it is a subset of public data, and bots, the reference agents and the web client decide only from it. What is enforced is who may order for a civ (its session key or wallet). MagicBlock PER (TEE) is the upgrade path (V4 §7).
- **Decisions.** The digest in each batch binds the decision to the observation root the server published before the tick (`obs_root`), a policy name and a salted rationale. The reveal comes in a later batch, so a player cannot rewrite its reasoning after seeing the outcome. It proves what was claimed and when, not that the claim is true.
- **Entry (x402).** The payment is the program's own `JoinSeason`, signed by the payer. The facilitator (gateway) only adds its fee-payer signature after checking that the transaction contains exactly that instruction for this season and vault, and never signs as the payer; it cannot change the amount or the payee. `permutation-gateway/scripts/x402-check.mjs` tries tampered payments.
- **Replay.** `permutation-server --bin verify` rebuilds genesis from the Season account and replays every tick from the `PS_TICK` records read from the ER's transaction logs, checking every root and the payouts.
- **Operator.** The operator (the crank) can delay genesis steps, delegation and commits. It cannot change outcomes. Liveness steps are permissionless except delegation.
