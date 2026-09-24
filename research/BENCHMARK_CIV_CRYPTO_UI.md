# Benchmark: Civilization map/HUD and crypto-native game UI → PERMUTATION STATE V4.1

Status: 2026-09-24. This replaces the earlier V4.1 mock direction, which was rejected: it simplified the map and buried it under opaque panels. The rule from here on: **the map stays the hero at Civ quality, and the chain is visible but never in the way.**

Inputs:
- A web benchmark of Civ VI/VII, with Civ V, Humankind and Old World for comparison. Official 2K patch notes, the Civilopedia, Steam and CivFanatics threads, and reviews are cited inline.
- A web benchmark of on-chain games (Eternum, Dark Forest, Primodium, Sky Strife, Influence, Loot Survivor, Pirate Nation, Supersize, Alpha Arena, Stake provably fair).
- An audit of our `map.mjs`.

## 1. Where our map stands

**Already Civ-like:**
- Procedural terrain art with trees, peaks, rivers and resources.
- Three fog states.
- Civ-coloured territory with a dash pattern per civ.
- Reach overlay (this tick vs. 2–3 ticks).
- Planned-move paths.
- City pills with pop and a defence bar.
- Five lenses and a minimap.

**Missing vs. Civ:**

| Civ convention | Ours today | Gap |
|---|---|---|
| City banner: pop + growth, ★ capital, name, production icon + turns, defence/wall bars, civ colour | Small pill: pop square, name, 3 px defence bar. Text shrinks with zoom (≈5.5 px) | Production and growth, walls, owner kind, constant screen size |
| Unit flag: shield in civ colours, HP bar, stack count, idle marker | Pennant plus a troop number. Stacked units overlap. `moved` is never shown | Flag shape, HP/troop bar, stack badge, idle/moved marker |
| Borders: solid civ line with a dark outline and a soft inner glow | A dashed line in a darker shade of the civ colour, no outline | Solid line, outline, allied double line |
| Path with numbered turn markers | Marching dashes, and an arrival tag on hover | ①②③ per tick |
| Combat preview on the target | Numbers only in the side panel | Forecast chip on the target hex |
| City-state banner: distinct shape, suzerain, influence | Tower plus the label "都市国家 N" | Specialty glyph, suzerain stripe, my influence |
| Top-right leader ribbon: rivals, relation, "ended turn" ✓ | None | New |
| World Rankings / Legacy Paths progress | Three pills plus a drawer | Always-visible track tracker with every civ's marker |
| Next Turn button that names the blocker and counts down | Small ⏭ icon, and the dock button "確定" | Big round next-action button with a ring timer |
| Notification stack above Next Turn | Up to 3 toasts that fade | Persistent grouped stack, click to jump |
| Semantic zoom | Only "hide labels below 0.55" | Constant-size banners; far zoom shows banners and flags only |

Civ VII warnings we heed:
- It hid what matters and shouted what doesn't.
- The map was hard to read and flags overlapped.
- Its crisis had almost no visual presence.
- Its grey "iPad" panels felt placeholder-like.

We keep our paper-and-gold look, which Civ VI players liked, and make the numbers readable.

## 2. How much chain to show

Lessons from on-chain games:
- **Never make the player do crypto work.** Sign once (session key), no popups, no gas numbers. The worst example is Axie's wallet-first onboarding; the best examples are Pirate Nation and Cartridge.
- **Put pending chain work in one quiet place:** Primodium's corner indicator, Influence's log, Dark Forest's transaction log.
- **Make the tick a public heartbeat**, as Dark Forest Punk's ticker and Sky Strife's 15 s turns do.
- **Show the pot as game stakes**: Eternum's prize pool, Supersize buy-ins.
- **Verify, don't trust.** Stake's commit → reveal → verify flow. Every ✓ must open something reproducible.
- **Agents are players with visible wallets and visible reasoning.** Alpha Arena went viral on exactly that.

**Our rule:** at most three chain elements are always on screen: the tick heartbeat, the USDC pool and the session chip. Everything else lives in the **Chain lens** (the ` key). There the style switches to monospace with one Solana-teal→violet accent, the only "hacker" styling in the game.

## 3. V4.1 UI, concretely (prototype at `/v41/` on the game server)

**Map (canvas, `web/v41/map.mjs`)**
1. **City banner** at a constant screen size, laid out as `[pop ● growth arc] ★ Name [production glyph · n] `. Below it sit a defence bar (green→red) and a walls bar (blue). The band is in the civ colour. A HUMAN/AGENT tag shows on rivals' cities.
2. **City-state banner:** diamond shape with a specialty glyph (学 / 商 / 農), a suzerain colour stripe, and "your influence / top influence".
3. **Unit flags:** a shield in the civ colour with the unit glyph, a troop bar (troops / max for that stack), a ×n stack badge, a "z" idle mark on your units that got no order, and a ⋯ mark for units moved last tick.
4. **Borders:** a solid civ-coloured line with a dark outer stroke and a soft inner glow. Allies get a double line. Wars keep the existing hatch.
5. **Planned paths:** ①②③ markers where each tick's movement ends. Attack plans show the forecast chip (`−18 / −6`) on the target.
6. **Semantic zoom:** banners and flags keep their size. Below 0.6 zoom, decorations fade and only banners, flags and borders remain.

**HUD (`web/v41/index.html`, `app.mjs`, `styles.css`)**

7. **Top bar:** resources with deltas (as now), then the **tick heartbeat**: a ring timer with `TICK n/N` that pulses on seal with "✓ 検証済み". Then the **USDC pool** chip.
8. **Leader ribbon (top right):** six civ medallions. Each has a colour ring, initial, kind badge (人/AI/Bot), relation icon (⚔ ☮ 🤝), a ✓ when that civ ended its turn, and a hover card: persona, wallet short address, x402 join, scores.
9. **Tracks tracker (under the ribbon)**, the Legacy Paths analog: three thin bars (覇権 / 科学 / 協調) with one marker per civ in its colour. Yours is larger. The projected payout sits alongside.
10. **Next-action button (bottom right):** a big round button with a countdown ring. Its label names the blocker (「研究を選ぶ」「都市が待機中」「部隊に命令」) or 「手番を終える」 / 「提出済み ✓」. Clicking jumps to the blocker or ends the turn.
11. **Notification stack** above it: grouped icons with counts. Click to jump; × to dismiss.
12. **Session chip:** `◎ 7fK2…a91 · セッション有効` (chain mode), or 「ローカル」.
13. **Sealed dispatches:** your rationale shows as a sealed letter until the tick resolves, then opens with ✓ once the digest verifies. The same happens for rivals in the ticker.
14. **Chain lens (`)**, a right drawer in monospace:
    - A live ticker of sealed ticks: tick, ER slot, CU, orders, root, ✓, and an explorer link.
    - The vault address and pool split.
    - The session scope.
    - The verify command.
    - On the map, each tile is tinted by "changed this tick".

**Out of scope for this round:** a new terrain art style, a phone layout, the spectator page beyond the crash fix, and a custom icon font.
