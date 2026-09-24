# UI redesign V4.1 (proposal) — superseded

> **Superseded 2026-09-24.** The standalone mock ([ui-mock-v4.1.html](ui-mock-v4.1.html)) was rejected: it simplified the map and covered it with opaque panels. The current direction is [BENCHMARK_CIV_CRYPTO_UI.md](BENCHMARK_CIV_CRYPTO_UI.md), with a playable prototype of the real client at `/v41/` on the game server. The data needs in §5 below still apply.

Status: draft for review, 2026-09-24. It goes with [Game Design V4.1](../PERMUTATION_STATE_GAME_DESIGN_V4.1.md).

**Mock:** [ui-mock-v4.1.html](ui-mock-v4.1.html). Open it in a browser; the buttons at the top switch between four states: playing, turn ended, tick report, season over.

The layout and map stay as they are: full-bleed map, floating paper panels, the existing `map.mjs` renderer. What changes is what the panels say and how they look.

## 1. Visual design

| Now | V4.1 |
|---|---|
| Text at 8–10px in many places (eyebrows, badges, the minimap footer, the prototype label) | Type scale **11 label · 13 body · 15 emphasis · 20 title · 28 display**. Nothing below 11px |
| Long explanatory paragraphs in the inspector and drawers | One line of guidance at most. Details move to the help or verification drawer |
| Protocol words on screen (digest, obs_root, コミット・リビール) | Plain words: 「判断メモ」, 「🔒 解決後に公開」. The proofs stay in the 「検証」 drawer |
| English eyebrows (YOUR TURN, ARMY) mixed with Japanese | Japanese labels throughout; English kept only in the brand |
| Three small track pills and a separate victory drawer | One **勝ち筋** panel, always visible (§2) |
| Panels with competing weights | Clear hierarchy: the topbar holds the state of the world; left is you vs. the others; right is what you selected; bottom is what you will do |

The palette (paper, deep green, gold, USDC blue) and the civ colours stay as they are. Tokens are defined once in `styles.css :root`: the type scale, radius, shadow, and semantic colours (good, warn, bad, usdc).

## 2. Prize and season, always visible

- **Season bar** (topbar):
  - Tick n / N and the current phase (建国 · 拡大 · 競合 · 危機 · 決着).
  - The phase segments, a "now" marker, and the freeze ticks: the Exchange at 120, transfers at 162.
  - Every boundary comes from the server's `season` object, not from constants.
- **Prize pool** (topbar): total USDC in the vault. On chain it is read from the Season account.
- **勝ち筋 panel** (left), one card per track:
  - Your rank, and your score in the track's own unit.
  - The leader and your gap.
  - A progress bar toward the leader.
  - The card that currently pays you most is highlighted (the one-top-3 rule).
  - Below the cards: **「いま終わったら ≈ x USDC」**, computed by the server with the rules' own `scoring::payouts`.
- **Season over** (modal):
  - The top 3 of each track with their payouts.
  - Your prize and why you won it.
  - **受け取る** (claim, in chain mode).
  - "全ティックの検証済み", and the verify command.

## 3. Play feedback

- **Preflight in the dock.** While you draft, the client calls `/api/validate` (debounced). A chip that would be skipped at resolution turns amber, with its reason (for example 「移動先に敵の部隊がいる」).
- **Tick report**, shown after each tick where the summary is today:
  - 「n件実行 · m件見送り」 and one line per order.
  - Skipped orders give their reason, from the engine's new `last_skipped` (design §4).
  - Public events follow in one line.
- **Chain mode controls:**
  - There is no pause button.
  - The primary button reads **「確定して手番を終える」**.
  - After it, the dock becomes the **waiting** view: every seat with 考え中 or ✓, a note that the tick resolves as soon as everyone is done, and 「命令を直す（締切まで可）」.
  - Chain errors appear inline in the dock, not only as a toast.
- **Local mode** keeps pause. The tooltips no longer say "single-player only". Pause shows who paused.
- **Errors and loading:**
  - Every panel that fetches shows a loading state and an error state with a retry.
  - Offline shows a banner and disables commit.
- **Removed hardcoding** (6 civs, 16 techs, budget max 8, freeze 120, NAP bond 30, purchase 60): values come from the server.

## 4. Known bugs fixed along the way

- `AcceptNap` sends the proposal's own bond instead of 30.
- The Exchange review step reads the values the player typed.
- Toast text is escaped.
- Spectator page: `new WorldMap(canvas, {})` throws on every pointer event. It gets no-op callbacks (the only spectator change in this round).

## 5. Server data the UI needs (new fields in `/api/state`)

| Field | Content |
|---|---|
| `season` | preset, ticks, tick seconds, phase boundaries, freeze ticks, entry fee, track shares |
| `pool` | vault USDC. Local: entry fees. Chain: from the gateway's Season account |
| `projection` | payout per civ if the season ended now (`scoring::payouts`) |
| `skipped` | this viewer's skipped orders of the last tick, as (order, reason) |
| `ranks` | per track: order of civs and scores, as the rules compute them |

## 6. Not in this round

- The spectator page beyond the crash fix.
- The phone layout.
- Keyboard navigation of the map.
- A contrast audit.

These are recorded for later.
