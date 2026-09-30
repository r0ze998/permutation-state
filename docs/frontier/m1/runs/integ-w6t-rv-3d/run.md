# integ-w6t-rv-3d: 3 game days at 20× on the review-fixed tree

The pre-exit rehearsal the integ-W6t review asked for (finding 4): the exit run's scale, population and environment for 3 game days, with the join window unscaled (no `--season-end-at-play-end`: the season keeps the 7-day `end_bell` 1,008 and `join_close_bell` 756; the bots' 3-day mix deals every join by bell 324, so all 1,000 joined). Tree `frontier/m1-integ` at `d66fa99` (binaries built from it; the bots' verdict-only change `6b9f078` came after). Release `.so` `d85e1bd7…2281` pinned, real rounds from the G0 archive.

```
frontier-stack up --mode accel --beacon archive --scale 20 --days 3 --bots 1000 \
    --run-id integ-w6t-rv-3d --base-port 41300 --chaos --adversary --viewers 5000 --viewer-window-hours 24 \
    --chaos-force herald:2 --chaos-force herald:7 \
    --expect-so-sha256 d85e1bd74e29dc361925306839f4ea3bd10b709302e9e6cd9ee2b517aa3f2281
frontier-stack verify|tamper|report|down --run-id integ-w6t-rv-3d
```

Run record (JST; load average 1/5/15 min):

```
start      2026-09-30 11:28:17  load 5.22 4.79 4.56
up exit 0  2026-09-30 15:18:54  load 6.23 5.44 5.62   (432 play bells + 26 drain)
verify 0 (PASS, 47,129 txs; read 1.42 s, verify 2.43 s), tamper 0 (30/30), report 1 (criterion 3), down 0
```

Per-bell load average p50 4.76, p99 7.94, max 10.03 (bell 94). Other work on the machine meanwhile: the paused `w6-s7` stack; this pass's W5 smoke (41600), scripted onboarding (41700) and six nightlies (41500), one at a time. A first attempt at `dcfece9` (`slots-below` at 1,500) was stopped at bell ≈ 130: it re-armed `slots-below` 12 times over bells 40–88 without a defence claim (kept as `.local/frontier/integ-w6t-rv-3d-aborted-1500`; `integ-W6t-review-NOTES.md` §2).

## Results

| check | result |
|---|---|
| row E | **exit-grade**: all 9 holds fired, the pin, real rounds. `slots-below` at 1,900 on a sealed march (bell 41, slots 3200–3387, 1 write inside) opened a claim; `defence-pool` held it at bell 43 (open claims 1); no re-arm was needed |
| criterion 1 | pass (the run stops before `end_bell`): 538 DEPART, 538 settled once, 0 unsettled due, 0 stuck province-bells, 2,018 ClashInputs closable, 0 blocked |
| criterion 2 | pass (every kind within its budget) |
| criterion 3 | **fail on round → anchor p99 4 slots** (target 2; p50 1, max 153, n 7,312). S → first cache p99 1, anchor → last reveal p99 1 (n 524), S → resolve p99 3 meet their targets. **Idle province-days over 6: 0** (62 idle days, p99 6, max 6; resident 2, active 2, churned 414 reported; 1,259 nudges served and 1,863 resident actions attributed) |
| criterion 4 | pass: 0 ValidSealUnrevealed; by rule: bounced 2 |
| criterion 5 | **n.a.**: no persona violated; `settle_racer` pending (see below) |
| criterion 6 | **pass** (in-run window, 5,000 viewers, 24 game hours, 4 herald kills incl. the two forced): ingest → WS p99 **0.72 s** (herald share p99 0.69 s, delivery p99 0.035 s; 24.9 M messages), error rate 0 of 3.45 M, WS coverage 100 % outside 4 outage windows, 16,000 stale retries / 4,000 reconnects all inside them; file p99 9.2 ms (answered only 10.2 ms; 1.53 M 404s) |
| criterion 8 | pass: 3 garbage and 4 bad-plaintext transits due, each settled bad-seal (codes 2 and 5) |
| criterion 9 | pass |
| verify / tamper | PASS / 30 of 30 |
| keeper A status | answered every bell; reveal effective N 150, floor 215,076,060; anchor and seed latency p99 1 |
| failed transactions | 531 of 47,129: redundancy 416 (PostBeacon `AlreadyDone` 304, Reveal A/B 38, SettleTransit A/B 32), expected 81, waste 28 (bot policy), unclassified 6 |
| no-landing windows | one, slots 876–1027 (bells 9–11): the stacked `ticket` holds of every 20× run (O-M1-29) |

**Round → anchor.** Recomputed per anchor (my script over the verify input, the report's reference points): 65 anchors more than 2 slots late: bells 8 and 10 (32; the stacked-hold stall above), bell 93 (16; it coincides with the machine's load peak, 10.03 at bell 94, and no hold or kill), bell 148 (16; the above-cap `keeper-payers` hold) and one at bell 196 (the `lag` hold). None follows a chaos kill of localnet, drand-replay or a keeper. These are fixed counts per run: over 7 days (16,128 anchors) the same windows are ≈ 0.4 %, below the p99 (as in `w6-s7`: round → anchor max 154, p99 6 from the keeper causes U2 removed). O-M1-29 is unchanged.

**Criterion 5.** Every persona but one is observed or exercised (the merged report of 3 fleet lifetimes): late_revealer, forger, spammer, zero_tip observed; min_tip 5 Departs, garbage_seal 1, bad_plaintext 4 (8 `BadPlaintext` refusals of its Reveal), squatter 10, double_arrival 2, self_tip 1, prefunder 2 prefunds, ticket_holder 4 holds. `settle_racer` (5 bots, 2 Departs) never tried its re-depart: its window is the few slots between the destination's resolve of the arrival bell (≈ 60–100 game seconds into the next bell) and the keepers' SettleTransit, and a bot's duties run 0–20 s into each bell. Fixed after this run (`frontier_bots::fleet::settle_racer_polls`: a racer in its race polls every 8 game seconds); see `integ-W6t-review-NOTES.md` §4 and the 20× racer check `runs/integ-w6t-rv-racer/`.

The full report is `report.md`; the tamper table `tamper.md`.
