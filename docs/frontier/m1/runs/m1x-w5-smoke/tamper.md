# Tamper suite

Base run: **PASS**. Required classes detected: **yes**.

| class | on | variant | expected | status | FAIL codes |
|---|---|---|---|---|---|
| T1 | run | drop the last Depart | ChainGap/HeadMismatch | detected | ChainGap, ClashReplayMismatch, HeadMismatch, HoldingReplayMismatch, OriginValueMismatch, QuotaSetMismatch, RevealCommitMismatch, TransitOutcomeMismatch, VerdictDisagreesWithTlock |
| T2 | run | flip a Reveal plaintext byte | RevealCommitMismatch | detected | RevealCommitMismatch |
| T3 | run | shift an unused anchor's A | SeedRoundRule | detected | SeedRoundRule |
| T4 | run | inject a second anchor | DuplicateAnchor | detected | DuplicateAnchor |
| T5 | run | swap a cache signature | SeedRoundRule/BeaconSigInvalid | detected | BeaconSigInvalid |
| T6 | run | set_account on a Province | HeadMismatch/ClashReplayMismatch | detected | HeadMismatch |
| T7 | fixture march-synth | valid seal settled as bad | VerdictDisagreesWithTlock | detected | VerdictDisagreesWithTlock |
| T8 | run | fill slot (no displacement) | QuotaSetMismatch | detected | QuotaSetMismatch |
| T9 | fixture march-synth | departure mass | TransitMassMismatch | detected | TransitMassMismatch |
| T10 | run | wrong drand key | BeaconSigInvalid | detected | BeaconSigInvalid |
| T11 | run | wrong ruleset hash | RulesetMismatch | detected | RulesetMismatch |
| T12 | run | truncate the last game day | HeadMismatch/ChainGap | detected | ChainGap, HeadMismatch |
| T13 | run | Reveal forced past A + W | RevealAfterClose | detected | RevealAfterClose |
| T14 | run | duplicate a FOLD | DuplicateEvent | detected | DuplicateEvent |
| T15 | fixture march-synth | origin values | OriginValueMismatch | detected | OriginValueMismatch |
| T16 | run | wrong genesis round | GenesisSeedRule | detected | GenesisSeedRule, RingSeedRule |
| T17 | run | skip over an arrival | SkipNotQuiet/SkipOverArrival | detected | ClashReplayMismatch, SkipOverArrival |
| T18 | run | SETTLE score | TicketScoreMismatch | detected | TicketScoreMismatch |
| T19 | run | terrain digest | TerrainMismatch | detected | TerrainMismatch |
| T20 | run | claim injected | DefenceRefundMismatch | detected | DefenceRefundMismatch |
| T21 | run | explore find | ExploreRollMismatch | detected | ExploreRollMismatch |
| T22 | run | bad seal logged as surviving | BadSealSurvived | detected | BadSealSurvived, VerdictDisagreesWithTlock |
| T24 | run | DEPART arriving at end_bell | ArrivalAfterEnd | detected | ArrivalAfterEnd |
| T1b (extra) | run | drop an unchained transaction | ChainGap | detected | ChainGap |
| T6b (extra) | run | rogue write between resolves | ClashReplayMismatch | detected | ClashReplayMismatch |
| T23 (extra) | run | forged clash write-back | ClashReplayMismatch | detected | ClashReplayMismatch |
| T23b (extra) | run | forged skip write-back | ClashReplayMismatch | detected | ClashReplayMismatch |
| H1 (extra) | run | forged Harvest | HoldingReplayMismatch | detected | HoldingReplayMismatch |
| H1b (extra) | run | Holding forged at a non-owner write | HoldingReplayMismatch | detected | HoldingReplayMismatch |
| V9a (extra) | run | non-canonical address | NonCanonicalAddress | detected | NonCanonicalAddress |

27 classes judged on the run, 3 on a committed fixture (the run could not express them).
