# Wylls — Game Design V4.1 (revision draft)

> **2026-09-24 note — superseded in part by [Game Design V5](PERMUTATION_STATE_GAME_DESIGN_V5.md).** §2, the Dominion/Concord scoring that differentiated the victory tracks, is replaced by V5's four paths and achievement points. §3, the rule fixes H1–H9, and §4, skipped orders and projections, carry over into V5.

Status: draft for review, 2026-09-24. This revision changes [Game Design V4](PERMUTATION_STATE_GAME_DESIGN_V4.md) as listed below; everything else in V4 stands. The rule changes it requires are listed in [Rules Spec v0.2 changes](PERMUTATION_STATE_RULES_SPEC_v0.2_CHANGES.md). The UI changes are in [UI redesign V4.1](research/UI_REDESIGN_V4_1.md).

Evidence comes from `cargo run --release --bin sim -- 40` (permutation-server). It plays 40 six-bot Blitz seasons. The personas rotate across start positions each seed, and every bot decides from its own fog of war. The "proposed" numbers use the same matches, rescored with the V4.1 formulas. **The bots have not yet adapted to the new scoring, so these are lower bounds.**

---

## 1. What is wrong in V4 as built

### 1.1 The three tracks do not reward three ways of playing

| 40 seasons, current rules | Result |
|---|---|
| All three tracks won by different civs | **16 / 40** |
| One civ won two tracks | 23 / 40 |
| One civ won all three | 1 / 40 |
| Dominion winners | Warlord 16 · Diplomat 14 · Builder 7 · Scholar 3 |
| Concord winners | Diplomat 22 · Builder 7 · Scholar 7 · Warlord 4 |
| Science winners | Scholar 33 · Builder 7 |
| Cities captured by the end | about 0 in most seasons |

- **Dominion and Concord both measure size.** Dominion counts owned tiles; Concord counts total population. A civ that expands peacefully tops both, so the one-top-3 payout rule is the only thing spreading the prizes.
- **Conquest barely pays.** A captured city scores `5 × pop` per tick, only after being held 30 ticks, and makes the captor an aggressor, which zeroes its Concord for 12 ticks. So war is at best a way to take tiles.
- **Concord pays for doing nothing.** Population grows under the default governor with no orders at all. A civ that never allies also gets ×1.25.

### 1.2 Holes and bugs that decide outcomes

| # | Problem | Effect |
|---|---|---|
| H1 | Production stores are unbounded, and a city can complete several queue items in one tick | A civ can bank production and finish Star Gate I–III in a single tick, invisible to rivals until it happens |
| H2 | Several `Purchase` orders for the same city in one tick | Buys up to about 88% of an item at once, including Star Gate stages. Money should not buy a victory stage (V4 §4.10) |
| H3 | The `Transfer` cap is checked per order, not per tick | Several transfers multiply the cap |
| H4 | Ranged attacks take no retaliation, including from cities | Archers strip garrisons and walls for free; the rock-paper-scissors triangle is lopsided |
| H5 | The Science focus has no effect (a fixed-point product truncates) | A visible option that does nothing |
| H6 | A war declaration adds exactly the casus-belli threshold, and grievance decays in the same tick | The victim can never answer with a justified war |
| H7 | The Exchange allows self-trades and trades between civs at war, and delivers Production to any city | Money can reach a Star Gate city; wash trades |
| H8 | Participation counts any non-empty batch, including one holding only a free `RevealRationale` | "Activity" can be faked for free |
| H9 | In a long simulation, two units ended up on one tile (invariant 2 violated, seed 35, tick 79) | An engine bug in movement or capture ordering |

---

## 2. The V4.1 win paths

**One sentence per track:**
- **Dominion** is winning ground by force.
- **Science** is racing to the Star Gate.
- **Concord** is holding the world together: city-states, treaties and peace.

### 2.1 Dominion — conquest and territory
- **Per tick:** each owned tile scores 1, or 2 if it holds a resource (unchanged). **Each captured city scores `10 × pop`**, up from 5, once held for **12 ticks**, down from 30.
- The capture rules stay the same: the founder must be a different civ, the city must be at least 12 ticks old when captured, and a civ scores the same city at most once per season.
- **Why:** conquest now pays within the season and outweighs a few extra tiles, while the aggressor still loses Concord. Choosing Dominion is choosing war.

