# Wylls — Design V3「Proof of Consequence」

Status: **不採用（2026-09-24）。ゲーム設計そのものを [Game Design V4](PERMUTATION_STATE_GAME_DESIGN_V4.md) で見直したため、履歴として残す。**
対象: ゲーム全体の設計と、Colosseum（Solana＋MagicBlock）提出（締切 2026-10-15）。
位置づけ: [map-first rebuild](PERMUTATION_STATE_REBUILD.md) を「舞台」として残し、[Game Constitution](PERMUTATION_STATE_GAME_CONSTITUTION.md) の核（Handoff）をその上に戻す。コード変更はこの文書の承認後に行う。

---

## 0. 結論

> **Every contribution is proven by who used it.**
> 貢献は「自分が何をしたか」ではなく「自分の成果を**他の誰が使ったか**」で決まり、その因果の連鎖はチェーン上で改ざんできない。

現行の `/civilization/` は、地図・リアルタイム・全員でひとつの文明という点では正しい方向です。一方で、次の3つの理由から「Eternum の縮小版」になりかけています。

| 症状 | 場所 | Constitution の何に反するか |
|---|---|---|
| `player.contribution` は数値を加算するだけ（探索・道路・建設・納品・研究） | `core.mjs:493-563, 683` | Rule 4「匿名のバーの0.001%にしない」、Pillar 2 |
| `events` は文字列ログで、100件で切り捨てる。誰の何が誰の何を可能にしたかを持たない | `core.mjs:68-72` | Pillar 3（Handoff）、Pillar 6（因果が読めること） |
| 進行中の①経済・②土地の拡張は、資源シミュレーションを深める方向 | `IMPLEMENTATION_STATUS.ja.md` | Rule 18「広く浅いより、深い因果の連鎖ひとつ」 |
| チェーン側は旧 East Sluice／8人 World PDA 用で、新しいゲームとつながっていない | `solana-receipt-spike/src/lib.rs` | 審査で「チェーンを使う必然性」を示せない |

V3 では、このシミュレーションに **Consequence Ledger（因果台帳）** を足します。台帳は、ゲームの体験・貢献の評価・Sybil 対策・シーズン精算のすべての根拠になります。

---

## 1. 目的（一文）

**知らない市民が残した道・発見・計画の上で自分が行動し、自分の成果が次の誰かに使われる。その連鎖が文明の歴史になり、誰にも書き換えられない形で精算される。**

プレイヤーが最後に言えるべきこと（Constitution の Final Standard を V3 向けに言い換えたもの）:

> 「誰かが通した道の先に、私は鉱山を建てた。その鉱石で、会ったこともない人が学術院を完成させた。シーズンの終わりに、それが全部つながって記録されていた。」

### 審査員が20秒で理解すべきこと
1. 全員でひとつの文明を共有している
2. **A の成果を B が使ったことが画面に出て、チェーンに刻まれる**
3. 自分の成果を自分で使っても貢献にならない（Sybil 対策と精算が同じ仕組み）

---

## 2. 残す／変える／凍結する

### 残す（現行の資産）
- 217ヘックスの共有マップ、地形を考慮した経路探索、現地か隣接地でしか行動できないルール
- 250ms 刻みのサーバー権威シミュレーション、複数クライアント、ディスク保存（`civilization-service.mjs`）
- 共同計画（`projectCommand`: 予約・担当・辞退・期限切れ・着工）。**Handoff の器として一番近い**
- 研究3種、NPC の働き手と運び手、表示レンズ
- Solana 側: Season PDA（purse・`FinalizeSeasonArgs{outcome_hash, chronicle_root, claim_root}`・`claim_leaf_hash`・merkle 証明の検証）、guard 付きで順番に追記する仕組み（`expected_seq`・`expected_head_event_hash`）、MagicBlock の Delegate → ER → Commit のライフサイクルと transport

