# Tamper suite

Base run: **FAIL** (ArrivalAfterEnd). Required classes detected: **no**.

| class | on | variant | expected | status | FAIL codes |
|---|---|---|---|---|---|
| T1 | run | drop the last Depart | ChainGap/HeadMismatch | detected | ArrivalAfterEnd, ChainGap, ClashReplayMismatch, HeadMismatch, OriginValueMismatch |
| T2 | run | flip a Reveal plaintext byte | RevealCommitMismatch | detected | ArrivalAfterEnd, RevealCommitMismatch |
| T3 | run | shift an unused anchor's A | SeedRoundRule | detected | ArrivalAfterEnd, SeedRoundRule |
| T4 | run | inject a second anchor | DuplicateAnchor | detected | ArrivalAfterEnd, DuplicateAnchor |
| T5 | run | swap a cache signature | SeedRoundRule/BeaconSigInvalid | detected | ArrivalAfterEnd, BeaconSigInvalid |
| T6 | run | set_account on a Province | HeadMismatch/ClashReplayMismatch | detected | ArrivalAfterEnd, HeadMismatch |
| T7 | run | valid seal settled as bad | VerdictDisagreesWithTlock | detected | ArrivalAfterEnd, VerdictDisagreesWithTlock |
| T8 | run | displacement slot | QuotaSetMismatch | detected | ArrivalAfterEnd, QuotaSetMismatch |
| T9 | run | departure mass | TransitMassMismatch | detected | ArrivalAfterEnd, TransitMassMismatch |
| T10 | run | wrong drand key | BeaconSigInvalid | detected | ArrivalAfterEnd, BeaconSigInvalid |
| T11 | run | wrong ruleset hash | RulesetMismatch | detected | ArrivalAfterEnd, RulesetMismatch |
| T12 | run | truncate the last game day | HeadMismatch/ChainGap | detected | ArrivalAfterEnd, ChainGap, HeadMismatch |
| T13 | run | Reveal Clock past A + W | RevealAfterClose | detected | ArrivalAfterEnd, RevealAfterClose |
| T14 | run | duplicate a FOLD | DuplicateEvent | detected | ArrivalAfterEnd, DuplicateEvent |
| T15 | run | origin values | OriginValueMismatch | detected | ArrivalAfterEnd, OriginValueMismatch, TransitOutcomeMismatch |
| T16 | run | wrong genesis round | GenesisSeedRule | detected | ArrivalAfterEnd, GenesisSeedRule, RingSeedRule |
| T17 | run | skip over an arrival | SkipNotQuiet/SkipOverArrival | detected | ArrivalAfterEnd, ClashReplayMismatch, SkipOverArrival |
| T18 | run | SETTLE score | TicketScoreMismatch | detected | ArrivalAfterEnd, TicketScoreMismatch |
| T19 | run | terrain digest | TerrainMismatch | detected | ArrivalAfterEnd, TerrainMismatch |
| T20 | run | claim injected | DefenceRefundMismatch | detected | ArrivalAfterEnd, DefenceRefundMismatch |
| T21 | run | explore find | ExploreRollMismatch | detected | ArrivalAfterEnd, ExploreRollMismatch |
| T22 | run | bad seal logged as surviving | BadSealSurvived | detected | ArrivalAfterEnd, BadSealSurvived, VerdictDisagreesWithTlock |
| T24 | run | DEPART arriving at end_bell | ArrivalAfterEnd | detected | ArrivalAfterEnd |
| T1b (extra) | run | drop an unchained transaction | ChainGap | detected | ArrivalAfterEnd, ChainGap |
| T6b (extra) | run | rogue write between resolves | ClashReplayMismatch | detected | ArrivalAfterEnd, ClashReplayMismatch |
| T23 (extra) | run | forged clash write-back | ClashReplayMismatch | detected | ArrivalAfterEnd, ClashReplayMismatch |
| T23b (extra) | run | forged skip write-back | ClashReplayMismatch | detected | ArrivalAfterEnd, ClashReplayMismatch |
| H1 (extra) | run | forged Harvest | HoldingReplayMismatch | detected | ArrivalAfterEnd, HoldingReplayMismatch |
| H1b (extra) | run | Holding forged at a non-owner write | HoldingReplayMismatch | detected | ArrivalAfterEnd, HoldingReplayMismatch |
| V9a (extra) | run | non-canonical address | NonCanonicalAddress | detected | ArrivalAfterEnd, NonCanonicalAddress |

30 classes judged on the run, 0 on a committed fixture (the run could not express them).
