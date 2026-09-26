// Display names, blocked-reason texts and chain error texts, in Japanese and
// English. Keys are the engine's own enum names, as sent by the API.
//
// Every exported table follows the display language (lang.mjs `twin`):
// reading an entry gives the English one while English is shown and the
// Japanese one otherwise, so callers keep writing T.TECH[t], T.PATH_JA[i],
// Object.entries(T.FOCUS). The *_JA names are historical: they follow the
// language too. Functions (blockedText, errorText, chronicleText, …) answer
// in the current language. English terms: lang/GLOSSARY.md.
import { lang, twin } from './lang.mjs';

const en = () => lang() === 'en';
const own = (o, k) => Object.prototype.hasOwnProperty.call(o, k);

export const CIV_COLORS = ['#c1504a', '#2f8f84', '#c28f2c', '#7a5fb0', '#3f78c2', '#b5527f'];
export const CIV_NAMES = twin(
  { Aster: 'アステル', Borealis: 'ボレアリス', Cinder: 'シンダー', Dunmar: 'ダンマール', Ember: 'エンバー', Fjordal: 'フィヨルダル' },
  { Aster: 'Aster', Borealis: 'Borealis', Cinder: 'Cinder', Dunmar: 'Dunmar', Ember: 'Ember', Fjordal: 'Fjordal' },
);

// Offices of a faction (V5 §5), in the engine's order.
export const ROLES = ['General', 'Steward', 'Science', 'Diplomat'];
export const ROLE_JA = twin(
  { General: '将軍', Steward: '内政官', Science: '科学官', Diplomat: '外交官' },
  { General: 'General', Steward: 'Steward', Science: 'Science Officer', Diplomat: 'Diplomat' },
);
export const ROLE_GLYPH = { General: '⚔', Steward: '⌂', Science: '✧', Diplomat: '✉' };
// The four paths (V5 §6), in the engine's order (tiers arrays use it).
export const PATHS = ['Hegemony', 'Prosperity', 'Science', 'Concord'];
export const PATH_JA = twin(['覇権', '繁栄', '科学', '協調'], ['Hegemony', 'Prosperity', 'Science', 'Concord']);
/** Merit buckets (member.merit keys, merit log paths lower-cased). */
export const MERIT_JA = twin(
  { hegemony: '覇権', prosperity: '繁栄', science: '科学', concord: '協調', common: '役職の務め' },
  { hegemony: 'Hegemony', prosperity: 'Prosperity', science: 'Science', concord: 'Concord', common: 'Office duty' },
);
/** Diplomatic proposal kinds (view.proposals[].kind). */
export const PROPOSAL_KIND = twin(
  { Peace: '講和', Nap: '不可侵条約', Alliance: '同盟' },
  { Peace: 'peace', Nap: 'non-aggression pact', Alliance: 'alliance' },
);

