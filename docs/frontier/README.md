# The Sixfold Frontier: design and M0 records

The open-world redesign of PERMUTATION STATE (owner decisions of 2026-09-27). Paths written as `(session scratch)/…` point to lab files from the working session; they are not in the repository.

- [DESIGN.md](DESIGN.md): the design, revision 3.1 (English), with the M1 amendments in place (§21 wave 1, §22 wave 6).
- [SUMMARY.ja.md](SUMMARY.ja.md): the owner's summary, in Japanese.
- [DECISIONS.md](DECISIONS.md): the decisions log (owner decisions after revision 3.1, N1–N5, D23, the M1 integration decisions I-01…I-58 and the open owner questions O-M1-01…24), kept current through M1.
- [m1/M1-CONTRACT.md](m1/M1-CONTRACT.md): the M1 "First Bell" implementation contract (v1.9); `m1/*-NOTES.md` are the unit reports.
- [m1/RUN-A-KEEPER.md](m1/RUN-A-KEEPER.md): how to run a keeper (roles, bidding, payers, configuration, monitoring) on the M1 local stack, and what a public network still needs.
- [m1/PLAYTEST-RUNBOOK.md](m1/PLAYTEST-RUNBOOK.md): the private devnet playtest's configuration and order of steps — **not approved, not run** (O-M1-18).
- [m1/c4-v3/](m1/c4-v3/): the final C4 model v3 with the measured Reveal CU and `L(reveal)` (scripts and output; tables in DESIGN §22).
- [m0/M0-FINAL.ja.md](m0/M0-FINAL.ja.md) and [m0/M0-FINAL.md](m0/M0-FINAL.md): M0 status; [m0/M0-CLOSE.md](m0/M0-CLOSE.md): how each remaining M0 item was closed in M1's first week and a half. Also in m0/: the first-pass report, simulator results and spike results.
- research/: Eternum, other on-chain MMOs, the current game, and the earlier scale study.
- audit-v8/: the chain audit of the v8 game (Japanese) and the MagicBlock VRF devnet spike.

Code: the rules v10 kernels are in `permutation-rules/src/frontier/`, and the balance simulator is in `frontier-sim/`.
