# m1x-onboarding: E7 scripted onboarding (JA, EN) and the 24-game-hour spectator memory check

`permutation-gateway/screens/live/run-onboarding.sh --base-port 41700 --run-id m1x-onboarding --spectator` on `frontier/m1-integ` at the exit commit `1e5701b` (fresh 20× stack, test key, 2 game days, 20 bots, herald 41740; base 41700 because 41000–41099 is the paused `m1-exit` stack, which the contract names for this run: its chain is paused and must stay so). 2026-10-01 02:31:00–03:57:12 JST (5,172 s), concurrent with Gate W7 part A and the latency line. Exit 0.

- `ok 1 - onboarding on the local stack @ 390×844 in JA` and `ok 2 - … in EN`: welcome, join, site, build, scout, one catch-up nudge, march, report, verify-in-browser.
- `ok 1 - spectator open 24 game hours …`: 74 samples, bells 23 → 168 (24.2 game hours); retained JS heap 3.09 → 3.49 MB (growth 0.40 MB; limit 200 MB), DOM nodes 175 → 262, listeners 46 flat.
- The runner stopped its stack (no 417xx listener afterwards). Shots in `permutation-gateway/screens/artifacts/live/` (not committed).

```
ports ok: localnet 41710, localnet-ws 41711, drand-replay 41720, relay-operator 41730, relay-public 41733, herald 41740, keeper-a 41750, keeper-b 41751, bots 41770, viewers 41775
stack m1x-onboarding up: herald http://127.0.0.1:41740 (2 game day(s) at 20x)
TAP version 13
# [1.6 s] ja step welcome
# [1.9 s] ja step join
# [4.3 s] ja step site
# [71.9 s] ja step build
# [73.8 s] ja step scout
# [103.5 s] nudge: Muster enabled (a final holding, the province caught up)
# [169.1 s] ja step march
# [228.6 s] ja step report
# [336.2 s] ja step verify
# Subtest: onboarding on the local stack @ 390×844 in JA
ok 1 - onboarding on the local stack @ 390×844 in JA
  ---
  duration_ms: 336471.745084
  ...
# [336.7 s] en step welcome
# [337.0 s] en step join
# [341.4 s] en step site
# [407.4 s] en step build
# [409.3 s] en step scout
# [437.0 s] nudge: Muster enabled (a final holding, the province caught up)
# [504.7 s] en step march
# [564.3 s] en step report
# [667.6 s] en step verify
# Subtest: onboarding on the local stack @ 390×844 in EN
ok 2 - onboarding on the local stack @ 390×844 in EN
  ---
  duration_ms: 331438.132333
  ...
1..2
# tests 2
# suites 0
# pass 2
# fail 0
# cancelled 0
# skipped 0
# todo 0
# duration_ms 668163.914625
TAP version 13
# [0 s] bell 23 heap 3.09 MB nodes 175 listeners 46 chronicle 10
# [60 s] bell 25 heap 3.16 MB nodes 178 listeners 46 chronicle 11
# [120 s] bell 27 heap 3.21 MB nodes 178 listeners 46 chronicle 11
# [180 s] bell 29 heap 3.23 MB nodes 178 listeners 46 chronicle 11
# [240 s] bell 31 heap 3.29 MB nodes 178 listeners 46 chronicle 11
# [300 s] bell 33 heap 3.31 MB nodes 178 listeners 46 chronicle 11
# [360 s] bell 35 heap 3.31 MB nodes 178 listeners 46 chronicle 11
# [420 s] bell 37 heap 3.31 MB nodes 178 listeners 46 chronicle 11
# [480 s] bell 39 heap 3.32 MB nodes 178 listeners 46 chronicle 11
# [540 s] bell 41 heap 3.37 MB nodes 178 listeners 46 chronicle 11
# [600 s] bell 43 heap 3.38 MB nodes 178 listeners 46 chronicle 11
# [660 s] bell 45 heap 3.39 MB nodes 178 listeners 46 chronicle 11
# [720 s] bell 47 heap 3.39 MB nodes 178 listeners 46 chronicle 11
# [780 s] bell 49 heap 3.39 MB nodes 178 listeners 46 chronicle 11
# [840 s] bell 51 heap 3.39 MB nodes 178 listeners 46 chronicle 11
# [900 s] bell 53 heap 3.4 MB nodes 178 listeners 46 chronicle 11
# [960 s] bell 55 heap 3.4 MB nodes 178 listeners 46 chronicle 11
# [1020 s] bell 57 heap 3.4 MB nodes 178 listeners 46 chronicle 11
# [1080 s] bell 59 heap 3.4 MB nodes 178 listeners 46 chronicle 11
# [1140 s] bell 61 heap 3.4 MB nodes 182 listeners 46 chronicle 13
# [1203 s] bell 62 heap 3.4 MB nodes 182 listeners 46 chronicle 13
# [1263 s] bell 64 heap 3.41 MB nodes 186 listeners 46 chronicle 14
# [1323 s] bell 66 heap 3.42 MB nodes 186 listeners 46 chronicle 14
# [1383 s] bell 68 heap 3.45 MB nodes 186 listeners 46 chronicle 14
# [1443 s] bell 70 heap 3.45 MB nodes 186 listeners 46 chronicle 14
# [1503 s] bell 72 heap 3.45 MB nodes 186 listeners 46 chronicle 14
# [1563 s] bell 74 heap 3.45 MB nodes 186 listeners 46 chronicle 14
# [1623 s] bell 76 heap 3.45 MB nodes 190 listeners 46 chronicle 15
# [1683 s] bell 78 heap 3.45 MB nodes 190 listeners 46 chronicle 16
# [1743 s] bell 80 heap 3.46 MB nodes 194 listeners 46 chronicle 17
# [1803 s] bell 82 heap 3.46 MB nodes 198 listeners 46 chronicle 18
# [1863 s] bell 84 heap 3.46 MB nodes 198 listeners 46 chronicle 19
# [1923 s] bell 86 heap 3.46 MB nodes 206 listeners 46 chronicle 22
# [1983 s] bell 88 heap 3.46 MB nodes 210 listeners 46 chronicle 23
# [2043 s] bell 90 heap 3.46 MB nodes 214 listeners 46 chronicle 25
# [2103 s] bell 92 heap 3.46 MB nodes 218 listeners 46 chronicle 26
# [2163 s] bell 94 heap 3.47 MB nodes 222 listeners 46 chronicle 28
# [2223 s] bell 96 heap 3.47 MB nodes 226 listeners 46 chronicle 29
# [2283 s] bell 98 heap 3.47 MB nodes 226 listeners 46 chronicle 29
# [2343 s] bell 100 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [2403 s] bell 102 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [2463 s] bell 104 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [2523 s] bell 106 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [2584 s] bell 108 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [2644 s] bell 110 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [2704 s] bell 112 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [2764 s] bell 114 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [2824 s] bell 116 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [2884 s] bell 118 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [2944 s] bell 120 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [3004 s] bell 122 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [3064 s] bell 124 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [3124 s] bell 126 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [3184 s] bell 128 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [3244 s] bell 130 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [3304 s] bell 132 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [3364 s] bell 134 heap 3.48 MB nodes 226 listeners 46 chronicle 29
# [3424 s] bell 136 heap 3.48 MB nodes 230 listeners 46 chronicle 30
# [3484 s] bell 138 heap 3.48 MB nodes 230 listeners 46 chronicle 31
# [3544 s] bell 140 heap 3.48 MB nodes 234 listeners 46 chronicle 32
# [3604 s] bell 142 heap 3.48 MB nodes 234 listeners 46 chronicle 32
# [3664 s] bell 144 heap 3.48 MB nodes 234 listeners 46 chronicle 32
# [3724 s] bell 146 heap 3.48 MB nodes 234 listeners 46 chronicle 32
# [3784 s] bell 148 heap 3.48 MB nodes 234 listeners 46 chronicle 32
# [3844 s] bell 150 heap 3.48 MB nodes 234 listeners 46 chronicle 32
# [3904 s] bell 152 heap 3.48 MB nodes 234 listeners 46 chronicle 32
# [3964 s] bell 154 heap 3.49 MB nodes 238 listeners 46 chronicle 34
# [4024 s] bell 156 heap 3.49 MB nodes 246 listeners 46 chronicle 36
# [4084 s] bell 158 heap 3.49 MB nodes 246 listeners 46 chronicle 37
# [4144 s] bell 160 heap 3.49 MB nodes 254 listeners 46 chronicle 39
# [4204 s] bell 162 heap 3.49 MB nodes 254 listeners 46 chronicle 40
# [4264 s] bell 164 heap 3.49 MB nodes 258 listeners 46 chronicle 41
# [4324 s] bell 166 heap 3.49 MB nodes 258 listeners 46 chronicle 41
# [4384 s] bell 168 heap 3.49 MB nodes 262 listeners 46 chronicle 43
# Subtest: spectator open 24 game hours on the local stack: retained JS heap growth ≤ 200 MB
ok 1 - spectator open 24 game hours on the local stack: retained JS heap growth ≤ 200 MB
  ---
  duration_ms: 4385574.339542
  ...
1..1
# tests 1
# suites 0
# pass 1
# fail 0
# cancelled 0
# skipped 0
# todo 0
# duration_ms 4385858.084209
```
