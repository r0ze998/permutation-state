# WP11 VRF spike report (2026-09-27) — saved by the main session from the spike agent's hand-back

Verdict: Plan A works as specified (ER 11/11 answered with DEFAULT_EPHEMERAL_QUEUE and a non-delegated payer; base 6/6 with DEFAULT_QUEUE; callback data `[tag] ‖ randomness[32] ‖ args`, accounts `[scoped identity (s, ro), callback accounts...]`; identity binding enforced by the VRF program (InvalidSeeds on mismatch); devnet ER exposes the Instructions sysvar (A3 PASS); an ER request naming the base queue is refused).

Numbers: ER latency 2 slots (~245 ms from send), fee 0, callback CU 342k-375k at high priority; base 2-3 slots (1.4-2.4 s), fee 800,000 lamports high / 500,000 regular, callback CU ~367k. Unanswered requests are purged after 120 s; a full queue refuses with AccountDataTooSmall; MagicBlock can pause a queue (QueuePaused).

Recommendations (adopted as amendments A18-A22):
1. Callbacks (ConsumeTickRandomness, ConsumeSeasonSeed) arrive as CPI at stack height 2 inside the oracle's transaction: exempt from the alone rule and any stack-height-1 check; authenticate by scoped identity (signer + key) and expected account only.
2. Take the VRF CPI out of the freezing transaction: FreezeTick only freezes and sets PENDING; a permissionless RequestTickRandomness (the Retry instruction, also allowed when rand_requests == 0) makes the CPI; crank sends both in one tx and falls back to FreezeTick alone then retries. Same for StartSeason → Seeding + RetrySeasonSeed.
3. Bind the queue to the play mode stored on chain: delegated season → VRF_QUEUE_ER only; non-delegated → VRF_QUEUE_BASE only (replaces A1's "either queue").
4. Document: anyone can fill the shared free ER queue (~70-90 requests, 120 s TTL), forcing our requests to fail and eventually the visible fallback (verifier ✗). Owner options: keep D2 fallback and disclose; no fallback; flag rand_requests > 1.
5. Use high-priority scoped requests (tag 11).
6. Timers fine (VRF_RETRY 10 s, SEED_RETRY 60 s); a retry files a new request.
7. Optional: verifier checks the VRF proof (229 B in the fulfilment tx) against the oracle key in 8BKQ….
8. Ops: sol_remaining_compute_units not enabled on devnet; public devnet RPC timed out on getAccountInfo and 429'd bursts — use a dedicated RPC.

Addresses: VRF program Vrf1RNUjXmQGjmQrQLvJHs9SNkvDJEsRVFPkfSQUwGz; DEFAULT_QUEUE Cuj97ggrhhidhbu39TijNVqE74xvKJ69gDervRUXAxGh; DEFAULT_EPHEMERAL_QUEUE 5hBR571xnXppuCPveTrctfTU7tJLSN94nq7kv7FRK5Tc; oracle vrfkfM4uoisXZQPrFiS2brY4oMkU9EWjyvmvqaFd5AS; oracle data 8BKQLGQNYynn8vU8HPjyAnouBvKCq1G3g6wPzJ6iiLVX.

Cost: net 0.00798812 devnet SOL; throwaway program YXhbXJHx… closed; 2.992 SOL returned to the deployer. J4aZxe3y… untouched. Evidence: evidence.jsonl and *.log in this folder.
