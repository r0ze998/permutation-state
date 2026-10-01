# Season replay: people and battles (for the M1 chat's replay page)

`frontier/ui-shell` adds `web/frontier/people/replay.mjs` (UI plan F3). The replay page
(`replay/main.mjs` on `frontier/demo-replay`) is not on this branch, so the wiring is a patch:
`replay-people.patch` (apply on `frontier/demo-replay` after merging `frontier/ui-shell`:
`git apply docs/frontier/ui-shell/replay-people.patch`).

What it does, from data the replay already reads (no new requests besides the roster, one small cached file per ring):
- holders' names on the map, only of owners founded by the bell shown (`roster.ownerOf(..., bell)`);
- when the playhead enters a bell with clashes, its battles play on the provinces in view at tile detail —
  arrivals from the envelope's ClashInputs, and (new in `people/battle.mjs`) fights among hosts already in
  the province, from the province before and after (`residentScene`).

Checked in a throwaway worktree (frontier/demo-replay + merge of ui-shell + the patch, served on 41042 against
the 41040 herald, season 7): bell 568, province −3,2 plays its battle with the lord tags.

Notes for the replay:
- A merge of ui-shell into demo-replay was clean; one English string had to agree (`第{0}輪がひらいた` → "Ring {0} opens", fixed on ui-shell).
- The per-bell "clashes" count in the replay counts every resolved ClashInputs; in season 7 most have
  `resolveSummary.engagements === 0` (nothing fought: e.g. bells 565–567 at −3,0). Counting only
  `engagements > 0` would match what plays.
