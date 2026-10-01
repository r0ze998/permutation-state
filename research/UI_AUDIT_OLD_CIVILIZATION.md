# UI/UX audit: Wylls "私たちの文明" prototype

All files are in `/Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/outputs/permutation-state-prototype/civilization/`. `styles.css` is minified onto 14 lines, so CSS below is quoted by selector, not line number. Line numbers for `.mjs` files are exact.

## Why it works, in short
- **Full-bleed canvas, floating panels.** The map fills the viewport. Every UI piece is an absolutely positioned "paper card" on top of it.
- **Muted palette.** Warm paper tones, deep green and gold, with a serif display font (Georgia / Mincho). It reads like an illustrated atlas rather than a game HUD.
- **One strict flow.** Select a tile → the right panel explains it → every option shows cost, time and a Japanese reason when blocked → a confirm dialog shows the full cost and what happens next → a toast confirms.
- **Map drawn from state only.** The renderer draws only what the simulation contains. The file header says "No decorative game entities are invented." It uses fake depth: flattened hexes plus side walls.

---

## 1. Layout and information architecture

Structure (`index.html`): `<main id="game">` is `position:relative; width/height:100%; isolation:isolate`. `html,body` use `overflow:hidden`, the body is `#adc1b1`, and base type is `13px`. Every element listed below is `position:absolute` over `#world-map`, which uses `inset:0; touch-action:none`.