export const TERRAIN = twin(
  { Grassland: '草原', Plains: '平原', Forest: '森', Hills: '丘陵', Mountain: '山岳', Water: '水域' },
  { Grassland: 'Grassland', Plains: 'Plains', Forest: 'Forest', Hills: 'Hills', Mountain: 'Mountains', Water: 'Water' },
);
/** Upper-case terrain names for eyebrows (the same in both languages). */
export const TERRAIN_EN = { Grassland: 'GRASSLAND', Plains: 'PLAINS', Forest: 'WOODLAND', Hills: 'HILLS', Mountain: 'MOUNTAIN', Water: 'WATER' };
/** Tile resources and tradeable goods (tile.resource, pools[].good, good.kind). */
export const RESOURCE = twin(
  { Wheat: '小麦', Iron: '鉄', Horses: '馬', Gold: '金', Food: '食料', Production: '生産' },
  { Wheat: 'Wheat', Iron: 'Iron', Horses: 'Horses', Gold: 'Gold', Food: 'Food', Production: 'Production' },
);
export const goodName = g => RESOURCE[g.kind];
export const UNIT = twin(
  { Spearman: '槍兵', Archer: '弓兵', Horseman: '騎兵', Pikeman: '長槍兵', Crossbowman: '弩兵', Knight: '騎士', Scout: '斥候', Settler: '開拓者' },
  { Spearman: 'Spearman', Archer: 'Archer', Horseman: 'Horseman', Pikeman: 'Pikeman', Crossbowman: 'Crossbowman', Knight: 'Knight', Scout: 'Scout', Settler: 'Settler' },
);
export const UNIT_GLYPH = { Spearman: '⟋', Archer: '➶', Horseman: '♞', Pikeman: '⫽', Crossbowman: '⤓', Knight: '♘', Scout: '◎', Settler: '⚑' };
export const BUILDING = twin(
  { Granary: '穀物庫', Workshop: '工房', Temple: '神殿', Market: '市場', Academy: '学術院', Barracks: '兵舎', Walls: '城壁', StarGate1: 'スターゲート I', StarGate2: 'スターゲート II', StarGate3: 'スターゲート III' },
  { Granary: 'Granary', Workshop: 'Workshop', Temple: 'Temple', Market: 'Market', Academy: 'Academy', Barracks: 'Barracks', Walls: 'Walls', StarGate1: 'Star Gate I', StarGate2: 'Star Gate II', StarGate3: 'Star Gate III' },
);
export const BUILDING_GLYPH = { Granary: '❧', Workshop: '⚒', Temple: '✧', Market: '⇄', Academy: '▤', Barracks: '♜', Walls: '▥', StarGate1: '✦', StarGate2: '✦', StarGate3: '✦' };
export const BUILDING_EFFECT = twin({
  Granary: '食料 +2', Workshop: '生産 +2', Temple: '快適度 +2・影響力 +2', Market: '金 +3、都市の金 ×1.2', Academy: '科学 +3',
  Barracks: '部隊の生産コスト ×0.75', Walls: '都市防御への被害 ×0.67', StarGate1: '科学勝利の第1段階', StarGate2: '科学勝利の第2段階', StarGate3: '科学勝利の最終段階',
}, {
  Granary: 'Food +2', Workshop: 'Production +2', Temple: 'Amenities +2 · Influence +2', Market: 'Gold +3, city gold ×1.2', Academy: 'Science +3',
  Barracks: 'Unit production cost ×0.75', Walls: 'Damage to city defense ×0.67', StarGate1: 'Science victory, stage 1', StarGate2: 'Science victory, stage 2', StarGate3: 'Science victory, final stage',
});
export const TECH = twin({
  Agriculture: '農業', BronzeWorking: '青銅器', Archery: '弓術', HorsebackRiding: '騎乗', Masonry: '石工', Mysticism: '神秘主義',
  Writing: '筆記', Currency: '通貨', IronWorking: '製鉄', Mathematics: '数学', Chivalry: '騎士道', Philosophy: '哲学',
  Engineering: '工学', Astronomy: '天文学', Physics: '物理学', CelestialMechanics: '天体力学',
}, {
  Agriculture: 'Agriculture', BronzeWorking: 'Bronze Working', Archery: 'Archery', HorsebackRiding: 'Horseback Riding', Masonry: 'Masonry', Mysticism: 'Mysticism',
  Writing: 'Writing', Currency: 'Currency', IronWorking: 'Iron Working', Mathematics: 'Mathematics', Chivalry: 'Chivalry', Philosophy: 'Philosophy',
  Engineering: 'Engineering', Astronomy: 'Astronomy', Physics: 'Physics', CelestialMechanics: 'Celestial Mechanics',
});
export const TECH_UNLOCK = twin({
  Agriculture: '小麦の食料 +1', BronzeWorking: '兵舎', Archery: '弓兵', HorsebackRiding: '騎兵・馬の産出', Masonry: '城壁', Mysticism: '神殿',
  Writing: '学術院', Currency: '市場・金の市場', IronWorking: '長槍兵・鉄の産出', Mathematics: '弩兵', Chivalry: '騎士', Philosophy: '神殿の影響力 +1',
  Engineering: '城壁の強化', Astronomy: 'スターゲート I', Physics: 'スターゲート II', CelestialMechanics: 'スターゲート III',
}, {
  Agriculture: 'Wheat food +1', BronzeWorking: 'Barracks', Archery: 'Archer', HorsebackRiding: 'Horseman · horses yield', Masonry: 'Walls', Mysticism: 'Temple',
  Writing: 'Academy', Currency: 'Market · gold market', IronWorking: 'Pikeman · iron yield', Mathematics: 'Crossbowman', Chivalry: 'Knight', Philosophy: 'Temple influence +1',
  Engineering: 'Stronger walls', Astronomy: 'Star Gate I', Physics: 'Star Gate II', CelestialMechanics: 'Star Gate III',
});
export const FOCUS = twin(
  { Balanced: '均衡', Food: '食料', Production: '生産', Gold: '金', Science: '科学' },
  { Balanced: 'Balanced', Food: 'Food', Production: 'Production', Gold: 'Gold', Science: 'Science' },
);
export const SPECIALTY = twin(
  { Scientific: '学術', Mercantile: '商業', Agrarian: '農業' },
  { Scientific: 'Scientific', Mercantile: 'Mercantile', Agrarian: 'Agrarian' },
);
export const SPECIALTY_BONUS = twin(
  { Scientific: '宗主に科学 +3/ティック', Mercantile: '宗主に金 +4/ティック', Agrarian: '宗主の首都に食料 +2/ティック' },
  { Scientific: 'Suzerain gets science +3/tick', Mercantile: 'Suzerain gets gold +4/tick', Agrarian: "Suzerain's capital gets food +2/tick" },
);
export const RELATION = twin(
  { self: 'あなた', peace: '平和', war: '戦争', nap: '不可侵', alliance: '同盟' },
  { self: 'You', peace: 'Peace', war: 'War', nap: 'NAP', alliance: 'Alliance' },
);
/** Season phases [name, NAME]; their start ticks come from the server (season.phases). */
export const PHASES = twin(
  [['草創', 'FOUNDING'], ['拡大', 'EXPANSION'], ['競合', 'CONTENTION'], ['危機', 'CRISIS'], ['決着', 'RESOLUTION']],
  [['Founding', 'FOUNDING'], ['Expansion', 'EXPANSION'], ['Contention', 'CONTENTION'], ['Crisis', 'CRISIS'], ['Resolution', 'RESOLUTION']],
);
const PHASE_STARTS = [0, 18, 60, 120, 162]; // only if a view has no season.phases
export const DIPLO_ACTION = twin({
  DeclareWar: '宣戦する', ProposePeace: '講和を申し入れる', ProposeNap: '不可侵条約を申し入れる', ProposeAlliance: '同盟を申し入れる',
  AcceptPeace: '講和を受け入れる', AcceptNap: '不可侵条約を受け入れる', AcceptAlliance: '同盟に加わる',
}, {
  DeclareWar: 'Declare war', ProposePeace: 'Offer peace', ProposeNap: 'Offer a NAP', ProposeAlliance: 'Offer an alliance',
  AcceptPeace: 'Accept peace', AcceptNap: 'Accept the NAP', AcceptAlliance: 'Join the alliance',
});

