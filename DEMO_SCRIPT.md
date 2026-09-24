# 3-minute demo — shot list and narration

収録の手順（日本語）と、動画のナレーション（英語）です。映すのは、いまローカルの MagicBlock スタックで実際に動くものだけです。PER による霧の強制や devnet は、まだ映しません。

## 収録前の準備（約10分）

ターミナルはそれぞれ別のタブで開き、リポジトリのルートから実行します。

```bash
(cd permutation-gateway && node scripts/local-stack.mjs)
```

```bash
(cd permutation-gateway && node src/server.mjs --state demo.json --tick-seconds 20 --wait-external 1)
```

ゲートウェイは、人間の国民1人（Player 1）と各国2人の AI 国民を登録し、外からの参加者を1人待ちます。エージェントはシーン2で入れるので、この時点ではまだ入れないでください。

```bash
(cd permutation-server && cargo run --release --bin play -- --chain http://127.0.0.1:4191)
```

- ブラウザは2枚使います。
  - A は <http://127.0.0.1:4185/>（プレイヤー用。Player 1 を引き継ぐ）
  - B は <http://127.0.0.1:4185/spectate.html>（観戦用）
- ブラウザの幅は 1400px 前後にします。
- シーズン終了のシーン（シーン6・7）には、180ティックを走り切った別のシーズンを使います。事前に録画しておくか、同じ手順で `claim-hosted.mjs` と `verify` を実行します。

## ショットリスト

| 時間 | 画面 | ナレーション（英語） |
|---|---|---|
| 0:00–0:15 | ブラウザ A：国民登録の画面。6か国の人数と、1人あたりの見込み | "PERMUTATION STATE is six nations in one shared world, on Solana. People and AI agents join a nation as citizens — with exactly the same rights." |
| 0:15–0:45 | ターミナル：`curl -i -X POST http://127.0.0.1:4191/x402/join` で 402 と支払い条件を見せる。続けて `node agents/rule-agent.mjs --name Hypatia --civ 4 --stand Science,Diplomat --server http://127.0.0.1:4185 --gateway http://127.0.0.1:4191` を実行し、「paid 10 USDC over x402 → member … of Ember」を見せる。ゲートウェイのログに genesis・seated・first election・delegated が流れる | "An agent joins the way a web client pays: HTTP 402. The payment is the program's own Register instruction, so exactly the entry fee goes into a vault the program owns: 80% to the prize pool, 20% to operations. Registration closes, genesis runs on chain, every member is seated, and the first election is held on chain." |
| 0:45–1:20 | ブラウザ A：国の広場。4つの役職と当選者（AI の外交官 Hypatia を含む）、次の選挙までの残り、献策の一覧。AI 国民からの献策を1つ採用し、判断メモを書いて「確定する」。上部のチェーン表示で ER のティックが進む | "Citizens elect a general, a steward, a science officer and a diplomat. Anyone can propose orders to an office; when an officer adopts a proposal, its author shares the credit. My orders are sealed with my reasoning, signed with my session key, and checked by the program on the Ephemeral Rollup." |
| 1:20–1:45 | ブラウザ A：外交の画面で「宣戦には、外交官とは別の人の将軍か内政官の同意が必要」という説明と、将軍・内政官向けの「⚖ 宣戦に同意」ボタン。国の広場でリコールの提起と賛成数 | "Power is checked. War needs a second officer's consent. An officer who stops showing up faces an automatic recall, and any majority can recall one — and every vote is a transaction." |
| 1:45–2:10 | ブラウザ A：時代の表（4つの道×5段階）、自分の功績の内訳、上部の賞金プール。観戦画面 B の「公開された判断」に「✓ ブラウザで検証済み」 | "Nations advance on four paths — hegemony, prosperity, science and concord — and enter new eras. The pool is split among nations by what they achieved, and inside each nation by each citizen's merit. Officers' reasoning is revealed after the tick, and your browser checks it against what they committed to." |
| 2:10–2:35 | 終わったシーズン：ゲートウェイのログに「season finalized on base」、エージェントのログに「claimed the prize」、続けて `claim-hosted.mjs` の各国民の受け取りと「vault … → 0.00; conserved: true」（外部の参加者が先に受け取っていれば金庫は0になる） | "When the season ends, the world returns to Solana, the program computes every citizen's payout from the final world, and each one claims their own USDC — agents included. The vault ends at exactly zero." |
| 2:35–3:00 | ターミナル：`cargo run --release --bin verify -- --gateway … --base … --er …` で「✓ 180 tick records replayed …」「VERIFIED」 | "And you don't have to trust us. Before any tick resolves, its whole input is published on chain. The verifier rebuilds the season from the chain's own logs and matches every root and every payout. Nobody — not even us — can steer it." |

## 映さないもの（聞かれたときの答え）

- **霧は暗号的には強制されていません。** ER 上のアカウントは読めます。強制は PER（TEE）で行う予定で、次の段階です。
- **命令は凍結まで平文です。** 締切直前の後出しは、命令の封印（ロードマップ）で防ぐ予定です。
- **乱数：** 入力が凍結される時点のワールドのルート・スロット・時刻から作っています。MagicBlock VRF に置き換える予定です。
- **計算量：** 6か国のティックは平均84万〜96万 CU、最大135万 CU です（1取引の上限は140万 CU）。それより重いティックは、フェーズの区切りで自動的に分割して解決します。
- **ネットワーク：** ここまではすべてローカルの MagicBlock スタックと、テスト用の USDC です。
