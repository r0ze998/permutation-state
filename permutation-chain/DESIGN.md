# permutation-chain — design

The on-chain half of PERMUTATION STATE (Game Design V5). One native Solana program runs the same `permutation-rules` crate as the game server, the AI members and the replay verifier, so a tick resolved on chain is byte-identical to one resolved anywhere else. Registration, the prize vault and the final world live on the Solana base layer; the world and the nations' accounts are delegated to a MagicBlock Ephemeral Rollup (ER) while the season plays.

## Code layout

| File | Contents |
|---|---|
| `instruction.rs` | the instructions and their accounts (the borsh tag is the variant index; never reorder) |
| `state.rs` | account layouts, sizes, and `Chunks` (the world across its chunk accounts) |
| `processor/` | one module per stage: `registration`, `genesis`, `delegation`, `play`, `roster` (`RevealRoster`, `AnchorTalk`), `settlement`; `accounts` holds the shared checks and loaders |
| `finalize.rs` | what `FinishSeason` computes from the final world, as a pure function; the replay verifier calls the same function |
| `error.rs` | error codes (clients map them by `ChainError::ALL`; version 7 adds `RosterPending` and `RosterMismatch`) |
| `token.rs`, `heap.rs` | SPL token calls; the bump allocator for the large world |

Every account an instruction reads is checked for its owner and its PDA address (nation accounts by their stored bump), not by its content alone: world chunks are program-owned too, and their bytes are partly shaped by player input.

## Accounts

