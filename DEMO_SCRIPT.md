# 3-minute demo — shot list and narration

収録の手順（日本語）と、動画のナレーション（英語）です。映すのはすべて、いま実際に動くものだけです。PER による霧の強制や devnet は、まだ映しません。

## 収録前の準備（約10分）

ターミナルはそれぞれ別のタブで開き、リポジトリのルートから実行します。

```bash
(cd permutation-gateway && node scripts/local-stack.mjs)
```

```bash
(cd permutation-gateway && node src/server.mjs --new-season --open-seats 1 --port 4191 --state demo.json --tick-seconds 20)
```

この時点ではまだエージェントを入れないでください。シーン2で入れます。

```bash
(cd permutation-server && cargo run --release --bin play -- --port 4186 --chain http://127.0.0.1:4191)
```

play サーバーは、全席がそろってシーズンが始まるまで「waiting for the on-chain world」と表示して待ちます。これで正常です。

- ブラウザは2枚使います。A は <http://127.0.0.1:4186/>（プレイヤー、アステルの席を取る）、B は <http://127.0.0.1:4186/spectate.html>（観戦）です。
- シーズン終了のシーン（シーン6）は、180ティックを走り切った別のシーズンを使います。事前に録画しておくか、同じ手順で `verify` を実行します。

## ショットリスト

| 時間 | 画面 | ナレーション（英語） |
|---|---|---|
| 0:00–0:15 | 観戦画面 B：地図と、人間・ボット・エージェントのラベルが付いた文明一覧 | "PERMUTATION STATE is Civilization with one save for everyone. Humans and AI agents lead rival civilizations in one shared world, and the whole game runs on Solana." |
| 0:15–0:45 | ターミナル：`curl -i -X POST http://127.0.0.1:4191/x402/join` で 402 と PaymentRequirements を見せる。続けて `node agents/rule-agent.mjs --name Gaia --server http://127.0.0.1:4186 --gateway http://127.0.0.1:4191` で「paid 10 USDC over x402 → civ 5」を見せ、観戦画面に Gaia が現れる | "An agent joins the way a web client pays: HTTP 402. The payment is the program's own JoinSeason, so exactly the entry fee goes into a vault the program owns. The season starts, genesis runs on chain, and the world moves to a MagicBlock Ephemeral Rollup." |
| 0:45–1:25 | ブラウザ A：席を取り、研究を選んで都市の生産を決め、判断メモを書いて確定→「次のティックへ」。エージェントのターミナルに「tick N: … orders」。観戦画面のチェーン欄で、ER の取引、CU、前後のルートが更新される | "Every civilization gets the same order budget and submits one batch per tick. My orders and the agent's orders are each signed with their own session key and checked by the program. Once everyone has submitted, the tick resolves on the rollup: about 400 thousand compute units, with the state root logged." |
| 1:25–1:50 | 観戦画面：Gaia の行をクリックしてエージェントの霧の視界に切り替え、もう一度クリックして全体表示に戻す | "Everyone decides from the same fog of war — this is exactly what the agent sees. Bots, agents and humans all read this same view." |
| 1:50–2:15 | 観戦画面の「公開された判断」：Gaia と各ボットの理由に「✓ ブラウザで検証済み」 | "Each batch also commits to a hash of the observation it was made from and the reasoning behind it. The next tick reveals it, and your browser checks the hash. Agents can't rewrite their story after the outcome." |
| 2:15–2:40 | 終わったシーズン：ゲートウェイのログに「season finalized on base」、`/season` の支払い一覧、Claim の取引 | "When the season ends, the world returns to Solana, the program computes payouts from the final world — conquest, science and concord — and each winner claims their own USDC." |
| 2:40–3:00 | ターミナル：`cargo run --release --bin verify -- --gateway … --base … --er …` で「✓ … VERIFIED」 | "And you don't have to trust us: the verifier rebuilds the season from the chain's own logs and matches every root. Nobody — not even us — can steer it." |

## 映さないもの（聞かれたときの答え）

- **霧は暗号的には強制されていません**：ER 上のアカウントは読めます。強制するのは PER（TEE）で、次の段階です。
- **乱数**：いまはスロットハッシュを使っています。MagicBlock VRF に置き換える予定です。
- **ネットワーク**：ここまではすべてローカルの MagicBlock スタックと、テスト用 USDC です。
