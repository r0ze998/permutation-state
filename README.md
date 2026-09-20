# PERMUTATION STATE

> **One civilization. Thousands of citizens. One verifiable history.**

PERMUTATION STATE is a seasonal generative civilization RPG. Every player enters the same living civilization as a named citizen, inherits consequences created by earlier citizens, and leaves a verified change for someone else.

The hackathon vertical slice proves one complete **Handoff**:

```text
Mara makes a civic promise
→ Tala remembers it
→ Ivo inherits different repair options
→ Ivo resolves one local Worksite
→ new Mandates enter the still-active civilization
```

![Read-only Observer showing the three-citizen causal chain](permutation-state-prototype/proof/observer-proof.png)

## Run the playable prototype

No install or build step is required.

```bash
cd permutation-state-prototype
python3 -m http.server 4173
```

Open the main experience at:

```text
http://localhost:4173/
```

For the strongest judge view, open:

```text
http://localhost:4173/?preview=scene-b-oath&judge=1
```

For the separate-client evidence view, open:

```text
http://localhost:4173/proof/?proof=1&session=aster-demo&role=observer
```

The Observer links to role-locked Mara, Ivo, and successor clients. Open them in separate tabs to see accepted events propagate through the same deterministic world history.

## What the prototype proves

- One persistent civilization rather than one private kingdom per player.
- A prior citizen's decision changes a later citizen's available actions.
- AI-facing character expression is separated from deterministic game rules.
- Four branch-gated Worksite resolutions preserve an active season and create downstream Mandates.
- A local append-only event log can be replayed, hash-verified, exported, and inspected by a read-only Observer.
- A host-tested Solana program scaffold defines how the same causal receipt can be authorized and committed onchain.

## Honest boundary

The browser experience is a local hackathon prototype. It uses same-origin browser storage and authored NPC dialogue. It does **not** connect a wallet, send a Solana transaction, move USDC, run a live AI model, or settle a season.

The Solana receipt program is tested locally but **not deployed**. A result may only be presented as a Solana receipt after a devnet transaction is confirmed and the Worksite PDA is read back with the expected state root.

## 90-second judge path

1. Show Aster's shared resources and active Season Zero map.
2. As Mara, bind the Grain Oath at the River Guild.
3. Hand control to Ivo and show that Tala remembers Mara's promise.
4. Point out the Service Stair that exists only because of Mara's action.
5. Resolve the East Sluice through the Service Stair.
6. Show the before/after resource deltas, active-season status, and three generated Mandates.
7. End on the Observer: actor, rule, state diff, hashes, and invariant checks are independently visible.

## Repository map

| Path | Purpose |
|---|---|
| [`permutation-state-prototype/`](permutation-state-prototype/) | Playable Living Civic Atlas and multi-client Handoff Proof. |
| [`permutation-state-solana-receipt-spike/`](permutation-state-solana-receipt-spike/) | Native Rust Solana receipt scaffold and JavaScript adapter. |
| [`PERMUTATION_STATE_GAME_CONSTITUTION.md`](PERMUTATION_STATE_GAME_CONSTITUTION.md) | Product promise, game rules, economic principles, and scope boundaries. |
| [`PERMUTATION_STATE_HANDOFF_PROOF.md`](PERMUTATION_STATE_HANDOFF_PROOF.md) | Judge-facing explanation and evidence for the causal handoff. |
| [`PERMUTATION_STATE_QA_REPORT.md`](PERMUTATION_STATE_QA_REPORT.md) | Recorded browser and state-machine verification results. |
| [`PERMUTATION_STATE_EVIDENCE_LEDGER.md`](PERMUTATION_STATE_EVIDENCE_LEDGER.md) | Claim-by-claim evidence status and remaining gates. |
| [`PERMUTATION_STATE_PLAYTEST_KIT.md`](PERMUTATION_STATE_PLAYTEST_KIT.md) | Structured comprehension and playtest protocol. |
| [`solana-ethereum-hackathon-games-2023-2026.xlsx`](solana-ethereum-hackathon-games-2023-2026.xlsx) | Background hackathon benchmark dataset. |

## Verify the receipt scaffold

```bash
cd permutation-state-solana-receipt-spike
cargo test --locked
npm ci --ignore-scripts
npm test
npm audit --audit-level=moderate
```

The repository CI also checks the browser JavaScript syntax and runs both receipt test suites on every push and pull request.

## Design rule

AI may vary expression, memory narration, and proposed intent. It may not decide resource effects, eligibility, victory, settlement, or value movement. Those remain deterministic and inspectable.