const CITY_NAMES = twin(
  ['ラナ', 'ヴェル', 'オルト', 'セナ', 'カロ', 'ミラ', 'トーレ', 'ウルム', 'ネス', 'ハルカ', 'イゼル', 'ボラ', 'エダ', 'クオン', 'サイラ', 'ティモ', 'リュカ', 'ファロ', 'ジン', 'アルバ'],
  ['Lana', 'Vel', 'Ort', 'Sena', 'Karo', 'Mira', 'Tore', 'Ulm', 'Ness', 'Haruka', 'Izel', 'Bora', 'Eda', 'Kuon', 'Saira', 'Timo', 'Ryuka', 'Faro', 'Jin', 'Alba'],
);
export const cityName = id => CITY_NAMES[id % CITY_NAMES.length] + (id >= CITY_NAMES.length ? ` ${Math.floor(id / CITY_NAMES.length) + 1}` : '');
export const civName = name => CIV_NAMES[name] || name;
/** [start, name, NAME] of the phase `tick` is in. */
export function phaseOf(tick, starts = PHASE_STARTS) {
  let i = 0;
  while (i + 1 < PHASES.length && tick >= (starts[i + 1] ?? Infinity)) i++;
  return [starts[i] ?? 0, ...PHASES[i]];
}

// ------------------------------------------------------------------ why an order is blocked
// code → text, or (b, techName) => text for the codes that carry values.
const BLOCKED_JA = {
  NeedsTech: (b, t) => `「${t}」の研究が必要です`,
  TooCloseToCity: b => `都市から${b.distance}マス。${b.min}マス以上離す必要があります`,
  TooCloseToCityState: b => `都市国家から${b.distance}マス。${b.min}マス以上離す必要があります`,
  NeedsPop: b => `人口${b.need}以上が必要です（現在${b.have}）`,
  InTruce: b => `講和後の休戦中です。ティック${b.until}まで宣戦できません`,
  BondTooSmall: b => `保証金は${b.min}金以上が必要です`,
  NotEnoughGold: b => `金が足りません（必要${b.need}・所持${b.have}）`,
  AllianceFull: b => `同盟は${b.cap}勢力までです`,
  OutOfRange: b => `射程外です（距離${b.distance}・射程${b.range}）`,
  OverCap: b => `上限${b.cap}を超えています`,
  OutOfBounds: b => `${b.min}〜${b.max}の範囲で指定してください`,
  ProtectedCapital: b => `首都の保護区域です${b.until !== null && b.until !== undefined ? `（ティック${b.until}まで）` : ''}`,
  UnknownUnit: '部隊が見つかりません', UnknownCity: '都市が見つかりません', UnknownCiv: '勢力が見つかりません',
  NotYours: 'あなたのものではありません', SameCiv: '自分の勢力です', NotASettler: '開拓者ではありません',
  Impassable: '通行できない地形です', ForeignTerritory: '他の勢力の領土です', InProtectedZone: '他の勢力の保護区域内です',
  AlreadyBuilt: '建設済みです', AlreadyQueued: 'すでに生産予定です', StarGateInAnotherCity: 'スターゲートは1都市だけに建てられます',
  NeedsPreviousStage: '前の段階を先に完成させてください', InvalidTroopCount: '兵数が正しくありません', AlreadyResearched: '研究済みです',
  NotAtPeace: '平和な関係のときだけできます', AlreadyAtWar: 'すでに戦争中です', NotAtWar: '戦争中ではありません',
  UnderNap: '不可侵条約中です。宣戦するには先に条約を破棄します', Allied: '同盟相手には宣戦できません', NoProposal: '申し入れがありません',
  AlreadyInAlliance: 'すでに同盟に加わっています', AllianceLeaving: '離脱手続き中の同盟には加われません',
  CivilianCannotAttack: '非戦闘ユニットは攻撃できません', TargetProtected: '保護区域内の相手は攻撃できません',
  TargetNotHostile: '戦争中の相手ではありません。先に宣戦が必要です', TargetGone: '目標がもういません',
  Frozen: '終盤のため取引は凍結中です', NothingToSell: '売れる在庫がありません',
  WrongStandingTarget: 'この対象には使えない継続命令です',
  BatchRejected: 'この役職の命令全体が受け付けられませんでした（枠・役職・献策の採用を確認）', WrongOffice: '担当外の役職の命令です',
  NeedsConsent: '宣戦には将軍か内政官（外交官とは別の人）の同意が必要です', AlreadyPurchased: '購入は1都市1ティックに1回までです',
  CannotBuyStarGate: 'スターゲートはお金で買えません', NothingQueued: '生産予定がありません', NotEnoughInfluence: '影響力が足りません',
  NoCounterparty: '自分の勢力・交戦中の勢力とは取引できません', NeedsSpendConsent: '一定額を超える支出には別の役職者の同意が必要です',
  NotEnoughUsdc: '勢力の資金（USDC）が足りません',
  ForeignCity: '他の勢力の都市には占領しないと入れません',
  TooManyContracts: '出している契約が上限に達しています', UnknownContract: 'その契約はありません（受け入れ済み・期限切れ・相手違い）',
  BadContract: '契約の条件が今の世界に合いません（戦争中か・条約があるか・期限・金額を確認）',
};
const BLOCKED_EN = {
  NeedsTech: (b, t) => `Requires the tech “${t}”`,
  TooCloseToCity: b => `${b.distance} tiles from a city: must be at least ${b.min} away`,
  TooCloseToCityState: b => `${b.distance} tiles from a city-state: must be at least ${b.min} away`,
  NeedsPop: b => `Needs population ${b.need} or more (now ${b.have})`,
  InTruce: b => `Truce after peace: no war before tick ${b.until}`,
  BondTooSmall: b => `The bond must be at least ${b.min} gold`,
  NotEnoughGold: b => `Not enough gold (need ${b.need}, have ${b.have})`,
  AllianceFull: b => `An alliance has at most ${b.cap} factions`,
  OutOfRange: b => `Out of range (distance ${b.distance}, range ${b.range})`,
  OverCap: b => `Over the limit of ${b.cap}`,
  OutOfBounds: b => `Must be between ${b.min} and ${b.max}`,
  ProtectedCapital: b => `A capital's protected zone${b.until !== null && b.until !== undefined ? ` (until tick ${b.until})` : ''}`,
  UnknownUnit: 'Unit not found', UnknownCity: 'City not found', UnknownCiv: 'Faction not found',
  NotYours: 'Not yours', SameCiv: 'That is your own faction', NotASettler: 'Not a Settler',
  Impassable: 'Impassable terrain', ForeignTerritory: "Another faction's territory", InProtectedZone: "Inside another faction's protected zone",
  AlreadyBuilt: 'Already built', AlreadyQueued: 'Already in production', StarGateInAnotherCity: 'A Star Gate can be built in one city only',
  NeedsPreviousStage: 'Complete the previous stage first', InvalidTroopCount: 'Invalid troop count', AlreadyResearched: 'Already researched',
  NotAtPeace: 'Only possible at peace', AlreadyAtWar: 'Already at war', NotAtWar: 'Not at war',
  UnderNap: 'A NAP is in force: break it first to declare war', Allied: 'You cannot declare war on an ally', NoProposal: 'No such offer',
  AlreadyInAlliance: 'Already in an alliance', AllianceLeaving: 'You cannot join an alliance you are leaving',
  CivilianCannotAttack: 'Civilian units cannot attack', TargetProtected: 'The target is inside a protected zone',
  TargetNotHostile: 'You are not at war with the target: declare war first', TargetGone: 'The target is gone',
  Frozen: 'Trading is frozen for the endgame', NothingToSell: 'Nothing in stock to sell',
  WrongStandingTarget: 'This standing order does not apply to this target',
  BatchRejected: "This office's whole batch was refused (check the budget, the office and the adopted proposals)", WrongOffice: "An order of an office you don't hold",
  NeedsConsent: 'War needs the consent of the General or the Steward (someone other than the Diplomat)', AlreadyPurchased: 'One purchase per city per tick',
  CannotBuyStarGate: 'A Star Gate cannot be bought', NothingQueued: 'Nothing in production', NotEnoughInfluence: 'Not enough influence',
  NoCounterparty: 'No trade with yourself or with a faction at war with you', NeedsSpendConsent: "Spending above the limit needs another officer's consent",
  NotEnoughUsdc: 'Not enough USDC in the treasury',
  ForeignCity: "Another faction's city: capture it to enter",
  TooManyContracts: 'Your open contracts are at the limit', UnknownContract: 'No such contract (accepted, expired, or for another faction)',
  BadContract: "The contract's terms don't fit the world now (check war, treaties, the deadline and the amount)",
};