| Region | Element | Position / size (CSS) | Contents |
|---|---|---|---|
| Vignette | `.map-vignette` | `inset:0; box-shadow: inset 0 -30px 90px #233d351c; z-index:1` | Darkens the bottom edge slightly |
| Top bar | `header.topbar` | `top:0; height:78px; padding:0 24px; gap:28px; background: linear-gradient(180deg,#fbf8f4fc,#f7f4eefa); border-bottom:1px solid #d1d4c0; box-shadow:0 2px 16px #2f4f3520; z-index:10` | Brand (✳ + "PERMUTATION / STATE"), resource strip, connection status, `?` help button |
| Brand | `.brand` | `min-width:165px`, serif `12px`, `letter-spacing:2px`; `em` is `24px`, `letter-spacing:6px`; `.brand-mark` is a `44px` gold ✳ | |
| Resource strip | `#resources .resource` | `flex:1; justify-content:center`; each item `padding:0 20px; min-width:91px; border-left:1px solid #deddd0; grid-template-columns:22px 1fr` | Icon (`23px`, colour from `--resource-color`), amount (serif `23px`, `tabular-nums`), rate "+x / 分" (`10px`, `#65816a`, `.negative` = `#ac6251`). A hover or focus-within tooltip `.resource-tip` (`top:56px; min-width:170px; max-width:270px`) shows capacity and reserved amounts. Each item is a `<button>` that opens the economy drawer |
| Civ plate (top-left) | `.civ-plate` | `top:108px; left:27px; width:238px; text-shadow:0 1px 12px #ffffffe8` (no card; text sits directly on the map) | Eyebrow with live dot "ONE SHARED CIVILIZATION", h1 "私たちの文明" (final override: `29px`, `letter-spacing:.02em`) with an English subtitle as a block at `8px`, `letter-spacing:2.1px`. Also the world clock "開拓の時代 · DAY 01 · 06:00" and the ambitions toggle (`width:225px`, frosted `#faf8efbc` + `backdrop-filter:blur(16px)`), which expands to progress bars (`.progress-track` `4px` tall, fill `#708765`, `transition: width .4s`) |
| World nav (left rail) | `.world-nav` | `left:27px; top:277px`, column, `gap:8px`; buttons `50×58px`, `border-radius:7px`, bg `#fbf8eee8`; `.active` = `--green-deep` bg with paper text | 5 drawers: ⌂街, ▥経済, ✧研究, ⚑計画, ☷歴史. Glyph `23px` serif, label `9px` |
| Drawer | `#drawer.drawer.panel` | `top:278px; left:88px; width:295px; max-height:calc(100% - 418px); padding:19px; overflow-y:auto; z-index:6` | Settlement list (`.building-row` with a status dot), economy table (`grid-template-columns:1fr 44px 68px`), tech cards, project cards, chronicle timeline (`.event` with a left rule and a 5px dot) |
| Inspector (right) | `#inspector.inspector.panel` | `right:23px; top:104px; bottom:190px; width:304px; padding:20px; overflow:auto; z-index:5` | The contextual panel for the selected tile (see §4) |
| Hover label | `.hover-label` | `top:90px; left:50%` centred; bg `#213d35e8`, text `#fffaf0`, `10px`, `radius:4px`, `pointer-events:none` | "森 · 2, -1" |
| Notifications | `.notifications` | `top:93px`, centred, `width:min(430px,70vw); z-index:40` | Toasts (see §2) |
| Map tools | `.map-tools` | `bottom:205px; left:28px`, column; `30×30`, `radius:5px` | + / − / ⌖ (focus self, F) |
| Compass | Drawn on the canvas | `x=47, y=height-160`, `globalAlpha .48`. Hidden if `width<800` or `height<620` | |
| Minimap | `.minimap-card` | `bottom:45px; left:26px; width:186px`, bg `#f7f4e9de`, `radius:7px`; canvas `170×104` | Footer at `8px` shows "12 / 40 地形を発見" and the citizen count. Click jumps to the nearest tile |
| Bottom centre | `footer.bottom-center` | `bottom:37px; left:50%; translateX(-50%)`, column, `pointer-events:none` (children re-enable it) | Lens bar, lens description, citizen card |
| Lens bar | `.lens-bar` | bg `#fcfaf1f2`, `radius:6px`, `padding:4px; gap:3px`; buttons `padding:8px 11px; font-size:10px`, glyph `15px`; `.active` = `#385845` / `#fbf9ec`; `aria-pressed` | ◈地形 ❧食料 ⚒産業 ⇄物流 ⌁地勢 |
| Lens description | `.lens-description` | `9px`, `#4f6953`, white text-shadow, `margin:9px 0 11px` | One line per lens |
| Citizen card | `#citizen-card` | `min-width:258px; padding:9px 13px; radius:8px`; avatar `30×35` with `border-radius:15px 15px 8px 8px` and a `linear-gradient(135deg,#ad8c64,#735743)` fill showing the initial in serif `22px` | Name, live status (e.g. "木材 4 を運搬中", "建設中 · 残り12秒"), `F` key-cap |
| Next action | `.next-action.panel` | `bottom:44px; right:24px; width:304px; padding:13px 16px 10px` | Heading "あなたにできること" plus a ↻ skip button, a title (`14px/500`) with detail (`10px`), a round `35px` `--green-deep` "↗" button, and a foot "提案は自由に選べます" with an `N` key-cap |
| Prototype label | `.prototype-label` | `bottom:11px; left:26px; 8px; letter-spacing:1.1px` | Honesty label: "共有サーバー版 · 実資金なし" |
| Loading | `.loading-screen` | `inset:0; z-index:50; bg #eeeede` | `66px` gold ✳ spinning over `16s`, serif h2 "ひとつの世界に、集まろう。", and a retry button that never overwrites the saved world |
| Dialogs | `dialog` | `radius:12px; box-shadow:0 25px 100px #1f392754`; `::backdrop` `#203c3266` + `blur(6px)`; help dialog `530px` with `padding:37px 40px`; confirm dialog `385px` with `padding:28px` | |

The right column is split into two stacked cards: inspector (`top:104 → bottom:190`) and next-action (`bottom:44`). The canvas compensates with `_usableCenter()` (map.mjs:157). On widths above 1000px the logical centre shifts `-115px` so the focused tile isn't hidden behind the inspector. On widths of 850px or less it uses `(0.52w, 0.48h)`, and `0.34h` when focusing a selection, because the inspector becomes a bottom sheet.

**Breakpoints:**
- `min-width:1600px`: topbar `gap:45px`, resource padding `0 29px`, inspector and next-action `326px`, left elements shifted to `35px`.
- `max-width:1150px`: resource `min-width:74px`, amount `21px`, resource name shrinks to `8px`. Inspector and next-action `right:15px; width:276px`. Drawer `270px`. Minimap `155px` (canvas `139×86`).
- `max-width:850px`: topbar grows to `100px` and wraps, with resources on their own row (`order:2; justify-content:space-between`). Minimap hidden. Lens buttons become icon-only (`font-size:0`, span `17px`). Bottom-centre anchors to `left:20px`. **Inspector and drawer become bottom sheets:** `top:auto; bottom:192px; left:75px; right:15px; max-height:34vh` (drawer `38vh`). Next-action `max-height:148px`.
- `max-width:540px`: nav at `top:240px`, inspector and drawer `left:65px; right:12px; bottom:190px`. Next-action `width:calc(100% - 235px)` with its detail hidden. h1 `23px`.
- `prefers-reduced-motion` sets all animations and transitions to `.001ms`.
- JS: on `innerWidth<650`, opening a drawer clears the selection and vice versa (app.mjs:149, 306), so only one sheet shows at a time.