### 変える
| 現在 | V3 |
|---|---|
| `player.contribution += n` | 因果台帳から算出する **Consequence Score**（§3.4） |
| `events`（文字列、100件） | **Consequence Ledger**（追記のみ、ハッシュ連鎖）。`events` は表示用の要約として残す |
| 共同計画 = 材料予約の仕組み | 共同計画 = **Handoff の単位**（要請 → 予約 → 担当 → 完成 → **他者が利用**） |
| NPC = 規則で動く労働力 | 労働力に加えて、**台帳を記憶として語る名前付き NPC を1人**置く |
| チェーン = 旧デモの状態を複製 | チェーン = **因果エッジと精算だけを記録** |

### 凍結（未完了の項目として残し、削除はしない）
- ② 土地の収量差・遠方の産地
- ① のうち、増産モード（通常・休止・道具）と倉庫増設の細部。**容量と予約は残す**（材料の取り合いを生むため。§4）
- 公開サーバー、他端末からの一般参加（⑦）

---

## 3. コアモデル: Consequence Ledger

### 3.1 Artifact（出所を持つ成果物）

「誰が作ったか」を持ち、他の市民が**使える**ものだけを Artifact として扱います。

| kind | 作られるとき | 作者（creator） | 現行コードでの位置 |
|---|---|---|---|
| `road` | ROAD の完了 | 道路を整備した市民 | `core.mjs:495`（`tile.road = true`） |
| `survey` | EXPLORE／見張り塔で土地が開けたとき | 開けた市民 | `revealAround`（`core.mjs:273`） |
| `building` | 建物の完成 | 建てた市民（計画経由なら提案者と担当者の両方） | `startBuilding`（`core.mjs:344`）、完成時 |
| `plan` | CREATE_PROJECT | 提案者（材料を予約した人） | `projectCommand`（`core.mjs:355`） |
| `tech` | 研究の完了 | `research.ownerId` | `core.mjs:683` |
| `lot` | 手作業の採集で備蓄に納めたとき、運び手が生産物を納めたとき | 採集者、または建物の作者 | `core.mjs:518, 563` |

データ構造（world に追加。既存フィールドは改名しない）:

```js
world.ledger = {
  artifacts: { [artifactId]: { id, kind, creatorIds: [actorId], tileId, eventSeq, createdMs } },
  edges: [ { seq, fromArtifactId, toEventSeq, consumerId, kind, weight, timeMs } ],
  entries: [ { seq, type, actorId, payloadHash, prevHash, hash } ], // 追記のみ
  head: { seq, hash },
};
// タイルに出所を持たせる
tile.roadArtifactId, tile.surveyArtifactId
```

`events` と違って `entries` は切り捨てません。サーバーはシーズン単位で保持し、チェーンには head だけを送ります（§6）。

### 3.2 Cause Edge（使われたことの記録）

**市民 B の行動が、A（A ≠ B）の Artifact を前提に成立したとき**、エッジを1本作ります。

| エッジ | 条件 | フックを付ける位置 |
|---|---|---|
| `TRAVELED_ON` | B の MOVE の経路が A の道路を通った（道路による時間短縮があった場合のみ） | MOVE（`core.mjs:440-445`）の `findPath` の結果 |
| `BUILT_ON_SURVEY` | B が A の開けた土地で BUILD／ROAD／GATHER した | `startBuilding`、ROAD と GATHER の開始 |
| `FULFILLED_PLAN` | B が A の計画を担当して着工・完成した | START_PROJECT（`core.mjs:386`） |
| `SUPPLIED_BY` | B の建設・計画で消費した材料に、A が納めたロットが含まれていた | `pay()`（`core.mjs:75`）を FIFO のロット消費に置き換える |
| `ENABLED_BY_TECH` | 研究で解放された施設や効果を B が使った | 建設の可否判定 |
| `FED_BY` | A の建物の生産物が B の建物に投入された | 投入時の運び手（`core.mjs:617`） |

ルール:
- **A = B のときはエッジを作らない**（自分で使っても貢献にならない）
- 同じ (A, B, kind) の組み合わせは、1シーズンあたり上限を設けて逓減させる（例: 1本目 1.0、2本目 0.5、4本目以降 0）
- 同じ操作権限・同じ端末指紋から生まれた市民同士は同一人物とみなす（ローカル試作では token の発行元単位。本番ではウォレットとセッションキー単位）
- エッジは**受理されたアクション**からだけ作る。拒否されたアクションや予測表示からは作らない

