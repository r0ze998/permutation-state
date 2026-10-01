# Wylls — Solana Receipt Spike

This is a local, non-deployed scaffold for the smallest credible Solana devnet proof behind the current multi-client demo. It does **not** touch devnet, use a wallet, or modify the prototype.

## What the program canonizes

One PDA stores the authoritative state for one `season_id + worksite_id`. It contains:

- the published `ruleset_hash`;
- the assigned Envoy, Maker, and Successor wallet addresses;
- the current sequence, branch, Worksite resolution, compact resources, and Mandate queue;
- the current state root and previous accepted event hash;
- immutable-in-this-program `season_active`, `settlement_not_started`, and `claim_unavailable` flags.

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

## Instructions

### `Initialize`

Accounts:

1. authority/payer — signer, writable;
2. Worksite PDA — writable;
3. System Program.

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

There is deliberately no settlement or claim instruction in this spike.

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

The Observer must only call this a Solana receipt after the signature is confirmed and the read-back account root equals `newStateRoot`.

## Local verification

```sh
cargo test
npm ci --ignore-scripts
npm run test:client
npm audit --audit-level=moderate
```

Rust tests cover the three-citizen cooperative path, all four branch-gated resolutions, authorization, stale/ruleset/root rejection, signer enforcement, persistence, and the no-claim invariants. JavaScript tests cover identical transition commitments, account metadata, instruction bytes, and the Explorer receipt shape.

The checked lockfile pins `@solana/web3.js` 1.99.0 and secure transitive overrides used by this narrow adapter. The verified install reported zero known npm vulnerabilities. Re-run the audit before any deployment rather than treating this snapshot as a security guarantee.

## Exact devnet gate

This machine currently has Rust and Node but not the Solana CLI or Anchor. Nothing was deployed. To cross the gate later:

1. install the Solana CLI;
2. build with `cargo build-sbf`;
3. deploy this program to devnet with a dedicated hackathon keypair;
4. put the resulting Program ID in the web adapter;
5. connect three wallet addresses and initialize one fresh Worksite PDA;
6. submit one Mara event and one Ivo event;
7. confirm both transactions, read back the PDA, and render the real signatures/Explorer links in Observer;
8. rerun the four-path program tests before claiming parity.

No Anchor dependency is required for this spike. That keeps the proof surface small; Anchor can be added later if IDL generation and wallet-client ergonomics become more valuable than the extra abstraction.