## 2. Visual language

**Tokens** (`:root` in styles.css):
`--ink:#243c36; --muted:#768278; --paper:#fbf8ef; --paper-soft:#efece0; --line:#d8d7c7; --green:#365a46; --green-deep:#213f34; --gold:#ad8644; --shadow:0 8px 36px #172f2821`.

`guide.css` adds `--ink-soft:#4c5e57; --paper-deep:#e4dfd0; --green-pale:#dce4d7; --gold-pale:#e7d8b7`, and uses a larger `--shadow:0 18px 55px rgba(23,47,40,.12)`.

**Other recurring hard-coded colours:**
- Tags: `#e9ecdf` / `#65765e` / border `#dce1d0`. `.warning` = `#f1e6d5` / `#a4773e`. `.positive` = `#e4eddf` / `#527a4b`.
- `.explanation` (the reason box): `border-left:2px solid #b89a67; background:#f1eddf; color:#7f7257`.
- Secondary button: `border:1px solid #bdc9b4; background:#f2f3e8`.
- Build option: `#f6f7ed` / border `#d6ddca`; disabled `opacity:.72; #f0eee6`. `.locked-reason` is `#a18867`, `9px`.
- Error / negative: `#ac6251`, `#b65f4b`, `#b65c44`. Positive / live: `#6a986a`, `#739268`.
- Focus ring: `outline:3px solid #bf8934; outline-offset:3px`.
- `theme-color` meta: `#243e38`.

**Resource colours** (app.mjs:32): `food #829852, wood #9b7950, stone #7e8c8a, ore #a27151, tools #5b8278, knowledge #7b7698`. These differ from `RESOURCE_META` colours and icons in core.mjs:2-9 (e.g. `food #d3b55b ◒`). A new game should pick one source of truth.

**Typography:**
- `--serif: Georgia,'Yu Mincho','Hiragino Mincho ProN',serif` for display: brand, h1/h2, resource amounts, stat values, step numbers, nav glyphs.
- `--sans: Inter,-apple-system,BlinkMacSystemFont,'Hiragino Kaku Gothic ProN','Noto Sans JP',sans-serif` for body.
- Serif headings are always `font-weight:400`, never bold. Emphasis in sans uses `500`.
- The size scale is very small and dense: 7/8/9/10/11/12/13/14/17/18/20/23/25/29/31/46 px.
- Micro-labels are uppercase English with letter-spacing: `.eyebrow` `9px/700`, `1.6px`; `.selection-eyebrow` `9px`, `1.3px`, e.g. "WOODLAND · 2, -1", "A SHARED INVESTMENT".
- Numbers use `font-variant-numeric:tabular-nums` and `toLocaleString('ja-JP')`.
- Body line-height is generous: `1.7–1.9`.

**Spacing and radii:**
- Spacing is loosely 4/6/7/8/10/12/13/15/20px.
- Radii: tags `3`, buttons `4–5`, lens bar `6`, nav and minimap `7`, panels `8`, dialogs `12`, round icon buttons `50%`.
- Panel recipe: `.panel{background:#fbf8f3f7;border:1px solid #fcfbef;box-shadow:var(--shadow);backdrop-filter:blur(12px);border-radius:8px}`. It is nearly opaque warm paper with a light border that reads as a highlight.
- Shadows are always green-tinted (`#172f28…`, `#27432d…`), never pure black.

**Iconography:** there is no icon font or SVG. Unicode glyphs are set in the serif font: resources ❧♧◆⬡⚒✧, buildings ♜❧♧◆⬡⚒♖✧▤ (app.mjs:30,34), nav ⌂▥✧⚑☷, lenses ◈❧⚒⇄⌁, actions ⌖ ↗ ↻ →. Key-caps `.keyboard-key` are `8px` with `border:1px solid #cdd3c4; radius:3px`.

