# Permutation State — Playable MagicBlock Slice

> **Current game:** the active map-first civilization game is at `/civilization/`.
> See the [project README](../README.md) for its current status and the chain-independent startup path.
> The onchain models and receipts documented below belong to earlier experiments. They do not prove
> that the new civilization simulation is onchain. New-game integration is still in progress.

This package is the Solana program, MagicBlock transport, and local gateway behind PERMUTATION STATE. It includes a canonical Season PDA with a mock-USDC ledger, the earlier causal-proof Worksite PDA, and a new eight-citizen spatial World PDA with verified MagicBlock ER-to-Solana checkpoints.

The verified target is **localnet**, using disposable demo signers and no real funds. Nothing is deployed to public devnet or mainnet. The gateway holds those disposable signers for the hackathon slice; production wallet approval and player Session Keys are not implemented. Role URLs are presentation and turn-taking aids, not authentication: the HTTP caller is not yet bound to a player wallet.

The program follows MagicBlock's current Native Rust lifecycle without BOLT:

```text
Base Layer: Initialize / credit canonical Season PDA
Base Layer: Initialize Worksite PDA
Base Layer: Delegate Worksite PDA
ER:         ApplyWorksiteEvent ... (low-latency shared play)
ER:         Commit checkpoint(s)
ER:         CommitAndUndelegate
Base Layer: fixed-discriminator Undelegate callback restores the PDA
```

The World path uses the same lifecycle with a separate `['world', world_id]` PDA:

```text
Base Layer: Initialize World PDA -> Delegate World PDA
ER:         Join -> Move -> Gather -> Deposit -> Repair ...
ER:         Commit checkpoint(s) / CommitAndUndelegate
Base Layer: verify the same World state root and event-chain head
```

## Play it locally

Prerequisites: Node.js, Rust 1.89, Solana/Agave CLI 3.1.9 with `cargo-build-sbf`, and `@magicblock-labs/ephemeral-validator` 0.14.10 available on `PATH`.

From this directory:

```sh
npm ci
cargo build-sbf
```

Start the official local stack in one terminal:

```sh
npm run start:stack:local
```

The launcher refuses to run if a local validator is already reachable, verifies the SBF artifact hash and required executables before deleting anything, and resets only this package's ignored `magicblock-test-storage/`, generated gateway session receipts, and dedicated local ledger. Receipts are coupled to that ledger and cannot survive a world reset honestly. Stop the existing stack before intentionally starting a fresh world.

Start the game gateway in another:

```sh
npm start
```

Open the primary shared simulation:

```text
http://127.0.0.1:4173/civilization/
```

This is the new offchain civilization prototype. The earlier `/world/` route remains an archived
walking-and-watergate experiment. Neither simulation is automatically synchronized with the compact
World PDA merely because it is served by this gateway.

The localnet gateway creates three disposable keypairs under the ignored workspace `work/` directory when missing and funds them only from the local validator. It never auto-creates or funds devnet credentials.

Open the shared Observer view:

```text
http://127.0.0.1:4173/proof/?proof=1&session=aster-demo&role=observer&transport=magicblock
```

Use the Observer links to open Mara, Ivo, and Nia in separate tabs. All clients poll the same delegated Worksite PDA. Run the automated live path while both services are up with:

```sh
npm run test:e2e:local
```

That test creates a fresh session, executes `oath → service → M-032`, requires a verified base-layer checkpoint after Ivo, and finishes only if sequence, state roots, exact event-chain heads, and the accepted Mandate agree. This is a finite three-action vertical slice; ongoing post-Nia play and the full season loop remain future work.

Run the new spatial World PDA path with:

```sh
npm run test:e2e:world:local
```

It initializes and delegates a fresh World PDA, executes 15 signer-authorized events—join, movement, timber and stone gathering, physical deposit, and four repair stages—then commits and verifies the identical completed state on the local Solana base layer.

The Program ID is intentionally **not compiled into the Rust program**. PDA derivation uses the deployed program address passed by the runtime; the checked local deployment config pins the currently built artifact's Program ID.

