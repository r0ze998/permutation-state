# 3分デモ動画 — 台本（ショットリスト・ナレーション・収録手順）

映すのは、実際に動いているものだけです。ゲームの操作場面はローカルの MagicBlock スタックで撮ります（ティックを速く回せて、公開 RPC の遅延がないため）。「本番のチェーンでも動く」ことは、devnet で完走したシーズン（[ログ抜粋](demo/devnet-season-1790312639004.txt)）とエクスプローラ、devnet に対する検証で見せます。

- 長さ：3:00（ナレーションは英語、約330語。ゆっくり読んで約2分30秒、残りは画面の間）
- 画面：ブラウザ 1400×860 前後、ターミナルは文字を大きめ（18pt 以上）
- 話の軸：**人間と AI が同じ国を一緒に治め、その結果をチェーンが保証する**

## 登場人物

| 名前 | 役 | 国と役職 |
|---|---|---|
| Player 1 | 人間（収録者） | アステルの将軍・内政官 |
| Hypatia | 外部の AI エージェント（ルール型）。x402 で参加 | アステルの科学官・外交官 |
| 各国の AI 国民 | ゲートウェイが登録した AI | 各国1人 |

ゲートウェイを `--ai 1` で起動し、Hypatia を `--civ 0` で参加させると、上の顔ぶれになります（2026-09-25 のリハーサルで確認済み）。

## 収録前の準備（約15分）

リポジトリのルートから、ターミナルのタブを分けて実行します。

**1. ローカルのチェーン（操作場面用）**

```bash
(cd permutation-gateway && node scripts/local-stack.mjs)
```

```bash
(cd permutation-gateway && node src/server.mjs --state demo.json --tick-seconds 30 --ai 1 --wait-external 1)
```

```bash
(cd permutation-server && cargo run --release --bin play -- --chain http://127.0.0.1:4191)
```

ゲートウェイは Hypatia の参加を待ちます。エージェントはシーン2で入れるので、**まだ起動しないでください**。

**2. devnet のシーズンの記録（シーン6・7用）**

完走済みの devnet のシーズンを、2つ目のゲートウェイ（ポート 4192）から読み出します。新しいシーズンは作りません。

```bash
(cd permutation-gateway && node src/server.mjs --cluster devnet --base https://api.devnet.solana.com --er https://devnet-as.magicblock.app --er-validator MAS1Dt9qreoRMQ14YQuhg8UTZMMzDdKhmkZMECCzk57 --state devnet-5.json --port 4192)
```

```bash
(cd permutation-gateway && node scripts/rpc-proxy.mjs --port 18999 --target https://api.devnet.solana.com)
```

```bash
(cd permutation-gateway && node scripts/rpc-proxy.mjs --port 17999 --target https://devnet-as.magicblock.app)
```

`devnet-5.json` は `permutation-gateway/.local/` にある、このシーズンの状態ファイルです（Git の管理外）。

**3. ブラウザのタブ（左から順に）**

