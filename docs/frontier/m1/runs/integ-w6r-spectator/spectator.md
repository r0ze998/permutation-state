# integ-w6r-spectator: run-onboarding.sh --spectator (base 41300, herald 41340, 20x, 2 game days) — the combined onboarding + §13.6 24-game-hour memory check

Run on frontier/m1-integ 7b30594, 2026-09-29 12:05-13:31 JST (5,131 s), concurrent with the Gate W6 latency run on 41000. Exit 0: JA ok, EN ok (problems 0), then the spectator page open from bell 24 to bell 168 (24.01 game hours, 73 samples after a forced GC): retained JS heap 3.09 -> 3.49 MB (growth 0.40 MB, max 3.59 MB; limit 200 MB), DOM nodes 171 -> 262, listeners 46 flat, 0 page or console errors. On a 1-day stack the chain would have paused at bell 170 (the review's margin); with --days 2 it runs to bell 314.

```
ports ok: localnet 41310, localnet-ws 41311, drand-replay 41320, relay-operator 41330, relay-public 41333, herald 41340, keeper-a 41350, keeper-b 41351, bots 41370, viewers 41375
stack integ-w6r-spectator up: herald http://127.0.0.1:41340 (2 game day(s) at 20x)
TAP version 13
# [0.7 s] ja step welcome
# [1.0 s] ja step join
# [5.3 s] ja step site
# [71.1 s] ja step build
# [73.0 s] ja step scout
# [102.7 s] nudge: Muster enabled (a final holding, the province caught up)
# [164.4 s] ja step march
# [223.8 s] ja step report
# [335.3 s] ja step verify
# Subtest: onboarding on the local stack @ 390×844 in JA
ok 1 - onboarding on the local stack @ 390×844 in JA
  ---
  duration_ms: 335554.24375
  ...
# [335.7 s] en step welcome
# [336.0 s] en step join
# [340.3 s] en step site
# [406.0 s] en step build
# [407.9 s] en step scout
# [437.6 s] nudge: Muster enabled (a final holding, the province caught up)
# [499.2 s] en step march
# [558.6 s] en step report
# [696.5 s] en step verify
# Subtest: onboarding on the local stack @ 390×844 in EN
ok 2 - onboarding on the local stack @ 390×844 in EN
  ---
  duration_ms: 361149.402
  ...
1..2
# tests 2
# suites 0
# pass 2
# fail 0
# cancelled 0
# skipped 0
# todo 0
# duration_ms 696889.522
TAP version 13
# [0 s] bell 24 heap 3.09 MB nodes 171 listeners 46 chronicle 9
# [60 s] bell 26 heap 3.25 MB nodes 339 listeners 46 chronicle 10
# [120 s] bell 28 heap 3.28 MB nodes 254 listeners 46 chronicle 10
# [180 s] bell 30 heap 3.29 MB nodes 254 listeners 46 chronicle 10
# [240 s] bell 32 heap 3.35 MB nodes 254 listeners 46 chronicle 10
# [300 s] bell 34 heap 3.59 MB nodes 174 listeners 46 chronicle 10
# [360 s] bell 36 heap 3.37 MB nodes 254 listeners 46 chronicle 10
# [420 s] bell 38 heap 3.38 MB nodes 254 listeners 46 chronicle 10
# [480 s] bell 40 heap 3.32 MB nodes 174 listeners 46 chronicle 10
# [540 s] bell 42 heap 3.46 MB nodes 339 listeners 46 chronicle 10
# [600 s] bell 44 heap 3.44 MB nodes 254 listeners 46 chronicle 10
# [660 s] bell 46 heap 3.45 MB nodes 254 listeners 46 chronicle 10
# [720 s] bell 48 heap 3.45 MB nodes 254 listeners 46 chronicle 10
# [780 s] bell 50 heap 3.4 MB nodes 174 listeners 46 chronicle 10
# [840 s] bell 52 heap 3.4 MB nodes 174 listeners 46 chronicle 10
# [900 s] bell 54 heap 3.4 MB nodes 174 listeners 46 chronicle 10
# [960 s] bell 56 heap 3.41 MB nodes 174 listeners 46 chronicle 10
# [1020 s] bell 58 heap 3.42 MB nodes 174 listeners 46 chronicle 10
# [1080 s] bell 60 heap 3.41 MB nodes 174 listeners 46 chronicle 11
# [1140 s] bell 62 heap 3.41 MB nodes 178 listeners 46 chronicle 12
# [1200 s] bell 64 heap 3.41 MB nodes 182 listeners 46 chronicle 13
# [1260 s] bell 66 heap 3.42 MB nodes 182 listeners 46 chronicle 13
# [1320 s] bell 68 heap 3.44 MB nodes 182 listeners 46 chronicle 13
# [1380 s] bell 70 heap 3.5 MB nodes 182 listeners 46 chronicle 13
# [1440 s] bell 72 heap 3.46 MB nodes 182 listeners 46 chronicle 13
# [1500 s] bell 74 heap 3.46 MB nodes 182 listeners 46 chronicle 13
# [1560 s] bell 76 heap 3.46 MB nodes 182 listeners 46 chronicle 14
# [1620 s] bell 78 heap 3.5 MB nodes 186 listeners 46 chronicle 15
# [1680 s] bell 80 heap 3.46 MB nodes 190 listeners 46 chronicle 16
# [1740 s] bell 82 heap 3.46 MB nodes 190 listeners 46 chronicle 17
# [1800 s] bell 84 heap 3.54 MB nodes 399 listeners 46 chronicle 18
# [1860 s] bell 86 heap 3.51 MB nodes 202 listeners 46 chronicle 21
# [1920 s] bell 88 heap 3.52 MB nodes 202 listeners 46 chronicle 22
# [1980 s] bell 90 heap 3.46 MB nodes 210 listeners 46 chronicle 24
# [2040 s] bell 92 heap 3.47 MB nodes 214 listeners 46 chronicle 25
# [2100 s] bell 94 heap 3.47 MB nodes 218 listeners 46 chronicle 27
# [2161 s] bell 96 heap 3.51 MB nodes 218 listeners 46 chronicle 28
# [2221 s] bell 98 heap 3.54 MB nodes 350 listeners 46 chronicle 28
# [2281 s] bell 100 heap 3.47 MB nodes 222 listeners 46 chronicle 28
# [2341 s] bell 102 heap 3.48 MB nodes 222 listeners 46 chronicle 28
# [2401 s] bell 104 heap 3.48 MB nodes 222 listeners 46 chronicle 28
# [2461 s] bell 106 heap 3.48 MB nodes 222 listeners 46 chronicle 28
# [2521 s] bell 108 heap 3.48 MB nodes 222 listeners 46 chronicle 28
# [2581 s] bell 110 heap 3.55 MB nodes 350 listeners 46 chronicle 28
# [2641 s] bell 112 heap 3.48 MB nodes 222 listeners 46 chronicle 28
# [2701 s] bell 114 heap 3.48 MB nodes 222 listeners 46 chronicle 28
# [2761 s] bell 116 heap 3.56 MB nodes 350 listeners 46 chronicle 28
# [2821 s] bell 118 heap 3.52 MB nodes 222 listeners 46 chronicle 28
# [2881 s] bell 120 heap 3.48 MB nodes 222 listeners 46 chronicle 28
# [2941 s] bell 122 heap 3.48 MB nodes 222 listeners 46 chronicle 28
# [3001 s] bell 124 heap 3.54 MB nodes 350 listeners 46 chronicle 28
# [3061 s] bell 126 heap 3.48 MB nodes 222 listeners 46 chronicle 28
# [3121 s] bell 128 heap 3.56 MB nodes 350 listeners 46 chronicle 28
# [3181 s] bell 130 heap 3.55 MB nodes 350 listeners 46 chronicle 28
# [3241 s] bell 132 heap 3.48 MB nodes 222 listeners 46 chronicle 28
# [3301 s] bell 134 heap 3.48 MB nodes 222 listeners 46 chronicle 28
# [3361 s] bell 136 heap 3.56 MB nodes 354 listeners 46 chronicle 29
# [3421 s] bell 138 heap 3.48 MB nodes 226 listeners 46 chronicle 30
# [3481 s] bell 140 heap 3.48 MB nodes 230 listeners 46 chronicle 31
# [3541 s] bell 142 heap 3.55 MB nodes 366 listeners 46 chronicle 31
# [3601 s] bell 144 heap 3.55 MB nodes 366 listeners 46 chronicle 31
# [3661 s] bell 146 heap 3.48 MB nodes 230 listeners 46 chronicle 31
# [3721 s] bell 148 heap 3.54 MB nodes 230 listeners 46 chronicle 31
# [3781 s] bell 150 heap 3.56 MB nodes 366 listeners 46 chronicle 31
# [3841 s] bell 152 heap 3.48 MB nodes 230 listeners 46 chronicle 31
# [3901 s] bell 154 heap 3.48 MB nodes 230 listeners 46 chronicle 33
# [3961 s] bell 156 heap 3.55 MB nodes 234 listeners 46 chronicle 35
# [4021 s] bell 158 heap 3.57 MB nodes 391 listeners 46 chronicle 36
# [4081 s] bell 160 heap 3.49 MB nodes 250 listeners 46 chronicle 39
# [4141 s] bell 162 heap 3.49 MB nodes 254 listeners 46 chronicle 40
# [4201 s] bell 164 heap 3.49 MB nodes 258 listeners 46 chronicle 41
# [4261 s] bell 166 heap 3.57 MB nodes 422 listeners 46 chronicle 41
# [4321 s] bell 168 heap 3.49 MB nodes 262 listeners 46 chronicle 42
# Subtest: spectator open 24 game hours on the local stack: retained JS heap growth ≤ 200 MB
ok 1 - spectator open 24 game hours on the local stack: retained JS heap growth ≤ 200 MB
  ---
  duration_ms: 4321729.8765
  ...
1..1
# tests 1
# suites 0
# pass 1
# fail 0
# cancelled 0
# skipped 0
# todo 0
# duration_ms 4321937.04625
```