## What the program canonizes

The World PDA canonizes up to eight citizens, their fixed-point positions and timber/stone inventories, remaining resource-node supply, shared warehouse stock, four East Sluice repair stages, completion, and water-production rate (stored in hundredths per world minute: `70` = `0.70`, `480` = `4.80`). Every action includes the expected sequence, prior state root, event-chain head, and a unique nonzero event ID. The program derives the next root itself and rejects remote resource interaction, excess inventory, out-of-bounds movement, unauthorized actors, stale heads, and broken resource conservation.

One Worksite PDA stores the authoritative play state for one `season_id + worksite_id`. It contains:

- the published `ruleset_hash`;
- the assigned Envoy, Maker, and Successor wallet addresses;
- the current sequence, branch, Worksite resolution, compact resources, and Mandate queue;
- the current state root and previous accepted event hash;
- immutable-in-this-program `season_active`, `settlement_not_started`, and `claim_unavailable` flags.

A separate Season PDA stores the active season's committed payout rules, append-only ledger head, registered Worksite/citizen counts, and mock entry/marketplace allocations. The local gateway seeds two transparent demo credits on a fresh Season: `10.00` mock USDC entry (`7.00` purse / `3.00` operations) and `100.00` marketplace volume (`1.50` purse / `1.00` operations / `97.50` seller). These are integer micro-units recorded by real local Solana transactions; no SPL token or real funds move.

Every accepted `ApplyWorksiteEvent` must include the exact prior root, previous event hash, ruleset hash, predicted new root, event sequence, action, and the local proof's SHA-256 `eventHash`. The program:

1. requires the wallet assigned to the current role to sign;
2. rejects stale heads and wrong rulesets;
3. applies the same four branch-gated R31 resolutions as `proof/core.js`;
4. computes the new state root itself and rejects a mismatch;
5. rechecks active-season/no-settlement/no-claim invariants;
6. stores the new root and emits a binary `PERMSTATE_EVENT_V1` receipt in transaction logs.

The local browser event hash is a binding field, not the chain state root. The onchain root uses a fixed-width Borsh-compatible state view so it can be reproduced identically in Rust and JavaScript without depending on JSON formatting.

## Account schema

`WorksiteState` PDA seeds:

```text
["worksite", season_id[32], worksite_id[32]]
```

The account is fixed at 384 bytes for v1. The current serialized payload is smaller; the remainder is zero-filled for modest schema evolution.

`SeasonState` is a separate 384-byte PDA derived from `['season', season_id]`. Its 383-byte payload includes the payout-rule commitment, active/finalized status, ledger sequence and hash-chain head, all mock-unit allocation totals, and the three roots that remain zero until finalization. `WorksiteState` stores that Season PDA, and its state root binds the relationship.

`WorldState` is a 512-byte PDA derived from:

```text
["world", world_id[32]]
```

Its v1 payload is 503 bytes. Rust and JavaScript share a fixed genesis fixture for the PDA, state root, and event-chain head.

## Instruction wire contract

The ordinary instructions are a Borsh enum. Existing wire tags remain unchanged and the ER lifecycle variants are appended:

| Tag | Instruction | Payload |
| ---: | --- | --- |
| `0` | `Initialize` | `InitializeArgs` |
| `1` | `ApplyWorksiteEvent` | `ApplyEventArgs` |
| `2` | `Delegate` | none |
| `3` | `Commit` | none |
| `4` | `CommitAndUndelegate` | none |
| `5` | `InitializeSeason` | season/rules/payout-rule commitments and expected genesis root |
| `6` | `CreditSeasonPurse` | source, gross mock units, expected ledger head, event id |
| `7` | `FinalizeSeason` | expected head plus outcome, chronicle, and claim roots |
| `8` | `ClaimSeason` | mock amount, Merkle proof, expected ledger head, event id |
| `9` | `InitializeWorld` | world/ruleset commitments and expected genesis root |
| `10` | `JoinWorld` | guarded event head |
| `11` | `MoveWorldActor` | destination plus guarded event head |
| `12` | `GatherWorldResource` | resource kind plus guarded event head |
| `13` | `DepositWorldInventory` | guarded event head |
| `14` | `RepairEastSluice` | guarded event head |
| `15` | `DelegateWorld` | none |
| `16` | `CommitWorld` | none |
| `17` | `CommitAndUndelegateWorld` | none |