export function blockedText(b) {
  if (!b) return '';
  const table = en() && own(BLOCKED_EN, b.code) ? BLOCKED_EN : BLOCKED_JA;
  if (!own(table, b.code)) return b.code;
  const x = table[b.code];
  return typeof x === 'function' ? x(b, TECH[b.tech] || b.tech) : x;
}

// ------------------------------------------------------------------ chain, gateway and wallet errors
// Keys are the gateway's `code`s, the program's error names (codec.mjs
// CHAIN_ERRORS) and the web client's own (wallet.mjs, session.mjs,
// chainio.mjs). Every one reads the same whoever the member is.
const TICK_CLOSED = 'このティックは締め切られました。次のティックで出し直してください';
const TICK_CLOSED_EN = 'This tick is closed. Send it again next tick.';
export const CHAIN_ERROR_JA = twin({
  // registration (x402 / program)
  SessionInUse: 'このゲーム内の鍵はすでに別のメンバーが使っています。別のウォレットで登録してください。',
  KindHidden: 'このシーズンはメンバーの種別を公開しません（「未申告」でだけ登録できます）。',
  SeasonFull: 'このシーズンは満員です。',
  RegistrationClosed: '登録は締め切られました。',
  WrongStatus: 'シーズンが、この操作を受け付ける段階ではありません。',
  InvalidName: 'この名前は使えません（24バイトまで）。',
  AlreadyInitialized: 'このウォレットは、すでにこのシーズンのメンバーです。',
  AlreadyMember: 'このウォレットは、すでにこのシーズンのメンバーです。',
  InsufficientFunds: 'USDC が足りません。テスト USDC を受け取るか、残高を確認してください。',
  X402Mismatch: '支払いの条件がこのシーズンと合わないため、署名を求めませんでした。',
  RegistrationInFlight: 'このウォレットの登録を送信中です。少し待ってから確かめてください。',
  InvalidPayment: 'ゲートウェイが支払いの取引を受け付けませんでした。',
  SettlementFailed: '支払いを送れませんでした（登録はされていません）。もう一度お試しください。',
  AlreadyProcessed: 'この取引は送信済みです。',
  TokenError: 'USDC の口座でエラーが起きました（残高と口座を確かめてください）。',
  SimulationFailed: 'この取引は失敗する見込みのため、送りませんでした。',
  NonCanonicalTransaction: 'ゲートウェイが取引の形を受け付けませんでした。',
  PinMismatch: 'ゲームサーバーとゲートウェイのシーズンが一致しません。',
  // claims
  AlreadyClaimed: '賞金はすでに受け取り済みです。',
  NothingToClaim: '受け取れる賞金はありません。',
  NotFinalized: 'まだ精算中です（シーズン終了から最長で約1時間）。精算が終わると受け取れます。',
  SeasonNotOver: 'シーズンはまだ終わっていません。',
  NoSuchMember: 'このウォレットは、そのシーズンのメンバーではありません。',
  UnknownSeason: 'そのシーズンは、このゲートウェイで受け取れません。',
  // wallet and keys
  WalletRejected: 'ウォレットで取り消されました。',
  WalletError: 'ウォレットでエラーが起きました。',
  WalletUnsupported: 'このウォレットは使えません。',
  WalletModified: 'ウォレットが取引を書き換えました。署名は送っていません。',
  WalletBadSignature: 'ウォレットの署名を確認できませんでした。',
  NoWallet: 'ウォレットが接続されていません。',
  NoAccount: 'ウォレットに、このネットワークで使えるアカウントがありません。',
  SessionMismatch: 'この鍵は、登録されているゲーム内の鍵と一致しません。',
  BadBackup: '鍵のバックアップを読めませんでした（16進64文字の鍵が必要です）。',
  InsecureContext: 'https か 127.0.0.1 で開いてください（この接続ではブラウザの暗号機能が使えません）。',
  NoEd25519: 'このブラウザは Ed25519 署名に対応していません。最新の Chrome・Edge・Firefox・Safari で開いてください。',
  BlockhashExpired: '取引の有効期限が切れました（承認に時間がかかったため）。もう一度押すと、新しい条件で署名を求めます。',
  // the gateway
  RateLimited: 'リクエストが多すぎます。少し待ってからもう一度お試しください。',
  OperatorLowFunds: '運営の手数料用の SOL が不足しています。しばらくしてからお試しください。',
  FaucetCooldown: 'テスト USDC は受け取ったばかりです。少し待ってからお試しください。',
  FaucetBusy: 'テスト USDC の配布が混み合っています。少し待ってからお試しください。',
  FaucetDisabled: 'このネットワークにはテスト USDC の配布がありません。',
  RelayMismatch: 'ゲートウェイの応答がこのシーズンと合わないため、署名しませんでした。',
  RelayRejected: 'ゲートウェイがこの取引を受け付けませんでした。',
  WorldUnavailable: 'チェーンのシーズンをまだ読めません。少し待ってからお試しください。',
  Unavailable: 'いまは受け付けていません。少し待ってからお試しください。',
  NoGateway: 'ゲートウェイの場所がわかりません。',
  // playing (sealed orders, governance)
  WrongTick: TICK_CLOSED,
  TickFrozen: TICK_CLOSED,
  WrongPhase: 'いまは受け付けていない段階です（確定は各ティックの締切まで）。次のティックで出し直してください。',
  CommitMismatch: '封印した命令と公開した命令が一致しません。',
  NotOfficer: 'この役職の担当者ではありません。',
  BadSignature: '署名を確認できませんでした（鍵が登録と一致しません）。',
  TooManySeals: 'このティックに確定できる回数の上限に達しました。',
  BatchTooLarge: '命令が多すぎて1回の確定に収まりません。減らしてください。',
  MissingRationale: '前に封印した判断メモの公開が抜けています。',
  WrongOffice: '担当外の役職の命令です。',
  VacantOffice: 'この役職は空席です。',
  InvalidSeal: '封印の内容が正しくありません。',
  TooManyCommits: 'このティックに確定できる回数の上限に達しました。',
  TalkRefused: 'メッセージを送れませんでした（このティックは締め切られたか、上限に達しました）。',
  NotAMember: 'このシーズンのメンバーではありません。',
  network: 'サーバーに届きませんでした',
}, {
  // registration (x402 / program)
  SessionInUse: 'Another member already uses this in-game key. Register with a different wallet.',
  KindHidden: 'This season does not disclose member kinds (you can only register as “undeclared”).',
  SeasonFull: 'This season is full.',
  RegistrationClosed: 'Registration is closed.',
  WrongStatus: 'The season is not at a stage that accepts this.',
  InvalidName: 'This name cannot be used (24 bytes at most).',
  AlreadyInitialized: 'This wallet is already a member of this season.',
  AlreadyMember: 'This wallet is already a member of this season.',
  InsufficientFunds: 'Not enough USDC. Get test USDC or check your balance.',
  X402Mismatch: "The payment terms don't match this season, so no signature was requested.",
  RegistrationInFlight: "This wallet's registration is being sent. Wait a moment, then check.",
  InvalidPayment: 'The gateway did not accept the payment transaction.',
  SettlementFailed: 'The payment could not be sent (you are not registered). Please try again.',
  AlreadyProcessed: 'This transaction has already been sent.',
  TokenError: 'Something went wrong with the USDC account (check the balance and the account).',
  SimulationFailed: 'This transaction would fail, so it was not sent.',
  NonCanonicalTransaction: "The gateway did not accept the transaction's shape.",
  PinMismatch: "The game server's and the gateway's seasons don't match.",
  // claims
  AlreadyClaimed: 'The prize has already been claimed.',
  NothingToClaim: 'There is no prize to claim.',
  NotFinalized: 'Still settling (up to about an hour after the season ends). You can claim once settlement is done.',
  SeasonNotOver: 'The season is not over yet.',
  NoSuchMember: 'This wallet is not a member of that season.',
  UnknownSeason: 'That season cannot be claimed through this gateway.',
  // wallet and keys
  WalletRejected: 'Canceled in the wallet.',
  WalletError: 'The wallet reported an error.',
  WalletUnsupported: 'This wallet cannot be used.',
  WalletModified: 'The wallet altered the transaction. The signature was not sent.',
  WalletBadSignature: "The wallet's signature could not be verified.",
  NoWallet: 'No wallet is connected.',
  NoAccount: 'The wallet has no account usable on this network.',
  SessionMismatch: "This key doesn't match the registered in-game key.",
  BadBackup: 'Could not read the key backup (a key of 64 hex characters is needed).',
  InsecureContext: "Open this page over https or at 127.0.0.1 (the browser's cryptography is unavailable on this connection).",
  NoEd25519: "This browser doesn't support Ed25519 signatures. Open the page in a recent Chrome, Edge, Firefox or Safari.",
  BlockhashExpired: 'The transaction expired (the approval took too long). Press again to sign with fresh terms.',
  // the gateway
  RateLimited: 'Too many requests. Wait a moment and try again.',
  OperatorLowFunds: "The operator's SOL for fees is running low. Try again later.",
  FaucetCooldown: 'You have just received test USDC. Wait a moment and try again.',
  FaucetBusy: 'The test USDC faucet is busy. Wait a moment and try again.',
  FaucetDisabled: 'This network has no test USDC faucet.',
  RelayMismatch: "The gateway's reply doesn't match this season, so nothing was signed.",
  RelayRejected: 'The gateway did not accept this transaction.',
  WorldUnavailable: "The season can't be read from the chain yet. Wait a moment and try again.",
  Unavailable: 'Not accepting this right now. Wait a moment and try again.',
  NoGateway: "The gateway's address is unknown.",
  // playing (sealed orders, governance)
  WrongTick: TICK_CLOSED_EN,
  TickFrozen: TICK_CLOSED_EN,
  WrongPhase: "Not accepted at this stage (commits close at each tick's deadline). Send it again next tick.",
  CommitMismatch: "The revealed orders don't match the sealed ones.",
  NotOfficer: 'You do not hold this office.',
  BadSignature: "The signature could not be verified (the key doesn't match the registered one).",
  TooManySeals: 'You have reached the limit of commits for this tick.',
  BatchTooLarge: 'Too many orders for one commit. Remove some.',
  MissingRationale: 'The reveal of an earlier sealed rationale is missing.',
  WrongOffice: "An order of an office you don't hold.",
  VacantOffice: 'This office is vacant.',
  InvalidSeal: 'The seal is not valid.',
  TooManyCommits: 'You have reached the limit of commits for this tick.',
  TalkRefused: 'The message could not be sent (this tick is closed or the limit is reached).',
  NotAMember: 'Not a member of this season.',
  network: 'Could not reach the server',
});
const CODE_WORD = new RegExp(`\\b(${Object.keys(CHAIN_ERROR_JA).filter(k => /^[A-Z]/.test(k)).join('|')})\\b`);