### 3.3 Cause Receipt（プレイヤーに見せる）

エッジが生まれた瞬間、両者の画面に出します。

```
ENABLED BY  ミナが通した東の道（12分前）
CHANGED     あなたの鉱山の着工。移動 48秒 → 21秒
COST        石材 6・木材 4（うち木材 4 はソラが納めた分）
ENABLES     鉱石が工房へ届くようになる → 道具の研究が可能に
```

- A の側には「**あなたの道を ○○ が使った**」と通知を出す。これが一番の報酬体験です
- 地図に因果の線を引くレンズ（`logistics` の隣に `consequence` レンズを追加）
- 年代記パネルは `events` ではなく台帳から「A → B → C」の連鎖を描く

### 3.4 Consequence Score（contribution の置き換え）

```
score(A) = Σ_edges(from = A の Artifact) weight
         ただし consumer ≠ A、組み合わせごとの上限と逓減を適用
reach(A) = A の成果を使った「異なる市民」の数
```

- 表示の主役は `reach`（「あなたの成果を 7 人が使った」）。点数は精算の内部計算に使う
- 間接的な利用（A → B → C）は1段階だけ加点し、減衰させる（例: 0.3）。ループやなりすましで膨らまないようにする
- **North Star** = Active Citizen 1人あたりの「他者に使われた成果の数」。Constitution の North Star Metric と同じ

---

## 4. トレードオフ（共通の目標を持ちながら、進め方で争う）

Constitution Pillar 4 を、選択肢カードではなく**共有資源の奪い合い**で実現します。

- 共有備蓄は容量で頭打ちになり、計画は材料を**予約**する（既存の仕組み）。予約した分は他の計画に回らない
- **要請（Request）**: 本物のボトルネックからサーバーが自動生成する。例:「採石場が道路につながっていない」「食料が 3 分で尽きる」「鉱石の産地が未探索」。要請には受益者（止まっている建物、待っている計画）を名前付きで付ける
- 同じ材料を取り合う要請を2つ以上、常に並べる。「北の農場を救うか、東の鉱山を先に着工するか」
- どちらを選んでも台帳に残るので、「あのときミナの計画を優先したから、ソラの工房が遅れた」という**語れる因果**になる

---

## 5. NPC（最小構成）

- 名前付き NPC を1人（例: 年代記係のタラ）置く。**状態を変えない**
- タラは台帳だけを読み、「東の鉱山はミナの道のおかげで間に合った」「ソラの納めた木材は3つの計画に使われた」と語る
- 実装はまずテンプレート。LLM は台帳の抜粋を入力にして発話だけを生成する任意の層にする（Constitution §6「AI は表現、決定論のルールが効果」）
- 既存の働き手・運び手の NPC は今のまま（規則で動くことを明記）

---

## 6. チェーン境界

### 6.1 何をどこに置くか

| 層 | 置くもの | 置かないもの |
|---|---|---|
| サーバー（オフチェーン） | シミュレーション全体、移動、生産、描画用の状態、台帳の本体 | — |
| **MagicBlock ER** | `CauseLedger` PDA: エッジごとに `edge_hash` を追記し、`seq` と `head_hash` を進める。数秒ごとにまとめて commit | 足跡、在庫、アニメーション |
| **Solana base** | Season PDA: `ruleset_hash`・`payout_rules_hash`（事前に確定）、purse、`finalize(outcome_hash, chronicle_root = 台帳の head, claim_root)`、`claim` | 個々のエッジの中身（ハッシュだけ） |

### 6.2 既存の `lib.rs` をどう流用するか
- **新規** `InitializeCauseLedgerArgs {season_id, ruleset_hash}` と `AppendEdgesArgs {guard: WorldGuard と同じ形, edges: Vec<[u8;32]> (最大 N 件), new_head}`。guard の仕組み（`expected_seq`・`expected_head_event_hash`）はそのまま使う
- ER のライフサイクル: World PDA と同じ（Delegate → Append … → Commit → CommitAndUndelegate）。transport は `client/world-magicblock-transport.mjs` を流用
- 精算: シーズン終了時にサーバーが Consequence Score から配分額を計算し、`claim_leaf_hash(season_id, claimant, index, amount)` の merkle tree を作る。既存の `FinalizeSeasonArgs.claim_root` に入れ、`chronicle_root` には CauseLedger の最終 head を入れる
- 配分規則（Constitution §8 の既定値をそのまま使う）: 80% を Active Citizen で均等、20% を Consequence Score に比例、1人あたり中央値の2倍が上限。**規則の文言をハッシュ化して `payout_rules_hash` に先に入れる**
- mock USDC、localnet（可能なら devnet）。実資金は扱わない