The base-layer `Undelegate` callback is deliberately outside that enum. MagicBlock invokes the exact eight-byte discriminator `[196, 28, 41, 206, 48, 37, 51, 167]`, followed by a Borsh `Vec<Vec<u8>>` containing exactly:

```text
["worksite", season_id[32], worksite_id[32]]
```

The callback verifies those seeds against the finalized state in the canonical delegation buffer before recreating the PDA.

## Instructions and accounts

### `Initialize`

Accounts:

1. authority/payer — signer, writable;
2. Worksite PDA — writable;
3. System Program;
4. Season PDA — writable.

Data: season ID, Worksite ID, ruleset hash, three role wallet addresses, and the expected genesis state root. The program creates the PDA and checks the caller's predicted root.

### `ApplyWorksiteEvent`

Accounts:

1. current role wallet — signer;
2. Worksite PDA — writable.

Data:

```text
event_kind
action
expected_seq
ruleset_hash
prior_state_root
expected_new_state_root
expected_prev_event_hash
client_event_hash
```

The playable route deliberately keeps the Season active and never calls finalization or claim. Tags 7-8 exist to prove the lifecycle guardrails: outcome/chronicle/claim roots remain zero while active, and a claim cannot be created before a separately verified season boundary. All amounts are non-transferable mock ledger units.

### `Delegate` — Base Layer

Accounts, in order:

1. Worksite authority/payer — signer, writable;
2. System Program;
3. Worksite PDA — writable;
4. this owner program;
5. delegation buffer — writable;
6. delegation record — writable;
7. delegation metadata — writable;
8. MagicBlock Delegation Program;
9. optional ER validator identity.

Only the Worksite authority can delegate. The program uses a 30-second automatic commit interval and passes the optional validator selected by the client to the SDK.

### `Commit` — ER

Accounts, in order:

1. assigned Worksite participant/payer — signer, writable;
2. delegated Worksite PDA — writable;
3. Magic Program;
4. Magic Context — writable.

The authority, Envoy, Maker, or Successor may request a checkpoint. This schedules an ER-to-base-layer state commit without changing game state.

### `CommitAndUndelegate` — ER

Accounts are identical to `Commit`, but the signer must be the Worksite authority. Ending the low-latency session is authority-only because it affects every connected participant.

### `Undelegate` callback — Base Layer CPI

Accounts, in order:

1. Worksite PDA — writable;
2. canonical undelegation buffer — signer, writable;
3. callback payer — writable;
4. System Program.

The SDK additionally verifies that the buffer is signed, owned by the Delegation Program, and canonical for the Worksite PDA. The program validates the finalized Worksite identity and exact seed material before calling the SDK restore operation.

## MagicBlock JavaScript transport

[`client/magicblock-transport.mjs`](./client/magicblock-transport.mjs) is the transport boundary intended for the local gateway and, through a browser bundler, a wallet-connected client. It exports:

- connection factories for Magic Router, a direct Ephemeral Rollup endpoint, and Solana;
- Worksite, Season, Claim, and MagicBlock delegation PDA derivation;
- builders and senders for all nine program instructions, including Season initialization, credits, finalization, and claims;
- fixed-width `WorksiteState` and `SeasonState` decoding with recomputed state-root checks;
- WebSocket account subscription and bounded polling;
- ER execution metadata that explicitly remains unsettled;
- base commitment-signature resolution followed by mandatory Solana PDA root read-back.

The expected gateway flow is:

```text
bootstrap: Initialize on Solana -> Delegate Worksite PDA
action:    preview transition -> submit through Magic Router -> decode ER PDA
commit:    schedule checkpoint -> confirm base signature -> decode Solana PDA -> compare root + event head
```

