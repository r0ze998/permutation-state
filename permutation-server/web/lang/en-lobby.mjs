// English for the lobby group of the web client (lang.mjs merges every group).
// Keys are the Japanese exactly as marked in code (L`…` with values as {0},
// {1}…; data-i18n text with child elements as {0}, {1}…). Terms: GLOSSARY.md.
// A value may be a function of the values returning the template (plurals):
//   'メンバー {0}人': n => plural(n, '{0} member', '{0} members'),
// Files: lobby.mjs, wallet.mjs, session.mjs, connect.mjs, chainio.mjs,
// claim.mjs, api.mjs. (The wallet's session-key text, session.mjs
// sessionText, is never translated: the key is derived from its bytes.)
import { plural } from './helpers.mjs';

export default {
  // ================================================================ lobby.mjs
  // local mode: the member dialog
  'ローカルのシーズン（テスト用USDC）': 'A local season (test USDC)',
  'シーズンは始まっていて、新しく参加することはできません。': 'The season has started; no one new can join.',
  'メンバー {0}人': n => plural(n, '{0} member', '{0} members'),
  '今の1人あたりの見込み {0} USDC': 'projected per member now: {0} USDC',
  '選択中 ✓': 'Selected ✓',
  'えらぶ': 'Choose',
  '名前': 'Name',
  'あなたの名前': 'Your name',
  '立候補する役職（{0}つまで）': 'Offices to stand for (up to {0})',
  '参加費は全員同じ {0} USDC（{1}%が賞金プール、{2}%が運営）。メンバーがいない役職はルールの代行が務めます。':
    'The entry fee is {0} USDC for everyone ({1}% to the prize pool, {2}% to the operator). Offices with no member are run by the rules\' caretaker.',
  '{0}に加わる': 'Join {0}',
  '勢力を選んでください': 'Choose your faction',
  '開幕できませんでした': 'Could not start the season',

  // chain mode: the registration lobby (titles, progress)
  'ウォレットで勢力に加わる': 'Join a faction with your wallet',
  'シーズンを準備しています': 'Preparing the season',
  'このシーズンに入る': 'Enter this season',
  '登録しました': 'Registered',
  '登録は締め切られました': 'Registration is closed',
  '残高を確認しています…': 'Checking your balance…',
  '支払いの条件を確認しています…': 'Checking the payment terms…',
  'ウォレットで参加費の支払いを承認してください': 'Approve the entry fee payment in your wallet',
  '送信しています…（確認まで15秒ほどかかることがあります）': 'Sending… (confirmation can take about 15 s)',
  'ウォレットの接続を待っています…': 'Waiting for the wallet to connect…',
  'テスト USDC を受け取っています…': 'Getting test USDC…',
  'ウォレットでメッセージに署名してください（無料・取引ではありません）': 'Sign the message in your wallet (free, not a transaction)',
  'ゲートウェイからシーズンを読み込んでいます…': 'Loading the season from the gateway…',
  // notices
  'テスト USDC {0} を受け取りました（価値のないテスト用トークンです）。': 'Received {0} test USDC (a test token with no value).',
  'このウォレットは、このシーズンのテスト USDC を受け取り済みです。': 'This wallet has already received test USDC for this season.',
  '鍵を読み込みました。': 'Key loaded.',
  '鍵をコピーしました。パスワードマネージャーなど安全な場所に保存してください。': 'Key copied. Keep it somewhere safe, such as a password manager.',
  'コピーできませんでした。下の内容を選んで、安全な場所に保存してください。': 'Could not copy. Select the text below and keep it somewhere safe.',
  '鍵をファイルに保存しました。安全な場所に保管してください。': 'Key saved to a file. Keep it somewhere safe.',
  'シーズン {0} の賞金を受け取りました。': 'Claimed the prize for season {0}.',
  'このウォレットは、シーズン {0} のメンバーではありませんでした。': 'This wallet was not a member of season {0}.',
  // the deadline
  '締切を過ぎました': 'Deadline passed',
  '勢力{0}': 'Faction {0}',
  '締切 <b>{0}</b>（あと <b class="num" data-countdown></b>）に、人数に関わらずシーズンが始まります':
    'The season starts at the deadline, <b>{0}</b> (in <b class="num" data-countdown></b>), however many have joined',
  '必要な人数がそろうとシーズンが始まります（テスト用の設定）': 'The season starts once enough members have joined (a test setting)',
  '勢力は{0}つ。自分の Solana ウォレットで参加費（USDC）を払って、どれかの勢力に加わります。手数料（SOL）は運営が払うので、ウォレットに SOL は要りません。': n => plural(n,
    '{0} faction. Pay the entry fee (USDC) with your own Solana wallet to become a member. The operator pays the fees (SOL), so your wallet needs no SOL.',
    '{0} factions. Pay the entry fee (USDC) with your own Solana wallet to become a member. The operator pays the fees (SOL), so your wallet needs no SOL.'),
  // the key backup
  '鍵のバックアップを読み込む': 'Load a key backup',
  '鍵（16進64文字）またはバックアップの内容': 'Key (64 hex characters) or backup text',
  '読み込む': 'Load',
  'ファイルを選ぶ': 'Choose file',
  '鍵をバックアップ': 'Back up your key',
  'ゲーム内の鍵はこのブラウザに保存されています。別の端末では、同じウォレットで署名すれば同じ鍵が作られます。ウォレットが同じ鍵を作れないときのために、バックアップを保存してください。':
    'Your in-game key is saved in this browser. On another device, signing with the same wallet makes the same key. Save a backup in case your wallet cannot make the same key again.',
  'ゲーム内の鍵はこのブラウザに<b>保存できませんでした</b>（再読み込みすると、もう一度署名が必要です）。別の端末では、同じウォレットで署名すれば同じ鍵が作られます。ウォレットが同じ鍵を作れないときのために、バックアップを保存してください。':
    'Your in-game key <b>could not be saved</b> in this browser (after a reload you will need to sign again). On another device, signing with the same wallet makes the same key. Save a backup in case your wallet cannot make the same key again.',
  '鍵をコピー': 'Copy key',
  'ファイルに保存': 'Save to file',
  // a member (registered)
  '{0}のメンバー（#{1}）': 'Member of {0} (#{1})',
  '登録済み ✓': 'Registered ✓',
  'ゲーム内の鍵 ✓': 'In-game key ✓',
  'このウォレットの署名から作った鍵が、登録されている鍵と一致しません（ウォレットによっては同じ署名を再現できません）。鍵のバックアップを読み込んでください。観戦と賞金の受け取りはこのままできます。':
    "The key made from this wallet's signature doesn't match the registered key (some wallets cannot reproduce the same signature). Load your key backup. You can still spectate and claim prizes.",
  'このブラウザには、このメンバーのゲーム内の鍵がありません。ウォレットでもう一度署名すると、登録時と同じ鍵を作り直します（無料・取引ではありません）。':
    "This browser doesn't have this member's in-game key. Sign again with your wallet to recreate the key you registered (free, not a transaction).",
  '署名して鍵を作り直す': 'Sign to recreate key',
  'このブラウザには、このメンバーのゲーム内の鍵がありません。登録したウォレット（{0}）を接続して署名するか、鍵のバックアップを読み込んでください。':
    "This browser doesn't have this member's in-game key. Connect the registered wallet ({0}) and sign, or load a key backup.",
  '別のウォレットに切り替える': 'Switch to another wallet',
  '{0}。このページは閉じても大丈夫です（同じブラウザか、同じウォレットで戻れます）。': '{0}. You can close this page (come back with the same browser or the same wallet).',
  'シーズンを準備しています…（世界の生成と着任。数分かかることがあります）': 'Preparing the season… (building the world and seating members; this can take a few minutes)',
  '入場しています…': 'Entering…',
  '鍵がなくても、自分の勢力を見ることはできます（命令・投票・会話はできません）。': "Without the key you can still view your faction (but you can't give orders, vote or talk).",
  '鍵なしで見る': 'View without key',
  // the six steps
  'ウォレットを接続': 'Connect a wallet',
  'USDC の残高': 'USDC balance',
  '勢力・名前・立候補する役職': 'Faction, name and offices',
  '参加費と開始': 'Entry fee and start',
  '署名して鍵を作る': 'Sign to create key',
  '参加費を払って参加': 'Pay and join',
  '切り替える': 'Switch',
  '登録済みの方は、登録したウォレットを接続すると自動で見つかります（この端末にそのウォレットがなければ、鍵のバックアップを読み込めます）。':
    "Already registered? Connect your registered wallet and you'll be found automatically (if that wallet isn't on this device, you can load a key backup).",
  'ウォレットを接続すると、残高を確かめます。': 'Connect a wallet to check its balance.',
  'このウォレットに USDC を入金してから「残高を更新」を押してください。': 'Deposit USDC into this wallet, then press “Refresh balance”.',
  'テスト USDC を受け取る（無料）': 'Get test USDC (free)',
  '残高 <b>{0} USDC</b>（必要 {1} USDC）': 'Balance <b>{0} USDC</b> ({1} USDC needed)',
  '残高を更新': 'Refresh balance',
  'テスト USDC は運営のテスト用トークンです（価値はありません）。': "Test USDC is the operator's test token (it has no value).",
  '名前（全員が同じ方法でランダムに選びます）': 'Name (drawn at random, the same way for everyone)',
  '引き直す': 'Redraw',
  '立候補する役職（1〜{0}つ。第1回の選挙は立候補者からランダムに決まります）':
    'Offices to stand for (1–{0}; the first election is drawn at random among the candidates)',
  '参加費 <b>{0} USDC</b>{1}（参加費の{2}%が賞金プール、{3}%が運営）。':
    'Entry fee <b>{0} USDC</b>{1} ({2}% of the entry fee to the prize pool, {3}% to the operator).',
  ' ＋ 勢力の資金への預け入れ {0} USDC': ' + {0} USDC deposit to the treasury',
  '参加費は返金されません': 'The entry fee is not refunded',
  '{0}。いま {1} 人が登録しています。': (deadline, n) => plural(n,
    '{0}. {1} member has registered so far.',
    '{0}. {1} members have registered so far.'),
  '✓ ゲーム内の鍵': '✓ In-game key',
  'ウォレットでメッセージに署名します（<b>無料</b>・取引ではありません）。この署名から、このシーズン専用の「ゲーム内の鍵」をこのブラウザで作ります。鍵は命令・投票・会話の署名に使います。賞金の受け取りやウォレットのトークンの移動はできませんが、役職者になると勢力の資金（市場・契約）は動かせます。':
    "Sign a message in your wallet (<b>free</b>, not a transaction). From this signature, this browser makes an “in-game key” for this season only. The key signs your orders, votes and talk. It cannot claim prizes or move tokens from your wallet, but once you hold an office it can spend your faction's treasury (markets, contracts).",
  '参加費を払って参加（{0} USDC）': 'Pay and join ({0} USDC)',
  'ウォレットに {0} USDC を支払う取引の承認を求めます。手数料（SOL）は運営が払います。{1}':
    'Your wallet will ask you to approve a transaction paying {0} USDC. The operator pays the fees (SOL). {1}',
  '勢力を選んでください。': 'Choose your faction.',
  // registration closed
  '登録は締め切られました。まもなくシーズンが始まります。': 'Registration is closed. The season starts soon.',
  '登録は締め切られました。シーズンを準備しています…': 'Registration is closed. Preparing the season…',
  'このシーズンの登録は締め切られました。': 'Registration for this season is closed.',
  'このウォレット（{0}）は、このシーズンのメンバーではありません。': 'This wallet ({0}) is not a member of this season.',
  'このシーズンのメンバーの方は、登録したウォレットを接続するか、鍵のバックアップを読み込んでください。':
    'If you are a member of this season, connect your registered wallet or load a key backup.',
  // who is in this season (othersText; also in the inspector), the help dialog (describeSeason)
  'この季節のメンバーは{0}人。{1}メンバーのいない役職はルールの代行が務めます。': n => plural(n,
    'This season has {0} member. {1}Offices with no member are run by the caretaker.',
    'This season has {0} members. {1}Offices with no member are run by the caretaker.'),
  'うち{0}人は運営のAIメンバーです（誰かは、住む都市が落ちたときとシーズンの終わりに公開）。': n => plural(n,
    "{0} of them is the operator's AI member (identity revealed when its home city falls and at the season's end). ",
    "{0} of them are the operator's AI members (identities revealed when their home city falls and at the season's end). "),
  'PERMUTATION STATE — ひとつの文明、{0}つの勢力': n => (Number(n) === 6
    ? 'PERMUTATION STATE — One civilization, six factions'
    : 'PERMUTATION STATE — One civilization, {0} factions'),
  'ひとつの文明、{0}つの勢力。そのひとつのメンバーとして、勢力を動かす。': 'One civilization, {0} factions. As a member of one, run your faction.',
  '{0}つの勢力が同じ地図を共有しています。{1}{2}秒ごとの「ティック」で、全ての勢力の命令が同時に解決されます。':
    "{0} factions share one map. {1} Every faction's orders resolve at the same time at each “tick”, every {2} seconds.",
  'このシーズンはオンチェーンです（MagicBlock ER）。時計は止まりません。': 'This season is on chain (MagicBlock ER). The clock never stops.',
  '「確定する」で各役職の命令を封印し、中身を運営のゲートウェイに預けてから、封印（ハッシュ）をこのブラウザのゲーム内の鍵で署名してチェーンに送ります。締切のあと数秒でゲートウェイが公開し、全ての勢力を同時に解決します。':
    "“Commit” seals each office's orders, leaves their contents with the operator's gateway, then signs the seal (a hash) with this browser's in-game key and sends it to the chain. A few seconds after the deadline the gateway reveals them, and every faction resolves at once.",
  '締切の約{0}秒前には下書きを自動で確定します（このタブを表示している間だけ）。「手番を終える」は、まだ何も送っていない役職に空の封印を送ります。':
    'About {0} s before the deadline, drafts are committed automatically (only while this tab is showing). “End turn” sends an empty seal for each office that has sent nothing yet.',
  '投票・立候補・献策・支持・リコール・会話も同じ鍵で署名して送ります（締切後の数秒間に出したものは次のティックに送ります）。手数料は運営が払います。':
    'Votes, candidacies, proposals, support, recalls and talk are signed with the same key too (anything sent in the few seconds after a deadline goes to the next tick). The operator pays the fees.',
  'ウォレットが署名するのは参加費の支払いと賞金の受け取りだけです。': 'Your wallet signs only the entry fee payment and the prize claim.',
  '閉じてはじめる →': 'Close and start →',

  // ================================================================ claim.mjs
  '精算の状況を確認しています…': 'Checking the settlement…',
  'ウォレットで受け取りを承認してください（手数料は運営が払います）': 'Approve the claim in your wallet (the operator pays the fee)',
  'RESULT · あなたの賞金': 'RESULT · Your prize',
  '結果を読み込んでいます…': 'Loading the results…',
  '見込み {0} USDC': 'Projected: {0} USDC',
  '精算待ち（最長約1時間）': 'Awaiting settlement (up to about 1 hour)',
  'シーズンの精算（運営のAIメンバーの公開と配分の確定）が終わると、ここから受け取れます。':
    "Once the season is settled (the operator's AI members revealed and the payouts fixed), you can claim here.",
  'あなたの賞金 {0} USDC': 'Your prize: {0} USDC',
  '賞金 {0} USDC{1}。登録したウォレット（{2}）のトークン口座に届きます。': 'Prize {0} USDC{1}. It goes to a token account of your registered wallet ({2}).',
  ' ＋ 勢力の資金の残りの返還 {0} USDC': ' + {0} USDC returned from the treasury',
  '受け取り済み ✓': 'Claimed ✓',
  'エクスプローラーで見る →': 'View in explorer →',
  '受け取るには、登録したウォレットを接続してください。': 'To claim, connect your registered wallet.',
  '接続中のウォレット（{0}）は登録したものと違います。{1} に切り替えてください。': 'The connected wallet ({0}) is not the registered one. Switch to {1}.',
  '接続を切る': 'Disconnect',
  '受け取る（{0} USDC）': 'Claim ({0} USDC)',
  '賞金を受け取りました（{0}）': 'Prize claimed ({0})',
  // earlier seasons (in the lobby)
  'シーズン {0}': 'Season {0}',
  '金額をゲートウェイから読めませんでした': "Couldn't read the amount from the gateway",
  '前のシーズンの賞金を受け取る': 'Claim prizes from earlier seasons',
  '前のシーズンの賞金を確かめる（{0}シーズン）': n => plural(n,
    'Check earlier seasons for prizes ({0} season)',
    'Check earlier seasons for prizes ({0} seasons)'),
  'このウォレットがそのシーズンのメンバーだったかを、ゲートウェイから読めませんでした。受け取りを試すとわかります（ウォレットの署名が必要です。メンバーでなかったシーズンでは受け取れません）。':
    "The gateway couldn't tell whether this wallet was a member in these seasons. Trying to claim will tell (it needs your wallet's signature; nothing can be claimed for a season you weren't a member of).",

  // ================================================================ connect.mjs (the wallet picker)
  '・': ', ', // list separator (the install links)
  'Solana のウォレットがほかに必要です：{0}。インストール後にページを再読み込みしてください（拡張機能はページを開いたときに読み込まれます）。':
    'You need another Solana wallet: {0}. After installing, reload the page (extensions load when the page opens).',
  'Solana のウォレットが必要です：{0}。インストール後にページを再読み込みしてください（拡張機能はページを開いたときに読み込まれます）。':
    'You need a Solana wallet: {0}. After installing, reload the page (extensions load when the page opens).',
  'このシーズンは devnet（テストネット）です。接続する前に、ウォレットをテストネット/devnet に切り替えてください。':
    'This season runs on devnet (a test network). Before connecting, switch your wallet to testnet/devnet.',
  'ローカルネットです。ふつうのウォレットはこのチェーンに接続できません（ゲートウェイを <code>--dev-wallet</code> 付きで起動すると Dev Wallet が使えます）。':
    "This is a localnet. Ordinary wallets can't connect to this chain (start the gateway with <code>--dev-wallet</code> to use the Dev Wallet).",
  'そのウォレットは見つかりません': 'That wallet was not found',

  // ================================================================ wallet.mjs
  'Phantom：設定 → 開発者設定 →「テストネットモード」をオン（Solana Devnet）': 'Phantom: Settings → Developer Settings → turn on “Testnet Mode” (Solana Devnet)',
  'Solflare：設定 → ネットワーク →「Devnet」に切り替える': 'Solflare: Settings → Network → switch to “Devnet”',
  'Backpack：設定 → Solana → RPC 接続 →「Devnet」に切り替える': 'Backpack: Settings → Solana → RPC Connection → switch to “Devnet”',
  '{0}：ウォレットのネットワークをテストネット/devnet に切り替える': "{0}: switch the wallet's network to testnet/devnet",
  // why a wallet cannot be used (under its name in the picker)
  '接続に対応していません': 'Connecting not supported',
  '取引の署名に対応していません': 'Transaction signing not supported',
  '従来形式（legacy）の取引に対応していません': 'Legacy transactions not supported',
  'メッセージの署名に対応していません': 'Message signing not supported',
  'このネットワーク（{0}）に対応していません': 'This network ({0}) not supported',
  'このウォレット': 'This wallet',
  '{0}は使えません：{1}': '{0} cannot be used: {1}',
  '{0}に、このネットワーク（{1}）で使えるアカウントがありません': '{0} has no account usable on this network ({1})',
  'ウォレットで取り消されました': 'Canceled in the wallet',
  'ウォレットのエラー：{0}': 'Wallet error: {0}',
  'ウォレットが接続されていません': 'No wallet is connected',
  'ウォレットが署名を返しませんでした': 'The wallet returned no signature',
  'ウォレットが署名済みの取引を返しませんでした': 'The wallet returned no signed transaction',

  // ================================================================ session.mjs (UI texts only)
  'https か 127.0.0.1 で開いてください（この接続ではブラウザの暗号機能が使えません）':
    "Open this page over https or at 127.0.0.1 (the browser's cryptography is unavailable on this connection)",
  'このブラウザは Ed25519 署名に対応していません。最新の Chrome・Edge・Firefox・Safari で開いてください。':
    "This browser doesn't support Ed25519 signatures. Open the page in a recent Chrome, Edge, Firefox or Safari.",
  'ウォレットの署名を確認できませんでした': "The wallet's signature could not be verified",
  '鍵のバックアップを読めませんでした（16進64文字の鍵が必要です）': 'Could not read the key backup (a key of 64 hex characters is needed)',
  'この鍵は、このシーズンのどのメンバーの鍵とも一致しません': "This key doesn't match any member's key in this season",

  // ================================================================ chainio.mjs
  'シーズンの口座がプログラムから導いたものと一致しません': "The season's accounts don't match the ones derived from the program",
  'ゲートウェイの場所がわかりません': "The gateway's address is unknown",
  'ウォレットの応答を取引として読めませんでした': "Could not read the wallet's reply as a transaction",
  'ウォレットが取引を書き換えました。署名は送っていません。': 'The wallet altered the transaction. The signature was not sent.',
  'シーズンがまだわかりません': 'The season is not known yet',
  'ゲートウェイのシーズンがゲームサーバーのものと一致しません': "The gateway's season doesn't match the game server's",
  'このウォレットはすでにこのシーズンのメンバーです': 'This wallet is already a member of this season',
  'ゲートウェイが支払い条件を返しませんでした': 'The gateway returned no payment terms',
  '支払いの条件がこのシーズンと合いません（{0}）': "The payment terms don't match this season ({0})",
  '、': ', ', // list separator (the mismatched payment fields)
  '取引の形が想定と違います（{0}）': "The transaction's shape is not as expected ({0})",
  'ゲートウェイの手数料支払い者がシーズンのものと違います': "The gateway's fee payer is not the season's",
  'ゲートウェイのプログラムがシーズンのものと違います': "The gateway's program is not the season's",

  // ================================================================ api.mjs (server errors)
  '命令の枠が足りません（必要{0}・使える枠{1}）': 'Not enough order slots (need {0}, {1} available)',
  '終盤のため凍結中の命令が含まれています': 'Some orders are frozen for the endgame',
  '同じ部隊に2つの命令があります': 'One unit has two orders',
  'このティックは締め切られました。次のティックで出し直してください': 'This tick is closed. Send it again next tick.',
  'サーバーに届きませんでした': 'Could not reach the server',
  'オンチェーンのシーズンでは、この操作はこのブラウザのゲーム内の鍵で署名して送ります':
    "In an on-chain season, this is signed with this browser's in-game key and sent from here",
};
