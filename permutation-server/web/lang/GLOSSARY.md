# English glossary — Wylls web client

The canonical English for every game term, so that every screen reads as one
game. When a Japanese term below appears in a text you translate, use this
English (and its capitalisation). `llms.txt` (the agents' guide) uses the
same words (faction, one civilization); where in doubt, follow this glossary.

Enum names (offices, paths, units, buildings, techs, terrain, resources,
focus, relations, phases, blocked reasons, chain errors, chronicle lines,
standing orders, merit sources) are already translated in `i18n.mjs`: use
those tables (`T.ROLE_JA[r]`, `T.TECH[t]`, `T.blockedText(b)` …), never
retype their names in a dictionary.

## Style

- **Short game UI English.** Labels and buttons: sentence case, no final
  period (`End turn`, `Open diplomacy`). Sentences in descriptions and
  toasts: normal sentences with periods.
- **Proper game nouns are capitalised**: offices (General, Steward, Science
  Officer, Diplomat), paths (Hegemony, Prosperity, Science, Concord), unit,
  building and tech names, Star Gate, Faction Plaza. Generic nouns are not:
  tick, era, tier, member, proposal, recall, treasury, city-state.
- **Numbers with units**: `Tick 12`, `Era 3`, `Tier 2`, `12 tiles`,
  `30 s` (or `30s` in tight labels), `5 USDC`, `12 gold`, `+3 / tick`,
  `Food +2`, `3 pts` (tight) / `3 points`. Keep `×`, `/`, `·`, `→` as in the
  Japanese.
- **Counters** (人, 件, マス, 秒, 点, つ) become English nouns with plurals —
  use `plural(n, '{0} member', '{0} members')` in the dictionary.
- **Spelling and apostrophes**: American spelling (`Defense`, `color`,
  `in favor`); straight apostrophes (`don't`, `faction's`), curly
  double quotes only for 「X」.
- **Punctuation**: 「X」 → “X” (or no quotes for a name); （…） → (…);
  ： → `: `; 、 → `, `; 。 → `. `; list separator ・ → `, ` (or ` · ` in
  compact lines); 〜 → `–`; ＋ → `+`; full-width spaces → a normal space.
- **Keep ALL-CAPS eyebrows as they are** (`YOUR TURN`, `CITY`, `ERAS`): the
  Japanese half after `·` goes away in English (`YOUR TURN · このティック` →
  `YOUR TURN · This tick`, or just `YOUR TURN` when it would repeat).
- **You / your faction**: address the player as “you”; あなたの勢力 → “your
  faction”; 自分の勢力 (formerly 自国) → “your faction”; 自軍 → “your army”.
- **Names are not translated**: member names, AI member names, city-state
  numbers (`City-state 3`), wallet names (Phantom, Solflare, Backpack),
  MagicBlock ER, Solana, USDC, SOL, x402, RPC, CU, slot.

## The world and the season

| Japanese | English | Notes |
|---|---|---|
| 世界 | world | |
| 勢力 | faction | the six playable powers inside the one civilization (formerly 国 / nation) |
| 文明 | civilization | the one civilization every player lives in (all six factions together); never a single faction; the `ONE CIVILIZATION` label |
| メンバー（旧: 国民） | member | a person or agent in a faction; メンバー N人 → `N members` |
| 人（メンバーの数） | members / people | |
| シーズン | season | |
| ティック | tick | never “turn”, except in “End turn” |
| 締切 | deadline | the tick's commit deadline |
| 時代 / 第N時代 | era / Era N | |
| 4つの道 | the four paths | |
| 覇権 | Hegemony | path |
| 繁栄 | Prosperity | path |
| 科学 | Science | path, yield |
| 協調 | Concord | path, lens |
| 節目 | milestone | |
| 段階 | tier (a path's milestone level) / stage (Star Gate) | 第N段階 → `tier N` / `stage N` |
| 点 | points (`pts` in tight labels) | |
| 取り分 | share | a faction's or member's share of the pool |
| 見込み | projected | 今の1人あたりの見込み → `projected per member now` |
| 賞金 | prize | |
| 賞金プール | prize pool | |
| 参加費 | entry fee | |
| 勢力の資金（旧: 国庫） | treasury | a faction's USDC (not 金庫, the prize vault) |
| 預け入れ（勢力の資金への） | deposit | |
| 運営 | the operator | |
| 運営のAIメンバー | the operator's AI members | |
| AI の代行 / 代行 | caretaker | the rules acting for a vacant office; `the caretaker` in sentences |
| 功績 | merit | |
| 活動したメンバー / 活動中 | active members / active | |
| 区間 | window | activity windows (`9 of 18 windows`) |
| 歴史 / 歴史の層 | history / earlier seasons | |
| 歴史のルート | history root | |
| 草創 / 拡大 / 競合 / 危機 / 決着 | Founding / Expansion / Contention / Crisis / Resolution | season phases (i18n.mjs PHASES) |
| 開幕 / 開幕する | start (of the season) / Start the season | |
| 開幕前 | before the start | |
| シーズン終了 | Season over | |
| 結果 | results | |
| 暗黒時代 | dark age | |
| ユーリカ | eureka | |

## Government

| Japanese | English | Notes |
|---|---|---|
| 役職 | office | |
| 役職者 | officer | |
| 将軍 | General | |
| 内政官 | Steward | |
| 科学官 | Science Officer | |
| 外交官 | Diplomat | |
| 担当 / 担当の役職 | your office(s) | 担当外 → `outside your offices` |
| 空席 | vacant | |
| 選挙 / 第1回選挙 | election / first election | |
| 任期 | term | |
| 立候補 / 立候補する | candidacy / stand | 〜に立候補 → `Stand for …`; 立候補中 → `Standing` |
| 立候補者 | candidate | |
| 投票 / 票 | vote / votes | 投票受付中 → `Voting open` |
| 次点 | runner-up | |
| 献策 | proposal (noun) / propose (verb) | orders to an office you don't hold |
| 献策者 | proposer | |
| 採用 / 採用する | adopt / Adopt | 採用予定 → `To adopt` |
| 支持 / 支持する | support / Support | 支持済み → `Supported` |
| リコール | recall | リコールに賛成 → `vote to recall` |
| 解任 | removal (by recall) | |
| 賛成 | in favor / yes votes | |
| 過半数 | majority | |
| 同意（宣戦・支出への） | consent | ConsentWar, ConsentSpend |
| 勢力の広場 | Faction Plaza | the government drawer |
| 政府 | government | 〜の政府 → `Government of …` |

## Orders and the tick

| Japanese | English | Notes |
|---|---|---|
| 命令 | order | |
| 枠 / 命令の枠 | slot / order slots | the budget; 枠1 → `1 slot` |
| 勢力の枠 | faction budget | |
| 繰越 / 繰越の枠 | banked / banked slots | |
| 下書き | draft | 下書き · 未確定 → `Draft · not committed` |
| 確定 / 確定する | commit / Commit | 確定済み → `Committed` |
| 封印 / 封印する | seal / seal | 確定で封印 → `Sealed on commit` |
| 公開 / 公開する | reveal | the sealed batch after the deadline; also “public” for information everyone sees |
| 公開済み | revealed | |
| 解決 / 解決中 | resolution / Resolving | |
| 手番を終える | End turn | 手番を終えた → `Turn ended` |
| 見送られた命令 | skipped orders | |
| 継続命令 | standing order | |
| 自動防衛 / 撤退 / 巡回 | auto-defend / retreat / patrol | |
| 生産の繰り返し / 自動購入 | repeat production / auto-buy | |
| 次の判断 | next decision | Space key |
| 判断メモ | rationale | |
| 判断ログ | decision log | |
| 判断の証拠 | decision proofs | |
| 約束（digest） | commitment | |
| 観測 / 観測ルート | observation / observation root | |
| 根（ルート） | root | 前の根 / 新しい根 → `previous root` / `new root` |
| 葉 | leaf | Merkle proof |
| 証明する | Prove | |
| 一致 / 不一致 | match / mismatch | |
| 検証 / 検証中 / 検証済み | verify / verifying / verified | |
| 霧 / 視界 / 未踏 | fog / sight / unexplored | |

## Map, cities and armies

| Japanese | English | Notes |
|---|---|---|
| 地図 / 全体図 | map / minimap | |
| マス / 土地 | tile | |
| 地形 | terrain | |
| 川 | river | |
| 資源 | resource | |
| 領土 | territory | |
| 都市 | city | |
| 首都 | capital | |
| 自由都市 | free city | |
| 遺跡 | ruins | |
| 都市国家 | city-state | `City-state 3` |
| 宗主 | suzerain | |
| 使節 | envoy | |
| 影響力 | influence | |
| 交易拠点 | trade hub | |
| 保護区域 | protected zone | |
| 蛮族 | barbarians | |
| 部隊 | unit | 軍 → `army` |
| 兵 / 兵数 | troops | 兵5 → `5 troops` |
| 非戦闘ユニット | civilian unit | |
| 開拓者 / 斥候 | Settler / Scout | |
| 移動 / 移動中 | move / moving | |
| 攻撃 | attack | |
| 占領 / 落とす | capture / take | |
| 征服 | conquer | |
| 破壊 | raze | |
| 捕獲 | capture (a civilian) | |
| 射程 | range | |
| 待機 / 待機中 | idle | |
| 到着 | arrives / arrival | |
| 生産 / 生産予定 | production / production queue | 生産予定が空 → `Nothing in production` |
| 方針 | focus | city focus (土地の割り当て → `tile assignment`) |
| 建物 | building | |
| 購入 | purchase / buy | |
| 研究 / 研究中 / 研究済み | research / researching / researched | |
| 技術 | tech | |
| 解放 | unlocks | |
| 蓄積 | stored | |
| スターゲート | Star Gate | |
| 人口 / 成長 | population (Pop) / growth | |
| 食料 / 生産 / 金 | Food / Production / Gold | yields |
| 快適度 / 忠誠 / 防御 | Amenities / Loyalty / Defense | |
| 産出 | yields | also the lens name |
| 維持費 | upkeep | |
| レンズ | lens | 地形 / 勢力 / 産出 / 軍事 / 協調 → Terrain / Factions / Yields / Military / Concord |
| 住む都市 | home city | where an operator AI member lives |
| 懸賞金 | bounty | |

## Diplomacy

| Japanese | English | Notes |
|---|---|---|
| 外交 | diplomacy | |
| 関係 | relation | |
| 平和 / 戦争 | peace / war | |
| 宣戦 / 宣戦する | declare war / Declare war | |
| 講和 | peace | 講和を申し入れる → `Offer peace` |
| 休戦 | truce | |
| 不可侵条約 / 不可侵 | non-aggression pact / NAP | first mention in a sentence may spell it out |
| 同盟 | alliance | 同盟から離脱 → `Leave the alliance` |
| 条約 / 条約相手 | treaty / treaty partner | |
| 条約破棄 | breaking a pact | |
| 保証金 | bond | |
| 申し入れ | offer | diplomatic; never “proposal” (that is 献策) |
| 届いた申し入れ | offers received | |
| 受け入れる / 受諾 | accept | |
| 失効 | expires | |
| 成立 | takes effect / made | |
| 不満 | grievance | |
| 正当な開戦理由 | casus belli | |
| 侵略 / 侵略中 | aggression / aggressor | |
| 契約（勢力の資金の契約） | contract (treasury contract) | |
| 預かり | escrow | |
| 期限（契約） | deadline | `by tick N` |
| 取り下げる | Withdraw | |
| 分割払い | installments | |

## Markets and money

| Japanese | English | Notes |
|---|---|---|
| 市場 | market | 金の市場 → `gold market` |
| 取引所 / USDC取引所 | exchange / USDC exchange | |
| 取引（市場） | trade | |
| 取引（ウォレット・チェーン） | transaction | |
| 売買 / 買う / 売る | side / Buy / Sell | |
| 品目 / 数量 / 単価 | good / quantity / unit price | |
| 見積もる / 見積もり | Quote / quote | |
| 在庫 | stock | |
| 手数料 | fee | |
| 関税 | tariff | |
| 約定 | fill | 一括競売 → `uniform-price auction` |
| 輸送中 | in transit | |
| 累計支出 / 支出 | total spent / spending | |
| 交易 | trade | |
| 富 | wealth | |
| 凍結 / 凍結中 | frozen | |
| 原材料 | raw goods | |

## Chain, wallet and membership

| Japanese | English | Notes |
|---|---|---|
| チェーン / オンチェーン | chain / on chain | |
| ローカル / ローカルモード | local / local mode | |
| ゲートウェイ | gateway | |
| ゲームサーバー | game server | |
| ウォレット | wallet | |
| 接続 / 切り替える | connect / switch | |
| 署名 / 署名する | signature / sign | |
| ゲーム内の鍵 | in-game key | the session key |
| 鍵のバックアップ | key backup | |
| テスト USDC | test USDC | “no value” |
| 残高 | balance | |
| 受け取る | claim (a prize) / get (test USDC) | |
| 精算 / 精算待ち | settlement / awaiting settlement | |
| 登録 / 登録済み | registration / registered | 勢力を選ぶ → `Choose your faction` |
| 未申告 | undeclared | member kind |
| 送信 / 送信中 / 送信済み | send / sending / sent | |
| 観戦 / 観戦する / 観戦中 | spectate / Spectate / spectating | |
| 全体表示 | whole civilization | spectator view of all factions |
| エクスプローラー | explorer | |
| 手数料（SOL） | fees (SOL) | 運営が払う → `paid by the operator` |

## Talk and chronicle

| Japanese | English | Notes |
|---|---|---|
| 会話 | talk | |
| メッセージ | message | |
| 全員へ / 〜へ | to everyone / to … | |
| 年代記 | chronicle | |
| 出来事 | events | |
| 戦い / 外交 / 発展 | war / diplomacy / growth | chronicle filters |

## Never translate

- The wallet session-key message (`session.mjs` `sessionText`) and anything
  that is signed or hashed (order batches, rationales as written, talk
  messages, seals, domain tags such as `PS/session/v1`): the keys and
  commitments are derived from those exact bytes.
- Engine enum names sent to or from the server (`General`, `SetResearch`,
  error `code`s), localStorage keys, CSS classes, data-* attributes.
- What people typed (member names, rationales, talk).