The gateway persists Ivo's accepted ER event before attempting its separate checkpoint. If the checkpoint fails, later play is held, the UI offers a retry, and the retry schedules a fresh same-root commit rather than looping forever on a dead scheduling transaction.

Use Magic Router as the submission endpoint. Use the selected validator's direct ER connection when calling `resolveSolanaCheckpoint`; MagicBlock's official `GetCommitmentSignature` helper reads the ER scheduling transaction and returns the corresponding base-layer commitment signature.

The sender accepts either existing Node `Signer` objects or an injected browser wallet implementing `signTransaction`. It never generates, persists, exports, or logs key material.

[`client/world-magicblock-transport.mjs`](./client/world-magicblock-transport.mjs) adds the corresponding World PDA derivation, fixed 503-byte decoder, Rust-compatible state-root computation, instructions and senders for tags 9–17, MagicBlock lifecycle account derivation, bounded polling, and the published 10× map-coordinate contract.

## Explorer receipt payload

After confirmation, the UI should read the Worksite PDA and display/export:

```json
{
  "schemaVersion": "permutation-state.solana-receipt.v1",
  "network": "devnet",
  "signature": "<confirmed transaction signature>",
  "slot": 0,
  "explorerUrl": "https://explorer.solana.com/tx/<signature>?cluster=devnet",
  "programId": "<program address>",
  "worksitePda": "<state address>",
  "actor": "<signing wallet>",
  "sequence": 1,
  "eventKind": 1,
  "action": 11,
  "rulesetHash": "<32-byte hex>",
  "priorStateRoot": "<32-byte hex>",
  "newStateRoot": "<32-byte hex>",
  "previousEventHash": "<32-byte hex>",
  "eventHash": "<program-computed 32-byte hex>",
  "clientEventHash": "<proof/core.js eventHash>",
  "invariants": {
    "seasonActive": true,
    "settlementNotStarted": true,
    "claimUnavailable": true
  }
}
```

The Observer must only call this a Solana receipt after the signature is confirmed and the **base-layer** read-back account root and event-chain head equal their ER values at or after that transaction slot. The receipt constructor enforces confirmed/finalized status plus slot ordering, so an arbitrary nonempty signature cannot be mislabeled as verified settlement. The transport represents execution and settlement separately:

```json
{
  "layer": "ephemeral-rollup",
  "settlement": "not-yet-proven-on-solana",
  "signature": "<ER transaction signature>"
}
```

```json
{
  "layer": "solana",
  "settlement": "checkpoint-read-back-verified",
  "signature": "<MagicBlock commitment signature on Solana>",
  "sourceErSignature": "<ER scheduling transaction>",
  "stateRoot": "<root read back from the Solana Worksite PDA>"
}
```

## Local verification

```sh
cargo test
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo build-sbf
cargo test-sbf
npm ci --ignore-scripts
npm test
# With the local stack and gateway running:
npm run test:e2e:local
npm run test:e2e:world:local
npm audit --audit-level=moderate
```

Twenty-one Rust tests cover the three-citizen cooperative proof, all four branch-gated resolutions, the World gather/deposit/repair loop, eight-citizen state shape, authorization, proximity, inventory and resource-conservation invariants, stable instruction tags, callback discrimination, exact undelegation seeds, and lifecycle authorization. Thirty-two package JavaScript tests cover Rust/JavaScript commitments, client-callable instructions, World transport, exact account order, decoding, subscription, polling, shared gateway sessions, schema rejection, network identity guards, and the ER-versus-Solana receipt boundary. The separate simulation core has thirteen tests.

`rust-toolchain.toml` pins Rust 1.89.0. `Cargo.toml` pins `ephemeral-rollups-sdk` 0.16.2, the version used by MagicBlock's current Native Rust counter example, together with Solana Program 4.0.0. The lockfile is committed. Upgrade this matrix deliberately and rerun both host and SBF/integration tests; do not accept an unattended dependency refresh.

