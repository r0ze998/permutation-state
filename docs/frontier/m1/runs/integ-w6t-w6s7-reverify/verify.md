# Frontier verifier report: **FAIL**

- Season 7 of program `GS8ULJMRSgBqVLHxBkDFo6DR1Bdja2g2X4CrUvopa515`, slots 6–77749, 1034 bells, 33142 program accounts
- Input: frontier-stack run w6-s7 (archive, 20.0x, 1000 bots) over http://127.0.0.1:41010
- Checks run: V1, V2, V3, V4, V5, V6, V7, V8, V9, V11, V12, V13, V10
- Transactions 177765 (37042 failed); seals 1706, reveals 1656, bad seals 20, clashes 17553, skips 10740, tickets 1004, explores 1080
- Liveness: 0 valid seals unrevealed and routed (36 more unrevealed by rule: shielded-own 27, shielded-dest 0, path 0, arrival-bell 0, bounced 9), 0 reveals near the close, largest anchor delay 1240.0 s, 433 contested province-bells

| Severity | Check | Code | Entity | Bell | Detail |
|---|---|---|---|---|---|
| fail | V5 | `ArrivalAfterEnd` | host 864220434399240 | 1002 | a landed DEPART arriving at bell 1010, at or after end_bell 1008 |
| fail | V5 | `ArrivalAfterEnd` | host 2210022666797056 | 1004 | a landed DEPART arriving at bell 1008, at or after end_bell 1008 |
| fail | V5 | `ArrivalAfterEnd` | host 532167922810884 | 1004 | a landed DEPART arriving at bell 1008, at or after end_bell 1008 |
| fail | V5 | `ArrivalAfterEnd` | host 1342507992481796 | 1005 | a landed DEPART arriving at bell 1008, at or after end_bell 1008 |
| fail | V5 | `ArrivalAfterEnd` | host 1612987852914693 | 1006 | a landed DEPART arriving at bell 1009, at or after end_bell 1008 |
| fail | V5 | `ArrivalAfterEnd` | host 2448616690024453 | 1006 | a landed DEPART arriving at bell 1010, at or after end_bell 1008 |
| fail | V5 | `ArrivalAfterEnd` | host 1452459155259400 | 1006 | a landed DEPART arriving at bell 1010, at or after end_bell 1008 |
| fail | V5 | `ArrivalAfterEnd` | host 428813829799938 | 1007 | a landed DEPART arriving at bell 1011, at or after end_bell 1008 |
| fail | V5 | `ArrivalAfterEnd` | host 1781213131964417 | 1007 | a landed DEPART arriving at bell 1011, at or after end_bell 1008 |
| unverifiable | V5 | `MissingData` | host 428813829799938 | 1011 | an unsettled march with no verified signature of T(arrive) in the archive |
| unverifiable | V5 | `MissingData` | host 532167922810884 | 1008 | an unsettled march with no verified signature of T(arrive) in the archive |
| unverifiable | V5 | `MissingData` | host 1452459155259400 | 1010 | an unsettled march with no verified signature of T(arrive) in the archive |
| unverifiable | V5 | `MissingData` | host 1342507992481796 | 1008 | an unsettled march with no verified signature of T(arrive) in the archive |
| unverifiable | V5 | `MissingData` | host 1612987852914693 | 1009 | an unsettled march with no verified signature of T(arrive) in the archive |
| unverifiable | V5 | `MissingData` | host 864220434399240 | 1010 | an unsettled march with no verified signature of T(arrive) in the archive |
| unverifiable | V5 | `MissingData` | host 2448616690024453 | 1010 | an unsettled march with no verified signature of T(arrive) in the archive |
| unverifiable | V5 | `MissingData` | host 2210022666797056 | 1008 | an unsettled march with no verified signature of T(arrive) in the archive |
| unverifiable | V5 | `MissingData` | host 1781213131964417 | 1011 | an unsettled march with no verified signature of T(arrive) in the archive |
| warn | V9 | `PrefundedAddress` | accounts | 0 | 1 accounts hold more than their rent (pre-funded or escrow; informational) |
