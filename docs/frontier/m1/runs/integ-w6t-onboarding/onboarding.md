# integ-w6t-onboarding: the scripted onboarding run (Gate W6 pass condition)

`permutation-gateway/screens/live/run-onboarding.sh --base-port 41700 --run-id integ-w6t-onboarding` on `frontier/m1-integ` at `36386b0` (fresh 20× stack, 1 game day, 20 bots, herald 41740; base 41700 because 41000–41099 is the paused `w6-s7` stack). 06:58:18–07:11:53 JST, load 4.77 → 3.47.

- `ok 1 - onboarding on the local stack @ 390×844 in JA` (336 s): welcome, join, site, build, scout, one catch-up nudge ("Muster enabled"), march, report, verify.
- `ok 2 - onboarding on the local stack @ 390×844 in EN` (361 s): the same steps, one nudge.
- tests 2, pass 2, fail 0; the stack was stopped by the runner (no 417xx listener afterwards).
