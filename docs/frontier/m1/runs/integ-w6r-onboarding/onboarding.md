# integ-w6r-onboarding: the Gate W6 scripted onboarding line (permutation-gateway/screens/live/run-onboarding.sh, defaults: base 41000, herald 41040, scale 20, JA then EN at 390x844)

Run on frontier/m1-integ 7b30594 (the integ-W6r code fixes: the live run now judges cut-off content, landmarks and the bell chip and whitelists only the two not-yet 404 shapes), 2026-09-29 11:27-11:40 JST, after the Gate W5 stack lines on the same ports (a fresh stack: the runner starts and stops its own). Exit 0 (811 s): JA ok (335 s), EN ok (361 s); problems 0 in both; 404s only /h/bell/N/region/N 12 and /h/province/N,N/N 7 per language; one catch-up tap per language.

```
ports ok: localnet 41010, localnet-ws 41011, drand-replay 41020, relay-operator 41030, relay-public 41033, herald 41040, keeper-a 41050, keeper-b 41051, bots 41070, viewers 41075
stack w6d-onboarding-20260929022703 up: herald http://127.0.0.1:41040 (1 game day(s) at 20x)
TAP version 13
# [0.5 s] ja step welcome
# [0.8 s] ja step join
# [5.2 s] ja step site
# [71.0 s] ja step build
# [72.9 s] ja step scout
# [102.5 s] nudge: Muster enabled (a final holding, the province caught up)
# [164.2 s] ja step march
# [223.6 s] ja step report
# [335.1 s] ja step verify
# Subtest: onboarding on the local stack @ 390×844 in JA
ok 1 - onboarding on the local stack @ 390×844 in JA
  ---
  duration_ms: 335386.547875
  ...
# [335.5 s] en step welcome
# [335.8 s] en step join
# [340.2 s] en step site
# [405.8 s] en step build
# [407.7 s] en step scout
# [437.4 s] nudge: Muster enabled (a final holding, the province caught up)
# [499.0 s] en step march
# [558.3 s] en step report
# [696.5 s] en step verify
# Subtest: onboarding on the local stack @ 390×844 in EN
ok 2 - onboarding on the local stack @ 390×844 in EN
  ---
  duration_ms: 361406.840042
  ...
1..2
# tests 2
# suites 0
# pass 2
# fail 0
# cancelled 0
# skipped 0
# todo 0
# duration_ms 696972.863042
```