### 2.2 Science — the Star Gate
- **Unchanged ranking:** stages, then the earliest completion, then cumulative science.
- **New:** a civ's stages must be **at least 6 ticks apart**. Each completion is announced in the public chronicle.
- **Why:** rivals see each stage coming and have time to respond (H1). The race stays a race.

### 2.3 Concord — holding the world together
- **Per tick,** counted only when the civ was **active** (its batch had at least one order other than a reveal) and **not an aggressor**:
  - **20 × city-states it is suzerain of**
  - **+ 10 × civs it has a NAP or an alliance with**
  - **+ 2 × average population per city** (capped at 8)
- **Final score:** ×1.10 if the civ never joined an alliance (down from ×1.25). NAPs never cost the bonus.
- **Why:**
  - Concord now rewards diplomacy (envoys, bonds, treaties) instead of size.
  - The activity rule stops idle civs from farming it.
  - The smaller neutrality bonus keeps "stay unaligned" a real choice without punishing treaties.

### 2.4 Evidence (same 40 seasons, rescored)

| | V4 now | V4.1 proposed |
|---|---|---|
| All three tracks won by different civs | 16 / 40 | **27 / 40** |
| Dominion winners | Warlord 16 · Diplomat 14 | **Warlord 19** · Diplomat 11 · Builder 7 |
| Concord winners | Diplomat 22, spread across all | **Diplomat 31** · Builder 6 · Scholar 3 · Warlord 0 |
| Science winners | Scholar 33 | Scholar 33 (unchanged) |

- **Target for Phase 2:** at least **70%** of seasons with three different track winners, no persona winning all three, and Warlords winning Dominion by capturing cities, not only by expanding.
- **Where the rest must come from:** the rescoring reaches 68% without any bot changes. The remaining gap is bot behaviour: today's bots spend only **4–17% of their order budget** and end with thousands of unspent gold. Warlords must actually besiege and take cities.

### 2.5 Payouts
- The one-top-3-per-civ rule, the geometric weights and the participation share stay as they are.
- The track split stays the placeholder 30 / 25 / 30 / 15 (V4 §12 #4 remains open).

---

## 3. Rule fixes (details in the spec changes)

| # | Fix |
|---|---|
| H1 | A city completes **at most one** queue item per tick. Stored production is capped at the current item's remaining cost. Star Gate stages are at least 6 ticks apart (§2.2) |
| H2 | **One `Purchase` per city per tick.** Gold cannot buy Star Gate stages |
| H3 | The `Transfer` cap applies **per pair of civs per tick** |
| H4 | **Cities strike back at ranged attackers** with a share of their defence. Melee retaliation is unchanged |
| H5 | The Science focus: fix the fixed-point product so the Academy bonus applies |
| H6 | A war declaration's grievance is created **after** the tick's decay, so the victim can answer with casus belli |
| H7 | The Exchange forbids self-trades and trades between civs at war. **Exchange Production cannot go to a city building a Star Gate stage** |
| H8 | An "active tick", for participation and Concord, needs at least one order that is not a reveal |
| H9 | Find and fix the occupancy bug; add the failing seed as a regression test |

---

## 4. Play feedback the rules must supply

The UI changes need the engine to say what it did:
- **Skipped orders.** The world state records, for the tick just resolved, which orders of which civ were skipped and why (`last_skipped`: civ, order index, reason code). This is deterministic and part of the state root, so it is verifiable like everything else.
- **Projection.** The server computes "payouts if the season ended now" with the same `scoring::payouts` the program uses, so the numbers on screen are the rules' own.

---

## 5. Out of scope for V4.1, recorded as risks

- **Hidden orders.** Batches are plain text on the ER, so a last mover can react. The next step is commit-reveal of batches.
- **Resolver-chosen randomness.** It will be replaced by MagicBlock VRF.
- **Advisory fog.** It will be enforced with PER (TEE).
- **Crisis, Season Law, prize coalitions, and soft reset across seasons.** Still V4 roadmap.