**Copy style:**
- Japanese first, with English eyebrows as ornament ("OUR CIVILIZATION", "BEYOND THE KNOWN WORLD").
- Polite, invitational register: "あなたにできること", "提案は自由に選べます", "地形を選択して、文明の次の一手を。"
- Every action states its consequence: "文明の共有備蓄から材料を使います…", "拒否された行動で共有資源が消費されることはありません。"
- Honesty disclosures appear throughout, e.g. "NPCはルールで働く試作版です", "実資金なし".
- `copy.mjs` (`chronicleText`, lines 2-11) rewrites legacy event text for display without mutating stored history.

**Toasts:** `.toast` has `border-left:3px solid #739268`, `backdrop-filter:blur(10px)`, `animation: toast-in .25s` (fade and rise 7px). `.error` uses `#b65f4b`. At most 3 show at once. They auto-dismiss after 4.5s, or 6.5s for errors (app.mjs:63-68).

## 3. Map renderer (map.mjs)

**Geometry** (lines 2-52):
- `RADIUS = 47` and `FLATTEN = 0.76`. The Y axis is squashed, which is the whole "2.5D" trick.
- Pointy-top axial projection: `project(q,r) = {x: √3·R·(q + r/2), y: R·1.5·r·FLATTEN}`.
- `hexPoints(x,y,inset)` uses angles `i*60-30` with the y component multiplied by `FLATTEN`.
- `inverseHex` divides y by `FLATTEN` and then does standard cube rounding. Hit-testing is exact.
- `seed(q,r,n)` is a `sin*43758.5453` hash that gives deterministic per-tile decoration.

**Two-layer rendering:**
1. **Static cache.** An offscreen canvas holds background, tiles, roads, decor and lenses. It is redrawn only when `dirty`, i.e. on pan, zoom, state or lens change (`_renderStatic`, lines 310-327). Only visible tiles are drawn (`_visible` with 120px padding).
2. **Dynamic overlay.** Every frame draws highlights, paths, units, labels and the compass (`_frame`, 276-303). The frame is throttled to about 30fps (`now - lastFrame < 32`) and skipped while `document.hidden`. The code comments say this keeps laptops cool.

The background is a radial gradient `#eee9d9 → #e6e4d5 (.58) → #cfd8cd`, plus 280 one-pixel grain dots at `rgba(90,111,96,.055)`.

**Painter order:** `sortedTiles` is sorted by `r`, then `q`. Units are sorted by `r`.

**Tiles** (`_drawTile`, 329-374):
- `COLORS` (9-16) gives three shades per terrain `[top-light, top-dark/front wall, side wall]`. Grass is `#c6cea0/#a1ad80/#899a71`; water `#77a6a6/#558a91/#467880`; fog `#d2d6cb/#b7c1b7/#a5b3aa`.
- Extrusion: the first three edges are drawn as quads dropped by `depth` (7px, or 3 for water), which reads as a solid tile block.
- The top face uses a linear gradient with an `.65` inset and a light rim stroke `rgba(239,237,208,.48)`.
- Texture: 15 speckles per tile (warm `rgba(246,234,181,.25)` every third one, otherwise dark `rgba(63,89,61,.10)`). Empty grass gets 5 small "V" grass strokes. Water gets 5 bezier ripples at `rgba(204,229,208,.5)`.

**Fog:**
- Unexplored tiles use the fog palette at `globalAlpha .46`.
- Frontier tiles (adjacent to explored ones) are drawn at `.9` with a small "+" crosshair. This shows where exploration is possible.
- Units on unexplored tiles are skipped.

**Roads** (376-390): three stacked strokes per connection, from tile centre to edge midpoint: `10px rgba(116,105,77,.24)` shadow, then `6.5px #d9cbb1`, then a `1px rgba(246,232,195,.52)` highlight, all with `lineCap round`. There is a centre ellipse of `5×3`. Roads also connect to buildings with `connected`.

**Decor** (392-411):
- Forest: 4 `_tree`s, or 2 if a building is present.
- Mountain: 3 `_mountain` peaks at heights 42/67/28.
- Hill: 2 `_rock`s, plus rust-coloured `#b47c4f` triangles when there is ore.

