# PERMUTATION STATE — Multi-Citizen Handoff Proof

**Build:** `handoff-proof-2026-09-21`  
**Result:** **PASS — 10/10 automated browser scenarios**  
**Boundary:** **LOCAL MULTI-CLIENT PROOF · NOT SOLANA · NOT LIVE AI**

![Read-only Observer view after the third citizen accepts generated work](permutation-state-prototype/proof/observer-proof.png)

## What this artifact proves

The original Living Civic Atlas explains the game. This companion artifact proves its smallest novel mechanic:

> A stranger's accepted action changes what another stranger can legally do next.

Four browser clients share one local Aster session:

1. **Mara / Player A** chooses a Grain Oath or invokes the Founding Charter.
2. **Tala's remembered state** changes access, cooperation, debt, and the legal action set.
3. **Ivo / Player B** receives only the two actions unlocked by Mara's branch and resolves Worksite 31.
4. **Nia / Player C** can accept one of the three Mandates created by Ivo's result.
5. **Observer** remains read-only and displays the ordered events, state diffs, hashes, invariants, and client-head agreement.

Every Worksite path leaves `Season Zero = ACTIVE`, `Outcome = UNRESOLVED`, `Purse = ACCUMULATING`, `Settlement = NOT_STARTED`, and `Claim = UNAVAILABLE`.

## Open the proof

Serve `permutation-state-prototype/` locally, then open:

```text
http://localhost:4173/proof/?proof=1&session=aster-demo&role=observer
```

From the Observer view, open Mara, Ivo, and Nia in separate tabs. The `session` value is the shared room identifier; changing it creates an isolated local world.

Recommended judge path:

1. Keep all four role URLs visible.
2. On Ivo, show that action is blocked before Mara commits.
3. On Mara, choose **Bind the Grain Oath**.
4. Return to Ivo without reloading. Point to the causal ribbon:

   `MARA PROMISED 12 FOOD → TALA KEPT HER ENGINEERS → SERVICE STAIR UNLOCKED FOR YOU`

5. Choose **Repair through the Service Stair**.
6. On Nia, accept one generated Mandate.
7. On Observer, show three accepted events, matching client heads, the resource diffs, all passing invariants, and export `session-proof.json`.

The cooperative route is the clearest 90-second story. The other three routes remain available for technical Q&A.

## What is enforced

- Roles are fixed by separate URLs: Mara, Ivo, successor, and read-only Observer.
- The stored source of truth is an append-only event log; current state is rebuilt by replay.
- The reducer rejects an actor acting out of order, an action outside the inherited branch, a foreign Mandate, a duplicate command, and a stale or competing head.
- A SHA-256 genesis hash binds the session seed. Every accepted event binds its previous event hash and before/after state hashes.
- Any modified past payload fails verification and halts new commits rather than being silently repaired.
- Reloaded and late-joining clients reconstruct the same state from the event log.
- `BroadcastChannel` accelerates live notification. `localStorage` remains the local source of truth and recovers state if the notification path is unavailable.
- The JSON export contains the seed, full event log, final state, hashes, disclosure, and invariant results needed for independent replay.

## Verification evidence

The real browser suite ran at 1280 × 720 with four separate pages per session.

| Run | Path or injected failure | Result |
|---|---|---|
| 1 | Oath → Service Stair → `M-032`; wrong-branch and foreign-Mandate rejection; history tamper | PASS |
| 2 | Oath → Old Millrace → `E-020` | PASS |
| 3 | Charter → Cut Through → `E-021` | PASS |
| 4 | Charter → Reconcile → `M-033` | PASS |
| 5 | Ivo tries to act before Mara, is rejected, then completes `R31-1` | PASS |
| 6 | Mara sends the same command twice; exactly one is accepted | PASS |
| 7 | Ivo reloads after Mara; replay restores the inherited branch | PASS |
| 8 | Observer joins after Ivo; the complete chain is reconstructed | PASS |
| 9 | `BroadcastChannel` is disabled; `localStorage` propagation completes `R31-1` | PASS |
| 10 | Two competing Ivo commands race; exactly one wins, with no cross-session leakage | PASS |

Every run additionally passed:

- three accepted events in correct order;
- the exact branch resources, Memory Receipt, resolution, and three downstream Mandates;
- identical final-state and head-event hashes in all four clients;
- 100 repeated deterministic replays with one final hash;
- export-and-replay equality;
- no runtime, console, or browser-log errors;
- no desktop horizontal overflow;
- no season settlement or claim side effect.

Machine-readable details: [`qa-proof-result.json`](permutation-state-prototype/proof/qa-proof-result.json).

## What this does not prove

- It is not a Solana transaction, validator guarantee, devnet receipt, or wallet-authenticated multiplayer session.
- The clients share one browser origin on one device. This is a causal-state proof before network integration.
- Tala's dialogue is authored. It demonstrates the proposed AI boundary but is not generated by a live model.
- The hashes are local integrity evidence, not signatures, identity, anti-sybil protection, or consensus.
- No real USDC moves. No Worksite action can trigger settlement or a claim.

## The next proof gate

Port this exact event contract to Solana devnet without changing the demo story:

1. wallet-authenticate the actor for each accepted event;
2. publish the ruleset hash and prior state root;
3. submit one accepted transition and resulting state root;
4. display the devnet signature and Explorer link in the same Observer receipt;
5. preserve the active-season and no-claim invariants in program tests.

That next gate converts the current statement from “the causal handoff works across clients” to “Solana canonized the accepted handoff under published rules.”

The local, non-deployed implementation contract for that gate now lives in [`permutation-state-solana-receipt-spike/`](permutation-state-solana-receipt-spike/README.md). Its native-Rust tests pass 4/4 and its JavaScript adapter tests pass 3/3. It remains explicitly unverified on Solana until an SBF build is deployed, a transaction is confirmed, and the Worksite PDA is read back with the expected root.

## Why this is the right slice

Entropy's AI-native game architecture emphasizes persistent character memory, action alignment, shared narrative state, and open-ended play inside designer-defined guardrails. That supports the division used here: AI may express remembered social state, while a fixed resolver owns legal actions and state transitions. See [Building the Infrastructure for AI-Native Games](https://entropyai.co/research/building-the-next-generation-of-games/).

[Realms / Eternum](https://eternum-docs.realms.world/) remains the reference for an open-source seasonal world. PERMUTATION STATE's proposed wedge is narrower and more personal: one citizen's local RPG result rewrites the next citizen's affordances inside the same civilization.

For later onchain scale, [Honeycomb Protocol's documentation](https://docs.honeycombprotocol.com/) is relevant to compressed and regular Solana state. It is a candidate implementation dependency, not part of this local build and not evidence that the current proof is onchain.