The checked lockfile pins `@magicblock-labs/ephemeral-rollups-sdk` 0.17.2, `@solana/web3.js` 1.99.0, `@noble/hashes` 1.8.0, and `buffer` 6.0.3. MagicBlock's declared `@solana/web3.js ^1.98.0` range includes the pinned 1.99.0 client. The adapter uses Noble's browser-safe SHA-256 implementation while preserving the original commitment bytes.

`npm audit --audit-level=moderate` passes. The full audit currently reports three low-severity findings inherited through MagicBlock's TEE dependency chain (`@phala/dcap-qvl` -> `elliptic`) and offers no non-breaking fix for the current SDK. Do not force-downgrade the SDK merely to hide that report; reassess it before deployment.

## Verified local gate and remaining public gate

The checked-in implementation passes its host suite and builds a 381,648-byte SBF artifact with Agave/Solana CLI 3.1.9. The following have been verified against the official local MagicBlock stack:

1. initialize a fresh Worksite PDA on the base layer;
2. delegate it through the real Delegation Program;
3. execute Mara, Ivo, and successor events on the ER with role-specific signers;
4. schedule `Commit` after Ivo's local resolution;
5. resolve the base commitment signature and read back the same state root;
6. keep the account delegated so the next citizen can continue immediately;
7. show ER execution and Solana checkpoint as separate evidence in the browser;
8. synchronize the updated action set across Observer, Mara, and Ivo tabs.
9. initialize and delegate an eight-citizen World PDA;
10. execute the 15-event spatial resource-and-repair loop on the ER;
11. checkpoint the completed sluice state and read back the identical root from the local Solana base layer.

Before loading signers or sending transactions, the gateway verifies the actual base-layer genesis hash. Localnet also requires loopback endpoints and rejects all known public Solana genesis hashes; devnet requires the devnet genesis plus the official TLS MagicBlock devnet router and ER hosts. A mislabeled mainnet RPC therefore fails closed.

Still unverified:

- public Solana devnet deployment (faucet requests were rate-limited during this run);
- the final `CommitAndUndelegate` callback path in a live validator run;
- production wallets, Session Keys, remote multiplayer hosting, token entry fees, marketplace settlement, and claims.

Do not relabel a localnet receipt as public devnet evidence. A public submission must use funded disposable devnet credentials, deploy the exact artifact, rerun the live path, and display public Explorer links.

No Anchor dependency is required for this spike. That keeps the proof surface small; Anchor can be added later if IDL generation and wallet-client ergonomics become more valuable than the extra abstraction.

The current artifact may accurately be called **local MagicBlock ER-confirmed with a local Solana base-layer checkpoint**. It must not be called public-devnet-confirmed or production-ready.

Primary sources reviewed:

- [Magic Router documentation](https://docs.magicblock.gg/pages/ephemeral-rollups-ers/introduction/magic-router)
- [Delegation, commitment, and undelegation lifecycle](https://docs.magicblock.gg/pages/ephemeral-rollups-ers/introduction/ephemeral-rollup)
- [MagicBlock Native Rust guide](https://docs.magicblock.gg/pages/ephemeral-rollups-ers/how-to-guide/rust-program)
- [Official Native Rust counter example at the reviewed commit](https://github.com/magicblock-labs/magicblock-engine-examples/tree/e137826af4969d538ef10d8f672a8d77deb6e194/counter/native-rust)
- [Official Web3.js lifecycle test used for account ordering and commitment-signature flow](https://github.com/magicblock-labs/magicblock-engine-examples/blob/e137826af4969d538ef10d8f672a8d77deb6e194/counter/native-rust/tests/web3js/rust-counter.test.ts)
- [`@magicblock-labs/ephemeral-rollups-sdk` 0.17.2 release commit](https://github.com/magicblock-labs/ephemeral-rollups-sdk/commit/57f3291eb1b9949d2327a61796e9016547ed7361)
- [`ephemeral-rollups-sdk` 0.16.2 Rust API](https://docs.rs/ephemeral-rollups-sdk/0.16.2/ephemeral_rollups_sdk/)

BOLT is not used. The maintained path here is Ephemeral Rollups SDK plus Magic Router; Session Keys remain deferred until the state transport works end to end.