**Primitives:**
- `_tree` (413): a shadow ellipse plus trunk. If `variant>.45` it is a two-tier conifer (`#416b53/#6d8d61/#517954/#799760`); otherwise a round three-blob broadleaf with a highlight. `variant<.16` gives an olive variant.
- `_mountain` (432): a five-point silhouette with a dark right face `#7d8a82`, a light left face `#c8c6b0`, and a snow cap `#e9e6d2`.
- `_rock` (442): a three-facet low-poly shape.
- `_house(x,y,w,h,roof,tower)` (448-469): the main reusable building. Oblique front and side walls (`#e9d7ad`, `#c5b996`), a two-tone roof with shingle lines, window, door with light, and an optional tower.

**Buildings** (`_buildingShape`, 493-551) are composed from those primitives:
- Farm: 5 furrow strips plus a small house.
- Lumbermill: house plus a log pile.
- Quarry: pad, rocks and a crane.
- Mine: rock with an adit.
- Workshop: house plus a chimney with a glowing `#e5ae62` furnace.
- Watchtower: stilts plus a cabin.
- Archive: house plus a dome.
- Townhall: plaza, towered house and a flag.

**Construction state** (471-491): the ghost building is drawn at `.38` alpha under scaffolding lines, with a progress bar under it (`#756d50` track, `#e8c271` fill). Blocked or unconnected buildings get a `#ba7850` "!" badge.

**Lenses** (`_drawLens`, 553-576): a tinted inset hex (`hexPoints(…,3)`) with a 1.6px stroke, plus an optional kanji chip (`22×18` rounded, `rgba(48,75,61,.86)`), e.g. 食/木/石/鉄/工/未接続.
- food: grass and farms.
- industry: resource tiles and production buildings.
- logistics: roads and buildings, with unconnected ones in red. Animated dashed caravan routes are drawn in `_drawHighlights` (`setLineDash([3,7])`, `lineDashOffset=-now/170`).
- danger: water and mountains.

**Selection and hover** (578-590):
- Hover draws a hex at `rgba(255,252,225,.10)` with a `1.4/zoom` stroke.
- Selection uses stroke `#ffedb0` at `2.4/zoom`, plus six glowing corner dots (`shadowBlur 8`). Stroke widths are divided by zoom so they stay crisp at any scale.

**Own path** (603-613): a dashed `#f4e3a4` 2px line with dash `[2,6]` that marches (`-now/120`).

**Citizens** (615-653):
- A procedural figure: legs with stride `sin(now/100)*2.3`, bob `sin(now/115)` while moving, torso colour own `#e9bd65` / other player `#9c87aa` / NPC `#e1d6b2`, a head, and a hair ellipse.
- A cargo backpack is drawn when carrying.
- The own character gets a pulsing gold ground ring and a dark `#284d40` name pill with a pointer.
- Non-own units are fanned out by `spread` to reduce overlap.

**Couriers** (655-665): a cart with two wheels and a cargo top.

**Interpolation** (`setState` 183-206, `_entityPosition` 243-252, `_modelPosition` 254-274):
- Each entity keeps `error = previous − newTarget`, which decays linearly over 320ms.
- Movement is extrapolated along the server-accepted path for at most 700ms ("bounded to one polling interval").
- Per-hex time is `900ms × (road ? .65 : terrain cost)`.
- Couriers interpolate along `route` by `elapsedMs/durationMs`.

**Building labels** (668-682): hidden below `zoom<.72`. Pills are `17px` tall with `9px` text. The townhall is inverted: dark `rgba(43,74,60,.95)` with `#f4e5b8` text.

**Camera:**
- Initial zoom is `.85` if `w<680`, `1` if `h<680`, otherwise `1.25`. It is clamped to `.55–2.6`.
- Wheel zoom: `exp(-clamp(deltaY,±140)*.0015)`, anchored at the cursor (`_zoomAt`, 226-231).
- Drag threshold is `5px` before pan starts; pointer capture is used; the cursor switches between grab and grabbing.
- `focusTile` / `focusPlayer` set `targetOffset`, which eases at `0.2` per frame and snaps under `.6px`.
- On resize the offset shifts by the change in usable centre, so the view doesn't jump.

**DPR** (169-171): `dpr = min(devicePixelRatio, 2)`. Both canvas and cache get backing size `css × dpr`. The cache is blitted at identity transform, then the overlay is drawn with `setTransform(dpr,…)`. The context is `alpha:false` for speed.

**Accessibility:** the canvas is focusable with an aria-label. Arrow keys pan 65px, `+`/`-` zoom, `F` focuses self, `Enter` moves to the selection (144-154). `destroy()` removes all listeners, the ResizeObserver and the rAF loop.

