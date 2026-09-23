# Benchmark: Civilization, 4X and persistent strategy games, for a seasonal shared-world hex civ game with AI agents

**Evidence tags:** **[S]** means a source is cited for the fact. **[K]** means well-established general knowledge that I did not re-verify this session. **[I]** means my own inference or design judgement.

**Two decisions the user confirmed shape the whole report:**
- There is one shared map with many civilizations. Each human or AI agent runs its own civ. Victory is per civ or per coalition.
- External AI agents are full equals under identical rules and costs, as in Screeps.

This makes three things matter most: fairness between humans and bots, resistance to Sybil attacks (one player running many fake accounts), and objective scoring. The report weights them accordingly.

---

## 1. The Civilization series (V / VI / VII)

### 1.1 Core model
- **Map.** Civ V brought in hexes and "one unit per tile". Civ VI keeps both and splits cities into districts placed on adjacent hexes, with adjacency bonuses. Civ VII keeps districts, splits settlements into **Cities and Towns**, and lets you "build tall" by stacking buildings in urban quarters **[K]**.
- **Yields.** Food, production, gold, science, culture and faith. Faith is dropped in VII, which adds influence as a diplomacy currency **[K]**.
- **Growth friction.** Civ VI deliberately makes growth harder as you grow:
  - Settler cost is roughly `80 + 20n` (n = settlers already built).
  - Builder cost is roughly `50 + 4n`.
  - Food to grow is roughly `15 + 8n + n^1.5`.
  - Housing and amenities cut growth by 50–75%.
  - One analysis calls this "dynamic friction" and estimates it makes games about 20–24% longer ([Todorov case study](https://denistodorovhonours.wordpress.com/2018/04/03/economy-case-studies-vol-2-civilization-vis-control-of-snowballing/)) **[S]**.
  - Civ V used global happiness plus tech and policy costs that rise with city count **[K]**. Its community still concluded that "Four Cities was the clear only way to go", which shows how narrow the viable strategies became ([CivFanatics](https://forums.civfanatics.com/threads/the-snowball-effect.504239/)) **[S]**.
- **Trees.** Civ VI has separate tech and civics trees, with "eureka/inspiration" boosts for doing related actions. Civ VII resets both trees each Age ([CivFanatics – Ages](https://civfanatics.com/civ7/civ-vii-gameplay-mechanics/civilization-vii-ages/)) **[S]**.

### 1.2 Civ VII Ages: the closest analogue to seasons
All of the following is **[S]** ([Fandom Legacy Path](https://civilization.fandom.com/wiki/Legacy_Path_(Civ7)), [PCGamesN Ages](https://www.pcgamesn.com/civilization-vii/ages), [CivFanatics](https://civfanatics.com/civ7/civ-vii-gameplay-mechanics/civilization-vii-ages/), [Steam thread](https://steamcommunity.com/app/1295660/discussions/0/591762915933093951/)):
- There are three Ages: Antiquity, Exploration and Modern.
- A shared **Age Progress** meter fills a little every turn and faster on key actions, such as eliminating a civ.
- At about 70% progress, a multi-stage **Crisis** starts and forces everyone to adopt negative "crisis policies".
- Each Age has four **Legacy Paths**: science, culture, economic and military. Each has three milestones that pay **Legacy Points**, which you spend on carry-over bonuses.
- If you hit no milestone at all, you get a **Dark Age legacy**. This is an explicit catch-up and consolation mechanic.
- At the transition:
  - All wars reset to peace, but relationships persist.
  - Units are reset to the capital, and the most obsolete are deleted first.
  - City-states dissolve and respawn.
  - Cities revert to Towns unless you bought a legacy to keep them.
  - Tech and civics trees reset.
  - The leader, commanders and "ageless" buildings persist.
- Modern Age victories are:
  - **Science:** a staffed space flight.
  - **Culture:** the World's Fair.
  - **Military:** 20 Ideology Points from first-time settlement conquests, then Operation Ivy.
  - **Economic:** Railroad Tycoon, then the Great Bank.
  - Source: [PCGamesN victories](https://www.pcgamesn.com/civilization-vii/victories), [GameRant](https://gamerant.com/civilization-7-all-victory-conditions-explained-civ-7-victories-guide-culture-economic-military-science-legacy-path-modern-age/) **[S]**.
- Criticism:
  - The culture victory was "way too easy" and needed a patch.
  - The economic path depends on random treasure-resource placement.
  - Legacy Paths "force players into doing the same things every game", because you need points on every path ([Steam](https://steamcommunity.com/app/1295660/discussions/0/689746295486698285/), [Tobold](http://tobolds.blogspot.com/2025/03/civilization-vii-legacy-paths-and.html)) **[S]**.

**Lesson [I].** Civ VII shows that periodic soft resets work: auto-peace, pruned armies, respawned neutral actors and consolation legacies. They cut snowballing and give a natural "season boundary" for a world that otherwise persists. The failure mode is making every path mandatory. If every track contributes to one total, players converge on doing everything.

### 1.3 Victory conditions and why they are criticized
- **Domination.** Capture all original capitals (V/VI) **[K]**. Usually the most decisive outcome, and in multiplayer the dominant one.
- **Science.** A space race. It is the most deterministic path and the most snowball-prone, because it rewards the biggest economy with a passive project chain **[K]**.
- **Culture.** Tourism beats others' domestic culture. Players find it opaque **[K]**.
- **Religion (VI).** The least-liked victory in a poll, with over 25% of votes as least popular. It is tedious and players "accidentally win" through it ([NME](https://www.nme.com/news/gaming-news/civilization-vi-players-hate-religious-victories-3012143)) **[S]**.
- **Diplomatic (VI Gathering Storm).** 20 Diplomatic Victory Points, earned by voting for winning World Congress outcomes and from certain wonders ([Fandom World Congress](https://civilization.fandom.com/wiki/World_Congress_(Civ6))) **[S]**. It is criticized as a slow late-game bribery race ([ResetEra](https://www.resetera.com/threads/do-you-turn-off-diplomatic-victory-in-civilization.69164/)) **[S]**.
  - **Directly relevant [I]:** "peace" victories built on votes turn into vote buying. In a world open to Sybils, votes are nearly free to manufacture.
- **Score.** A turn-limit fallback. It is rarely anyone's goal **[K]**.

### 1.4 Diplomacy, grievances, war weariness, loyalty
- **War weariness (VI):** grows with each battle, worse abroad and on casualties, and becomes negative amenities. Rebels spawn if it persists ([Fandom](https://civilization.fandom.com/wiki/War_weariness_(Civ6))) **[S]**.
- **Grievances (VI GS):** a per-pair ledger of transgressions such as declaring war or taking cities. It hurts relations with the victim and with third parties, and lowers loyalty in conquered cities the victim founded ([Fandom](https://civilization.fandom.com/wiki/Grievances_(Civ6))) **[S]**.
- **Loyalty (VI Rise & Fall):** nearby population exerts pressure, and cities can flip to Free Cities ([Fandom](https://civilization.fandom.com/wiki/Loyalty_(Civ6))) **[S]**.
- **Why this matters [I].** These are **computable, deterministic brakes on conquest**. Grievances in particular are an objective "aggressor record" that a peace or neutrality victory could read.

### 1.5 City-states as neutral actors
- Civ V and VI city-states award a contestable suzerainty through envoys **[K]**.
- In Civ VII, **Independent Powers** spawn each Age. The first player to 60 Befriend Points becomes suzerain, and **suzerainty is locked for the rest of the Age**. Suzerains can levy units or incorporate the city-state ([Fandom Suzerain](https://civilization.fandom.com/wiki/Suzerain_(Civ7))) **[S]**.
- **Lesson [I].** NPC neutrals make a good arena for non-violent competition (influence races). Locking suzerainty stops endless flip-flopping and last-minute sniping.

### 1.6 Multiplayer pain points
- Dynamic turns switch to sequential play between players at war, and have long-standing freeze bugs ([Steam](https://steamcommunity.com/app/289070/discussions/4/1291816880505599654/)) **[S]**.
- Players who disconnect leave "nothing happening" versus AI takeover, a trade-off players argue about ([Steam](https://steamcommunity.com/app/289070/discussions/0/224446340333202433/)) **[S]**.
- Late games are slow and inevitable, "one civ marches towards victory", and many multiplayer games are never finished ([Steam](https://steamcommunity.com/app/289070/discussions/0/312265473879309388)) **[S]**.
- As one forum puts it, the right amount of snowball for single-player "could be way too much for multiplayer" ([CivFanatics](https://forums.civfanatics.com/threads/how-would-you-solve-the-snowballing-endgame-problem.672991/)) **[S]**.

---

## 2. Other 4X and strategy games

| Game | Mechanic relevant to us | Evidence |
|---|---|---|
| **Humankind** | There is one winning currency, **Fame**, mainly from Era Stars (deeds per category), and the fame reward decays over time. The game *ends* on triggers such as a turn limit, elimination, all techs, the last Era Stars or a Mars mission, but the *winner* is whoever has most Fame. | [Fandom Fame](https://humankind.fandom.com/wiki/Fame), [End Condition](https://humankind.fandom.com/wiki/End_Condition) **[S]** |
| | Criticism: Fame snowballs and "there is no possible way to challenge a superpower", and it accrues from normal play, so there is no real trade-off. | [Amplitude forum](https://community.amplitude-studios.com/amplitude-studios/humankind/forums/169-game-design/threads/38484-fame-as-the-sole-win-condition-is-too-snowbally) **[S]** |
| **Old World** | **Orders**, a per-turn action budget scaled by Legitimacy, are spent on moves, combat, building and diplomacy. Ambitions add Legitimacy. | [PCGamesN](https://www.pcgamesn.com/old-world/review), [Mohawk](https://mohawkgames.com/2020/04/14/old-world-first-post/) **[S]** |
| | Soren Johnson kept AI players off the Ambition victory because "a surprise ending to a 20-hour 4X game is not a good ending". He also uses a double-victory condition to shorten endgames. | [Designer notes via CivFanatics](https://forums.civfanatics.com/threads/old-world-designer-notes-11-soren-johnson-on-the-game-end.673707/) **[S]** |
| **Endless Legend** | Nine victories. Two matter here: **Expansion** (hold more than 80% of regions) and **Diplomatic** (accumulated peace points). Both are precedents for our "territory" and "peace" victories. | [EL Wiki](https://endlesslegend.wiki.gg/wiki/Victory) **[S]** |
| **Freeciv LongTurn** | One turn per 23 hours, up to about 300 humans on a browser map. **Late joiners get gold for each missed turn.** Unplayed "idler" nations can be taken over by new players, and the idle threshold grows as the game ages. Players confirm a few days before start to reduce idlers. | [Freeciv Fandom Longturn](https://freeciv.fandom.com/wiki/Longturn), [Multiplayer](https://freeciv.fandom.com/wiki/Multiplayer) **[S]** |
| **Unciv** | Open-source Civ V clone. Multiplayer is asynchronous via uploading and downloading save files. | [GitHub docs](https://github.com/yairm210/Unciv/blob/master/docs/Other/Multiplayer.md) **[S]** |
| **Polytopia** | A 16×16 map and roughly 30-minute games. **Perfection** mode is highest score in 30 turns. Multiplayer modes are **Glory** and **Might**. | [Polytopia wiki](https://polytopia.fandom.com/wiki/Game_Modes), [Pixelated Playgrounds](https://www.pixelatedplaygrounds.com/sidequests/game-design-perspective-the-battle-of-polytopia) **[S]** |
| **Stellaris** | **Endgame Crises** exist explicitly to break the "victory is inevitable" late game. They are a shared threat that will not negotiate, and the Galactic Community can pass resolutions against them. | [Wikipedia](https://en.wikipedia.org/wiki/Stellaris_(video_game)), [Fandom](https://stellaris.fandom.com/wiki/Crisis) **[S]** |

---

## 3. Persistent and MMO strategy games

### 3.1 Screeps: the key precedent for AI agents
- **Model.** Every player's JavaScript runs 24/7 in a persistent open world. A tick lasts 2–5 s depending on shard, and a tick ends only when all scripts finish ([Game loop docs](https://docs.screeps.com/game-loop.html), [Store](https://store.screeps.com/world)) **[S]**.
- **Fairness lever.** CPU is the scarce resource. There is a per-tick limit plus a "bucket" that holds up to 10,000 CPU, with bursts of up to 500 per tick ([CPU docs](https://docs.screeps.com/cpu-limit.html)) **[S]**.
  - Subscription tiers once gave 300 vs 120 CPU. After inequality complaints, CPU was tied to Global Control Level instead ([Support](https://support.screeps.com/hc/en-us/articles/206598969-Important-change-The-same-CPU-subscription-for-everyone)) **[S]**.
  - The community still complains that **multi-account empires and alliances dominate** ([Forum](https://screeps.com/forum/topic/2027/explicitly-allow-multi-accounting-with-narrower-restrictions)) **[S]**.
- **Newcomer protection.**
  - **Novice areas** are for players at GCL 3 or lower. They are walled off by indestructible walls, capped at 3 rooms, allow unlimited safe mode and ban nukes.
  - The walls fall on a timer, quarter by quarter, so newcomers meet neighbours in stages.
  - Separate **respawn areas** exist, and the system creates new areas as the population needs them ([Start areas](https://docs.screeps.com/start-areas.html)) **[S]**.
  - GCL and code carry over when you respawn ([Respawn](https://docs.screeps.com/respawn.html)) **[S]**.
- **Seasonal World.** A separate, paid-entry server that runs for about 2 months, with a different scoring rule each season:
  - S1–2: deliver resources to scoring structures.
  - S4: caravans.
  - S5: reactors scored as `1 + floor(log10(continuous ticks))`, which rewards *holding* over sprinting.
  - S6: rooms open and close daily, forcing expansion.
  - Top-100 rewards.
  - Sources: [ScreepsPlus wiki](https://wiki.screepspl.us/Seasonal_World/), [Forum S1](https://screeps.com/forum/topic/3073/season-1-announcement) **[S]**.
- **Lessons [I]:**
  1. Seasonal scoring on a separate or reset layer works.
  2. **Scoring on continuous uptime** (log of held-duration) resists end-of-season rushes.
  3. Rotating the scoring rule each season stops overfitted bots and mega-alliances from repeating wins.
  4. In a world of pure bots, the equalizer is compute budget. In a mixed human/agent world, the equalizer has to be an **action budget** instead.

### 3.2 Dark Forest: onchain, fog of war, prizes, automation
- **Model.** An onchain real-time strategy game. Moves are submitted with zkSNARK proofs, so coordinates stay hidden while moves remain verifiable. The world is procedurally generated and has to be "mined" (explored by hashing) ([Announcing DF](https://blog.zkga.me/announcing-darkforest), [intro](https://blog.zkga.me/intro-to-zksnarks)) **[S]**.
- **Prizes.** The first round (v0.3, 2020) lasted one week with a **1024 DAI prize pool** ([Accelerated Capital](https://acceleratedcapital.substack.com/p/dark-forest-crypto-native-gaming)) **[S]**.
- **v0.6 Round 1.** Two weeks, over 1,700 players, over 2M transactions and about 1.5T gas ([Wrap-up](https://blog.zkga.me/v6-r1-wrapup)) **[S]**.
  - The **winner used "an incredible suite of custom plugins, scripts, and servers"**.
  - Second place played "almost completely manually, using Twitter diplomacy", and finished close.
  - Special prizes went to tooling, documentation and bug reports.
- **Official plugin system and remote miners.** The client is scriptable by design ([Dev guides](https://dev-guides.zkga.me/plugins/what-is-a-plugin)) **[S]**.
- **Lessons [I]:**
  - Real-time plus an unlimited action rate means automation wins at the top. Social and diplomatic play can keep humans close, but only close.
  - Legitimizing automation through a public API and plugins is healthier than a hopeless bot ban.
  - Onchain rounds with prize pools attract optimizers, so every scoring edge will be found in days.

### 3.3 Travian: servers, Natars and the Wonder of the World
All of the following is **[S]** ([Travian support: WW](https://support.travian.com/en/articles/103-world-wonder), [Natars](https://travian.fandom.com/wiki/Natars), [End game](https://travian.fandom.com/wiki/End_game), [Alliance](https://travian.fandom.com/wiki/Alliance), [Confederation](https://travian.fandom.com/wiki/Confederation)):
- **How a server ends.** Each server is one "season":
  - NPC Natars arrive with **artifacts** (day 260 on classic servers, about day 90 on speed servers).
  - Later, **Wonder of the World construction plans** appear.
  - The first WW to level 100 wins. The server then stops and a new one starts after about 3 weeks.
  - Natars attack WWs every 5 levels, and every level after 95.
  - Passing level 50 requires a *second* plan holder inside the alliance, which forces cooperation.
- **Alliances.**
  - The cap is **60 members**. Confederations link up to 3 alliances.
  - Big groups get around the cap with "wings", and in the endgame they form **meta-alliances** to pool plans, resources and defence.
- **Endgame dynamic [S]/[I].** The endgame is effectively a coalition-versus-coalition logistics war around a few fixed objectives. The NPC pressure (Natar waves) is a Stellaris-style crisis aimed at the leader.
- **Anti-abuse.**
  - One avatar per player per world. Up to 2 "sitters" with restricted permissions. Password sharing alone is bannable ([Sitters and Duals](https://support.travian.com/en/articles/14-sitters-and-duals)) **[S]**.
  - Automatic limits on resource sharing between accounts ([Support](https://support.travian.com/en/articles/125-automatic-resource-sharing-limits)) **[S]**.
  - Bots are banned ([Game rules](https://www.travian.com/international/gamerules)) **[S]**.
  - Automation is sold as premium features instead (Gold Club farm lists) **[K]**. This shows the demand is real.
- **Lesson [I].** An objective that needs the alliance's *collective* effort, has a fixed visible location and is under escalating NPC pressure produces a dramatic, legible finale. Its weakness is that it concentrates the win in the biggest meta-alliance and turns everyone else into supporting players. With a prize pool, that means many players pay entry fees to fund one coalition's payout.

### 3.4 OGame, Rise of Kingdoms, EVE, Conflict of Nations, Foxhole
- **OGame.**
  - Noob protection blocks attacks on players with under about 20% of your points.
  - **Vacation mode** freezes production and gives immunity.
  - Survival means "fleetsaving" (keeping your fleet in flight so it cannot be hit).
  - Players inactive for 7 days or more become farmable "(i)" targets.
  - Sources: [Fandom](https://ogame.fandom.com/wiki/Vacation_Mode), [ogame.life](https://ogame.life/ogame/blog/ogame-fleet-save-guide-how-to-protect-your-fleet-while-offline) **[S]**.
  - **Lesson [I].** Real-time raiding creates alarm-clock play. Immunity toggles are necessary, but they are a patch on a bad time model.
- **Rise of Kingdoms.**
  - KvK seasons match kingdoms together, with power caps per season (e.g. 15M/25M/35M) and migration rules based on character age ([Guides](https://heaven-guardian.com/rok-migration-guide-2026/)) **[S]**.
  - Whales, some spending $1,000 or more a month, lead, while free players "fill up buildings and donate resources" ([RoK Guides](https://riseofkingdomsguides.com/is-rise-of-kingdoms-pay-to-win-game-pay-to-win-vs-free-to-play/)) **[S]**.
  - **Lesson [I].** Power caps and matchmaking help. When the prize is money and power can be bought, the outcome is plutocracy.
- **EVE Online.**
  - Holding sovereignty rewards huge coalitions that cover every timezone ([EVE forums](https://forums.eveonline.com/t/nullsec-blobs-and-how-to-solve-them/278435)) **[S]**.
  - Time Dilation slows overloaded systems down to 10% speed instead of crashing them, as in the Battle of B-R5RB ([Wikipedia](https://en.wikipedia.org/wiki/Battle_of_B-R5RB)) **[S]**.
  - CCP keeps redesigning sovereignty to make space cost upkeep (Equinox adds power and workforce requirements) ([Equinox](https://www.eveonline.com/news/view/equinox-in-focus-reinvigorating-nullsec)) **[S]**.
  - **Lesson [I].** Holding territory needs upkeep that scales faster than linearly with size, or the largest blob wins by default.
- **Conflict of Nations.**
  - Real-time games lasting days. Every province is worth VP, and cities are worth more depending on population and buildings.
  - **Coalitions of up to 5 need a *higher* VP threshold** than solo players.
  - Victory is checked only at the day change.
  - Inactive players are removed from the coalition after 48 game-hours.
  - Sources: [Wiki VP](https://conflictnations.fandom.com/wiki/Victory_Points), [Coalition](https://conflictnations.fandom.com/wiki/Coalition), [Help](https://bytro.helpshift.com/hc/en/6-conflict-of-nations/faq/292-i-have-enough-victory-points-but-the-game-did-not-end-why/) **[S]**.
  - This is a strong precedent for **coalition handicaps**.
- **Foxhole.**
  - Wars last weeks. Victory means holding a set number of Victory-Condition towns (default 32).
  - War *Charlie 9* stalemated for 71 days. The developer then **lowered the VP requirement gradually over 48 hours** to force an end, because new players never got to see early- or mid-war play ([PCGamesN](https://www.pcgamesn.com/foxhole/longest-deadliest-war), [Wiki](https://foxhole.wiki.gg/wiki/World_Conquest)) **[S]**.
  - **Lesson [I].** Put a *pre-committed* decay on the victory threshold into the rules, so it is not an ad-hoc intervention.

---

## 4. Cross-cutting design problems and best-known solutions

**1. Always-online pressure vs asynchronous play.**
- Real-time arrivals (Travian, OGame) reward being awake at 4 a.m. Every fix is a patch: vacation mode, sitters, fleetsave **[S above]**.
- LongTurn's 23-hour turns and Old World's order budgets remove the reflex component **[S]**.
- **Recommendation [I].** Use simultaneous-resolution ticks (all orders submitted during a window, then resolved together). Everyone submits orders in the window, the server resolves them deterministically, and **when you submitted doesn't matter**.

**2. Newcomer vs veteran snowball.**
- Known tools:
  - Protected starting zones that open in stages (Screeps).
  - Late-joiner gold (LongTurn).
  - Point-ratio attack limits (OGame).
  - Costs that rise with size (Civ VI).
  - Consolation legacies (Civ VII Dark Age).
  - Soft resets each season (Civ VII Ages).
  - Scaling NPC pressure against the leader (Travian Natars, Stellaris crises).
  - All **[S]**.

**3. Alliance and blob dominance.**
- Known tools:
  - Hard member caps (Travian 60) **[S]**.
  - Higher victory thresholds for coalitions (Conflict of Nations) **[S]**.
  - Upkeep that grows faster than linearly (EVE Equinox direction) **[S]**.
  - Objectives that need *many separate locations* held at once, which stretches a blob's timezone coverage **[I]**.
- Caps only work if Sybil wings are expensive. Travian "wings" show that caps get routed around **[S]**.

**4. Multi-accounting and bots.**
- Travian bans both and still fights them **[S]**.
- Screeps and Dark Forest *legitimize* automation instead **[S]**.
- With agents as equals, "bots" stop being cheating. **Sybil civs** remain the real threat: one owner running feeder civs to transfer resources or vote.
- **Mitigations [I]:**
  - A per-civ entry fee as a Sybil cost.
  - Travian-style caps on transfers between civs.
  - Transaction fees on inter-civ trade.
  - No prize for any metric that feeder civs can inflate.
  - Onchain clustering heuristics used only for review, never as automated guilt.

**5. Kingmaking.**
- Known mitigations: hidden or partially hidden scores, limiting how much one player can hurt another, and avoiding all-or-nothing swings ([Skeleton Code Machine](https://www.skeletoncodemachine.com/p/kingmaking), [Wikipedia](https://en.wikipedia.org/wiki/Kingmaker_scenario)) **[S]**.
- In a prize game, kingmaking becomes **bribery**: "I'll hand you my cities for 20% of your payout". Mitigations:
  - Transfers are banned or nerfed close to season end.
  - Conquest points count only for cities held for some time.
  - Payouts are split into several category prizes, so no single throne is worth buying **[I]**.

**6. End-of-season rush.**
- Solutions:
  - Score by **time held** (Screeps' log uptime formula).
  - Check victory only at a fixed time (Conflict of Nations day change).
  - Freeze all transfers in the final window.
  - Sources: **[S]/[I]**.

**7. Making peace or neutrality a real victory.**
- Civ VI's vote-based diplomacy was criticized as a bribery race **[S]**.
- Endless Legend's accumulated peace points are measurable **[S]**.
- **Principle [I].** Score peace positively (prosperity and trade accumulated while not an aggressor, read from the grievance ledger), not as the absence of war. Being attacked must not remove it, or griefers can strip it at will.

**8. Meaningful war without griefing.**
- Tools:
  - War weariness and grievances (Civ VI) **[S]**.
  - Loyalty pressure makes distant conquests expensive to hold **[S]**.
  - Automatic peace at season end (Civ VII) **[S]**.
  - Protection based on point ratio (OGame) **[S]**.
  - "Raze cooldowns", so a civ cannot be fully eliminated within one day **[I]**.

**9. Time model.**
- Options: turns (Civ; broken by waiting), real-time ticks (Screeps, Travian; favour bots and insomniacs), or action points (Old World).
- **Recommendation [I].** A hybrid: fixed-interval simultaneous-resolution ticks (e.g. every 1–4 hours), plus a per-tick **order budget** that is identical for humans and agents, plus a small carry-over bank similar to Screeps' CPU bucket (e.g. up to 3 ticks' worth of orders) so a human who sleeps loses nothing.

### 4.1 Keeping humans competitive against agents (extra depth, as requested)
- **Facts [S]:**
  - Dark Forest's winner was the heaviest automator, with a manual diplomat close behind ([wrap-up](https://blog.zkga.me/v6-r1-wrapup)).
  - AlphaStar needed **camera-view restrictions and APM caps** before it could be compared fairly with humans, and the comparison stayed contested ([AI Impacts](https://aiimpacts.org/the-unexpected-difficulty-of-comparing-alphastar-to-humans/), [DeepMind](https://deepmind.google/blog/alphastar-grandmaster-level-in-starcraft-ii-using-multi-agent-reinforcement-learning/)).
  - Meta's **Cicero** scored more than twice the average human on webDiplomacy. It was in the top 10%, and humans did not detect it ([Gizmodo](https://gizmodo.com/meta-ai-cicero-diplomacy-gaming-1849811840), [Singularity Hub](https://singularityhub.com/2022/11/28/meta-created-an-ai-that-beat-humans-at-diplomacy/)).
  - So **diplomacy is not a safe human moat**.
- **Design implications [I]:**
  1. **Neutralize speed.** Orders submitted within a tick window resolve simultaneously, and order-of-arrival within a tick carries no advantage. That removes the APM and latency edge completely, unlike AlphaStar's partial caps.
  2. **Neutralize volume.** The order budget per civ per tick is identical for everyone (Old World orders). An agent cannot micro 500 units if everyone gets, say, 20 orders.
  3. **Neutralize information.** One public API serves both the official client and agents. The fog of war is enforced server-side, and nothing is exposed to agents beyond what a human sees. Where possible, publish fog-of-war state only as commitments (the Dark Forest zk approach) or through a trusted server view. Otherwise onchain transparency hands scraping bots an omniscient view.
  4. **Give humans first-party automation.** Standing orders, city governors, auto-explore, routine trade routes and "if attacked, then…" triggers, as in Travian's farm lists and Dark Forest plugins. The human then decides *strategy* and the automation handles *chores*. Humans will lose if the game is a test of tedium.
  5. **Reward judgement over optimization.** Rotate season rules (Screeps), include hidden or partly revealed objectives (Humankind-style deeds revealed per season), and add events that need choices under uncertainty. This narrows the edge of overfitted optimizers.
  6. **Do not rely on detecting humans.** Cicero shows it cannot be done reliably. Keep one ruleset for everyone. At most, publish the declared "human" or "agent" type as a cosmetic leaderboard filter, never as a prize split, unless the team decides separate divisions are worth it.

---

## 5. Assessment of the victory-condition candidates

| Candidate | Measurability | Exploitability | How it interacts with the others | Precedent | Verdict |
|---|---|---|---|---|---|
| **War / Conquest** | High. Count conquered settlements, weighted by population, that are **held continuously for 7 days or more** at season end, or use first-time-conquest points as in Civ VII Ideology Points **[S]**. | High if Sybil feeders give up cities. Mitigate by scoring only cities founded by a *different owner* at least N days earlier, requiring a minimum population, scoring each city at most once per season, and giving no score from coalition partners **[I]**. | Directly threatens every other path. Makes the others defensively interesting. | Civ domination, Civ VII Operation Ivy, Foxhole VP towns, Polytopia Might **[S]** | **Keep.** |
| **Most territory** | Very high: deterministic hex count. | Spamming worthless tiles; end-of-season land grabs; blobs. Mitigate with **tile-days** (time-integrated), weight by tile value, add upkeep that grows faster than linearly, and use a coalition handicap (Conflict of Nations) **[S]/[I]**. | Overlaps heavily with conquest, because conquest yields territory. | Endless Legend Expansion (80%), Conflict of Nations VP, Screeps S6 **[S]** | **Merge with Conquest** into one "Dominion" track, scored by valued tile-days *plus* conquered-city points. Or keep it separate only if the territory measure strongly favours peaceful settling. |
| **Largest army** | High (sum of unit strength). | **Degenerate.** It rewards hoarding and not fighting, it can be whaled if power can be bought, and it invites last-day mass builds. Measuring a snapshot invites rushing. | Undermines war (armies become trophies, not tools) and peace (dominant armies loom over everyone). | No major 4X uses it as a win condition. RoK "power" rankings are its closest cousin, and the source of RoK's whale dynamics **[S]** | **Drop.** Keep army size as an input to Dominion and as a public stat, not as a prize. |
| **Science** | Very high: tech count or a final project chain. | Tech gifting and trading from feeders; passive snowball. Mitigate by making tech non-transferable, requiring a **visible multi-stage project** that others can sabotage (spies or conquest of the project city), and following Soren Johnson's rule of no surprise endings **[S]/[I]**. | Gives builders a non-military path; the project city becomes a war target, which is good for interaction. | Civ space race, Civ VII staffed flight, Humankind end trigger **[S]** | **Keep.** It is the cleanest deterministic "builder" victory. |
| **Peace** | Medium. Measurable if defined as prosperity accumulated (trade volume, population growth, suzerainty over NPC neutrals) during ticks when the civ has no *aggressor* grievances. Not measurable if defined as "was at peace". | Collusive trade laundering between Sybils; vote buying if votes exist. Mitigate by weighting trade by counterparty diversity with fees and decay, avoiding voting by civs, and using NPC city-state influence (non-Sybil actors, with Civ VII's suzerainty lock) **[S]/[I]**. | Natural rival to conquest. Peaceful civs become targets, which is fine if defensive war does not break peace status. | Civ VI Diplomatic (criticized), Endless Legend Diplomatic peace points **[S]** | **Keep, merged with neutrality** into one "Concord" track. |
| **Neutrality** | Low on its own. "Never allied, never at war" is trivially satisfiable by passive or AFK civs and rewards doing nothing. | High: AFK Sybil civs farm it. Needs a minimum activity or prosperity floor. | Largely duplicates peace. Structurally conflicts with coalition play. | No strong precedent **[I]** | **Merge into Concord** as a multiplier. Civs with no alliance membership and no aggressor record get a bonus on Concord points (e.g. ×1.25), which makes neutrality a *strategy* rather than a separate prize. |

**Recommended prize structure [I].** Use **three to four tracks, each with its own pool share**: Dominion (war plus territory), Science, and Concord (peace plus neutrality). Optionally add a fourth "Legacy/Score" track for overall excellence, Humankind-style, so generalists are rewarded.
- Cap one top-tier prize per civ, and one per coalition, so one blob cannot sweep every track.
- Coalitions compete on coalition boards with a raised threshold and a share split fixed ahead of time (Conflict of Nations precedent).
- Split rewards across the top N of each track, not winner-take-all. This reduces kingmaking and bribery, and keeps mid-table players engaged through the endgame.
- The alternative is Humankind's single currency plus several end triggers. It is simpler, but it was criticized as too prone to snowballing **[S]**. Tracks keep niche strategies viable.

---

## 6. Design recommendations (20)

1. **Use simultaneous-resolution ticks, e.g. every 2–4 h, with an identical order budget per civ.** Ordering within a tick has no advantage. This is the foundation of fairness between humans and agents (Old World orders + LongTurn + AlphaStar lesson) **[I]**.
2. **Add an order bank** capped at about 3 ticks, similar to Screeps' CPU bucket, so sleeping or travelling humans are not punished **[S precedent]/[I]**.
3. **Ship one public API used by the official client, with the fog of war enforced server-side.** Agents get exactly what humans see, and nothing onchain reveals hidden state **[I]** (Dark Forest zk as the reference **[S]**).
4. **Give humans first-party automation**: governors, standing orders, conditional defence and trade routes (lesson from Travian Gold Club and Dark Forest plugins) **[S]/[I]**.
5. **Run a persistent world with seasonal soft resets, like Civ VII Age transitions.** At each boundary: auto-peace, prune armies to an upkeep cap, respawn NPC neutrals, and reset the season score tracks **[S]**.
6. **Carry over legacies across seasons, but cap them and make them mostly horizontal** (cosmetics, unlockable civ variants, small choices). Include a **Dark-Age consolation legacy** for civs that scored nothing **[S]/[I]**.
7. **Create protected newcomer zones that open in stages**, with timers and quarter-by-quarter wall drops as in Screeps, and give late joiners catch-up resources as in LongTurn **[S]**.
8. **Make upkeep grow faster than linearly** with tile count, cities and army size (Civ VI cost scaling, EVE sovereignty upkeep) **[S]**.
9. **Add NPC escalation against leaders**: crisis stages in the final third of the season that pressure the top-scoring civs hardest (Travian Natars, Stellaris crises, Civ VII crises) **[S]**.
10. **Score victories on time-integrated holdings** (tile-days, city-days, the Screeps log-uptime formula), not end-of-season snapshots **[S]/[I]**.
11. **Check victory only at a pre-announced time** (Conflict of Nations day-change) and **freeze inter-civ transfers in the last 72 h** **[S]/[I]**.
12. **Publish Foxhole-style automatic stalemate breakers**: if no track is decided by day X, thresholds decay on a published schedule **[S]**.
13. **Keep a grievance ledger** as an objective, onchain record of aggression. It drives war weariness, loyalty and eligibility for Concord **[S]**.
14. **Handicap coalitions**: hard member caps, higher thresholds for coalition victory, and fixed internal payout splits declared before season start **[S]/[I]**.
15. **Use NPC city-states as a non-Sybil arena for peace**, with locked suzerainty per season **[S]**.
16. **Require a visible, sabotageable final project for Science**. No hidden auto-wins ("no surprise endings") **[S]**.
17. **Treat the per-civ entry fee as the Sybil cost.** Cap resource transfers between civs, charge fees on trade, and weight trade scores by counterparty diversity **[S]/[I]**.
18. **Handle inactivity like LongTurn**: idle civs go into auto-defence governor mode, and after a threshold that grows as the season ages they become NPC free cities or can be taken over (with forfeit rules for the entry fee) **[S]/[I]**.
19. **Rotate one scoring modifier per season**, as Screeps Seasonal does, so optimizers cannot overfit and each season stays fresh **[S]**.
20. **Keep the map compact per player**, taking Polytopia-style compression as a guide, so a meaningful season fits in 4–8 weeks without Civ's late-game tedium **[S]/[I]**.

## 7. Anti-patterns (10)

1. **Real-time arrival combat with alarm-clock defence** (Travian and OGame fleetsaving). It punishes humans and hands the game to bots **[S]**.
2. **Victories decided by votes** that Sybils or bribes can buy (Civ VI World Congress criticism) **[S]**.
3. **"Largest army" as a paid victory.** It rewards hoarding and whaling (RoK power dynamics) **[S]/[I]**.
4. **Scoring on end-of-season snapshots.** It guarantees last-day rushes and bribed transfers **[I]**.
5. **A single snowballing win currency with no counterplay** (Humankind Fame criticism) **[S]**.
6. **Tracks that make every path mandatory**, which makes every game identical (Civ VII Legacy Path criticism) **[S]**.
7. **Selling power, or selling more compute or orders**, which Screeps had to walk back **[S]**.
8. **Uncapped alliances without coalition handicaps**. The result is one meta-alliance and paying spectators (Travian endgame, EVE blobs) **[S]**.
9. **Ad-hoc developer intervention to end stalemates**, which in a prize game creates legal and trust risk. Pre-commit the rules instead, unlike Foxhole's manual VP cut **[S]/[I]**.
10. **Trying to detect or ban agents, or leaking hidden state onchain.** Detection fails (Cicero went unnoticed), and a transparent chain gives scrapers perfect information **[S]/[I]**.

---

**Caveats.**
- Several game-rule numbers come from fan wikis (Fandom, wiki.gg). They may lag behind patches, especially for Civ VII, which is still being patched.
- The PC Gamer Civ VII transition article and the Fandom Age page could not be fetched, so the transition details come from CivFanatics, Steam threads and PCGamesN.
- The EVE "blob" analysis leans on forum and community sources, not official design statements.