/**
 * A chain, gateway or wallet error in the current language: `{code, error}`
 * (a chainio.mjs result, a thrown Error with `code`) or a message string. By
 * code first, then a program error named in the text, then the common
 * wallet and RPC wordings; anything else passes through.
 */
export function errorText(e) {
  if (e === null || e === undefined || e === '') return '';
  const code = typeof e === 'object' ? e.code : null;
  const text = typeof e === 'object' ? String(e.error ?? e.message ?? '') : String(e);
  if (typeof code === 'string' && own(CHAIN_ERROR_JA, code) && CHAIN_ERROR_JA[code]) return CHAIN_ERROR_JA[code];
  const named = text.match(CODE_WORD);
  if (named) return CHAIN_ERROR_JA[named[1]];
  if (text === 'network') return CHAIN_ERROR_JA.network;
  if (/insufficient (funds|lamports)/i.test(text)) return CHAIN_ERROR_JA.InsufficientFunds;
  if (/blockhash not found|block height exceeded|blockhash.*expired|expired.*blockhash/i.test(text)) return CHAIN_ERROR_JA.BlockhashExpired;
  if (code === 4001 || /user rejected|rejected the request|denied/i.test(text)) return CHAIN_ERROR_JA.WalletRejected;
  return text || String(code ?? '');
}

