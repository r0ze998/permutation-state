# permutation-chain — design

The on-chain half of PERMUTATION STATE (Game Design V5). One native Solana program runs the same `permutation-rules` crate as the game server, the AI members and the replay verifier, so a tick resolved on chain is byte-identical to one resolved anywhere else. Registration, the prize vault and the final world live on the Solana base layer; the world and the nations' accounts are delegated to a MagicBlock Ephemeral Rollup (ER) while the season plays.

## Code layout

| File | Contents |
|---|---|
| `instruction.rs` | the instructions and their accounts (the borsh tag is the variant index; never reorder) |
| `state.rs` | account layouts, sizes, and `Chunks` (the world across its chunk accounts) |
| `processor/` | one module per stage: `registration`, `genesis`, `delegation`, `play`, `settlement`; `accounts` holds the shared checks and loaders |
| `error.rs` | error codes (clients map them by `ChainError::ALL`) |
| `token.rs`, `heap.rs` | SPL token calls; the bump allocator for the large world |

Every account an instruction reads is checked for its owner and its PDA address (nation accounts by their stored bump), not by its content alone: world chunks are program-owned too, and their bytes are partly shaped by player input.

## Accounts

| Account | Seeds | Layer | Holds |
|---|---|---|---|
| Season (`PSSEASN5`, 4 KiB) | `["season", id]` | base | Admin, crank, USDC mint, preset, nation count, entry fee, tick length, market flag, status, seeds; member count and members per nation, seated count; pool (80% of fees), operations (20%), treasury deposits and final treasuries per nation; after `FinishSeason`, the payout per member and the final world root |
| Vault | `["vault", id]` | base | SPL token account holding fees and treasury deposits; its owner is the Season PDA |
| Member (`PSMEMBR5`) | `["member", id, wallet]` | base | Member index (registration order), nation, wallet (pays and receives), session key (signs orders and governance, cannot move USDC), self-declared kind, optional attestation hash, pre-season candidacy and first-election votes, treasury shares, claimed flag |
| World chunks | `["world", id, k]`, k = 0..20 | ER during play | One logical buffer over 20 accounts of 4 KiB (80 KiB; a Blitz world with ~15 members ends near 23 KB): header (magic `PSWORLD5`, body length, `WorldMeta`: season id, preset, nation count, tick length, deadline, finished, market, and the frozen-input state: frozen flag, tick randomness, chunks published) + the borsh `WorldState` (a `GenesisJob` while genesis runs) |
| Nation (`PSNATN06`, 8 KiB) | `["nation", id, civ]` | ER during play | The open tick; its officers and their session keys; each office's spendable budget; which offices submitted; up to 4 office batches for the open tick; the governance inbox (≤ 8 actions per signer per tick); the frozen flag. Submissions never decode the world |

## Season lifecycle

```
base  CreateSeason ─ AllocWorld ×20 ─ AllocNation ×6
base  Register ×N  (entry fee + optional treasury deposit → vault; wallet-signed, via x402 or directly)
base  StartSeason ─ GenesisStep ×~20 (permissionless) ─ SeatMembers ×⌈N/6⌉ ─ OpenGovernment (first election)
base  Delegate the 20 world chunks + 6 nation accounts to the ER
ER    loop per tick:
        SubmitGov (member's session key) … SubmitOrders (office holder's key, or the crank for a vacant office)
        LogTickInput chunk 0..n   (after the deadline or once every office submitted; freezes the input)
        ResolveTick { to } ×1..k  (the phases up to `to`; resumes at the phase cursor)
ER    CommitPart ×22 every 20 ticks … UndelegatePart ×22 once the last tick resolved
base  FinishSeason (permissionless: every member's payout from the final world) ─ Claim (each wallet) ─ WithdrawOps (admin)
```

