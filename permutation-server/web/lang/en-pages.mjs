// English for the pages group of the web client (lang.mjs merges every group).
// Keys are the Japanese exactly as marked in code (L`…` with values as {0},
// {1}…; data-i18n text with child elements as {0}, {1}…). Terms: GLOSSARY.md.
// A value may be a function of the values returning the template (plurals):
//   '国民 {0}人': n => plural(n, '{0} member', '{0} members'),
// Files: index.html, spectate.html, spectate.mjs, chain.mjs.
import { plural } from './helpers.mjs';

export default {
  // ================================================================ index.html
  'PERMUTATION STATE — 6つの国、ひとつの世界': 'PERMUTATION STATE — 6 nations, one world',
  // top bar and panels
  'あなたの文明の資源': "Your nation's resources",
  'チェーン（` キー）': 'Chain (` key)',
  '時計の停止・再開（このPCだけの試作）': 'Stop or restart the clock (prototype, this PC only)',
  'あなたの文明': 'Your nation',
  'パネル': 'Panels',
  // the nav's labels: one short line under the glyph (keys given by data-i18n="…（ナビ）";
  // the drawers' own titles and table heads keep their own keys)
  '国の広場（ナビ）': 'Plaza',
  '時代（ナビ）': 'Eras',
  '都市・軍（ナビ）': 'Cities',
  '国庫・市場（ナビ）': 'Markets',
  '判断ログ（ナビ）': 'Decisions',
  '首都へ（H）': 'To the capital (H)',
  '全体図。クリックでその場所へ': 'Minimap. Click to go there',
  // the dock
  'このティックの命令': "This tick's orders",
  '確定時にハッシュだけを送り、ティック解決後に公開されます（コミット・リビール）': 'Only its hash is sent on commit; it is revealed after the tick resolves (commit–reveal)',
  '判断メモ': 'Rationale',
  '判断メモ（任意）· 封印して送り、解決後に公開されます': 'Rationale (optional) · sealed now, revealed after resolution',
  '文明': 'Nation', // the ribbon's aria-label; the same key labels the nation picker in drawers/decisions.mjs
  '全員が同じ情報で判断 · USDCはテスト用': 'Everyone decides on the same information · test USDC',
  // the help dialog
  'HOW TO PLAY · 遊び方': 'HOW TO PLAY',
  'ひとつの国の国民として、国を動かす。': 'Run a nation as one of its members.',
  'いくつかの国が同じ地図を共有しています。一定の時間ごとの「ティック」で、全ての国の命令が同時に解決されます。':
    'Several nations share one map. At every “tick”, a fixed interval, the orders of all nations resolve at the same time.',
  '国民になり、役職を選挙で決める。': 'Join a nation and elect its officers.',
  '{0}国民は将軍・内政官・科学官・外交官を選びます（{1}ティックごと、投票は直前の{2}ティック）。1人2役職まで。立候補者がいない役職はルールの代行が務めます。':
    '{0} Members choose a General, Steward, Science Officer and Diplomat (every {1} ticks; voting in the last {2} ticks before). Up to 2 offices per person. An office with no candidate is run by the rules\' caretaker.',
  '役職者は担当の命令を出す。': 'Officers issue the orders of their office.',
  '{0}将軍＝軍と斥候、内政官＝都市と開拓者、科学官＝研究、外交官＝他国・都市国家・交易。国の枠（3＋都市数、最大8）を役職ごとに分け、使わない枠は役職ごとに繰り越せます。':
    '{0} General = armies and Scouts, Steward = cities and Settlers, Science Officer = research, Diplomat = other nations, city-states and trade. The national budget (3 + cities, up to 8 slots) is split between the offices; each office banks the slots it does not use.',
  '国民は献策・支持・投票・リコール。': 'Members propose, support, vote and recall.',
  '{0}担当外の命令は「献策」になり、役職者が採用すると功績を半分ずつ分けます。働かない役職者は過半数でリコールできます。宣戦には外交官と、別の人の将軍か内政官の同意が必要です。':
    '{0} An order outside your offices becomes a “proposal”; if the officer adopts it, you split the merit equally. A majority can recall an officer who does not act. Declaring war needs the Diplomat plus the consent of a General or Steward held by someone else.',
  '情報は全員に公開、命令は封印。': 'Information is public; orders are sealed.',
  '{0}チェーン上の状態は誰でも読めるので、全ての国の都市・部隊・数値が全員に見えます。代わりに命令は締め切りまでハッシュだけを出し、締め切りの後に公開します（誰も相手の命令を見てから動けません）。役職者は人間もAIも判断メモを封印して後で公開します。':
    "{0} Anyone can read the state on chain, so everyone sees every nation's cities, units and numbers. Orders, however, are sent only as a hash until the deadline and revealed after it (no one can move after seeing another nation's orders). Officers, human and AI alike, seal their rationale and reveal it later.",
  '4つの道で時代を進める。': 'Advance the eras along four paths.',
  '{0}覇権・繁栄・科学・協調の節目（各5段階）を達成すると点が入り、2つの道で同じ段階に届くと新しい時代へ。賞金プールは国の点で分け、国の中では20%を活動した国民で均等、残りを功績で分けます。':
    '{0} Reaching milestones in Hegemony, Prosperity, Science and Concord (5 tiers each) earns points; reaching the same tier on two paths opens a new era. The prize pool is split by nation points; within a nation, 20% goes equally to active members and the rest by merit.',
  '操作：クリック＝選択 · ダブルクリック＝選択中の部隊を移動 · ドラッグ＝地図移動 · スクロール＝拡大縮小 · Space＝次の判断 · Ctrl+Enter＝確定 · H＝首都へ · Esc＝閉じる。{0}{1}':
    'Controls: click = select · double-click = move the selected unit · drag = pan the map · scroll = zoom · Space = next decision · Ctrl+Enter = commit · H = to the capital · Esc = close.{0}{1}',
  'ローカルでは時計を止められます。「手番を終える」は自分の手番を終える操作で、人間の国民の全員が終えるとすぐに解決されます。':
    'In local mode you can stop the clock. “End turn” ends your own turn; once every human member has ended theirs, the tick resolves at once.',
  'はじめる（開幕前なら第1回選挙を行う）→': 'Start (first election if not started) →',
  // the registration dialog
  'JOIN A NATION · 国民登録': 'JOIN A NATION',
  'どの国の国民になりますか。': 'Which nation will you join?',
  '{0}。国は{1}つ、人数に上限はありません。人の少ない国ほど1人あたりの取り分は大きくなります。登録はこのブラウザに保存されます。':
    "{0}. There are {1} nations, with no limit on members. The fewer members a nation has, the bigger each one's share. Your registration is saved in this browser.",
  '登録せずに{0}（全ての国を表示）。AI エージェントとして参加する方法は {1} にあります（人間と同じ権利です）。':
    '{0} without registering (all nations shown). To join as an AI agent, see {1} (the same rights as humans).',
  '観戦する': 'Spectate',

  // ================================================================ both pages
  '拡大': 'Zoom in',
  '縮小': 'Zoom out',
  'レンズ': 'Lenses',
  '{0}地形': '{0}Terrain',
  '{0}勢力': '{0}Political',
  '{0}産出': '{0}Yields',
  '{0}軍事': '{0}Military',
  '{0}協調': '{0}Concord',
  'チェーン': 'Chain',
  '世界を読み込んでいます': 'Loading the world',
  'ゲームサーバーに接続しています。': 'Connecting to the game server.',

  // ================================================================ spectate.html
  'PERMUTATION STATE — 観戦': 'PERMUTATION STATE — Spectator',
  '国民として参加する': 'Join as a member',
  'AI エージェント向けの説明': 'Guide for AI agents',
  '文明の一覧': 'Nation list',
  'CIVILIZATIONS · 文明': 'CIVILIZATIONS',
  '行をクリックすると、その国の立場（外交関係・経済・視界の表示）で見られます。情報は全ての国に公開されています。もう一度クリックで全体表示に戻ります。':
    'Click a row to see the world as that nation (its relations, economy and sight). All information is public to every nation. Click again to return to the whole world.',
  'ON CHAIN · 検証': 'ON CHAIN · Verify',
  '出来事と判断': 'Events and decisions',
  '公開された判断': 'Revealed decisions',

  // ================================================================ spectate.mjs
  // loading
  'ゲームサーバーに接続できません。play サーバーを起動してから再読み込みしてください。': "Can't reach the game server. Start the play server, then reload.",
  'シーズンの開始を待っています': 'Waiting for the season to start',
  '登録を受け付けています。締切で、人数に関わらずシーズンが始まります。': 'Registration is open. The season starts at the deadline, however many have joined.',
  'ウォレットで国民として参加する →': 'Join as a member with a wallet →',
  'シーズンを準備しています（世界の生成と着任）…': 'Preparing the season (building the world, seating members)…',
  // top bar
  'ティック {0}': 'Tick {0}',
  '次の解決まで {0}秒': 'Next resolution in {0}s',
  '観戦': 'Spectating',
  '全体表示': 'Whole world',
  '全ての国': 'All nations',
  '<span class="badge fog">国</span><b>{0}</b><span>の立場から見ています（情報は全員に同じです）</span>':
    '<span class="badge fog">Viewing as</span><b>{0}</b><span>(everyone has the same information)</span>',
  // the nations board
  // a count, not a name: its own key, the badge's markup included (talk's 国民{0} names member #{0})
  '<span class="kind">国民{0}</span>': n => plural(n, '<span class="kind">{0} member</span>', '<span class="kind">{0} members</span>'),
  '代行のみ': 'Caretaker only',
  '第{0}時代': 'Era {0}',
  '覇権/繁栄/科学/協調': 'Hegemony/Prosperity/Science/Concord',
  '<tr><th>国</th><th>都市</th><th>人口</th><th>兵</th><th>時代</th><th>節目</th><th>点</th><th>取り分 USDC</th></tr>':
    '<tr><th>Nation</th><th>Cities</th><th>Pop</th><th>Troops</th><th>Era</th><th>Tiers</th><th>Pts</th><th>Share</th></tr>',
  // the chain box
  'ローカルモード（エンジンをこのプロセスで実行中）。<code>--chain</code> 付きで起動すると、MagicBlock ER 上のシーズンを表示します。':
    'Local mode (the engine runs in this process). Start with <code>--chain</code> to show a season on MagicBlock ER.',
  'シーズン': 'Season',
  'いまの層': 'Layer',
  'プログラム': 'Program',
  '直近の解決': 'Last resolution',
  'ティック {0} · {1} CU': 'Tick {0} · {1} CU',
  'まだありません': 'None yet',
  '<dt>取引</dt>': '<dt>Transaction</dt>',
  '前の根': 'Previous root',
  '新しい根': 'New root',
  'このシーズンを手元で再計算して、すべての根を照合します': 'Recomputes this season on your machine and checks every root',
  // the feed
  'まだ出来事はありません。': 'No events yet.',
  '✓ ブラウザで検証済み': '✓ Verified in your browser',
  '✕ 約束と一致しません': '✕ Does not match the commitment',
  '外部エージェント': 'external agent',
  '（理由なし）': '(no rationale)',
  '判断は、解決後の次のティックで公開されます。': 'Decisions are revealed in the tick after they resolve.',

  // ================================================================ chain.mjs
  '参加費 {0} USDC × 国民{1}人の{2}%、と市場の手数料・関税の{3}%': (fee, n) => plural(n,
    '{2}% of entry fees ({0} USDC × {1} member), plus {3}% of market fees and tariffs',
    '{2}% of entry fees ({0} USDC × {1} members), plus {3}% of market fees and tariffs'),
  'チェーンなし': 'No chain',
  'T{0} 封印 ✓': 'T{0} sealed ✓',
  '封印待ち': 'Awaiting seal',
  'ローカルモードです。エンジンはこのプロセスで動いています。': 'Local mode. The engine runs in this process.',
  '--chain で起動すると MagicBlock ER 上のシーズンを表示します': 'Start with --chain to show a season on MagicBlock ER',
  '選挙・解任・時代の記録はまだありません': 'No elections, removals or eras recorded yet',
  '読み込み中…': 'Loading…',
};