export function itemName(item) {
  if (!item) return '';
  if (item.kind === 'Building') return BUILDING[item.building] || item.building;
  if (item.kind === 'Troops') return `${UNIT[item.unit] || item.unit} ×${item.n}`;
  return UNIT[item.kind] || item.kind;
}
export function itemGlyph(item) {
  if (item.kind === 'Building') return BUILDING_GLYPH[item.building] || '▢';
  if (item.kind === 'Troops') return UNIT_GLYPH[item.unit] || '⚔';
  return UNIT_GLYPH[item.kind] || '•';
}

// ------------------------------------------------------------------ the chronicle
/** Chronicle lines come from the server as `kind|English text`; this gives them in the current language. */
export function chronicleText(line) {
  const [kind, text = ''] = line.split('|');
  // First match wins, so the table order matters (e.g. the two bounty forms).
  for (const [re, ja, enText] of CHRONICLE) {
    const m = text.match(re);
    if (m) return [kind, (en() ? enText : ja)(m, kind)];
  }
  return [kind, text];
}
const n = s => civName(s.trim());
// Chronicle lines name offices in lower case and paths in English.
const ROLE_OF = { general: 'General', steward: 'Steward', 'science officer': 'Science', diplomat: 'Diplomat' };
const roleName = r => ROLE_JA[ROLE_OF[r]];
// Unknown paths (a newer engine) fall back to the raw English name.
const pathName = p => (PATHS.includes(p) ? PATH_JA[PATHS.indexOf(p)] : p);
const actor = s => (s === 'the acting official' || s === 'acting' ? (en() ? 'the caretaker' : '代行') : s);
const electedText = x => {
  const r = Object.keys(ROLE_OF).find(k => x.startsWith(`${k} `));
  if (!r) return x;
  const who = actor(x.slice(r.length + 1));
  return en() ? `${who} (${roleName(r)})` : `${roleName(r)} ${who}`;
};
/** [English pattern, (match, kind) => Japanese, (match, kind) => English] — evaluated top to bottom by chronicleText. */
const CHRONICLE = [
  [/^(\w+) declares war on (\w+)(.*)$/,
    m => `${n(m[1])}が${n(m[2])}に宣戦${m[3].includes('pact') ? '（条約破棄）' : m[3].includes('casus') ? '（正当な理由あり）' : ''}`,
    m => `${n(m[1])} declares war on ${n(m[2])}${m[3].includes('pact') ? ' (breaking a pact)' : m[3].includes('casus') ? ' (with casus belli)' : ''}`],
  [/^(\w+) and (\w+) make peace$/, m => `${n(m[1])}と${n(m[2])}が講和`, m => `${n(m[1])} and ${n(m[2])} make peace`],
  [/^(\w+) and (\w+) form an alliance$/, m => `${n(m[1])}と${n(m[2])}が同盟を結成`, m => `${n(m[1])} and ${n(m[2])} form an alliance`],
  [/^(\w+) and (\w+) sign a non-aggression pact$/, m => `${n(m[1])}と${n(m[2])}が不可侵条約を締結`, m => `${n(m[1])} and ${n(m[2])} sign a non-aggression pact`],
  [/^The pact between (\w+) and (\w+) expires$/, m => `${n(m[1])}と${n(m[2])}の不可侵条約が満了`, m => `The pact between ${n(m[1])} and ${n(m[2])} expires`],
  [/^(\w+) and (\w+) end their alliance$/, m => `${n(m[1])}と${n(m[2])}の同盟が解消`, m => `${n(m[1])} and ${n(m[2])} end their alliance`],
  [/^(\w+) founds a new city$/, m => `${n(m[1])}が新しい都市を建設`, m => `${n(m[1])} founds a new city`],
  [/^(\w+) captures a city of (\w+)$/, m => `${n(m[1])}が${n(m[2])}の都市を占領`, m => `${n(m[1])} captures a city of ${n(m[2])}`],
  [/^(\w+) captures a free city$/, m => `${n(m[1])}が自由都市を占領`, m => `${n(m[1])} captures a free city`],
  [/^(\w+) conquers a city-state$/, m => `${n(m[1])}が都市国家を征服`, m => `${n(m[1])} conquers a city-state`],
  [/^A city of (\w+) revolts and becomes free$/, m => `${n(m[1])}の都市が反乱し自由都市に`, m => `A city of ${n(m[1])} revolts and becomes free`],
  [/^A city is razed to a ruin$/, () => '都市が破壊され遺跡になった', () => 'A city is razed to ruins'],
  [/^(\w+) completes Star Gate stage (\d)$/, m => `${n(m[1])}がスターゲート第${m[2]}段階を完成`, m => `${n(m[1])} completes Star Gate stage ${m[2]}`],
  [/^(\w+) becomes suzerain of city-state (\d+)$/, m => `${n(m[1])}が都市国家${m[2]}の宗主に`, m => `${n(m[1])} becomes suzerain of City-state ${m[2]}`],
  [/^(\w+) discovers (\w+)$/, m => `${n(m[1])}が「${TECH[m[2]] || m[2]}」を発見`, m => `${n(m[1])} discovers ${TECH[m[2]] || m[2]}`],
  [/^(\w+): (.+) becomes (general|steward|science officer|diplomat) \(was (.+)\)$/,
    (m, kind) => `${n(m[1])}：${actor(m[2])}が${roleName(m[3])}に${kind === 'recall' ? '（解任による交代）' : ''}（前任 ${actor(m[4])}）`,
    (m, kind) => `${n(m[1])}: ${actor(m[2])} becomes ${roleName(m[3])}${kind === 'recall' ? ' after a recall' : ''} (was ${actor(m[4])})`],
  [/^First election — (\w+): (.+)$/,
    m => `${n(m[1])}の第1回選挙：${m[2].split(', ').map(electedText).join('・')}`,
    m => `First election — ${n(m[1])}: ${m[2].split(', ').map(electedText).join(', ')}`],
  [/^(\w+) adopts (\d+) proposals?$/, m => `${n(m[1])}が献策を${m[2]}件採用`, m => `${n(m[1])} adopts ${m[2]} proposal${m[2] === '1' ? '' : 's'}`],
  [/^(\w+) reaches (\w+) (\d)$/, m => `${n(m[1])}が${pathName(m[2])}の第${m[3]}段階に到達`, m => `${n(m[1])} reaches ${pathName(m[2])} tier ${m[3]}`],
  [/^(\w+) loses (\w+) (\d)$/, m => `${n(m[1])}が${pathName(m[2])}の第${m[3]}段階を失った`, m => `${n(m[1])} loses ${pathName(m[2])} tier ${m[3]}`],
  [/^(\w+) enters era (\d)$/, m => `${n(m[1])}が第${m[2]}時代に入った`, m => `${n(m[1])} enters Era ${m[2]}`],
  [/^(\w+) conquers the home of (.+) of (\w+), an operator AI member: bounty (\d+) USDC$/,
    m => `${n(m[1])}が${n(m[3])}の${m[2]}（運営のAIメンバー）の住む都市を落とした：懸賞金 ${m[4]} USDC`,
    m => `${n(m[1])} takes the home city of ${m[2]} of ${n(m[3])}, an operator AI member: bounty ${m[4]} USDC`],
  [/^(\w+) conquers the home of (.+) of (\w+), an operator AI member: no bounty/,
    m => `${n(m[1])}が${n(m[3])}の${m[2]}（運営のAIメンバー）の住む都市を落とした：直前に条約があったため懸賞金なし`,
    m => `${n(m[1])} takes the home city of ${m[2]} of ${n(m[3])}, an operator AI member: no bounty (a treaty just before)`],
];
export const KIND_GLYPH = { bounty: '◎', war: '⚔', capture: '⚑', raze: '✕', peace: '☮', ally: '⚭', diplo: '✉', science: '✦', revolt: '!', found: '⌂', tech: '✧', gov: '⚖', recall: '⚠', milestone: '◆', era: '✺' };
/** What earned merit (engine event names, V5 §7.3). */
export const MERIT_WHAT = twin({
  office: '役職者として活動', gold: '都市が稼いだ金（内政官）', growth: '人口の成長', building: '建物の完成', found_city: '都市の建設', tech: '技術の完成',
  star_gate: 'スターゲートの段階', capture: '都市の占領', troops: '敵部隊への損害', held: '攻撃に耐えた都市（将軍）', treaty: '条約の成立', suzerain: '都市国家の宗主', trade: '交易',
}, {
  office: 'Active as an officer', gold: 'Gold earned by cities (Steward)', growth: 'Population growth', building: 'Buildings completed', found_city: 'Cities founded', tech: 'Techs completed',
  star_gate: 'Star Gate stages', capture: 'Cities captured', troops: 'Damage to enemy troops', held: 'Cities that withstood attacks (General)', treaty: 'Treaties made', suzerain: 'City-state suzerainty', trade: 'Trade',
});