### 6.3 信頼の前提（隠さずに書く）
- シミュレーションの権威はサーバー（オラクル）です。チェーンが保証するのは **(1) 一度記録されたエッジを後から書き換えられないこと**、**(2) 精算規則が参加前に確定していること**、**(3) claim が台帳から再計算できること** の3点
- 第三者による検証: 台帳の本体（JSON）を公開すれば、誰でも head と claim_root を再計算して、チェーン上の値と一致するかを確かめられる。検証用スクリプトを同梱する
- 市民ごとのセッションキーでアクションに署名させる方式は stretch（§8）

---

## 7. 3分デモ台本（審査員向け）

| 時間 | 画面 | 見せたいこと |
|---|---|---|
| 0:00-0:20 | タイトル → 共有マップに市民が3人 | 「全員でひとつの文明」 |
| 0:20-0:50 | ブラウザ A: ミナが東を探索し、道路を通す | Artifact が生まれる（地図に作者の印） |
| 0:50-1:30 | ブラウザ B: ソラが東の道を通って鉱山を建てる → Receipt「ENABLED BY ミナの道」。A 側に「あなたの道をソラが使った」 | **他人の成果が自分の今を変える** |
| 1:30-1:50 | Explorer 画面: CauseLedger の seq と head が進んでいる | エッジが ER に刻まれた |
| 1:50-2:10 | ミナの別アカウントがミナの道を使う → Receipt は出るが Score は 0 | Sybil 対策と精算が同じ仕組み |
| 2:10-2:30 | タラが「東の鉱山はミナの道のおかげで間に合った」と語る | 社会の記憶 |
| 2:30-3:00 | シーズンを締める → 因果グラフ → claim 額 → Solana の claim_root と検証スクリプトの結果が一致 | **改ざんできない因果と精算** |

---

## 8. 10/15 までのスコープ

### Must（提出に必要）
1. core: `world.ledger`、Artifact の生成、Cause Edge 6種のうち `TRAVELED_ON`・`BUILT_ON_SURVEY`・`FULFILLED_PLAN`・`SUPPLIED_BY` の4種、Consequence Score。`contribution` はフィールドとして残し、中身を Score に置き換える（保存互換のため）
2. UI: Cause Receipt、「あなたの成果が使われた」通知、reach の表示
3. 旧保存データの移行: 既存の道路・建物は作者不明の `civilization` を作者とし、Score には数えない
4. Solana: CauseLedger PDA（初期化・追記・commit）、ER 経由の追記、Season の finalize と claim を新しい台帳につなぐ
5. 検証スクリプト: 台帳の JSON から head と claim_root を再計算する
6. §7 のデモが通しで動くこと＋動画

### Should
- 要請の自動生成（§4）、`consequence` レンズ、台帳から描く年代記、タラのテンプレート発話、`ENABLED_BY_TECH`・`FED_BY` エッジ

### Won't（今回はやらない）
- 土地の収量差、戦闘・外交、LLM による行動の決定、実資金、公開サーバー、ウォレット署名（セッションキーは stretch）

### 目安の日程
| 期間 | 内容 |
|---|---|
| 9/25-9/30 | core の台帳・エッジ・Score＋テスト、保存データの移行 |
| 10/1-10/5 | Receipt UI・通知、CauseLedger PDA＋ER の追記 |
| 10/6-10/10 | Season の精算接続、検証スクリプト、Should 項目 |
| 10/11-10/15 | 外部の人による試遊（Retell Test・Stranger Test）、デモ動画、提出資料 |

---

## 9. 審査軸との対応