| Account | Seeds | Layer | Holds |
|---|---|---|---|
| Season (`PSSEASN7`, 4 KiB) | `["season", id]` | base | Admin, crank, USDC mint, preset, nation count, entry fee, tick length, market flag, status, seeds; member count and members per nation, seated count; pool (80% of fees), operations (20%), treasury deposits and final treasuries per nation; after `FinishSeason`, the payout per member and the final world root; the history layer: the season it follows, that season's history root, and its own history root; operator AI members (version 7): `ai_count`, `roster_chain`, `bounty_each`, `bond`, the roster revealed so far (`roster_acc`, `roster_revealed`), `roster_outcome` (0 none, 1 revealed, 2 forfeited) and the bounty paid per nation |
| Vault | `["vault", id]` | base | SPL token account holding fees, treasury deposits, and the operator's AI bounties and bond; its owner is the Season PDA |
| Member (`PSMEMBR6`) | `["member", id, wallet]` | base | Member index (registration order), nation, wallet (pays and receives), session key (signs orders and governance, cannot move USDC), self-declared kind, optional attestation hash, pre-season candidacy and first-election votes, treasury shares, claimed flag, registration `tag` (32 random bytes, or an operator AI's roster tag) |
| Roster (`PSROSTR1`) | `["roster", id]` | base | Only with operator AI members (at most `MAX_AI` = 64): created by `CreateSeason`; `RevealRoster` appends (member, nation, salt) in roster order |
| World chunks | `["world", id, k]`, k = 0..20 | ER during play | One logical buffer over 20 accounts of 4 KiB (80 KiB; a Blitz world with ~15 members ends near 23 KB): header (magic `PSWORLD5`, body length, `WorldMeta`: season id, preset, nation count, tick length, deadline, finished, market, and the frozen-input state: frozen flag, tick randomness, chunks published) + the borsh `WorldState` (a `GenesisJob` while genesis runs) |
| Nation (`PSNATN07`, 8 KiB) | `["nation", id, civ]` | ER during play | The open tick; its officers and their session keys; each office's spendable budget; each office's sealed commitment and the tick it was made for; the revealed batches (up to 4) with their salts and which offices revealed; the governance inbox (≤ 8 actions per signer per tick); the revealing and frozen flags. Submissions never decode the world |

## Season lifecycle

```
base  CreateSeason ─ AllocWorld ×20 ─ AllocNation ×6   (with AIs: bounties + bond → vault, Roster PDA)
base  Register ×N  (entry fee + optional treasury deposit → vault; wallet-signed, via x402 or directly; with a tag)
base  StartSeason ─ GenesisStep ×~20 (permissionless) ─ SeatMembers ×⌈N/6⌉ ─ OpenGovernment (first election)
base  Delegate the 20 world chunks + 6 nation accounts to the ER
ER    loop per tick:
        SubmitGov (member's session key) … CommitOrders (office holder's key; a vacant office takes none)
        CloseCommits              (after the deadline, anyone; closes commitments and governance; logs PS_COMMITS)
        RevealOrders              (the reveal window; must match the commitment)
        LogTickInput chunk 0..n   (once every commitment is revealed or the window ended; freezes the input, randomness from the salts, PS_SALTS)
        ResolveTick { to } ×1..k  (the phases up to `to`; resumes at the phase cursor)
        AnchorTalk                (the crank: the tick's message root, PS_TALK)
ER    CommitPart ×22 every 20 ticks … UndelegatePart ×22 once the last tick resolved
base  RevealRoster ×⌈AIs/8⌉ (with AIs; anyone holding the salts)
base  FinishSeason (permissionless: every member's payout from the final world; the history record, PS_HISTORY) ─ Claim (each wallet) ─ WithdrawOps (admin)
```

- **Registration.** `Register` moves exactly the entry fee (plus an optional treasury deposit) from the payer's USDC account into the vault and creates the Member PDA. The instruction is the same whether a wallet signs it directly or an agent pays over x402; the x402 facilitator only adds its fee-payer signature. Registration closes at `StartSeason`. There are at most 256 members per season, because the payout table lives in the Season account.
- **Operator AI members (version 7, V5 §18.2–§18.4).** `CreateSeason` takes `ai_count`, `roster_chain`, `bounty_each` and `bond`; with `ai_count` > 0 the admin's USDC account pays `ai_count × bounty_each + bond` into the vault and the Roster PDA is created (with no AIs, the other three must be zero). Every `Register` carries a `tag` (people: random bytes; an AI: `roster_tag(season, wallet, salt)`), which the member's wallet signs. After the last tick, `RevealRoster { salts }` (tag 24) reveals the next AIs in the committed order, one member PDA per salt: each salt must turn that wallet into its registered tag and, once all `ai_count` are in, the revealed tags must chain to `roster_chain`; otherwise `RosterMismatch`. The gateway's crank sends 8 per transaction before `FinishSeason`.
- **Members' messages (version 7, V5 §18.7).** `AnchorTalk { tick, count, root }` (tag 25, ER) logs `PS_TALK ‖ season ‖ tick ‖ count ‖ root`, the Merkle root of the signed messages the gateway relayed for that tick. The season's crank only, checked against the key a nation account records.
- **Seating and the first election.** After genesis, `SeatMembers` adds members to the world in registration order, with their candidacy and first-election votes; each call logs `PS_SEAT ‖ root ‖ borsh(members seated)`. `OpenGovernment` holds the first election, opens tick 0 on the nation accounts, starts the clock and logs `PS_OPEN ‖ root`.
- **Offices and sealed orders (version 6).** `CommitOrders { role, tick, commitment }` must be signed by the office holder's session key; a vacant office takes no batch from anyone (the rules' caretaker fills it, `permutation_rules::gov::caretaker`). `commitment = sha256("permutation-rules/orders" ‖ borsh(OrderBatch) ‖ salt)`. After the tick's deadline anyone may send `CloseCommits`: it opens the reveal window (`state::reveal_seconds`, a sixth of the tick, ≥ 2 s), refuses further commitments and governance for the tick, and logs `PS_COMMITS ‖ tick ‖ borsh(Vec<(civ, office, member, commitment)>)`. `RevealOrders { role, tick, digest, orders, adopt, salt }` (anyone holding the plaintext may send it) must hash to the commitment; the program then checks the non-zero decision digest, the order structure, the office's allowed orders and its spendable budget. `SubmitOrders` is retired (`Retired`). `SubmitGov { member, action }` queues a vote, candidacy, proposal, support or recall. The engine checks the signer against the member's registered key when the tick resolves.
- **Input publication (data availability).** A transaction's logs are capped at 10 KB, and a busy tick's input (every office's batch and every governance action) is bigger than that. So the input is not logged by `ResolveTick`:
  - `LogTickInput { chunk }` rebuilds the pending input from the nation accounts and logs `PS_INPUT ‖ tick ‖ chunk ‖ total ‖ sha256(input) ‖ bytes`, 6,000 bytes per chunk.
  - Chunk 0 is allowed in the reveal window once every commitment was revealed or the window ended. It **freezes** the input: it draws the tick randomness `tick_vrf(world root, revealed salts)` into the world header, logs `PS_SALTS ‖ tick ‖ world root ‖ borsh(salts)`, and marks every nation account frozen, so later reveals fail with `TickFrozen`.
  - Chunks are logged in order. `ResolveTick` fails with `InputNotPublished` until all of them are.
  - `ResolveTick` logs `PS_TICK ‖ tick ‖ to ‖ pre_root ‖ post_root ‖ sha256(input)`.
  - A verifier can therefore rebuild every tick from the ER's logs alone, whatever the size of the input.
- **Split resolution.** `ResolveTick { to }` runs the phases from the engine's `phase_cursor` up to `to`; the tick completes when the cursor wraps. A split tick logs one `PS_TICK` per part, with the same input hash. Parts are needed when a tick exceeds the compute budget or the heap: the program's bump allocator never frees, so running many phases in one transaction can run out of the 256 KiB heap before it runs out of compute. The crank learns, per starting phase, the furthest stop that fit, and probes further every 10 ticks. Stops are 2, 4, 5, 6, 7, 9 and 12.
- **Commits and undelegation (measured on devnet and the local stack).** An intent (one `CommitPart` or `UndelegatePart`) is finalized on the base layer by the ER's committor, and one transaction finalizes every target of the intent. Devnet showed three limits. Each of them left accounts stuck mid-undelegation, with no recovery:
  - **64 account keys per transaction.** A commit of all 14 accounts of the earlier layout needed about 90 keys and was silently dropped ([magicblock-validator#1693](https://github.com/magicblock-labs/magicblock-validator/issues/1693)).
  - **Compute per finalize.** The committor gives the delegation program's finalize 359,700 CU. Three 10 KiB world chunks in one intent exceeded it, and so did one densely written 10 KiB chunk. About 2.5 KB of data fit.
  - **The instruction trace**, for large intents.

  That is why the world is 20 chunks of 4 KiB. The crank sends every commit round and the final undelegation as small intents: the nation accounts in threes and each world chunk alone, with chunk 0 last (its header says whether the season is over). `Commit`/`CommitAndUndelegate` (everything in one intent) remain for local stacks only.
  - MagicBlock sponsors 10 commits per delegated account when the payer is not delegated. The crank commits every 20 ticks and stops at 9, keeping the 10th for the final undelegation. So that nobody else can use those commits up, only the crank may send `CommitPart` during play: the instruction carries a nation account, which records the crank's key, and the program refuses any other signer (`Unauthorized`). `UndelegatePart` stays open to anyone once the season is over, so a season can always be wound up.
- **Why chunks.** Accounts that a program creates or grows through CPI are limited to 10 KiB per step, and the committor's finalize limits above apply per intent. So the world is spread over twenty 4 KiB PDAs, which the program reads and writes as one buffer (`state::Chunks`). Transactions that pass every chunk stay within the size limit: 26 world and nation accounts for a resolve, and 6 members per `SeatMembers`.
- **Genesis is on chain,** split into bounded steps (`map::MapJob`; a symmetric six-nation map takes four or five steps: terrain, connectivity, rivers and resources). Running the steps back to back is exactly `generate`; a test checks equality. The map seed mixes the world seed with the season seed (which also mixes the previous season's history root), so the map exists only once registration closed.
- **The season seed** is the latest slot hash, mixed with the season id, the member count and the treasury deposits, taken when registration closes (`StartSeason`). The slot leader could grind it; MagicBlock VRF is the plan.
- **Settlement.** `FinishSeason` needs every account back on base and the world finished. It computes the V5 payout from the final world (`finalize::finalize`, which calls `permutation_rules::payout::settle_with`) and writes one payout per member into the Season account, together with the final treasuries and the final root. With operator AI members (version 7): if the roster is revealed in full, the Roster PDA is passed after the world chunks; bounties go to the nations that first conquered the AIs' home cities (unpaid ones join the pool), the AIs' payouts go to people, and the bond returns to operations. If not, `FinishSeason` fails with `RosterPending` until `ROSTER_GRACE_SECONDS` (1 hour) after the last tick's deadline; then anyone may finish without the roster, everyone is settled as a person, and the bounties and the bond join the pool. A treasury nobody deposited into (it may hold contract income or market sales) joins the pool instead of staying in the vault with nobody to refund (fixes a version 6 leak). `Claim` pays the member's payout plus `shares × final treasury / deposits` of its nation's treasury, only into a token account owned by the member's wallet. Rounding dust goes to operations, so the vault ends at exactly zero once everyone has claimed and operations are withdrawn.

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
| After the phase optimisations below, local (four seasons) | 0.81–0.85M average, last 30 ticks 0.95–1.06M, peak 0.98–1.14M | 1 |
| After the phase optimisations, devnet (seasons 1790312639004 and 1790317008509, an outside agent playing) | 0.80–0.89M average, peak 1.04–1.23M; no tick split | 1 |
| `LogTickInput` | small | 1–2 per tick (inputs of 3.5 KB on average, 6.3 KB at most) |
| All on-chain roots vs a native replay | identical | — |

Most of a part's cost is fixed: decoding the world, re-encoding it and hashing it take roughly 0.4–0.45M CU late in a season (the cheapest parts measured). Splitting a tick therefore costs more in total than resolving it in one transaction, and the crank splits only as far as it has to.

Where a late tick's compute goes (about 1.0M CU in total, from a `cu-trace` build): decoding the world ~130k, writing it back ~90k, checking the 20 chunk addresses ~64k, reading the input from the nation accounts ~35–47k and reopening them ~27–40k; of the phases, production (6) ~150–200k, society (8) ~55k, intake (0) ~55–80k, standing rules (3) and movement (4) ~40–60k each.

Optimisations, none of which changes a result (the golden seasons in `permutation-server/tests/golden.rs` pin every root):

- SHA-256 goes through the `sol_sha256` syscall.
- Tile indexing is arithmetic, and a neighbourhood costs one `index_of` per column.
- Territory and yield scans only look at the neighbourhood.
- The world is encoded into one buffer and copied once.
- Each phase copies only the orders it handles (`tick::accepted` with a filter), not the whole tick's orders.
- Movement keeps a per-tile index of who stands where instead of scanning every unit and city per step; society indexes garrisons once; production counts cities once; upkeep updates the effective troops as they disband.

To measure: `cargo build-sbf --features cu-trace` logs the compute units left after each step of `ResolveTick` and inside phases 4 and 6 (`permutation_rules::probe`). Run a local season with that build and read the logs of the resolve transactions. Checking the chunk addresses with stored bumps instead of `find_program_address` would save about 35k more, but needs a new world header layout.

## Trust model (what the chain guarantees)

- **Rules.** The program enforces every rule, with the same crate and no operator override. Governance counts too: elections, recalls, the consent needed for war and for large treasury spending, and adoption credit.
- **Data availability.** No tick can resolve unless its complete input has been published on chain first. The replay verifier (`permutation-server --bin verify`) re-reads `PS_GENESIS`, `PS_SEAT`, `PS_OPEN`, `PS_COMMITS`, `PS_SALTS`, `PS_INPUT` and `PS_TICK` from the base layer's and the ER's transaction logs. It checks every root, that every revealed batch matches its commitment, that each tick's randomness follows from the revealed salts, recomputes every payout (with `finalize::finalize`, the function `FinishSeason` runs, including the revealed roster's tags and chain) and the history root, so the gateway is only an index. Tampering with the index makes verification fail.
- **Money.** Fees and deposits go into a vault that only the program controls. The program computes payouts from the final world, and each member claims with their own wallet signature. A relayer may pay the fee but cannot redirect the funds.
- **Information (version 6): perfect information.** The world accounts are readable on the ER, so a fog enforced by clients hid from people what agents reading the chain saw. Every player now sees the full state; only each office's orders for the open tick stay hidden, sealed until the deadline. A fog mode would need a private rollup (MagicBlock PER, TEE) and is not part of this design.
- **Decisions.** Each officer's batch binds the decision to the observation root the server published before the tick (`obs_root`), a policy name and a salted rationale, and a later batch of the same office reveals it. This proves what was claimed and when, not that the claim is true.
- **Timing and randomness (version 6).** Orders are sealed until the deadline, so nobody can react to others' orders within a tick. Reveals are refused after the reveal deadline (`NationAccount.reveal_deadline`), and governance actions may not fill a nation account so far that a reveal no longer fits (`REVEAL_ROOM` per office still to reveal). A hosted member trusts its host (the gateway holds its session key, sees its orders before the deadline and chooses when to reveal them); a member that signs and reveals itself does not. The tick randomness comes from the world root and the revealed salts, which nobody knows in full before the reveals, so whoever sends the transactions cannot choose it (no external VRF is needed). An officer who withholds a reveal to steer it forfeits that office's orders for the tick. Limit: the season seed (at `StartSeason`) still mixes a recent slot hash, which the slot leader could grind.
- **Operator AI members (version 7).** The operator commits the number of its AIs and the chain of their tags before registration, and escrows their bounties and a bond. It cannot name a person as an AI (a person's random tag has no salt), cannot reorder or swap the roster, and forfeits bounties and bond to the pool if it does not reveal in time. It cannot stop itself from playing wallets it leaves off the roster; that limit is disclosed (V5 §18.10). Messages are anchored per tick, not their content's truth; promises bind only through treasury contracts in the rules.
- **Operator.** The operator (the crank) can delay genesis steps, seating, delegation and commits during play. It cannot change outcomes: it has no key for vacant offices (version 5's acting official is gone; the rules' caretaker fills them), it cannot see or choose the map before registration closes, and it cannot choose the randomness. Closing commitments, publishing a tick's input, resolving it and undelegating a finished season are permissionless.
- **History (version 6).** `FinishSeason` writes the season's record (`permutation_rules::history`) and chains its root onto the previous season's (`Season.history_root`, `PS_HISTORY`); a new season may follow a finalized season of the same admin (`CreateSeason.prev_season_id`) and mixes that root into its seed. The record is never used by the rules or payouts.