- **Registration.** `Register` moves exactly the entry fee (plus an optional treasury deposit) from the payer's USDC account into the vault and creates the Member PDA. The instruction is the same whether a wallet signs it directly or an agent pays over x402; the x402 facilitator only adds its fee-payer signature. Registration closes at `StartSeason`. There are at most 256 members per season, because the payout table lives in the Season account.
- **Seating and the first election.** After genesis, `SeatMembers` adds members to the world in registration order, with their candidacy and first-election votes; each call logs `PS_SEAT ‖ root ‖ borsh(members seated)`. `OpenGovernment` holds the first election, opens tick 0 on the nation accounts, starts the clock and logs `PS_OPEN ‖ root`.
- **Offices and submissions.** `SubmitOrders { role, tick, digest, orders, adopt }` must be signed by the office holder's session key, or by the crank for a vacant office (the acting official). An officer's batch must carry a non-zero decision digest. The program checks the order structure, the office's allowed orders and its spendable budget. `SubmitGov { member, action }` queues a vote, candidacy, proposal, support or recall. The engine checks the signer against the member's registered key when the tick resolves.
- **Input publication (data availability).** A transaction's logs are capped at 10 KB, and a busy tick's input (every office's batch and every governance action) is bigger than that. So the input is not logged by `ResolveTick`:
  - `LogTickInput { chunk }` rebuilds the pending input from the nation accounts and logs `PS_INPUT ‖ tick ‖ chunk ‖ total ‖ sha256(input) ‖ bytes`, 6,000 bytes per chunk.
  - Chunk 0 is allowed once the deadline passed or every office submitted. It **freezes** the input: it draws the tick randomness `sha256("PS/tick-vrf/v2" ‖ world root ‖ slot ‖ unix time)` into the world header and marks every nation account frozen, so later `SubmitOrders`/`SubmitGov` fail with `TickFrozen`.
  - Chunks are logged in order. `ResolveTick` fails with `InputNotPublished` until all of them are.
  - `ResolveTick` logs `PS_TICK ‖ tick ‖ to ‖ pre_root ‖ post_root ‖ sha256(input)`.
  - A verifier can therefore rebuild every tick from the ER's logs alone, whatever the size of the input.
- **Split resolution.** `ResolveTick { to }` runs the phases from the engine's `phase_cursor` up to `to`; the tick completes when the cursor wraps. A split tick logs one `PS_TICK` per part, with the same input hash. Parts are needed when a tick exceeds the compute budget or the heap: the program's bump allocator never frees, so running many phases in one transaction can run out of the 256 KiB heap before it runs out of compute. The crank learns, per starting phase, the furthest stop that fit, and probes further every 10 ticks. Stops are 2, 4, 5, 6, 7, 9 and 12.
- **Commits and undelegation (measured on devnet and the local stack).** An intent (one `CommitPart` or `UndelegatePart`) is finalized on the base layer by the ER's committor, and one transaction finalizes every target of the intent. Devnet showed three limits. Each of them left accounts stuck mid-undelegation, with no recovery:
  - **64 account keys per transaction.** A commit of all 14 accounts of the earlier layout needed about 90 keys and was silently dropped ([magicblock-validator#1693](https://github.com/magicblock-labs/magicblock-validator/issues/1693)).
  - **Compute per finalize.** The committor gives the delegation program's finalize 359,700 CU. Three 10 KiB world chunks in one intent exceeded it, and so did one densely written 10 KiB chunk. About 2.5 KB of data fit.
  - **The instruction trace**, for large intents.

  That is why the world is 20 chunks of 4 KiB. The crank sends every commit round and the final undelegation as small intents: the nation accounts in threes and each world chunk alone, with chunk 0 last (its header says whether the season is over). `Commit`/`CommitAndUndelegate` (everything in one intent) remain for local stacks only.
  - MagicBlock sponsors 10 commits per delegated account when the payer is not delegated. The crank commits every 20 ticks and stops at 9, keeping the 10th for the final undelegation.
- **Why chunks.** Accounts that a program creates or grows through CPI are limited to 10 KiB per step, and the committor's finalize limits above apply per intent. So the world is spread over twenty 4 KiB PDAs, which the program reads and writes as one buffer (`state::Chunks`). Transactions that pass every chunk stay within the size limit: 26 world and nation accounts for a resolve, and 6 members per `SeatMembers`.
- **Genesis is on chain,** split into bounded steps (`map::MapJob`). Running the steps back to back is exactly `generate`; a test checks equality.
- **The season seed** is the latest slot hash, mixed with the season id, the member count and the treasury deposits, taken when registration closes (`StartSeason`). The slot leader could grind it; MagicBlock VRF is the plan.
- **Settlement.** `FinishSeason` needs every account back on base and the world finished. It computes the V5 payout from the final world (`permutation_rules::payout::settle`) and writes one payout per member into the Season account, together with the final treasuries and the final root. `Claim` pays the member's payout plus `shares × final treasury / deposits` of its nation's treasury, only into a token account owned by the member's wallet. Rounding dust goes to operations, so the vault ends at exactly zero once everyone has claimed and operations are withdrawn.

## Compute

Measured on the local MagicBlock stack and on devnet (program `J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n`, MagicBlock `devnet-as`). Each season has 6 nations, 13–15 members and 180 ticks.

| Operation | CU | Transactions |
|---|---|---|
| Genesis step (`work = 50`) | sized to fit one transaction | ~21 in total |
| One tick, average over a season | 0.84–0.96M of 1.4M | 1 |
| One tick, peak | 1.35M | 1 |
| 20 × 4 KiB layout, local | 0.88M average, 1.10M peak | 1 |
| 20 × 4 KiB layout, devnet (an outside agent playing) | 1.11M average per tick in total; 37 of 180 ticks split, every part under 1.4M | 1–3 |
| A tick too heavy for one transaction | parts of 0.45–0.9M each | 2–7 |
| `LogTickInput` | small | 1–2 per tick (inputs of 3.5 KB on average, 6.3 KB at most) |
| All on-chain roots vs a native replay | identical | — |

Most of a part's cost is fixed: decoding the world, re-encoding it and hashing it take roughly 0.4–0.45M CU late in a season (the cheapest parts measured). Splitting a tick therefore costs more in total than resolving it in one transaction, and the crank splits only as far as it has to.

The earlier optimisations still apply:

- SHA-256 goes through the `sol_sha256` syscall.
- Tile indexing is arithmetic.
- Territory and yield scans only look at the neighbourhood.
- The world is encoded into one buffer and copied once.

## Trust model (what the chain guarantees)

- **Rules.** The program enforces every rule, with the same crate and no operator override. Governance counts too: elections, recalls, the consent needed for war and for large treasury spending, and adoption credit.
- **Data availability.** No tick can resolve unless its complete input has been published on chain first. The replay verifier (`permutation-server --bin verify`) re-reads `PS_GENESIS`, `PS_SEAT`, `PS_OPEN`, `PS_INPUT` and `PS_TICK` from the base layer's and the ER's transaction logs. It checks every root and recomputes every payout, so the gateway is only an index. Tampering with the index makes verification fail.
- **Money.** Fees and deposits go into a vault that only the program controls. The program computes payouts from the final world, and each member claims with their own wallet signature. A relayer may pay the fee but cannot redirect the funds.
- **Fog** is enforced off chain. The world accounts are readable on the ER, so hidden information is hidden only by the clients. The game server serves each nation its fogged view, and the AI members, the reference agents and the web client decide only from it. MagicBlock PER (TEE) is the upgrade path.
- **Decisions.** Each officer's batch binds the decision to the observation root the server published before the tick (`obs_root`), a policy name and a salted rationale, and a later batch of the same office reveals it. This proves what was claimed and when, not that the claim is true.
- **Timing.** Batches are plaintext until the input freezes, so a late submitter can react to others; sealed orders are on the roadmap. Whoever sends the first `LogTickInput` fixes the slot and time in the tick randomness; MagicBlock VRF is the planned source.
- **Operator.** The operator (the crank) can delay genesis steps, seating, delegation and commits. It cannot change outcomes. Publishing a tick's input and resolving it after the deadline are permissionless.