## 4. Interaction patterns

**Main loop:**
1. Click a tile → `onSelect` → `selectTile` (app.mjs:145) → `renderInspector`.
2. Double-click, or `Enter`, sends a MOVE action directly.

**Inspector structure** (184-244):
- Header: eyebrow (terrain English name · q,r) and a × close button. h2 is the tile name (serif `25px`). Then a description and tags ("道路あり", resource, "活動できる距離"/"移動が必要").
- `tileValueMarkup`: site yield ("産地収量 ×1.4").
- `journeyMarkup`: "⌖ 現地まで 約8秒 · 地形と道路を考慮".
- For buildings: two `.stat-box`es (状態 / 物流), a progress bar with time remaining, an `.explanation` reason box, the local stock waiting for pickup, caravans in transit, and production-mode buttons.
- Actions: "⌖ ここへ移動" / "採集して運ぶ" (secondary), "周辺の未知の土地を探索 5秒 ↗" (primary, wide), and a road button with its cost row.
- "この場所に建てる": a list of `.build-option` cards filtered by terrain. Each shows icon, name, time, effect and costs, with any unaffordable cost in `.negative` red (`costMarkup`, line 62). If blocked, the card is disabled and shows `.locked-reason` with the exact reason from `getBuildPreview`. Under each card, attached flush, is a "＋ 共同計画として提案" button.
- Unexplored tile: an explanation plus an explore button, with a follow-up line saying what to do if disabled ("まず隣の既知の土地を選び、現地へ移動してください。").

**Preview → confirm:**
- Every costed action first calls a `get*Preview` function that returns `{allowed, reason, cost, durationMs, effect}` (core.mjs:223, 255, 282, 304, 319, 330). The reasons are an ordered check chain; for example `getBuildPreview` checks in this order: exists → joined → explored → occupied → terrain → research → road-connected → affordable → idle → adjacent.
- Clicking opens `confirm-dialog` (`confirmBuild`/`confirmRoad`/`confirmResearch`/`confirmProject`/`confirmMode`, app.mjs:307-345). Each has an English eyebrow, a serif title, the effect, a `14px` cost row, time and place, a `.confirm-disclosure` of side-effects, and a `戻る` (secondary) / verb (primary) pair.
- Actions go through a single `act()` (125-134). It sets `pending`, disables all buttons, re-renders, and shows a success toast from a per-action message map or an error toast with the server message.

**Event delegation:** one `document` click handler dispatches on `data-*` attributes (346-365): `data-review-build`, `data-focus-tile`, `data-drawer`, `data-lens`, `data-confirm-action`, and so on. This keeps the HTML-string templates simple.

**Keyboard:**
- Global: `F` focuses self, `N` runs the next suggestion, `Esc` clears selection and drawer. These are ignored in inputs or while a dialog is open (383-388).
- Canvas: arrows, `+`/`-`, `F`, `Enter`.
- Hints appear as key-caps on the citizen card and next-action card, and in the help dialog's "ドラッグ：地図移動 スクロール：拡大縮小 F：自分へ N：次の提案 Esc：閉じる".

**Next action:**
- `getCitizenTasks` (core.mjs:715-742) returns up to 7 tasks sorted by priority: gather when low 100, blocked building 95, farm 90, and so on.
- Each task has `{id,title,detail,tileId}`, and the detail states the trade-off ("産業への投資はその分遅れる").
- Clicking selects and focuses the tile but **does not act**; the player still confirms. ↻ cycles through tasks. The current task is kept across re-renders by id (app.mjs:245-256).

**Tooltips:** there are no custom tooltip components beyond the resource tip and the hover label. Everything else uses native `title` attributes on nav, disabled mode buttons (with the reason), and the next button ("2 / 5 件の提案").

**Empty states:** `.empty-copy` ("共同計画はまだありません。建設したい土地を選んで提案しましょう。", "この地形に建設できる施設はありません。"). Each drawer opens with a `.drawer-intro` paragraph explaining how that area works.

**Connection:** the `.connection` pill switches to `.offline` red with "再接続中". Polling runs every 650ms, or 1800ms when the tab is hidden, and re-polls on `visibilitychange`. The loading screen offers retry. Stale snapshots are rejected by `revision`/`timeMs` (line 82). `setHtml` skips DOM writes when the markup is unchanged (line 56), which avoids flicker and preserves hover and focus.

