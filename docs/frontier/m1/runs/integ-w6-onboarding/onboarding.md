# integ-w6-onboarding: the Gate W6 scripted onboarding line (permutation-gateway/screens/live/run-onboarding.sh, defaults: base 41000, herald 41040, scale 20, JA then EN at 390x844)

Run on frontier/m1-integ b38d3b6, 2026-09-29 09:13-09:26 JST, after the Gate W5 stack lines on the same ports (a fresh stack: the runner starts and stops its own). Exit 0.

```
ports ok: localnet 41010, localnet-ws 41011, drand-replay 41020, relay-operator 41030, relay-public 41033, herald 41040, keeper-a 41050, keeper-b 41051, bots 41070, viewers 41075
stack w6d-onboarding-20260929001311 up: herald http://127.0.0.1:41040
TAP version 13
# [0.8 s] ja step welcome
# [1.2 s] ja step join
# [5.6 s] ja step site
# [71.3 s] ja step build
# [73.1 s] ja step scout
# [102.8 s] nudge: Muster enabled (a final holding, the province caught up)
# [164.4 s] ja step march
# [223.7 s] ja step report
# [335.6 s] ja step verify
# Subtest: onboarding on the local stack @ 390×844 in JA
ok 1 - onboarding on the local stack @ 390×844 in JA
# [336.0 s] en step welcome
# [336.2 s] en step join
# [340.6 s] en step site
# [406.6 s] en step build
# [408.5 s] en step scout
# [436.1 s] nudge: Muster enabled (a final holding, the province caught up)
# [499.7 s] en step march
# [559.0 s] en step report
# [696.5 s] en step verify
# Subtest: onboarding on the local stack @ 390×844 in EN
ok 2 - onboarding on the local stack @ 390×844 in EN
1..2
# tests 2
# suites 0
# pass 2
# fail 0
# cancelled 0
# skipped 0
# todo 0
# duration_ms 696973.641459
```
