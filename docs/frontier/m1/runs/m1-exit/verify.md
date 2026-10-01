# Frontier verifier report: **PASS**

- Season 7 of program `9U2LmkyRveBwApBMRrx5SjBMwPFTdciS2xPaL3J1YHb7`, slots 7–77751, 1034 bells, 36084 program accounts
- Input: frontier-stack run m1-exit (archive, 20.0x, 1000 bots) over http://127.0.0.1:41010
- Checks run: V1, V2, V3, V4, V5, V6, V7, V8, V9, V11, V12, V13, V10
- Transactions 144300 (784 failed); seals 1712, reveals 1694, bad seals 24, clashes 21674, skips 9156, tickets 1006, explores 1087
- Liveness: 0 valid seals unrevealed and routed (9 more unrevealed by rule: shielded-own 0, shielded-dest 0, path 0, arrival-bell 0, bounced 9), 0 reveals near the close, largest anchor delay 1239.0 s, 510 contested province-bells

| Severity | Check | Code | Entity | Bell | Detail |
|---|---|---|---|---|---|
| warn | V9 | `PrefundedAddress` | accounts | 0 | 5 accounts hold more than their rent (pre-funded or escrow; informational) |