// ------------------------------------------------------------------ standing rules (§13)
export const STANDING_GLYPH = { AutoDefend: '⛨', Retreat: '↩', Patrol: '⟳', QueueRepeat: '↻', AutoPurchase: '◆' };
export function standingText(r) {
  if (!r) return '';
  const e = en();
  switch (r.kind) {
    case 'AutoDefend': return e ? `Auto-defend · radius ${r.radius}` : `自動防衛 · 半径${r.radius}`;
    case 'Retreat': return e ? `Retreat · when the enemy is over ${(r.ratioBps / 10000).toFixed(1)}×` : `撤退 · 相手が${(r.ratioBps / 10000).toFixed(1)}倍を超えたら`;
    case 'Patrol': return e ? `Patrol · ${r.route.length} point${r.route.length === 1 ? '' : 's'}` : `巡回 · ${r.route.length}地点`;
    case 'QueueRepeat': return e ? (r.on ? 'Repeat production ON' : 'Repeat production OFF') : (r.on ? '生産の繰り返し ON' : '生産の繰り返し OFF');
    case 'AutoPurchase': return e ? (r.maxGold ? `Auto-buy · up to ${r.maxGold} gold per tick` : 'Auto-buy OFF') : (r.maxGold ? `自動購入 · 毎ティック${r.maxGold}金まで` : '自動購入 OFF');
    case 'Clear': return e ? 'Clear standing orders' : '継続命令を解除';
    default: return r.kind;
  }
}
