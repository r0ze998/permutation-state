# Rebuild acceptance record — 2026-09-21

## Automated

- Civilization simulation: 26 tests, including a 15-minute simulated legal progression without grants.
- Renderer interactions/geometry: 14 tests. Canvas mocks do not substitute for visual inspection.
- Existing transport/client regression: 18 tests.
- Gateway/service regression: 23 tests, including HTTP restart persistence, token isolation and input validation.

All 81 tests passed together. The archived world's 13 regression tests also passed (94 total).
Source syntax and `git diff --check` passed.

## Naming and play-guide follow-up — 2026-09-21

- Replaced the temporary civilization name with 「私たちの文明」 across the active game's UI.
- Added two copy-compatibility tests: old system messages are presented without the retired name,
  while player names and persisted event text remain untouched. All 96 JavaScript tests passed.
- Restarted the integrated gateway on 4173: the same three citizens, four buildings and 22 discovered
  tiles remained. Existing browser authorization resumed without a world reset.
- Inspected the updated title, citizen card, legacy history entries and help dialog in the actual browser.
- Added the Japanese player guide; archived proof routes are not rebranded or advertised as the active game.

## Actual browser play

The following was exercised through the browser UI, not by injecting state:

1. Join `aster`; select the optional food suggestion; inspect alternatives and exact costs.
2. Build a farm. Verify material expenditure, timed construction, a new map structure, worker and food production.
3. Build a lumbermill. Verify its timber rate and delivered stock in the shared economy.
4. Join the same `aster` from another tab as Ivo. Existing farm/mill and resource stock are visible.
5. Ivo builds a quarry. Mara's town panel shows the same quarry under construction, then active.
6. Move Mara to the frontier; start a five-second survey. Previously unknown western terrain appears.
7. Reload and restart the service. The built structures, discovery and authorization survive.
8. Start the integrated server on 4173 using the same saved world; join through `/civilization/`.
9. Inspect the narrow 554px viewport and desktop-width DOM layout; adjust the narrow inspector to a bottom sheet.
10. Check browser error logs after shared construction: no recorded warning/error entries.

## Known limits (not claimed complete)

- NPC decisions are deterministic. Generative dialogue/social intentions are not connected.
- New gameplay runs in the shared local service, not the existing Solana World PDA.
- No combat, real entry fees, market/prize settlement, production identity system or public hosting.
- Milestones are ongoing-play achievements, not implemented season settlement or authored endings.
- Balance remains a prototype: storage is uncapped and established facilities can accumulate large idle stocks.
- Browser capabilities are local prototype authentication, not wallet ownership or anti-Sybil protection.
- Service downtime catches up at most 60 seconds; production while a running server has no active browser continues.

The previously deployed claim of a completed civilization game is withdrawn. This is a new playable
strategic foundation with verified shared state and spatial/economic choices.