| Colosseum の審査軸 | V3 がどう応えるか |
|---|---|
| Functionality | 動くリアルタイムの共有マップ＋ER への追記＋base での精算を、localnet と動画で示す |
| Novelty | 「誰が使ったか」で貢献を証明する仕組みは、既存の fully-onchain ゲームにも wagering 系にもない |
| Potential impact | 協力型ゲーム全般（共同建設・MMO 経済・DAO 的な共同作業）の貢献評価と精算に使える汎用の仕組み |
| UX | チェーンを意識させない。プレイヤーが見るのは「あなたの道を ○○ が使った」という体験だけ |
| Open-source／composability | CauseLedger PDA と検証スクリプトを、他のゲームが組み込める形で公開する |
| Business plan | シーズン参加費と、見た目だけの記念品の手数料。pay-to-win は禁止（Constitution §8） |

| Constitution | V3 での実現 |
|---|---|
| Pillar 2: 個人の行動と共有の結果 | すべてのエッジが「後の誰かの選択肢を変えた」記録そのもの |
| Pillar 3: Handoff | 固定シーンではなく「使われた Artifact」として、地図の上で自然に発生する |
| Pillar 6: 因果が読めること | Receipt の ENABLED BY／CHANGED／COST／ENABLES |
| Pillar 8: pay-to-win にしない | Score は「他者に使われた数」だけで決まり、支払いや繰り返しでは増えない |
| Rule 14: 繰り返しや支払いは貢献にならない | 組み合わせごとの上限・逓減、自分で使った分は 0 |

---

## 10. Constitution の非交渉ルールとの照合

| # | ルール | V3 |
|---|---|---|
| 1 | 人間の文明はひとつ | ✅ 変更なし |
| 2 | 中心は Handoff | ✅ Artifact → Edge として復活 |
| 3 | 核となる行動は文脈を消費し、下流を生む | ✅ Edge がその定義そのもの |
| 4 | 匿名のバーにしない | ✅ contribution スカラーを廃止し、reach を名前付きで見せる |
| 5 | 共通の目標でも犠牲と対立がある | ✅ 予約による材料の奪い合い（§4） |
| 6 | AI は表現、決定論のルールが効果 | ✅ タラは読むだけ |
| 7 | AI は資金・勝敗・配分を決めない | ✅ 配分は事前に確定した規則と台帳で決まる |
| 8 | Solana に記録するのは結果であり、足跡ではない | ✅ エッジのハッシュと精算だけ |
| 9 | 重要な状態変化は因果の連鎖で説明できる | ✅ |
| 10 | 金で買えるのは参加と表現だけ | ✅ |
| 11 | 通常の行動に課金しない | ✅ ER の手数料はサーバーが負担する |
| 12 | 1人の市民が文明を壊せない | ✅ 既存の検証処理を維持 |
| 13 | 非同期で成立する | ✅ Artifact は作者がオフラインでも使われる |
| 14 | 参加しただけでは Active Citizen にならない | ✅ reach ≥ 1 を Active Citizen の条件にする |
| 15 | 勝っても負けても歴史が残る | ⚠️ 敗北の条件は未定義。シーズン目標の未達を「敗北」として年代記に残す（Should） |
| 16 | 90秒以内に役に立てる | ⚠️ 要請の自動生成（Should）で担保する。Must だけでは未検証 |
| 17 | Purse を隠しても面白い | ✅ 体験の中心は「使われた」通知で、金額ではない |
| 18 | 広く浅いより深い連鎖ひとつ | ✅ ①②を凍結 |

---

## 11. 未決事項（実装前に決める）
1. `TRAVELED_ON` の粒度: 道路1区間ごとにエッジを作るとエッジが多すぎる → **経路1回につき、道路の作者ごとに1本**を提案
2. ER への追記頻度: エッジごとか、N 件ずつまとめるか → **2秒ごとにまとめる**を提案
3. 同一人物の判定: ローカル試作では token の発行元を使う。デモの Sybil シーンは「同じブラウザのプロファイルから作った2人目」で見せる
4. シーズンの長さ: デモは 30 分（現行の `season.durationMs`）、試遊は 1〜3 日
