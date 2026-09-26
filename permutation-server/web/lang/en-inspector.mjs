// English for the inspector group of the web client (lang.mjs merges every group).
// Keys are the Japanese exactly as marked in code (L`…` with values as {0},
// {1}…; data-i18n text with child elements as {0}, {1}…). Terms: GLOSSARY.md.
// A value may be a function of the values returning the template (plurals):
//   'メンバー {0}人': n => plural(n, '{0} member', '{0} members'),
// Files: inspector/*.mjs, map.mjs, selection.mjs.
import { plural } from './helpers.mjs';

export default {
  // ---------------------------------------------------------------- shared by several panels
  '兵{0}': '{0} troops',
  '兵 {0}': '{0} troops',
  '枠1': '1 slot',
  '{0}の{1}': "{0}'s {1}", // a faction's unit: アステルの槍兵
  '都市国家 {0}': 'City-state {0}',
  '都市国家{0}': 'City-state {0}',
  '{0}金': '{0} gold',
  'あり': 'Yes',

  // ---------------------------------------------------------------- inspector/index.mjs (nothing selected)
  'YOUR TURN · このティック': 'YOUR TURN · This tick',
  '・': ', ', // list separator (your offices)
  'あなたは{0}': 'You are {0}',
  'あなたはメンバー（役職なし）': 'You are a member (no office)',
  '都市・部隊・土地をクリックすると、ここに詳細と「できること」が出ます。できないことには理由が表示されます。':
    "Click a city, unit or tile to see its details and “what you can do” here. Anything you can't do shows the reason.",
  '{0}担当の役職の命令は封印して送り、担当外の命令は献策になります。':
    '{0} Orders for your offices are sealed and sent; orders outside your offices become proposals.',
  '{0}あなたの命令は担当の役職者への献策になります。採用されると功績を半分ずつ分けます。':
    '{0} Your orders become proposals to the officer in charge. If one is adopted, you and the officer split the merit equally.',
  '勢力の広場を開く（選挙・献策・リコール）': 'Open Faction Plaza (elections, proposals, recall)',
  '首都 {0} を見る': 'View the capital, {0}',

  // ---------------------------------------------------------------- inspector/tile.mjs
  '{0}の領土です。': "{0}'s territory.",
  'どの勢力の領土でもありません。': "No faction's territory.",
  '川 · 金+1': 'River · Gold +1',
  '交易拠点 · 金の市場の手数料1%を得る': 'Trade hub · earns 1% of gold market fees',
  '通行不可': 'Impassable',
  '移動コスト {0}': 'Move cost {0}',
  '守備側の被害 −20%': 'Defender damage −20%',
  'この土地の部隊': 'Units on this tile',
  '蛮族の{0}': 'Barbarian {0}',
  '非戦闘': 'Civilian',
  '土地は都市の領土に入ると、その都市の人口に応じて自動で使われます（産出の高い順）。':
    "Once a tile is in a city's territory, the city works it automatically according to its population (highest yields first).",

  // ---------------------------------------------------------------- inspector/city.mjs: my city
  'CAPITAL · 首都': 'CAPITAL',
  'CITY · 都市': 'CITY',
  '成長：{0} / {1}{2}': 'Growth: {0} / {1}{2}',
  '（あと約{0}ティック）': n => plural(n, ' (about {0} tick to go)', ' (about {0} ticks to go)'),
  '（食料が足りず停滞）': ' (stalled: not enough food)',
  '建物なし': 'No buildings',
  'この都市の部隊（もう一度クリックでも選べます）': 'Units in this city (or click the city again)',
  '方針（土地の割り当て）': 'Focus (tile assignment)',
  ' · 命令あり': ' · Order drafted',
  '生産予定': 'Production queue',
  '· 命令あり（確定で置き換わります）': '· Order drafted (replaces the queue on commit)',
  '蓄積 {0}{1}': 'Stored {0}{1}',
  ' · 毎ティック +{0}': ' · +{0} per tick',
  '予定をクリア（枠1）': 'Clear queue (1 slot)',
  '{0}金で生産を購入': 'Buy production ({0} gold)',
  '生産予定がありません。生産は都市に蓄積されるので無駄にはなりませんが、下から選んでください。':
    'Nothing in production. Production is stored in the city, so none is wasted, but pick something below.',
  '追加できるもの（最大{0}件・枠1）': n => plural(n, 'Add to the queue (up to {0} item · 1 slot)', 'Add to the queue (up to {0} items · 1 slot)'),
  '選択肢を計算しています…': 'Calculating options…',
  '{0}の部隊（兵5）': '{0} unit (5 troops)',
  '新しい都市を建てる（人口−1）': 'Founds a new city (Pop −1)',
  '偵察用': 'For scouting',
  ' · 約{0}ティック': n => plural(n, ' · ~{0} tick', ' · ~{0} ticks'),
  '予定は{0}件までです': n => plural(n, 'The queue holds at most {0} item', 'The queue holds at most {0} items'),
  // standing orders of a city
  '継続命令 · 毎ティック自動': 'Standing orders · automatic every tick',
  ' · 下書きあり': ' · Draft pending',
  '生産の繰り返し': 'Repeat production',
  '自動購入': 'Auto-buy',
  '繰り返し：予定が空になったら最後の部隊をもう一度作ります。自動購入：毎ティック指定額まで金で生産を進めます（手動の購入をしたティックは除く）。':
    'Repeat: when the queue runs empty, the last unit is built again. Auto-buy: every tick, gold up to the set amount speeds up production (except in a tick when you bought manually).',

  // ---------------------------------------------------------------- inspector/city.mjs: foreign city, city-state
  'FREE CITY · 自由都市': 'FREE CITY',
  'どの勢力にも属さない自由都市です。攻撃は侵略扱いになります。': 'A free city that belongs to no faction. Attacking it counts as aggression.',
  '{0}の首都です。関係：{1}': 'The capital of {0}. Relation: {1}',
  '{0}の都市です。関係：{1}': 'A city of {0}. Relation: {1}',
  'スターゲート {0}/3 段階。この都市を占領すると、完成した段階はすべて失われます。':
    'Star Gate: stage {0}/3. If this city is captured, every completed stage is lost.',
  '近くのあなたの部隊': 'Your units nearby',
  '選択して攻撃の予測を見る': 'Select to see the attack forecast',
  '{0}との外交を開く': 'Open diplomacy with {0}',
  'CITY-STATE · 都市国家 · {0}, {1}': 'CITY-STATE · {0}, {1}',
  '{0}の都市国家。{1}。': '{0} city-state. {1}.',
  'あなたの影響力': 'Your influence',
  '最多': 'Highest',
  '宗主になる条件：影響力{0}以上で最多（{1}ティックごとに見直し、全員の影響力が半分に）':
    "To become suzerain: the most influence, and at least {0} (reviewed every {1} ticks, when everyone's influence halves)",
  '使節を送る（枠1・所持影響力 {0}）': 'Send envoys (1 slot · you have {0} influence)',
  '影響力が足りません': 'Not enough influence',
  '影響力 {0}': 'Inf. {0}', // envoy buttons (three in a row)
  '都市国家への攻撃は侵略扱いです：すべての都市国家への影響力を失います（協調の道の節目に響きます）。':
    'Attacking a city-state counts as aggression: you lose your influence with every city-state (this hurts your Concord milestones).',

  // ---------------------------------------------------------------- inspector/unit.mjs: my unit
  '都市を建てられる場所（都市から3マス以上、他勢力の領土・保護区域の外）へ移動して建設します。':
    "Move to a site where a city can be built (3+ tiles from any city, outside other factions' territory and protected zones), then found it.",
  '森や丘陵でも1ずつ進める偵察役です。戦闘はできません。': 'A scouting unit: forest and hills cost it only 1 move. It cannot fight.',
  '移動先の土地をクリックかダブルクリック。攻撃できる相手は赤い枠で示されます。':
    'Click or double-click a tile to move there. Targets you can attack are outlined in red.',
  '待機中': 'Idle',
  '移動中 · 残り{0}マス': n => plural(n, 'Moving · {0} tile left', 'Moving · {0} tiles left'),
  '命令あり：{0}': 'Order: {0}',
  '行動を計算しています…': 'Calculating actions…',
  'ここに都市を建てる': 'Found a city here',
  'ここに都市を建てる（枠1）': 'Found a city here (1 slot)',
  '建設すると開拓者は消え、半径1の土地が領土になります。': 'Founding uses up the Settler; the tiles within radius 1 become your territory.',
  '攻撃できる相手': 'Attack targets',
  '射程内に相手はいません。戦争中の相手・蛮族・都市国家を攻撃できます（弓兵・弩兵は2マス、ほかは隣接）。':
    'No targets in range. You can attack factions at war with you, barbarians and city-states (Archers and Crossbowmen from 2 tiles, others when adjacent).',
  '移動': 'Move',
  'このティック {0}マス': n => plural(n, 'This tick: {0} tile', 'This tick: {0} tiles'),
  '2〜3ティック {0}マス': n => plural(n, '2–3 ticks: {0} tile', '2–3 ticks: {0} tiles'),
  '他勢力の首都の保護区域（半径{0}）': "Other capitals' protected zones (radius {0})",
  '行き先をダブルクリック（または選んで Enter）で下書きに入ります。遠い土地はカーソルを当てると到着ティックが出ます。移動は締切で全員同時に解決されます。':
    'Double-click a destination (or select it and press Enter) to add the move to your draft. Hover over a distant tile to see its arrival tick. Moves resolve for everyone at once at the deadline.',
  // standing orders of a unit
  '自動防衛': 'Auto-defend',
  '半径{0}': n => plural(n, '{0} tile', '{0} tiles'), // auto-defend radius buttons (tight row)
  '撤退': 'Retreat',
  '1倍': '1×',
  '1.5倍': '1.5×',
  '2倍': '2×',
  '継続命令 · 毎ティック自動（設定に枠1・実行は無料）': 'Standing orders · automatic every tick (1 slot to set, free to run)',
  '下書き：{0}': 'Draft: {0}',
  '巡回': 'Patrol',
  '道筋を引き直す': 'Redraw route',
  '地図で地点を選ぶ': 'Pick on map',
  '解除': 'Clear',
  '自動防衛：基点から半径内に入った敵軍のうち最も弱いものを攻撃。撤退：隣の敵の強さが自軍の指定倍を超えたら自分の都市へ1マス下がる。':
    'Auto-defend: attacks the weakest enemy army that comes within the radius of its post. Retreat: steps back 1 tile toward your city when an adjacent enemy is stronger than your army by more than the set multiple.',
  '{0}巡回：最大{1}地点を順に回り続けます。手動の命令を出したティックはそちらが優先されます。':
    '{0} Patrol: keeps visiting up to {1} points in order. In a tick when you give the unit a manual order, that order comes first.',
  // drawing a patrol
  '巡回の道筋': 'Patrol route',
  '地図で回る地点を順にクリックしてください（最大{0}）。最後の地点のあとは最初に戻ります。':
    'Click the points to visit on the map, in order (up to {0}). After the last point the unit returns to the first.',
  '削除': 'Remove',
  'まだ地点がありません': 'No points yet',
  'この道筋で巡回（枠1）': 'Start patrol (1 slot)',
  'やめる': 'Cancel',
  // attack options
  '{0}（都市）': '{0} (city)',
  '非戦闘ユニットを捕獲します': 'Captures the civilian unit',
  '予測：相手 −{0}（残り{1}）・自軍 −{2}': 'Forecast: enemy −{0} ({1} left) · your army −{2}',
  '中立への攻撃は侵略扱い：都市国家への影響力をすべて失います': 'Attacking a neutral counts as aggression: you lose all influence with city-states',
  '乱数±10%・他の勢力も同時に動くため目安です': '±10% random, and other factions move at the same time: an estimate only',

  // ---------------------------------------------------------------- inspector/unit.mjs: a target tile for my unit
  'この土地への移動は下書き済みです（約{0}ティック）。': n => plural(n, 'A move here is in your draft (about {0} tick).', 'A move here is in your draft (about {0} ticks).'),
  '下書きを取り消す': 'Remove from draft',
  'ここへ移動（約{0}ティック・{1}）': n => plural(n, 'Move here (~{0} tick · {1})', 'Move here (~{0} ticks · {1})'),
  '今の移動命令と差し替え': 'replaces the current move',
  'この部隊はここへ移動できません：{0}': "This unit can't move here: {0}",
  'この部隊はここへ移動できません。': "This unit can't move here.",
  '選択中の部隊で、この土地に対してできること。': 'What the selected unit can do on this tile.',
  '{0}に戻る': 'Back to {0}',

  // ---------------------------------------------------------------- map.mjs (canvas labels)
  '世界地図。クリックで選択、ダブルクリックで選択中の部隊を移動、ドラッグで地図を移動、ホイールで拡大縮小':
    'World map. Click to select, double-click to move the selected unit, drag to pan, scroll to zoom.',
  '→ このティックで到着': '→ Arrives this tick',
  '→ {0}ティックで到着': n => plural(n, '→ Arrives in {0} tick', '→ Arrives in {0} ticks'),
  '{0}人': '{0} ppl', // a faction's members on its city banners (a small pill)
  '捕獲': 'Capture',
  '敵 −{0} ／ 自 −{1}': 'Foe −{0} / You −{1}',
  // city-state emblem letters (Scientific, Mercantile, Agrarian)
  '学': 'S',
  '商': 'M',
  '農': 'A',
  '影響 {0}/{1}': 'Inf. {0}/{1}',

  // ---------------------------------------------------------------- selection.mjs (hover label, move reasons, patrol toasts)
  '・{0}': ' · {0}',
  '・川': ' · River',
  ' · {0}の領土': " · {0}'s territory",
  '{0}の首都の保護区域です{1}': "Inside the protected zone of {0}'s capital{1}",
  '（ティック{0}から入れます）': ' (open from tick {0})',
  '（保護が続く間は入れません）': ' (closed while the protection lasts)',
  '途中の道がふさがっているか、12マスより遠い場所です': 'The way is blocked, or it is more than 12 tiles away',
  'そこへは移動できません：{0}': "Can't move there: {0}",
  '地図で巡回する地点を順にクリックしてください（最大{0}）。': 'Click the patrol points on the map in order (up to {0}).',
  '巡回の地点は{0}つまでです。': n => plural(n, 'A patrol has at most {0} point.', 'A patrol has at most {0} points.'),
  '水域や山岳は巡回の地点にできません。': "Water and mountains can't be patrol points.",
};
