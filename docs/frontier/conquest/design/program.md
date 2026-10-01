# The conquest milestone: on-chain program design (area: program)

- **For:** the territory contest that the owner moved ahead of M2 money (owner decision, 2026-10-01). Goal: the faction map changes through play.
- **Base:** `codex/frontier` at `aae8617` (worktree `frontier-integ`, read-only), M1 program `permutation-frontier` and ABI `frontier-abi` as in M1-CONTRACT v1.13.
- **Rules followed:** this design follows `design/game-design.md` (the game-design area's proposal; its section numbers appear below as GD §x). Where the program needs a rule that GD leaves open, this note says so and gives a default.
- **Read:**
  - DESIGN rev 3.1: §3, §4, §5.4–§5.6, §6, §8.
  - M1-CONTRACT v1.13: §2 (I-17), §4–§10.
  - DECISIONS (T1, U-series); M1-EXIT-NOTES.
  - Kernels: `siege`, `geometry`, `laurel`, `clash` (`GarrisonResult`).
  - Program: `proc/{clash,transit,host,citizen}.rs`.
  - Simulator: `sim.rs` (`war`, `expand`, `capture`, `siege_done`).
- **Measured:** a lab probe on SBPF v2 under LiteSVM 0.16 (§11). No repository file was changed. No port, devnet or mainnet was used, and nothing was installed or downloaded (cargo `--offline`).
- **Tags:** [measured] = in the lab run; [M1 measured] = in M1's G1 table or exit report; [estimate]; [design] = a choice this note makes.

## オーナー向け要約 (Japanese summary)

- **作るもの:** 包囲 (DeclareSiege)、占領と解放、2・3 番目の拠点の奪取と破壊 (SettleCapture)、入植者による拠点 2・3 の建設 (SettleFounding)、貢納 (CollectTribute)、March (7 州) ごとの支配集計 (FoldMarch)。命令は合計 8 個 (タグ 0xA0–0xA7) です。
- **既存の命令も変えます:** 解決処理 (ResolveFromInputs と SkipQuiet) が毎ベル包囲を進め、勢力図用のスナップショットを 1 時間ごとに州アカウントへ書きます。
- **勢力図は「所有」ではなく「支配」で塗ります。** 最初の拠点は奪えないまま (D9)、占領中はその拠点の支配が占領側に移ります。州と March の支配はチェーン上で決まり、ヘラルドと Web はそれを描くだけです。
- **費用 (測定済み):**
  - 包囲の処理は 1 ベル・1 包囲あたり約 0.3k CU。最悪のケース (1 州で 12 件の包囲が同じベルに完了し、毎時スナップショットも重なる) で、解決処理に約 6.6k CU が加わります。M1 の最大値 271,673 に足しても、上限 290,000 に収まる見込みです。
  - March の集計は 1 回 8.3k CU です。
- **安全性:**
  - 誰でも凍結させられる経路はありません。
  - 後出しで結果を選べる場面もありません。完了は解決処理の中で決まり、後片付けは誰が実行しても同じ結果になります。
  - 世界全体で 1 つしかないアカウントに書く処理は増やしません。
- **見つけた問題:** M1 のコードのまま奪取時に世代番号を上げると、奪われた拠点の行軍記録が永久に精算できなくなります。この設計で対処しています (§6)。もう 1 つ、守備側の仲間が「わざと失敗する包囲」で免除期間を稼げます。ルールを 1 行足すことを推奨します (§9.5)。

---

## 0. The design in one page

1. **Versioning.**
   - The extension is **ABI v2**: a superset of v1. Every M1 offset, tag, error and log kind keeps its meaning.
   - The Province grows from 4,096 to **4,608 B** (a 512-B conquest block).
   - Other accounts gain fields only inside their reserved bytes. There is **one new account kind: MarchState** (`mc`).
   - A conquest season runs a new build, whose `RULESET_HASH` differs because the conquest kernels are bound into it. M1 seasons therefore stay on their own `.so`. The paused `m1-exit` stack keeps its binary (§1).
2. **Where conquest state lives.**
   - The **Province** is the single source of truth for each site's siege, occupation, capture and claim. It holds one 32-B record per site, plus 6 hourly control snapshots and capture counters.
   - It is written by the same resolve that writes the clash: ResolveFromInputs still writes only the Province and its ClashInputs.
   - Holdings and Citizens follow lazily through permissionless settle instructions.
3. **Eight new instructions:**

   | Tag | Instruction | Class |
   |---|---|---|
   | 0xA0 | DeclareSiege | P |
   | 0xA1 | SettleSiege (stake) | N |
   | 0xA2 | SettleCapture (capture or raze) | D |
   | 0xA3 | SettleFounding (settler claim → Holding) | D |
   | 0xA4 | CollectTribute | P |
   | 0xA5 | FoldMarch | D |
   | 0xA6 | RetireHost | N |
   | 0xA7 | CloseMarch | N |

   **About 15 M1 instructions change** (§5.2). The largest changes are in ResolveFromInputs and SkipQuiet (the conquest step), OpenProvince (Seam Towns), ReleaseDormant (Free Cities), the transit path (captured holdings) and the player prologue (the capture lock).
4. **Map layer.** A site's **control weight** is its tier weight (GD §3.1). It goes to the occupier while the site is occupied, and to neutral for Seam Towns and Free Cities.
   - Each Province **snapshots** its per-faction control weights at every hour boundary. The snapshot is taken inside its own resolve or skip, so it is deterministic and does not depend on lag.
   - **FoldMarch(m, hour)** combines the 7 member snapshots into MarchState: controller, contested flag, Dominion credit.
   - The herald and web read these fields; they do not recompute rules.
5. **Costs [measured, §11].**

   | Item | CU | Note |
   |---|---|---|
   | Conquest step in a resolve | +0.36k per active siege; +1.5k at an hour boundary | |
   | Worst resolve fill (12 completions + snapshot + log) | +6.6k | |
   | SkipQuiet, 24 bells, 12 active records | +75k | |
   | FoldMarch | 8.3k | 520-B transaction |
   | DeclareSiege's own checks | 6.7k | |
   | SettleCapture's bookkeeping | 8.1k | |
   | Code linked by the conquest probe | ≈ 78 KB | |

6. **Security.** Every conquest record reaches a terminal state through permissionless instructions (§9.1). Siege completion is decided inside the resolve, from the roster frozen at bell start (§9.2). No new global account is written during play (§9.3). The new init paths are pre-funding-safe, and every keyed read recomputes its address (§9.4).

---

## 1. Versioning: how a frozen ABI is extended

- **What is frozen.** `frontier-abi` v1 (contract §5.3): byte layouts, tags 0x01–0x70, errors 1–61, log kinds 1–70, entity kinds 1–7, seed tags, and `ABI_VERSION = 1` in every chained header's `layout_version`.
- **The extension is ABI v2,** under a new contract (working name C1-CONTRACT v1.0). It is a strict superset:
  - **No v1 offset moves.** v2 writes `layout_version = 2` in the chained header, and every v2 reader accepts only 2.
  - **The Province grows to 4,608 B.** The v1 bytes 0..4,096 are unchanged, including the 232-B reserve at 3,864 that M2 and M3 keep.
  - **Other accounts keep their sizes.** New fields go only into bytes the v1 tables mark reserved (Holding 1,248..1,280; Citizen 145 and 150; JoinShard 108..128; Season 896..1,024; site mirror 5..7 and 28..31).
  - **New numbers:**
    - tags 0xA0–0xA7, with 0xA8–0xAF reserved for later conquest work (Raid, AutoReinforce);
    - errors 62–75;
    - log kinds 80–88;
    - entity kind 8 (MarchState);
    - seed tag `mc`.
- **Per-season, not in place.**
  - The program embeds `RULESET_HASH`, and every prologue refuses a Season with a different hash (`RulesetMismatch`).
  - The conquest kernels (siege v3, a new `conquest` module, catalog changes for Settlers) are bound into that hash. So **a v2 program can never act on a v1 season, and v1 accounts are never migrated.**
  - A conquest season is announced and created on the v2 build. "Everything ends with the season" (T1) makes this the natural cut: nothing crosses a season boundary.
  - The `m1-exit` stack paused on 41000–41099 keeps running its own `.so` (`d85e1bd7…2281`). Conquest stacks use a separate localnet in 41300–41999.
- **`SeasonParams` v2.**
  - Created by CreateSeason when `program_version = 2`.
  - The 128-B conquest parameter block goes at Season 896..1,024. Season 516..896 stays reserved for M2/M3.
  - Fields (defaults: GD §3.10 Frontier-28 / Frontier-7):
    - `conquest_enabled u8`
    - `heartland_max_ring u8` (3 / 2)
    - `occupation_max_bells u32` (288 / 72)
    - `immunity_bells u16` (36)
    - `siege_stake_gold u32` (500)
    - `sieges_per_day u8` (4)
    - `extra_shield_bells u16` (12)
    - `free_city_window_bells u32` (288 / 72)
    - `claim_ttl_bells u16` (24)
    - `shield_secs u32`, `shield_late_secs u32`, `shield_late_after_secs u32`
    - `frontier_protect_secs u32`, `frontier_protect_after_secs u32`
    - `control_weight_tenths [4] u8` (10, 13, 16, 20)
    - `occupation_controls u8` (1)
    - `dominion_contested_q u8` (4), `dominion_home_q u8` (1)
    - `seam_towns u8` (1)
    - `retire_hosts u8` (1)
  - CreateSeason validates their ranges. The two M1 presets keep `program_version = 1`.
- **What the integrator regenerates:** `budgets.rs` (`MEASURED`, `L(kind)`), presets, every vector file (`addresses.json`, `logs.json`, layouts) and the web SDK. M1 contract §3.3's rule stands: no constant is copied by hand.

---

## 2. Accounts

### 2.1 Province v2 (4,608 B; `pv‖P,Q`; created by OpenProvince from its wedge's ProvinceFund)

M1 bytes 0..4,096 are unchanged. Two v1 byte ranges gain a meaning:

- **Site mirror (64 B per site):**
  - `state` gains **5 = neutral town** (Seam Town or Free City) and **6 = claimed** (a settler won the site; the Holding is not created yet).
  - **5 `tier_next u8`, 6 `neutral_kind u8`** (1 Seam Town, 2 Free City), 7 rsv. These replace v1 rsv 5..8.
  - **28 `tier_next_bell u32`** replaces v1 rsv 28..32. It is the bell from which `tier_next` counts for control, written by a tier-up Build.
- **The conquest block at 4,096 (512 B):**

| Off | Size | Field |
|---|---|---|
| 4,096 | 384 | `conquest [12] × 32 B`: one record per site (below) |
| 4,480 | 108 | `snap [6] × {hour u32, weight [7] u16}`: control weight per faction 0–5 and neutral (6) at the first bell of `hour` (bell `6 × hour`); ring slot `hour mod 6` |
| 4,588 | 12 | `captures_by [6] u16`: completed captures of holdings in this province, by captor faction (cumulative; Dominion input) |
| 4,600 | 8 | rsv |

**Conquest record (32 B)** — one per site, a tagged union on `kind`:

| Off | Field | `kind` 0 none | 1 siege | 2 occupation | 3 capture due | 5 claim |
|---|---|---|---|---|---|---|
| 0 | `kind u8` | — | — | — | — | — |
| 1 | `faction u8` | — | attacker | occupier | captor | claimer |
| 2 | `flags u8` | bit 1: stake owed to the site's holding; bit 2: stake owed to `src` | bit 0: held; bit 3: no vigil (neutral) | — | — | — |
| 3 | `progress u8` | — | progress | — | — | — |
| 4 | `required u8` | — | ≤ 84 (`siege::MAX_REQUIRED_BELLS`) | — | — | — |
| 5 | rsv | | | | | |
| 6 | 6 B | — | vigil snapshot `start_min u16, next_min u16, from_day u16` | `last_collect_bell u32` (tribute) | — | — |
| 12 | `bell u32` | holding site: `immune_until_bell`; neutral town: `neutral_until_bell` (`u32::MAX` = Seam Town) | declared bell (counting starts at `bell + 1`) | occupation start | completion bell | claim bell |
| 16 | `actor u64` | — | declarer `citizen_tag` | occupier tag | captor tag | claimer tag |
| 24 | `src u64` | stake return target (when bit 2) | the declarer's source Holding key | tribute recipient Holding key | stake return target | the settler's Holding key |

A "Holding key" is the host-id form `province_index << 44 | site << 40 | gen << 32` (contract §4.1). It names a Holding together with its generation.

- **Size and rent.**
  - (128 + 4,608) × 5,080 = **24,058,880 lamports**, which is +2,600,960 (+0.0026 SOL) per province against M1.
  - About 170 provinces in the 7-day 1,000-bot season adds +0.44 SOL. At 50k players, 7,644 provinces add +19.9 SOL of refundable ProvinceFund float. At `R_MAX` 64 full, 12,474 provinces add +32 SOL (+12% on DESIGN §3.4's 268 SOL).
  - OpenRing's fund check becomes `d × rent(4,608)`.
- **Init.** Still `init_funded` from `pf(w)`. 4,608 B is under the 10,240-B CPI allocation limit. The pre-funded test is unchanged in kind.
- **Close.** Unchanged (CloseProvince → `pf(w)`, after end + 72 h). Records die with the account. A stake still owed at close is lost: it is a game point, not money.

### 2.2 Holding (1,280 B, unchanged size): reserved bytes 1,248..1,280

| Off | Field |
|---|---|
| 1,248 | `prev_owner_tag u64`: the citizen it was captured from (0 = never captured) |
| 1,256 | `prev_gen u8`: the generation of the victim's hosts (accepted by the transit path, §6) |
| 1,257 | `capture_flags u8`: 1 captured, 2 razed (awaiting CloseHolding) |
| 1,258 | rsv u16 |
| 1,260 | `captured_bell u32` |
| 1,264 | `prev_home u64`: the victim's first-holding key (where the victim's hosts retire) |
| 1,272 | rsv 8 |

The M1 field `order u8` (113) gains values 2 and 3. Fields `shield_until` and `founded_ts` / `founded_day` are reused for holdings 2–3 and for captures.

### 2.3 Citizen (384 B, unchanged size)

- 145 `sieges_today u8` and 150 `siege_day u16`: DeclareSiege's daily cap. These are v1 rsv bytes; M2's money reserve at 312..384 is untouched.
- The Holding-rent escrow fields (`ticket_escrow`, `ticket_funder`) are reused as the **holding escrow** for settler marches and for neutral captures (M1 pattern: kept for the next use, returned by CloseCitizen).
- `holding [3]` and `holdings_n` are used as designed (M1 only ever filled slot 0).

### 2.4 JoinShard (256 B)

- 108 `extra_holdings u32` (live holdings 2–3)
- 112 `captured_in u32`
- 116 `captured_out u32`
- 120 `razed u32`
- 124 `founded_by_settler u32`

M2's sums move to 128+. FoldOccupancy adds `extra_holdings` to `occupied_sites`, so Join's reserve and the 20% founding gate see every holding.

### 2.5 MarchState (new; 256 B; `mc‖m i32, n i32`, seed 18 B; chained header, entity kind 8)

| Off | Field |
|---|---|
| 0 | H (64): magic `PSF1MRCH`, season, layout_version 2, seq, head |
| 64 | `m i32, n i32` |
| 72 | `next_hour u32`: the next hour to fold (hours fold strictly in order) |
| 76 | `controller u8` (0–5; 6 neutral; 0xFF contested or open), `contested u8`, `last_flip_hour u32` (at 78, unaligned is fine) |
| 82 | `weight [7] u16`: the last folded hour's weights |
| 96 | `dominion_q [6] u32`: Dominion in quarter-hours (contested ×4, uncontested home ×1, GD §3.9) |
| 120 | `control_hours [6] u32` |
| 144 | `captures [6] u16`: Σ of the members' `captures_by`, overwritten at each fold |
| 156 | `lost_hours u32`: hours folded without credit because a member's snapshot had been overwritten (§4.3) |
| 160 | `rent_to [32]` |
| 192 | rsv 64 (M3: Warden roll counts per faction) |

- **Rent:** 1,950,720 lamports.
- **Created by** the first FoldMarch of the March: `init_with_seed`, the fee payer pays, pre-funding-safe. `next_hour` is set to the first hour in which any present member had opened (`⌈min opened_bell / 6⌉`), so creation cannot skip hours.
- **Closed by** CloseMarch (0xA7, N) after end + 72 h, to `rent_to`.
- **Count:** about the number of provinces / 7, plus partial Marches at the rim (≈ 30 in the 7-day demo, ≈ 1,100 at 50k players).

### 2.6 Season (2,048 B): `SeasonParams` v2 at 896..1,024 (§1). FoldOccupancy is unchanged in shape.

### 2.7 No other new kinds

- **No ScoreBoard during play.** Faction Dominion is Σ over MarchStates of `dominion_q`, divided per active member. In this milestone the herald and the verifier compute it (game points). An on-chain `TallyDominion` into a ScoreBoard is an **M2 item**: it is needed only when payouts read it. It would be written only after `end_bell`, with one bitmap bit per March so that each March is tallied exactly once.
- **No per-siege accounts.** Keeping sieges inside the Province keeps the resolve at two writable accounts and removes a whole class of address squatting.

---

## 3. The site state machine

```
                          DeclareSiege (P, from the hex)
  holding / neutral  ───────────────────────────────────▶  SIEGE (counts bells > declared)
        ▲   ▲                                               │        │          │
        │   │ RFI: besiegers lose the hex                    │        │          │ season ends
        │   └──────────── FAILED: kind 0, immunity, ◀───────┘        │          └─▶ SettleSiege: stake → src
        │                 stake owed → defender                       │
        │                                       RFI: progress ≥ required (outside vigil)
        │                         ┌─────────────────────┴────────────────────┐
        │                order 1 (first holding)                 order 2–3 / neutral town
        │                         ▼                                          ▼
        │      OCCUPATION (owner keeps the Holding;      CAPTURE DUE: mirror owner and gen flip
        │      control → occupier; tribute)              at b+1; owner actions locked
        │                         │                                          │ SettleCapture (D, anyone)
        │   RFI: occupier gone / owner wins the hex / max length            ├─▶ capture: Holding → captor
        └─────────────── LIBERATED: kind 0, immunity ◀─┘                    └─▶ raze: site free, CloseHolding
  free site ── settler Stays on it (RFI lottery) ──▶ CLAIMED ── SettleFounding (D) ──▶ holding 2–3
                                                          └── TTL passed ──▶ free
  ReleaseDormant ──▶ neutral Free City ── window passed (lazy) ──▶ free
```

Every arrow out of a non-terminal state is a permissionless instruction or the resolve itself (§9.1).

---

## 4. The conquest step, control and the March fold

### 4.1 The conquest step (inside ResolveFromInputs and SkipQuiet)

- **Where it lives.** A pure function `conquest_model::step(province, bell, report_of, season)` in `frontier_abi`, next to `clash_model`. It is run by the program, the herald, the verifier, itest and the WASM client (M1's W4-A D8 pattern). ResolveFromInputs calls it after `apply` and `settle_bell`. SkipQuiet calls it for every skipped bell.
- **The report** for each record's site:
  - **Resolve:** read from the kernel's `out.garrisons` (`GarrisonResult.holders`, `defender_present`), binary-searched by the garrison id = `host_id(P, Q, site, gen, 0)`.
  - **Skip:** from the **quiet model**: one pass over the entries builds a per-tile faction mask of non-civilian residents, cached by `roster_epoch`. `holders = mask & !owner_bit`, `defender = mask & owner_bit ≠ 0`. In a quiet bell every resident stays on its tile, so this equals the kernel's report. The **G11 gate is extended** to require resolve and skip of the same quiet bell to be byte-identical across the whole 4,608-B Province.
- **Per record, for bell b (only if b > record.bell):**

| Kind | Rule | Result |
|---|---|---|
| Siege | Rebuild `Siege { last_bell: b − 1, held: flag, progress, required, status: Active }` and the owner's `Vigil` from the snapshot | Kernel `Siege::advance(b, bell_start(b), report, vigil)`. **Unchanged rules:** progress only while the attacker holds the hex, no defender is present and the bell is outside the vigil. "Fails" when the besiegers do not hold the hex. Because declaration needs a host on the hex (GD §3.4), `held` starts true, and the kernel's 72-bell start window never applies |
| Siege, failed | — | Kind becomes 0; `bell = b + 1 + immunity_bells` (holding sites only); stake owed to the site's holding (neutral: burned). Event `FAILED` |
| Siege, completed on a first holding | Order 1 | Kind becomes **2 occupation** (`bell = b`, `last_collect = b`). Event `OCCUPIED` |
| Siege, completed otherwise | Order 2–3 or a neutral town | Kind becomes **3 capture due**. The site mirror flips at once for the next bell: `faction = attacker`, `gen += 1`, `garrison = 0`, pending garrison slots cleared, walls halved (unless the attacker's doctrine keeps them, F: Iron), `shield_until_bell = 0`; `captures_by[attacker] += 1`. Event `CAPTURE_DUE` |
| Occupation | Ends when any of: no occupier-faction combatant on the hex (`holders & bit == 0`); the owner's side holds it (`defender && holders == 0`); `b ≥ bell + occupation_max_bells` | Kind 0 with immunity. Events `LIBERATED_GONE`, `_WON`, `_EXPIRED`. **The occupier sits in a hostile slot, and the owner's side keeps the holding hex's 3 owner slots** (GD C17; the kernel's rule as is, because the mirror owner does not change) |
| Claim | `b ≥ bell + claim_ttl_bells` | Lapses: site back to free. Event `CLAIM_LAPSED` |
| Neutral town | Free City with `neutral_until_bell ≤ b` | The site becomes free (state 3). Event `FREE_CITY_EXPIRED` |

- **Settler founding (in the resolve only).**
  - Among this bell's arrivals with unit Settler whose fate is `Stays` on a site tile in state free, released-free or expired Free City, and where no hostile combatant holds that hex, the lottery `rand(S, "found", P‖Q‖site‖citizen_tag)` picks one per site.
  - The winner's site becomes **6 claimed** with a kind-5 record, and its settler entry is freed (consumed). The losers' fates become `Bounced` (home, no loss). Event `CLAIM`.
  - The fate table in ClashInputs records the outcome, so SettleTransit settles the winner as "stays, consumed" and the losers as bounced.
- **The hour snapshot.** If `b mod 6 == 0`, compute `weight[f]` over the 12 sites and write ring slot `(b/6) mod 6`:
  - Each site in state 1 (holding) or 5 (neutral) adds `control_weight_tenths[tier at b]`. The tier is `tier_next` if `tier_next_bell ≤ b`.
  - The weight goes to the occupier if the record is an occupation and `occupation_controls`, to neutral (6) for state 5, and otherwise to the mirror faction.
  - Claimed (6) and free sites add nothing.
- **The log.** One `CONQUEST` record (§7) per resolved or skipped bell that has any event, any active record or a snapshot.
- **Work bound.** Active records per province ≤ 12. In SkipQuiet the step adds ≈ 260 CU per active record per bell and ≈ 1.5k per hour boundary [measured]. **SkipQuiet stops and commits its prefix** once `bells × active records` would pass 288 in one transaction. This is the same commit-the-prefix rule as M1's work bound (I-50), with no new failure mode.
- **Season end.** No resolve or skip exists for bells ≥ `end_bell` (M1 rule), so no record advances past `end_bell − 1`. Sieges still active at the end are closed by SettleSiege ("no winner; stake returns", GD §3.11). Occupations stop accruing tribute at `end_bell` (CollectTribute caps there). The control snapshot of the last hour before `end_bell` is the final map.

### 4.2 Why the March fold reads hourly snapshots taken by each Province

- **Per-bell March control cannot be computed exactly on chain.**
  - It is a non-linear function (≥ 50% of the summed weight) of 7 Provinces that resolve on their own schedules.
  - A MarchState written by each member's resolve would need a consistent cut across the 7 Provinces, which lagging members cannot give.
  - It would also put a seventh-of-a-March write lock on every resolve.
- **Sampling at fold time lets the folder choose the moment.** A FoldMarch that read the members' current state would credit whoever controls the March at that instant with the whole interval. That is a timing choice open to the controller (a last-look on credit).
- **The fix is to sample at a fixed bell.** Each Province records its own control weights at bell `6h`. It does so inside the transaction that resolves or skips that bell, which every Province processes exactly once and in order, whatever its lag. FoldMarch(m, h) only combines 7 such samples, so no caller can choose what is credited.
- **Residual.**
  - Control-relevant events that happen in wall time (SettleTicket, SettleFounding, SettleCapture, ReleaseDormant, Build tier-up) land in a Province's mirror when their transaction lands. A snapshot taken late therefore sees them early, by at most the Province's resolve lag at that moment.
  - Everything decided by the clash (occupy, liberate, capture due, claim) is bell-exact. The capture's mirror flip is bell-exact too; only the Holding bookkeeping is lazy.
  - The skew is ≤ about 3 bells while keepers resolve promptly. Exploiting it means holding a Province (delay-only, priced, §9.3) to move one event across one hour boundary. The gain is ≤ 1 March control-hour.

### 4.3 FoldMarch semantics

- Hours fold strictly in order (`hour == next_hour`). A transaction may fold up to 6 hours.
- For each of the 7 members (canonical `pv‖P,Q` from `march_members(m)`):

| Member state | Treatment |
|---|---|
| Absent (ring not open) | Weight 0 |
| `opened_bell > 6h` | Weight 0 |
| `resolved_next ≤ 6h` | **`TooEarly`**: the keeper resolves or skips it first. Lag only waits |
| Ring slot holds hour h | Add its weights |
| Ring slot has moved past h (the member ran more than 5 hours ahead of the fold) | The hour folds as **lost**: `lost_hours += 1`, no credit to anyone |

- **Credit.**
  - `controller` = the faction f with `2 × weight[f] ≥ total` and `weight[f] > 0`; otherwise contested or open.
  - `contested` = two or more parties have weight, or the controller's weight lies outside its own wedge.
  - `dominion_q[c] += 4` if contested, `+1` if uncontested home (GD §3.9); `control_hours[c] += 1`.
  - `captures[f]` = Σ members' `captures_by[f]` (overwrite).
- **"Lost" is the only way a held account changes a fold's credit.** It costs holding MarchState (or keeping a member's resolves going while holding MarchState) for more than 5 hours: ≈ $75k–204k per lost March-hour at the D-class price (§9.3). A **K = 6** ring was chosen to cover that skew within 108 B. Raising K costs 18 B per hour per Province.

---

## 5. Instructions

Common rules (unchanged from M1):

- player prologue P or keeper prologue K (contract §5.6);
- every keyed account is checked at its canonical with-seed address;
- pre-funding-safe init;
- `pay_or_divert` with sink `Holding.pool_owed`;
- no W or D instruction writes the DefencePool.

Budgets are G1 gates at the adversarial fill named, on the release `.so`. **"est."** marks a number derived from the lab measurement plus M1-measured analogues (§10).

### 5.1 New instructions (tags 0xA0–0xA7)

#### 0xA0 DeclareSiege — class P (via the relay), not top-level-only

- **Accounts:**
  - `[0 actor s] [1 payer s,w] [2 season r] [3 citizen w]`
  - `[4 src_holding w]`: the declarer's holding that owns the host on the hex; it pays the stake
  - `[5 province w]`: the target's
  - `[6 target_holding r]`: canonical; must be absent for a neutral town
  - `[7 owner_citizen r]`: canonical `ct‖tag`; absent for a neutral town
  - `[8 nearby_province r]`: canonical; any absent address when not needed
  - `[9 system]`
- **Data:** `site u8, entry u8, nearby_site u8` (4 B with the tag).
- **Checks, in order:**
  1. The prologue (Running, `bell < end_bell`, citizen, session, bucket). `src_holding` belongs to the citizen, is final, and passes the capture lock (§6.3); lazy settle.
  2. `province` canonical; resolved through b − 2 (`NotResident`); `site < site_count`; the mirror state is 1 (holding) or 5 (neutral) (`NotBesiegeable` 70).
     - **State 1:** `target_holding` canonical and present, `gen == mirror.gen` (`CapturePending` 62), and `owner_citizen == holding.owner_citizen` (`BadAccount`).
     - **State 5:** both absent at their canonical addresses (a Free City has no Holding).
  3. **On the hex:** entry `entry` is in state 1 with `from_bell ≤ now_bell`; its faction is the citizen's; its tile is the site's tile; it is not civilian; it has no pending Spend, Leave or Forfeit; and its host id names `src_holding` at its current generation. Otherwise `NotOnHex` (67).
  4. **The record:**
     - `kind == 0`, else `SiegeBusy` (64);
     - no stake owed, else `StakeUnsettled` (65);
     - for state 1, `immune_until_bell ≤ now_bell`, else `Immune` (63);
     - the citizen's `sieges_today < sieges_per_day` for `day(now)`, else `SiegeCap` (66).
  5. **`may_besiege`** (kernel v3), with:
     - `kind` = First (order 1), Other (order 2–3) or FreeCity (state 5);
     - relation Rivalry; March flags false (the M3 defaults);
     - `heartland_max_ring` from the Season (kernel change: `SiegeCheck.heartland_max_ring`);
     - shield and founding fields from `target_holding`;
     - dormant = `now ≥ last_owner_action + dormant_after_secs`;
     - `attacker_nearby` = `nearby_province` lies within 2 provinces and its `nearby_site` is a **first holding (order 1)** of the attacker's faction (GD C3).

     Refusals map to: Seat → `ReservedSite`; Friendly → `Friendly` (71); Shielded → `Shielded`; FrontierProtected → `FrontierProtected` (72); Heartland → `Heartland` (73).
  6. `required = siege::required_bells(walls at now, doctrine extra 0)`; `now_bell + required ≤ end_bell`, else `TooLate` (68).
  7. **Stake:** `src_holding` pays `siege_stake_gold` Gold (kernel `Holding::pay`, `Insufficient`).
     - **Neutral target:** the payer tops up `citizen.ticket_escrow` to `rent(1,280)` by System transfer, so that a later SettleCapture can create the Holding without the captor's signature. `ticket_funder = payer` if topped up (M1 FileTicket rule).
- **Effects:**
  - The record is written: kind 1, attacker = citizen faction, `flags.held = 1`, `progress = 0`, `required`, the vigil snapshot from `owner_citizen` (start, next, from_day, after M1's lazy roll-forward; "no vigil" for neutral), `bell = now_bell`, `actor = citizen_tag`, `src = src_holding key`.
  - Citizen siege counters.
  - Log `SIEGE_DECLARED`, chained Province, Citizen and Holding (src).
- **Budget:** est. **≈ 25k** → gate **30,000**.
  - Measured checks and write: 6.7k.
  - Plus the prologue and Holding settle (Harvest-like): 16.4k [M1 measured].
  - Plus the escrow transfer: ≈ 1.5k.
  - Transaction ≈ 560 B, 12 keys.
- **Why the vigil is snapshotted at the horn.** The siege uses the defender's schedule as it stood at declaration, including a change already requested. A change requested during the siege applies to the next siege. So the defender cannot lengthen the pause after seeing the horn. That would be a last-look; the kernel's 24-h notice alone does not stop it within a 14-h siege.

#### 0xA1 SettleSiege — class N, anyone

- **Accounts:** `[0 payer s] [1 season r] [2 province w] [3 recipient_holding w (canonical; may be absent)]`.
- **Data:** `site u8`.
- **Checks:**
  - **(a) Stake owed:** a stake is owed (flags bit 1 → recipient = the site's holding at its mirror gen; bit 2 → recipient = `src` key).
  - **(b) Season ended:** the record is kind 1 and the season has ended (effective Ended), so the siege finished with no winner and the recipient = `src`.
  - A recipient that is absent, or of another generation, means the stake is **burned** (logged).
- **Effects:** recipient `Holding::settle(now)`, then Gold += stake, capped at the store cap (excess burned). Flags cleared; case (b) also sets kind 0. Log `SIEGE_SETTLED`.
- **Budget:** est. ≈ 18k → gate 25,000.

#### 0xA2 SettleCapture — class D, anyone (the keeper; the captor's client usually sends it first)

- **Accounts:**
  - `[0 fee_payer s,w] [1 season r] [2 holding w]` (canonical `ho‖P,Q,site`; absent for a neutral town)
  - `[3 province w] [4 captor_citizen w] [5 captor_joinshard w]`
  - `[6 victim_citizen w] [7 victim_joinshard w] [8 victim_rent_payer w]`: canonical absent placeholders for a neutral town
  - `[9 stake_holding w]`: the record's `src`; may be absent
  - `[10 system]`
- **Data:** `site u8, beneficiary [32]` (liveness log only; no reward).
- **Checks, in order:**
  1. The record is kind 3 (else `AlreadyDone` if kind 0, `TransitState`-like `NotDue` 74 otherwise).
  2. `captor_citizen` canonical, with `citizen_tag == record.actor`.
  3. **Holding site:** `holding.gen + 1 == mirror.gen` (else `AlreadyDone`); `victim_citizen == holding.owner_citizen`; `victim_rent_payer == holding.rent_payer`; both JoinShards canonical for their citizens' (faction, shard).
- **Effects:**
  - **Capture.** Applies when the captor's `holdings_n < 3` and `ticket_escrow ≥ rent(1,280)`, or for a holding site, when the escrow covers the rent swap.
    - **Bonds:** every victim transit record in state 1–3 with its bond flag gets its bond refunded to `victim_rent_payer` and the flag cleared (§6.2).
    - **Holding fields:**
      - `prev_owner_tag = victim tag`, `prev_gen = old gen`, `prev_home = victim.holding[0] key`, `captured_bell`;
      - `owner_citizen = captor`, `gen = mirror.gen`, `order = holdings_n + 1`;
      - `reserve [8] = 0` (trained troops do not change hands [design]);
      - `last_owner_action = now`, `shield_until = 0`;
      - `rent_payer = captor.ticket_funder`.
    - **Rent swap:** `rent(1,280)` moves from `captor_citizen.ticket_escrow`, which is program-owned lamports, to `victim_rent_payer` by `pay_or_divert`. The victim's funder gets the Holding's rent back, and the captor's funder now carries it.
    - **Citizens:** the victim's holding list loses the entry and `holdings_n −= 1`; the captor's list gains it.
    - **JoinShards:** `captured_out`, `captured_in`, `extra_holdings` and `holdings` adjusted.
    - **Stake:** returned to `stake_holding`.
    - **Record:** kind 0, `immune_until = now_bell + immunity_bells`.
  - **Neutral capture.** The Holding is `init_funded` from the captor's escrow: Hamlet at the mirror tier, empty stores, order `holdings_n + 1`, `gen = mirror.gen`, `rent_payer = ticket_funder`. Mirror state 5 becomes 1.
  - **Raze.** Applies when the captor is at 3 holdings or the escrow is short (GD §3.6).
    - Site mirror: state 3 released-free, faction NEUTRAL.
    - Holding: `capture_flags = razed`, state 3. It is **closed later by CloseHolding (N)**, which may write the DefencePool for `pool_owed`. SettleCapture stays a D instruction that never writes the pool (I-48).
    - The victim's list and JoinShards are updated; the stake is returned.
  - Log `CAPTURE_SETTLED` (outcome 0 capture, 1 raze, 2 neutral capture, 3 neutral raze), chained Holding, Province, both Citizens and both JoinShards.
- **Budget:** est. ≈ 16k for capture, ≈ 21k for a neutral capture with init → gate **30,000**. The measured bookkeeping is 8.1k, plus the K prologue, presence checks and the init CPI. Transaction ≈ 560 B.

#### 0xA3 SettleFounding — class D, anyone

- **Accounts:** `[0 fee_payer s,w] [1 season r] [2 province w] [3 holding w (canonical, absent)] [4 citizen w] [5 joinshard w] [6 frontier r] [7 system]`.
- **Data:** `site u8, beneficiary [32]`.
- **Checks:**
  - The record is kind 5 and within its TTL (else lapse: the site becomes free, `AlreadyDone`).
  - `citizen_tag == actor`; `holdings_n < 3`; `ticket_escrow ≥ rent(1,280)`.
  - The folded free sites are ≥ `extra_free_bps` of the open sites (DESIGN §3.4; the same check as at Depart).
  - If any check fails, the claim lapses and the site becomes free.
- **Effects:**
  - The Holding is `init_funded` from the escrow, as SettleTicket's fresh path: `Holding::found(now, day, order = holdings_n + 1)`, no starter kit (holdings 2–3 [design]), `shield_until = now + extra_shield_bells × 600`, final at once. A settler march is already sealed and lottery-settled, so no ticket cohort is needed.
  - Site mirror: state 1, faction, order, Hamlet, `gen += 1`.
  - Citizen list; JoinShard `extra_holdings += 1`, `founded_by_settler += 1`; record kind 0.
  - Log `FOUNDING_SETTLED`.
- **Budget:** est. ≈ 22k (SettleTicket fresh measured 22,383 [M1 measured]) → gate 40,000.

#### 0xA4 CollectTribute — class P

- **Accounts:** prologue + `[4 recipient_holding w (= record.src at its gen)] [5 province w] [6 occupied_holding w]`.
- **Data:** `site u8`.
- **Checks:** the record is kind 2 with `actor == citizen_tag`; `occupied_holding` canonical, at the mirror gen, not capture-pending; `recipient_holding` belongs to the citizen.
- **Effects:**
  - `t1 = min(now, bell_start(end_bell))`. The occupied Holding is settled to `t1`.
  - `tribute[r] = min(stock[r], 20% × production[r] × (t1 − bell_start(last_collect)))`. Production is the Holding's current rate, the same simplification the simulator makes.
  - The tribute is paid; the recipient is credited up to its caps (excess burned); `last_collect = bell(t1)`.
  - Tribute not collected when an occupation ends is lost [design; stated on the join page].
  - Log `TRIBUTE`.
- **Budget:** est. ≈ 30k (two Holding settles) → gate 35,000.

#### 0xA5 FoldMarch — class D, anyone

- **Accounts:** `[0 fee_payer s,w] [1 season r] [2 march w] [3..9 province × 7 r]` (canonical in `march_members` order; absent allowed) `[10 system]`.
- **Data:** `m i32, n i32, hour u32, count u8 (1–6), beneficiary [32]`.
- **Checks:**
  - The season is Running, or Ended with `6 × (hour + count) ≤ end_bell`.
  - The March is canonical; if absent, init (§2.5); `hour == next_hour` (`OutOfOrder`).
  - Per §4.3.
- **Effects:** §4.3; log `MARCH_FOLD` per hour.
- **Budget:** **8,335 CU measured** for one hour with 7 present members (canonical addresses, snapshot scan, controller, write, log). Est. ≈ 14k with the prologue and first-fold init → gate 20,000.
- **Size and limits:** transaction 520 B measured (≈ 600 B with the system program and data). `L(foldmarch)` ≈ programdata + 45 + 7 × 4,672 + 2,112 + 320 + 64 → about one page more than Reveal's.

#### 0xA6 RetireHost — class N, anyone

- **Accounts:** `[0 payer s] [1 season r] [2 province w] [3 captured_holding r] [4 home_holding r]`.
- **Data:** `entry u8`.
- **Checks:** `retire_hosts`; the entry's host id names `captured_holding`; `id.gen == captured_holding.prev_gen` and `≠ gen`; `home_holding` key `== prev_home`, present at its generation; the entry is in state 1 with no pending op.
- **Effects:** pending op **Leave** issued at `bell(now)`, with `op_a = 1` (retire). It takes effect after that bell's clash (roster freeze), and the existing return settle (SettleDeparture 0xFF, extended to accept `prev_home` as the credited Holding when `op_a = 1`) credits `reserve` to the victim's first holding.
- **Budget:** est. ≈ 12k (DisbandStranded analogue) → gate 15,000.
- **If `retire_hosts = 0`:** M1's DisbandStranded applies as is and the troops are lost. That is the cheap fallback of GD decision 6.

#### 0xA7 CloseMarch — class N

- **Accounts:** `[any s] [season] [march w] [rent_to w]`.
- **Checks:** Ended and `now ≥ end + 72 h`.
- **Effects:** close to `rent_to`; log `CLOSE`.
- **Budget:** 8,000.

### 5.2 Changed M1 instructions (ABI v2 shapes)

| Tag | Instruction | Change | Budget impact |
|---|---|---|---|
| 0x01 | CreateSeason | `SeasonParams` v2 (§1); validates conquest ranges | none material |
| 0x20 / 0x22 | OpenRing / OpenProvince | Fund check at `rent(4,608)`. **Seam Towns:** pure kernel `conquest::seam_town(ring_seed, p) → Option<{site, tier, garrison, walls}>` (rules area: one per seam per ring band) sets site state 5, `neutral_kind 1`, `bell = u32::MAX` | est. +5k (148,459 measured → < 160k; gate 220k unchanged) |
| 0x35 | ReleaseDormant | The site becomes a **Free City** (state 5, `neutral_kind 2`, `garrison = FREE_CITY_GARRISON`, tier kept, `bell = now_bell + free_city_window_bells`), not released-free; still closes the Holding (N) | small |
| 0x36 | CloseHolding | Also during the season for `capture_flags = razed` | none |
| 0x40–0x42 | Harvest / Build / Train | **New account `[province r]`** (the Holding's own) for the **capture lock** (§6.3). Build tier-up writes `tier_next` and `tier_next_bell = bell_at(done_at) + 1` (Province `w` for tier-up, as for walls). Train allows **Settler** (kernel catalog: settler cost `SETTLER_COST × duplicate_cost(n)`, sim `expand`) | +≈ 1.2k each. **Harvest 16,409 and Train 16,641 exceed their 17,500 gates** → gates 19,000 / 23,500 (Build) / 19,000 |
| 0x43 | Muster | Allows a Settler host (civilian; fixed 100 troops [design]; kernel `Host::muster`) | none |
| 0x50 | Depart | **Settler hosts:** `holdings_n + open claims < 3`; reads `[frontier r]` for the 20% free-site gate; the payer tops up `ticket_escrow` to `rent(1,280)`. Shield rule unchanged | 23,167 measured + ≈ 3k → gate 24,500 → **28,000** |
| 0x51 | Reveal | Shield rule step 6: **neutral towns are always allowed targets**, also from a shielded holding (GD C18) | none |
| 0x52 | SettleDeparture (+ return settle) | Accepts `id.gen == prev_gen` on a captured Holding; a retire Leave credits `prev_home` | small |
| 0x54 | SettleTransit | For a captured Holding with `id.gen == prev_gen`: the gen check accepts it; a returning host retires to `prev_home` (**mandatory extra account `[prev_home_holding w]` when `holding.gen ≠ id.gen`**, at a fixed position, so a settler cannot drop it to destroy troops); bonds already refunded are not paid twice (flag). A Settler that won a claim settles as "stays, consumed" | +≈ 2k; tx ≤ 1,022 B |
| 0x60 | GatherClash | A slot whose host id has `gen == holding.prev_gen` on a captured Holding **gathers as present** (M1 drops it as "re-founded") | none |
| 0x61 | ResolveFromInputs | Conquest step (§4.1); neutral-town garrisons in `ClashInput` (faction NEUTRAL, walls, tier); settler founding; `CONQUEST` log; CLASH `input_digest` domain `PSF-CLASH-INPUT-v2` over `province[SITE_MIRROR..TICKET_COHORTS] ‖ province[4096..4608]` | **+6.6k worst [measured]** (§10) |
| 0x63 | SkipQuiet | Conquest step per bell with the quiet model; prefix commit at 288 record-bells; SKIP `quiet_digest` v2 adds the conquest block | +≈ 260 CU per active record per bell, +1.5k per hour boundary [measured] |
| 0x47 | SettleExplore | A record whose host gen ≠ the Holding's gen credits nothing (the victim's pending explore after a capture) | none |
| 0x05 | CloseSeason | Parts unchanged; MarchStates close through 0xA7 | none |

**Not changed:** Join, FileTicket and SettleTicket (first holdings stay ticketed in the own wedge), beacons, ArchiveAnchors, ClaimDefence, closes. FoldOccupancy only adds `extra_holdings`.

### 5.3 New error codes (stable forever)

| Code | Name | Code | Name |
|---|---|---|---|
| 62 | CapturePending | 69 | HoldingsFull |
| 63 | Immune | 70 | NotBesiegeable |
| 64 | SiegeBusy | 71 | Friendly |
| 65 | StakeUnsettled | 72 | FrontierProtected |
| 66 | SiegeCap | 73 | Heartland |
| 67 | NotOnHex | 74 | NotDue |
| 68 | TooLate (cannot finish before `end_bell`) | 75 | ClaimLapsed |

---

## 6. Capture: moving a Holding between wallets

### 6.1 What moves and what does not

**What moves:**

- the **site** (the Holding account keeps its address `ho‖P,Q,site`, so no account is created or closed);
- the stores, buildings, queue and tier;
- walls, halved unless the captor's doctrine keeps them;
- the Holding's rent obligation, by the rent swap.

**What does not move:**

- the garrison (reset to 0; the attacker's host on the hex is its field force);
- trained reserve troops (zeroed [design]);
- the victim's hosts, wherever they are (§6.2);
- the victim's escrowed seal bonds (refunded to the victim's funder at once);
- in M2, laurels by the pair rules (not in this milestone).

Holdings 2–3 only, plus neutral towns. **First holdings are occupied, never moved (D9 kept).**

### 6.2 The victim's hosts: the M1 trap and the fix

**The trap.** In M1, a host id carries its Holding's generation, and:

- SettleTransit refuses a transit whose host generation differs from the Holding's (`transit.rs`: `parts.gen != gen → BAD_ACCOUNT`);
- GatherClash treats such a slot as "re-founded" and drops it;
- Depart, Dissolve and Explore refuse a host of another generation (`NotOwner`), and DisbandStranded destroys it.

So **a capture that only bumped the generation would make the captured Holding's 4 transit records unsettleable forever.** Its Depart would then be blocked for good (`TransitState`) and its escrow locked. That is a permissionless freeze the victim could even trigger on purpose, by marching everything out just before completion.

**The fix:**

- The Holding keeps `prev_gen` and `prev_home`.
- **The transit path** (GatherClash, SettleDeparture, SettleTransit and the return settle) accepts `gen == prev_gen` on a captured Holding. The victim's in-flight hosts arrive and fight as theirs: a host's faction comes from the transit record, not from the Holding's owner.
- **Where returning hosts go:** to `prev_home` (the victim's first holding), never into the captured Holding's reserve. The return account is mandatory when the generations differ, so troops cannot be dropped by a malicious settler.
- **Command authority:** the captor cannot command the victim's hosts. Depart and friends compare the host's generation with the Holding's *current* `gen`, which differs.
- **Resident hosts elsewhere** keep fighting for the victim's faction until **RetireHost** (N, anyone) sends them home to `prev_home`.
- **Bonds:** refunded to the victim's funder at capture, so SettleTransit never pays the victim's bond to the captor's funder (`holding.rent_payer` has changed).
- **One generation of history is kept.** If a captured Holding is captured again before the first victim's transits settle (≥ 36 + immunity bells later), the older generation's records fall back to M1's stranded rule. The verifier reports this as `DoubleCaptureStranded`.

### 6.3 The capture lock: no last-look between completion and settlement

- **Completion is decided in the resolve.** From that transaction on, the Province's mirror carries the new owner and generation (`mirror.gen = holding.gen + 1`).
- **Every instruction that writes a Holding** either names that Holding's Province (Muster, Dissolve, Garrison, Explore and Depart already do; Harvest, Build and Train gain a read-only `[province]`) or is a settle instruction that names it. Each one refuses `CapturePending` when `mirror.gen ≠ holding.gen` and `capture_flags = 0`.
- **Effect:** the victim cannot drain stores or train once the completion has landed, and the captor cannot act before SettleCapture.
- **What stays possible:** draining *before* completion (scorched earth while progress is public) is ordinary play. A `siege_lock_bells` option, which refuses owner spending in the last k bells of a held siege, is listed for the rules area (§12), not proposed.

### 6.4 Raze

- **When:** the captor already has 3 holdings, or has no escrow.
- **Effects:** the site becomes free and the Holding is marked razed. It is closed by CloseHolding (N), which returns the rent to the victim's funder and sends `pool_owed` to the DefencePool.
- **Raze is deterministic:** neither the captor nor the settler chooses between capture and raze.

---

## 7. PS2 log records (verifier and herald input)

New kinds; the encoding is unchanged (contract §6). Every body is ≤ 128 B.

| Kind | Name | Key | Payload | Chains |
|---|---|---|---|---|
| 80 | SIEGE_DECLARED | P, Q, site | attacker u8, declarer tag u64, required u8, vigil {start u16, next u16, from_day u16}, stake u32, src key u64, target kind u8 (first / other / free city / seam), owner tag u64 | Province, Citizen, Holding (src) |
| 81 | SIEGE_SETTLED | P, Q, site | reason u8 (fail → defender, success → attacker, season end), recipient key u64, amount u32, burned u32 | Province, Holding (recipient) |
| 82 | CONQUEST | P, Q, bell | n u8, events [≤ 12] × {site u8, code u8, faction u8, progress u8}, records digest [32], snapshot present u8 + weight [7] u16 | Province |
| 83 | CAPTURE_SETTLED | P, Q, site | outcome u8, captor tag u64, victim tag u64, new gen u8, order u8, rent moved u64, bonds refunded u64, walls after u32 | Holding, Province, Citizen × 2, JoinShard × 2 |
| 84 | FOUNDING_SETTLED | P, Q, site | citizen tag, order u8, gen u8, shield_until i64, outcome u8 (founded / lapsed) | Holding, Province, Citizen, JoinShard |
| 85 | TRIBUTE | P, Q, site | occupier tag, from_bell, to_bell, amounts [8] u32 | Holding × 2, Province, Citizen |
| 86 | MARCH_FOLD | m, n, hour | weight [7] u16, controller u8, contested u8, credit_q u8, lost u8, captures [6] u16 | MarchState |
| 87 | RETIRE | host_id | troops u32, home key u64 | Province |
| 88 | NEUTRAL | P, Q, site | kind u8 (Seam placed, Free City made, Free City expired, claim lapsed), garrison u32, tier u8, until u32 | Province |

CONQUEST event codes:

| Code | Event | Code | Event |
|---|---|---|---|
| 1 | FAILED | 6 | LIBERATED_EXPIRED |
| 2 | OCCUPIED | 7 | CLAIM |
| 3 | CAPTURE_DUE | 8 | CLAIM_LAPSED |
| 4 | LIBERATED_GONE | 9 | FREE_CITY_EXPIRED |
| 5 | LIBERATED_WON | | |

Per-bell progress is not logged. The herald and the verifier recompute it with the shared `conquest_model` (they already replay every clash), and `records digest` pins the state after each bell.

**Verifier v3 additions (owner: the verifier area):**

- **V14:** conquest replay. Per bell, the conquest step over the replayed report must equal the CONQUEST digest. Skip and resolve must agree.
- **V15:** capture and founding bookkeeping (owners, generations, lists, rent swap, bond refunds).
- **V16:** March folds from the snapshots in the Province chain.
- **V17:** stake flows and tribute amounts.
- **Tampers T23–T30:**
  - T23 progress inflated;
  - T24 completion inside vigil;
  - T25 capture to a wrong citizen;
  - T26 bond paid to the captor;
  - T27 a fold credit with a missing member;
  - T28 a snapshot weight off by one tier;
  - T29 a tribute over 20%;
  - T30 a founding over 3 holdings.

---

## 8. Data the program gives the herald and the design chat

The program side of the hand-off. The herald area owns the JSON and binary shapes.

| Need (GD §8) | Program source |
|---|---|
| Per site: kind (first holding, holding 2–3, Seam Town, Free City, claimed, free, camp) | site mirror `state`, `order`, `neutral_kind`; `Province.camp` |
| Siege marker: attacker, progress, required, vigil paused now, since | conquest record (kind 1). "Vigil paused" = `Vigil::covers(bell_start(b))` from the snapshot (the shared model exports it) |
| Occupied mask and occupier per site | conquest record kind 2 |
| Immunity-until bell | record `bell` when kind 0 |
| Capture pending | record kind 3, or mirror gen ≠ Holding gen |
| Province controller and margin | `conquest_model::control_weights(province, bell)` (live), or the latest snapshot |
| March banner: controller, contested, since, Dominion | MarchState |
| Events ticker | CONQUEST event codes, CAPTURE_SETTLED, FOUNDING_SETTLED, NEUTRAL, MARCH_FOLD (controller change = banner flip) |
| Time-lapse | snapshots per province-hour; MARCH_FOLD per March-hour |
| Threat rings | unchanged: DEPART (origin, arrive bell, mass) |

**The overview binary** (`/h/overview/{ring}/{bell}.bin`, contract §9.3) needs per province: controller (u8), occupied mask (u16), siege mask (u16) and a neutral mask (u16). That is +7 B per province record. Owner: the herald and data area.

---

## 9. Security properties

### 9.1 No permissionless freeze

| Path | Why it cannot freeze anything |
|---|---|
| A siege record blocks other horns on that site | Only a host that holds the hex can declare, and a besieger that loses the hex fails at that bell. Holding the hex is the point of the contest, not a squat. A failed siege leaves immunity (§9.5). |
| A completed capture waits for SettleCapture | SettleCapture is permissionless and class D. Raze is the deterministic fallback for any missing precondition. |
| A claim waits for SettleFounding | The claim lapses after `claim_ttl_bells`, and the lapse is applied by any resolve or skip, or by SettleFounding itself. |
| A stake owed blocks a new horn (`StakeUnsettled`) | Anyone can run SettleSiege at any time (N). |
| Season end with active sieges | SettleSiege closes them after `end_bell`. |
| The victim's transits on a captured Holding | Accepted by `prev_gen`. Without this fix they would be a real freeze (§6.2). |
| SkipQuiet with many records | Work bound with prefix commit (§4.1), as in I-50. |
| FoldMarch blocked by a lagging member | `TooEarly` only waits; the keeper resolves or skips the member. A member running ahead loses the hour; it never blocks. |
| Neutral Free City never claimed | Expires lazily to a free site. |
| Pre-funding any new address (MarchState, a razed or recreated Holding) | §9.4 |

### 9.2 No last-look

- **Siege outcomes come only from clashes over frozen rosters.**
  - Progress, failure, completion, occupation and liberation are computed in ResolveFromInputs or SkipQuiet of bell b, from the roster frozen at `bell_start(b)` and the kernel's outcome.
  - No settle instruction chooses anything. Capture versus raze is fixed by state, and the fold hour is fixed.
  - Who sends a settle and when cannot change its result (property tests §11.2 P2, P5).
- **The horn reveals nothing sealed.** It is declared from the hex after the strike landed, so it publishes no sealed destination (GD C2). Counting starts at `bell + 1`, after the next roster freeze.
- **The defender cannot react to the horn by moving its pause.** The vigil is snapshotted at the horn (§5.1).
- **The capture lock** stops the victim acting on a completed capture (§6.3).
- **Lag only waits.** A held Province pauses its sieges, occupations and snapshots, but every bell is still counted in order with the same inputs. The resolve-order-independence of M1 (G7) extends to the conquest step (§11.2 P4).

### 9.3 No hot global writer; locks and their prices

New writable accounts and who writes them. Prices are DESIGN §8.7's model at the writer's keeper class, per 10 minutes.

| Account (writers) | Class, cap | Effect while held | Price per 10 min | Blast radius |
|---|---|---|---|---|
| Province (+ DeclareSiege P, CollectTribute P, SettleCapture D, SettleFounding D, SettleSiege N, RetireHost N) | D, 0.5 | Its resolves wait, so its sieges, occupations and snapshots pause; the outcome is unchanged | $2.5k–6.8k (unchanged) | one province (delay) |
| MarchState (FoldMarch) | D, 0.5 | Folds wait; after more than 5 hours a member's snapshot can be overwritten and that hour is **lost** for the March | $2.5k–6.8k; ≈ $75k–204k per lost March-hour | one March's Dominion credit |
| Holding, Citizen, JoinShard at SettleCapture or SettleFounding | D, 0.5 | Bookkeeping waits; the mirror already shows the outcome | $2.5k–6.8k | one capture or founding (delay) |
| Victim or occupied Holding (CollectTribute P) | P | The occupier's tribute waits | ≈ free (player) | one holding |

- **No new account is written by every province or every player.** There is no global siege table and no global capture counter.
- **Captures and Dominion are folded per March.** A faction score sums per-March values outside play, in M2's `TallyDominion`, after `end_bell`.
- **The Frontier is still written only by OpenRing and FoldOccupancy.** Depart and SettleFounding read it.

### 9.4 Pre-funding and forgery

- **New creation paths:**
  - MarchState: `init_with_seed`, fee payer;
  - Holding by SettleFounding and by neutral capture: `init_funded` from the Citizen escrow, as SettleTicket does.
  - **G2** gets one pre-funded test each: 19 + 3 = 22 creation paths.
- **New keyed reads, each recomputed:**

| Instruction | Reads |
|---|---|
| DeclareSiege | the target Province, target Holding, owner Citizen, nearby Province, and the source Holding (from the entry's host id) |
| SettleCapture | the Holding, both Citizens (canonical `ct‖tag` and `citizen_tag == record.actor` for the captor), both JoinShards (`(faction, shard)` from each Citizen) |
| FoldMarch | the 7 members (from `march_members`) and the March |
| RetireHost | `prev_home` (from the Holding) |
| SettleSiege | the recipient (from the record) |

  **G3** gets one forgery test per read, including a forged member Province in FoldMarch (lab: the probe's address check refuses it), a captor Citizen of the right faction but the wrong tag, and a `prev_home` of another citizen.
- **The 8-byte tag.** `citizen_tag` is the first 8 bytes of the Citizen address, which is itself sha256-derived from the wallet. Matching a record's tag *and* the canonical address of a present program-owned Citizen leaves only a 2⁻⁶⁴ collision. Grinding one needs about 2⁶⁴ wallet keys. Storing the full 32-B address would cost 24 B × 12 per Province.

### 9.5 Findings for the rules area

- **F1, immunity farming [design finding].**
  - GD §3.4 gives the target 36 bells of immunity after **any** failed siege, and pays the stake to the defender.
  - So a friend of the defender (an alt in another faction, or a faction in practice allied while the milestone has no formal alliances) can strike, sound a horn, leave the hex and fail on purpose. That buys 36 bells of immunity at a cost of 500 Gold, which goes *to the defender*: a free shield, renewable every 36 bells for the price of a 2-bell march.
  - **Proposed rule:** immunity and the stake payment only when the siege was **broken by the defender's side**, meaning the bell that failed it had `defender_present` or the owner's side held the hex. A siege that lapses because the besiegers left gives no immunity, and its stake is burned.
  - The program supports either rule. The kernel `BellReport` already carries `defender_present`.
- **F2, gold flows between factions [design finding].** Stakes paid to defenders, and tribute, move resources across factions. Alts can use them to move resources between factions. That is harmless while resources are not money. M2 must apply the pair-history rules to these flows, as to laurels.
- **F3, the M1 generation trap** (§6.2): fixed in this design.
- **F4, the keeper load of sieges [estimate].**
  - A besieged hex with a garrison or defender is never quiet, so each siege costs at least 36 full resolves (≈ 272k CU each [M1 measured]).
  - At the 7-day demo's scale, say 300 sieges, that is ≈ 3.7 G CU in all: negligible locally, about 0.07 SOL of base fees on a real cluster.
  - At 50k players with 10k sieges a day, ≈ 0.6% of base block CU: inside DESIGN §8.9's adversarial row, which already assumes every province resolves every bell.
- **F5, the snapshot fold needs every province resolved within 5 hours.**
  - Keepers must also skip idle provinces about every 4 hours: 6 a day per province, as M1's idle-province rate.
  - At 50k players: ≈ 46k extra SkipQuiet a day, plus ≈ 26k FoldMarch a day. That is ≈ 3 G CU and ≈ 0.4 SOL of base fees a day [estimate].

---

## 10. Budgets on SBPF v2 at adversarial fill

| Instruction | Class | Worst CU | Gate / limit (proposed) | Basis |
|---|---|---|---|---|
| ResolveFromInputs | D | **271,673 + 6.6k ≈ 278k** (12 completions + snapshot + CONQUEST) | 290,000 kept; regenerate the limit (≈ 292k at +5%; the 5%-of-gate rule allows it). If the real build exceeds 290k: **300,000** | [M1 measured] + [measured increment] |
| SkipQuiet (24 bells) | D | 170,412 + ≤ 75k (12 records) | 60k + 30k per recomputed bell **+ 3.2k per bell with active records**; prefix commit at 288 record-bells | [M1 measured] + [measured] |
| DeclareSiege | P | est. 25k (6.7k measured core) | 30,000 | [measured] + Harvest analogue |
| SettleSiege | N | est. 18k | 25,000 | analogue |
| SettleCapture | D | est. 16k / 21k (neutral init) (8.1k measured core) | 30,000 | [measured] + analogue |
| SettleFounding | D | est. 22k | 40,000 | SettleTicket fresh analogue |
| CollectTribute | P | est. 30k | 35,000 | two settles |
| FoldMarch | D | **8.3k measured** (one hour); est. 14k with prologue and init | 20,000 | [measured] |
| RetireHost / CloseMarch | N | est. 12k / 6k | 15,000 / 8,000 | analogue |
| Harvest / Build / Train | P | 16.4k / 19.1k / 16.6k + ≈ 1.2k | 19,000 / 23,500 / 19,000 | [M1 measured] + Province presence |
| Depart (settler) | P | 23.2k + ≈ 3k | 28,000 | [M1 measured] + Frontier read, escrow |
| SettleTransit (captured) | D | 63.3k + ≈ 2k | 85,000 kept | [M1 measured] |
| OpenProvince | D | 148.5k + ≈ 5k | 220,000 kept | [M1 measured] |

- **Heap.** The conquest step uses fixed arrays only. RFI already holds `out.garrisons`, and the skip's tile masks are a 61-B array. The heap peak stays M1's 15,320 B of 28 KiB.
- **Program size and `L(kind)`.**
  - The probe's linked conquest code is **≈ 78 KB** (91,952 B with it, 13,456 B without [measured]). The real handlers add validation and logging: est. **+100–140 KB** over the 875,824-B M1 release, so ≈ 0.98–1.02 MB.
  - `--max-len` = round_up(1.25 × `.so`, 4 KiB) ≈ 1.23–1.28 MB.
  - Every `L(kind)` grows by about 5 pages (+40 cost units). `tip_min` at p = 0.433 becomes ≈ 14,685 lamports (14,668 today).
  - All of these are regenerated from the release build. None is a hand-entered value.
- **Transaction sizes.** DeclareSiege ≈ 560 B; SettleCapture ≈ 560 B; FoldMarch ≈ 600 B; SettleTransit with `prev_home` ≤ 1,022 B. All are under 1,232 B; locks ≤ 13.

---

## 11. Lab: what was measured and how to rerun

- **Where:** `scratchpad/frontier/conquest/lab/program-conquest/`
  - A copy of `permutation-rules` at `aae8617`.
  - `probe/`: an SBPF v2 cdylib, never deployable.
  - `runner/`: LiteSVM 0.16, toolchain 1.95.0, its lock seeded from `svm-tests`.
  - Target dirs: `target-sbf`, `target-sbf-bare`, `target-host`.
  - Raw output: `results.txt`.
- **What the probe emulates:**
  - Over Province v2 bytes, the conquest step for k ∈ {0, 1, 4, 12} active records:
    - resolve mode (report from a kernel-shaped `GarrisonResult` vector);
    - skip mode (cached tile masks);
    - an hour boundary or not, completions, occupations.
  - FoldMarch over 7 × 4,608-B accounts with canonical address recomputation.
  - DeclareSiege's checks (6 addresses, `may_besiege`, the host-on-hex scan, the frontier-protection proof, record write, log, 3 chained heads).
  - SettleCapture's bookkeeping (6 addresses, Holding rewrite, 4 bond refunds, both lists, both JoinShards, rent swap by lamport arithmetic, log, 6 heads).
- **Results [measured, `results.txt`]:**
  - No-op transaction: 250 CU.
  - Resolve-mode step:
    - 10,429 CU at k = 0. This is the cost of *building* the garrison vector, which a real RFI already has.
    - +365 CU per active siege.
    - 11,917 CU at an hour boundary with k = 0.
    - 16,246 CU with k = 12 at a boundary.
    - **17,038 CU** with 12 completions at a boundary.
  - **Increment over the vector baseline: 6.6k worst.**
  - Skip mode, 24 bells: 16,584 CU (k = 0) → 92,112 CU (k = 12), i.e. ≈ 263 CU per record-bell. Six bells, k = 12: 24.7k.
  - Twelve occupations: 12.7k (resolve), 42.5k (24-bell skip).
  - FoldMarch: 8,335 CU, 520-B transaction.
  - DeclareSiege checks: 6,664 CU, 477-B transaction.
  - SettleCapture bookkeeping: 8,084 CU, 475-B transaction.
  - A first version of the skip that scanned the 56 entries per record per bell cost 991 CU per record-bell (300,770 CU for 12 records × 24 bells). **Hence the tile-mask cache is part of the design.**
- **Rerun** (offline; no install):

  ```
  L=…/lab/program-conquest
  export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
  (cd $L/probe && CARGO_TARGET_DIR=$L/target-sbf cargo-build-sbf --tools-version v1.52 --arch v2 --sbf-out-dir $L/deploy -- --offline)
  (cd $L/runner && CARGO_TARGET_DIR=$L/target-host cargo build --offline)
  $L/target-host/debug/conquest-runner $L/deploy/conquest_probe.so
  ```

- **Limits of the lab.**
  - The probe is not the program. It has no prologues, no `frontier-abi` accessors (checked bounds add ≈ 10–30%) and no real kernel resolve, so the totals above add M1-measured analogues.
  - Vigil coverage was exercised both covered and uncovered.
  - The gates below re-measure everything on the release `.so`.

### 11.2 Gates the implementation adds (svm-tests, prefix-filtered as in M1)

- **G1:** every new and changed kind at its worst fill:
  - 12 sites each with an active siege, all completing at an hour boundary, on top of RFI's Phase-B worst fill;
  - a 24-bell skip with 12 occupations;
  - FoldMarch with 7 members and a first-fold init;
  - SettleCapture neutral with init.
- **G2:** MarchState, the SettleFounding Holding and the neutral-capture Holding pre-funded.
- **G3:** forged member, forged captor tag, forged `prev_home`, forged nearby Province; re-creation of a razed Holding's address (tombstoned by `capture_flags` until CloseHolding).
- **G11 (extended):** resolve and skip of a quiet bell with siege and occupation records are byte-identical over 4,608 B.
- **New property tests:**
  - **P1 siege equivalence:** the program's progress equals a native replay of `Siege::advance` over 1,000 random rosters and vigils (incl. CL-09 changes).
  - **P2 no last-look:** the outcome is independent of SettleCapture's caller and timing, and of any order of DeclareSiege within a bell.
  - **P3 capture lock:** every Holding-writing instruction between completion and SettleCapture refuses `CapturePending`.
  - **P4 lag gate:** holding the Province, the anchors or the victim's Holding past completion leaves the final state byte-identical.
  - **P5 fold determinism:** FoldMarch results are independent of member resolve timing within K.
  - **P6 every record terminates:** a random walk over all instructions reaches kind 0 or a Holding for every record, using only permissionless calls.
  - **P7 captor cannot command the victim's hosts.**
  - **P8 bonds go to the victim's funder.**
  - **P9 transits of a captured Holding settle.** The M1 code fails this test (failing-first).
- **G13:** one test per (new instruction, error code).

---

## 12. Decisions needed

Program-relevant; the GD decisions are listed in GD §10.

1. **ABI v2 as a superset, with the Province at 4,608 B** (+0.0026 SOL refundable per province; +32 SOL float at `R_MAX` 64 full). Recommended. The alternative is to squeeze conquest state into the v1 reserve, which M2/M3 need.
2. **Hourly per-province control snapshots (K = 6) plus a FoldMarch that can lose an hour under a 5-hour hold,** instead of exact per-bell March control, which cannot be computed on chain (§4.2). Recommended. Dominion becomes "control-hours", not "control-bells".
3. **On-chain ScoreBoard deferred to M2.** In this milestone the herald and verifier sum MarchStates. Recommended.
4. **Retire the victim's hosts to the first holding** (RetireHost, `prev_gen`, mandatory return account) versus losing them. Retiring them costs one N instruction and changes to four transit-path instructions; losing them costs only a sentence on the join page. Recommended: retire (GD decision 6).
5. **Immunity and stake only when the defender's side broke the siege** (F1). Recommended.
6. **Trained reserve troops are zeroed at capture** (not transferred). Recommended.
7. **Holdings 2–3 by sealed Settler march** (GD §3.3: kernel Settler muster and train, lottery in RFI, SettleFounding) versus by **ticket** (`FileTicket(kind = extra)` reusing cohorts; no kernel change; about one engineer-week cheaper, but not sealed). Recommended: settler march, as GD proposes. The ticket path is the cut-list fallback.
8. **Gate changes:**
   - Harvest, Build and Train gates raised to 19k / 23.5k / 19k (the capture-lock Province account);
   - Depart 28k;
   - RFI 290k kept, with a measured fallback of 300k.
9. **Optional `siege_lock_bells`** (owner spending refused in the last k bells of a held siege). Not proposed for v1.

## 13. Risks

- **Scope.** Program work est. ≈ 13 engineer-weeks:
  - kernels (siege v3, conquest, Settler catalog): 2;
  - new instructions: 4;
  - changed instructions: 3;
  - ABI v2, vectors and budgets: 1.5;
  - svm gates: 3.

  Plus the verifier, herald, keeper and bot work in their areas.
- **The RFI margin is thin.** The estimate is ≈ 278k against a 290k gate. Accessor overheads could push it past, hence the 300k fallback. The clash budget then rises from 410k to 420k, which the C4 and capacity models must restate.
- **Kernel outcome changes** (Settler hosts in clashes, neutral-town garrisons, the heartland parameter) re-run the doctrine proxy gate and the bot criterion (contract §3.3). The rules area owns that.
- **The keeper load** of non-quiet besieged provinces and of hourly folds needs the cost model GD asks for (F4, F5).
- **One generation of capture history:** a double capture inside one victim's transit lifetime strands the oldest hosts (stated; the verifier flags it).

---

## Links

- This note: `scratchpad/frontier/conquest/design/program.md`
- Game design it follows: `scratchpad/frontier/conquest/design/game-design.md`
- Lab: `scratchpad/frontier/conquest/lab/program-conquest/` (`probe/src/lib.rs`, `runner/src/main.rs`, `results.txt`)
- Repo (read-only): `/Users/r0ze/Documents/Codex/2026-09-20/new-chat-2/outputs/.claude/worktrees/frontier-integ`
  - `docs/frontier/m1/M1-CONTRACT.md`
  - `permutation-rules/src/frontier/siege.rs`
  - `permutation-frontier/src/proc/{clash,transit}.rs`