1. A：<http://127.0.0.1:4185/>（Player 1 を引き継ぐ。まだ押さない）
2. B：<http://127.0.0.1:4185/spectate.html>（観戦）
3. C：[プログラム（devnet）](https://explorer.solana.com/address/J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n?cluster=devnet)
4. D：[シーズンのアカウント（devnet）](https://explorer.solana.com/address/J8DBWSUyivswpooBirhcuiAP6NaFzBSwzcM3yLNt6K5T?cluster=devnet)
5. E：[x402 の支払い取引（devnet）](https://explorer.solana.com/tx/5F73ziXfjUNUAmBwUhtoJaXm3khpVfvvfb4apqLZLzCNYGZu35W7Fux1bRXXKcJwU25MTAder3U6p5kW1KyY4ZRh?cluster=devnet)

**4. ターミナルに打っておくコマンド（Enter はまだ押さない）**

```bash
curl -s -i -X POST http://127.0.0.1:4191/x402/join | head -20
```

```bash
(cd permutation-gateway && node agents/rule-agent.mjs --name Hypatia --civ 0 --stand Science,Diplomat --server http://127.0.0.1:4185 --gateway http://127.0.0.1:4191)
```

```bash
(cd permutation-server && ./target/release/verify --gateway http://127.0.0.1:4192 --base http://127.0.0.1:18999 --er http://127.0.0.1:17999)
```

```bash
less demo/devnet-season-1790312639004.txt
```

## ショットリスト

| # | 時間 | 画面と操作 | ナレーション（英語） |
|---|---|---|---|
| 1 | 0:00–0:15 | B（観戦）：6つの国の地図をゆっくり動かす。右上の6つの国の丸いアイコン | "This is PERMUTATION STATE: six nations in one shared world, running on Solana. People and AI agents join a nation as citizens — with exactly the same rights." |
| 2 | 0:15–0:40 | ターミナル：`curl … /x402/join` で `402 Payment Required` と支払い条件。続けてエージェントのコマンドを実行し、「paid 10 USDC over x402 → member 7 of Aster」。ゲートウェイのログに registration closes → genesis → seated → first election → delegated が流れる（待ち時間は編集で詰める） | "An AI agent joins the way a web client pays: HTTP 402. Its payment is the program's own Register instruction — the entry fee goes straight into a vault the program owns. Then genesis runs on chain, every citizen is seated, and the first election is held on chain." |
| 3 | 0:40–1:15 | A：Player 1 を引き継ぐ →「国の広場」を開く。将軍・内政官＝Player 1（あなた）、科学官・外交官＝Hypatia（AI）。献策の一覧で「内政官へ・Hypatia」の献策を「採用する」→ 下の「確定する」→ 通知「献策1件の採用を確定」。上部のチェーン表示（T… 封印 ✓ MagicBlock ER）を指し、`（バッククォート）キーでチェーンの詳細を開く | "I'm the general and steward of Aster. The AI, Hypatia, won the science and diplomat offices. It can't command my armies — but it can propose. I adopt its proposal, and we'll share the credit. My orders are sealed with my reasoning, signed with my own session key, and checked by the program on MagicBlock's Ephemeral Rollup." |
| 4 | 1:15–1:35 | A：「外交」を開き、「宣戦には、外交官とは別の人の将軍か内政官の同意が必要」の一文と「⚖ 宣戦に同意」ボタン。続けて「国の広場」の「リコール」ボタン | "Power is checked, on chain. A diplomat can't start a war alone — a second officer must consent. And any majority of citizens can recall an officer, human or AI. Every vote is a transaction." |
| 5 | 1:35–2:00 | A：「時代」の表（4つの道×5段階）→「功績」の内訳（道ごとの功績。献策の採用による分は、その命令の成果が出たティックから入る）→ 上部の賞金プール。B（観戦）の「公開された判断」に「✓ ブラウザで検証済み」 | "Nations advance on four paths — hegemony, prosperity, science and concord — and enter new eras. The prize pool is split among nations by what they achieved, and inside each nation by each citizen's merit. Every officer's reasoning is revealed after the tick, and your browser checks it against what was committed." |
| 6 | 2:00–2:30 | C → D → E（エクスプローラ）：devnet のプログラム、シーズンのアカウント、x402 の支払い。続けてターミナルで `less demo/devnet-season-…txt`：「x402: Hypatia joined」「tick 179 resolved on ER」「season finalized on base」「claimed the prize」「vault 135.67 → 0.00; conserved: true」 | "And this isn't a local toy. On Solana devnet, a full season ran on MagicBlock's rollup: an agent joined over x402, played all 180 ticks, and when the world came back to Solana the program paid every citizen — the agent claimed its own prize — and the vault ended at exactly zero." |
| 7 | 2:30–3:00 | ターミナル：`verify`（devnet のログに対して）→ ✓ が並び「VERIFIED」（約35秒を早送り）。最後に README の URL をテロップで | "You don't have to trust us. Before any tick resolves, its whole input is published on chain. Anyone can rebuild the season from the chain's own logs — every root, every election, every payout. Nobody, not even us, can steer it. PERMUTATION STATE: a society of people and AI, on Solana." |

### ナレーション（通し）

録音用に、上のナレーションをつなげたものです（約330語）。

> This is PERMUTATION STATE: six nations in one shared world, running on Solana. People and AI agents join a nation as citizens — with exactly the same rights.
>
> An AI agent joins the way a web client pays: HTTP 402. Its payment is the program's own Register instruction — the entry fee goes straight into a vault the program owns. Then genesis runs on chain, every citizen is seated, and the first election is held on chain.
>
> I'm the general and steward of Aster. The AI, Hypatia, won the science and diplomat offices. It can't command my armies — but it can propose. I adopt its proposal, and we'll share the credit. My orders are sealed with my reasoning, signed with my own session key, and checked by the program on MagicBlock's Ephemeral Rollup.
>
> Power is checked, on chain. A diplomat can't start a war alone — a second officer must consent. And any majority of citizens can recall an officer, human or AI. Every vote is a transaction.
>
> Nations advance on four paths — hegemony, prosperity, science and concord — and enter new eras. The prize pool is split among nations by what they achieved, and inside each nation by each citizen's merit. Every officer's reasoning is revealed after the tick, and your browser checks it against what was committed.
>
> And this isn't a local toy. On Solana devnet, a full season ran on MagicBlock's rollup: an agent joined over x402, played all 180 ticks, and when the world came back to Solana the program paid every citizen — the agent claimed its own prize — and the vault ended at exactly zero.
>
> You don't have to trust us. Before any tick resolves, its whole input is published on chain. Anyone can rebuild the season from the chain's own logs — every root, every election, every payout. Nobody, not even us, can steer it. PERMUTATION STATE: a society of people and AI, on Solana.

## 編集のメモ

- シーン2：エージェントの参加から ER への移行までは実時間で約30秒です。ログが流れる様子を2〜3秒に早送りします。
- シーン3：献策が画面に出るのは、エージェントが参加してから1〜2ティック後（30〜60秒）です。待つ部分は切ります。1回目のティックで献策が出ていなければ、次のティックを待ってください。
- シーン7：verify は実時間で約35秒です。✓ の行が出るところだけ残して早送りします。
- テロップ（任意）：各シーンの左上に英語で短く。例：「An AI citizen joins over x402」「Human general adopts the AI's proposal」「On devnet: settled, claimed, verified」。

## 収録前のチェックリスト

- [ ] ローカルのゲートウェイが「registering」で待っている（`curl -s http://127.0.0.1:4191/season` の `phase`）
- [ ] 4192 のゲートウェイが devnet のシーズン `1790312639004` を返す（`curl -s http://127.0.0.1:4192/season`）
- [ ] 通知・メール・Slack などの通知を切った
- [ ] ブラウザの拡張機能やブックマークバーを隠した
- [ ] 一度通しでリハーサルし、シーン3の献策が出るまでの時間を把握した

## うまくいかないとき

| 症状 | 対処 |
|---|---|
| Hypatia が科学官にならない | ゲートウェイを `--ai 1` で起動したか、エージェントを `--civ 0` で入れたかを確認。新しい `--state` 名でやり直す |
| 献策が出てこない | エージェントのログに「proposed to the Steward/General」が出るまで1〜2ティック待つ |
| verify が 429 で遅い | devnet の公開 RPC の制限です。中継が自動で再試行するので待つ。撮り直しより早送りで対応 |
| ローカルのスタックが起動しない | 前回のプロセスが残っていないか確認（ポート 18899・17799・4185・4191） |

## 映さないもの（聞かれたときの答え）

- **霧は暗号的には強制されていません。** ER 上のアカウントは読めます。強制は PER（TEE）で行う予定です。
- **命令は凍結まで平文です。** 締切直前の後出しは、命令の封印（ロードマップ）で防ぐ予定です。
- **乱数：** 入力が凍結される時点のワールドのルート・スロット・時刻から作っています。MagicBlock VRF に置き換える予定です。
- **計算量：** 6か国のティックは、devnet の最新シーズンで平均約80万 CU、最大約104万 CU で、180ティックすべてが1取引で解決しました（上限は140万 CU）。上限を超えるティックが出た場合は、フェーズの区切りで自動的に分割して解決します。
- **devnet での制約：** MagicBlock の committor は大きすぎる取引を落とすため、世界データを 4KB×20 に分け、ベース層への保存と返却を小さな単位で送っています（[DESIGN.md](permutation-chain/DESIGN.md)）。
- **お金：** USDC はテスト用で、価値はありません。mainnet には出していません。
- **LLM エージェント：** 実キーで1ティック、モデルの判断をチェーンに提出するところまで確認済みです（その後、試験用アカウントの API 残高が尽きたため中断）。動画ではルール型のエージェントを使います。