**Help dialog:** four numbered steps (`counter(steps, decimal-leading-zero)` in gold serif), a note, the controls line, identity-save controls, a guide link, and a wide primary "文明に参加する →". `guide.html` is a long-form companion with sections for the first five minutes, movement, logistics, research, controls, a six-item "行動できないとき" checklist, and scope.

## 5. Reusable code for a multi-civ hex strategy game

**Directly reusable (map.mjs):**
- `SQRT3/RADIUS/FLATTEN`, `project`, `hexPoints`, `inverseHex`, `seed`, `NEIGHBORS`, `key` (2-52). These are pure hex math for the 2.5D look.
- `rounded`, `polygon` (25-37): canvas helpers.
- Camera and input: `_bind`, `_usableCenter`, `_resize`, `focusTile`, `zoomBy`, `_zoomAt`, `screenPosition`, `_hit`, `destroy` (92-241, 695-701).
- Frame architecture: `_frame`, `_renderStatic`, `_visible` (276-327). This covers the cache/overlay split, the 30fps throttle and DPR handling.
- Terrain art: `_drawTile`, `_drawRoad`, `_tree`, `_mountain`, `_rock`, `_house` (329-469).
- `_building`/`_buildingShape` (471-551): the construction/scaffold treatment is generic; the building types are game-specific.
- `_drawLens` (553-576): the pattern (tinted inset hex plus kanji chip) is reusable; the predicates need rewriting.
- `_drawHighlights` (578-601), `_drawLabels` (668-682), `_drawCompass` (684-693).
- Entity smoothing: `_entityPosition`/`_modelPosition` (243-274). This is a good server-authoritative interpolation pattern.

**Reusable from app.mjs:**
- `setHtml` (56), `notify` (63-68), `costMarkup` (62), `api` (73-78), `receive` with its revision guard (79-96).
- The single `act()` pending/toast wrapper (125-134).
- Data-attribute click delegation (346-365), keyboard handler (383-388), `renderMinimap` and minimap click (287-300, 375-382).
- The confirm-dialog template shape (307-345).
- From strategy-ui.mjs: `escapeHtml` (3) and the `projectCard` card pattern (47-67).

**Reusable from core.mjs:**
- `hexDistance` (46), `tileById` (50), `findPath` (196).
- The `get*Preview → {allowed, reason, cost, durationMs, effect}` contract (223-345). This is the most valuable UX contract to carry forward.
- `getCitizenTasks` (715) as a pattern: a prioritized, explained, non-binding list of suggestions.

**Tightly coupled to the old single-civ citizen model:**
- **One shared civilization.** A single global `world.stock`/`world.rates` feeds the resource strip (app.mjs:159-164). The copy assumes one civ throughout ("私たちの文明", "共有備蓄", "文明の資産"), as does the civ-plate h1. Multi-civ needs a per-civ economy and ownership colours. There is currently no owner tinting of tiles or borders anywhere in `_drawTile`.
- **One avatar per player.** The "citizen body" model drives these: `citizen()`, `nearTile`, `isBusy` (58-61); adjacency gating in every preview (`hexDistance(player,tile) > 1`); `focusPlayer`; the citizen card; the `F` key; own-unit rendering (`own`, name pill, ring); and `_drawPaths` (which only draws the actor's path).
- **Hard-coded content.** `LABELS` (map.mjs:5), `COLORS` terrains, and the `_buildingShape` switch. In app.mjs: the `resourceKeys`/`icons`/`resourceColors`/`buildingIcons`/`buildingEffects`/`terrains` tables (29-47) and the terrain → building filter (226-232). In map.mjs: the lens names at 210 and lens predicates at 557-570, and terrain move costs at 268 (duplicated from core `TERRAIN_COST`).
- **Townhall-hub logistics.** `connected` flags, caravans to "広場", `connectedRoads` from `HOME="0,0"`.
- **Session and identity.** `session='aster'`, sessionStorage identity, polling `/api/civilization/*` (9-15, 97-124), and `identity.mjs`.
- **World clock.** A fixed formula, `6 + timeMs/300000*24` (153-156), rather than turns.
- **Layout constants.** `_usableCenter`'s `-115px` and 850px breakpoint are tied to the 304px inspector and the CSS breakpoints. Keep them in sync if the panel widths change.
