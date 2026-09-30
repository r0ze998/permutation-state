# integ-w6t-rv-racer: the settle racer at 20×

A focused check of the settle racer's race (integ-W6t review, `integ-W6t-review-NOTES.md` §4): 300 bots with 3 of each persona (eager personas: the play is under a game day), 12 game hours at 20×, test key, the adversary schedule. Binaries built at 15:51 from the source committed as `de9bb3d`.

```
frontier-stack up --mode accel --beacon test-key --scale 20 --game-hours 12 --bots 300 --personas 3 \
    --run-id integ-w6t-rv-racer --base-port 41200 --adversary
frontier-stack verify|report|down --run-id integ-w6t-rv-racer
```

Run record (JST): start 2026-09-30 15:51:43 (load 5.81 5.32 5.60), up exit 0 16:42:04 (load 4.57 4.84 4.95); verify 0 (PASS); report 1; down 0. Per-bell load p50 5.14, max 6.65. Two earlier attempts were stopped before the racer's march: one at `6b9f078` (100 bots, 1 per persona: the racer's only ticket lost), one at `33f77cc` (the race started in the arrival bell and a `RateLimited` try ended it).

**The racer: observed.** `depart@relay:HostInTransit` 1 after 59 `NotResident` and 24 `RateLimited` tries (the relay's simulation refused them; nothing sent, nothing charged), no violation. Before this change no 20× run had observed it (`w6-s7`, R5, the 3-day rehearsal: pending).

Other personas: two verdicts read `needs-chain` for tries that never reached their check (forger: 2 forged settles `TooEarly`; late_revealer: 2 late tries `CommitMismatch`, the host already on a new march). Both are left out of the verdicts since `629d007`; criterion 5 is n.a. in this report for them (the bots' verdicts are computed by the bots build that ran).

**Not a criterion-3 run.** Round → anchor p99 232 and S → first cache p99 223 slots over 1,552 anchors: 113 anchors are in bells 10–13 and 16–18, the no-landing windows of slots 1112–1252 and 1420–1552, where the `ticket` hold and the three `ticket_holder` persona holds (0.5 each, the keepers' D cap) fill the block (O-M1-29's mechanism, with three holders instead of one). Criteria 1, 2, 4, 8, 9 pass; verify PASS. The full report is `report.md`.
