# The w6-s7 verifier input under the merged verifier (before / after)

A copy of `frontier-node/.local/frontier/w6-s7/verify/input.json.gz` (177,765 transactions, 7dcacdf program `072b1205…a98b`), judged by the verifier of `frontier/m1-integ` at `6512cda` (`frontier-verify --fixture … --program-hash 072b1205…`; `frontier-stack tamper` over a copied run directory). The live `w6-s7` run directory was not written.

| | 7dcacdf verifier (the run's own report) | merged verifier |
|---|---|---|
| verdict | FAIL | FAIL |
| fail codes | CampMismatch ×2 ((0,-3) @578, (4,2) @584) | **ArrivalAfterEnd ×9** (the 9 DEPARTs arriving at 1008–1011 ≥ `end_bell` 1008; the new V5 invariant, correct for this pre-U1 input) |
| MissingData (unverifiable) | 9 | 9 (the same 9 marches) |
| ValidSealUnrevealed (warn) | 27 | **0** |
| unrevealed_by_rule | 9 (no reason) | 36: shielded-own 27, bounced 9 |
| tamper | 29/29 detected; base FAIL (CampMismatch) | **30/30** detected (T24 new); base FAIL (ArrivalAfterEnd), 43.7 s |

`verify.md` and `tamper.md` are the merged verifier's reports.
