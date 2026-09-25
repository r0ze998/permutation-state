// Japanese-first display names and blocked-reason messages.
// Keys are the engine's own enum names, as sent by the API.

export const CIV_COLORS = ['#c1504a', '#2f8f84', '#c28f2c', '#7a5fb0', '#3f78c2', '#b5527f'];
export const CIV_NAMES = { Aster: 'アステル', Borealis: 'ボレアリス', Cinder: 'シンダー', Dunmar: 'ダンマール', Ember: 'エンバー', Fjordal: 'フィヨルダル' };

// Offices of a nation (V5 §5), in the engine's order.
export const ROLES = ['General', 'Steward', 'Science', 'Diplomat'];
export const ROLE_JA = { General: '将軍', Steward: '内政官', Science: '科学官', Diplomat: '外交官' };
export const ROLE_GLYPH = { General: '⚔', Steward: '⌂', Science: '✧', Diplomat: '✉' };
// The four paths (V5 §6), in the engine's order (tiers arrays use it).
export const PATHS = ['Hegemony', 'Prosperity', 'Science', 'Concord'];
export const PATH_JA = ['覇権', '繁栄', '科学', '協調'];
/** Merit buckets (member.merit keys, merit log paths lower-cased). */
export const MERIT_JA = { hegemony: '覇権', prosperity: '繁栄', science: '科学', concord: '協調', common: '役職の務め' };
/** Diplomatic proposal kinds (view.proposals[].kind). */
export const PROPOSAL_KIND = { Peace: '講和', Nap: '不可侵条約', Alliance: '同盟' };

export const TERRAIN = { Grassland: '草原', Plains: '平原', Forest: '森', Hills: '丘陵', Mountain: '山岳', Water: '水域' };
export const TERRAIN_EN = { Grassland: 'GRASSLAND', Plains: 'PLAINS', Forest: 'WOODLAND', Hills: 'HILLS', Mountain: 'MOUNTAIN', Water: 'WATER' };
/** Tile resources and tradeable goods (tile.resource, pools[].good, good.kind). */
export const RESOURCE = { Wheat: '小麦', Iron: '鉄', Horses: '馬', Gold: '金', Food: '食料', Production: '生産' };
export const goodName = g => RESOURCE[g.kind];
export const UNIT = { Spearman: '槍兵', Archer: '弓兵', Horseman: '騎兵', Pikeman: '長槍兵', Crossbowman: '弩兵', Knight: '騎士', Scout: '斥候', Settler: '開拓者' };
export const UNIT_GLYPH = { Spearman: '⟋', Archer: '➶', Horseman: '♞', Pikeman: '⫽', Crossbowman: '⤓', Knight: '♘', Scout: '◎', Settler: '⚑' };
export const BUILDING = { Granary: '穀物庫', Workshop: '工房', Temple: '神殿', Market: '市場', Academy: '学術院', Barracks: '兵舎', Walls: '城壁', StarGate1: 'スターゲート I', StarGate2: 'スターゲート II', StarGate3: 'スターゲート III' };
export const BUILDING_GLYPH = { Granary: '❧', Workshop: '⚒', Temple: '✧', Market: '⇄', Academy: '▤', Barracks: '♜', Walls: '▥', StarGate1: '✦', StarGate2: '✦', StarGate3: '✦' };
export const BUILDING_EFFECT = {
  Granary: '食料 +2', Workshop: '生産 +2', Temple: '快適度 +2・影響力 +2', Market: '金 +3、都市の金 ×1.2', Academy: '科学 +3',
  Barracks: '部隊の生産コスト ×0.75', Walls: '都市防御への被害 ×0.67', StarGate1: '科学勝利の第1段階', StarGate2: '科学勝利の第2段階', StarGate3: '科学勝利の最終段階',
};
export const TECH = {
  Agriculture: '農業', BronzeWorking: '青銅器', Archery: '弓術', HorsebackRiding: '騎乗', Masonry: '石工', Mysticism: '神秘主義',
  Writing: '筆記', Currency: '通貨', IronWorking: '製鉄', Mathematics: '数学', Chivalry: '騎士道', Philosophy: '哲学',
  Engineering: '工学', Astronomy: '天文学', Physics: '物理学', CelestialMechanics: '天体力学',
};
export const TECH_UNLOCK = {
  Agriculture: '小麦の食料 +1', BronzeWorking: '兵舎', Archery: '弓兵', HorsebackRiding: '騎兵・馬の産出', Masonry: '城壁', Mysticism: '神殿',
  Writing: '学術院', Currency: '市場・金の市場', IronWorking: '長槍兵・鉄の産出', Mathematics: '弩兵', Chivalry: '騎士', Philosophy: '神殿の影響力 +1',
  Engineering: '城壁の強化', Astronomy: 'スターゲート I', Physics: 'スターゲート II', CelestialMechanics: 'スターゲート III',
};
export const FOCUS = { Balanced: '均衡', Food: '食料', Production: '生産', Gold: '金', Science: '科学' };
export const SPECIALTY = { Scientific: '学術', Mercantile: '商業', Agrarian: '農業' };
export const SPECIALTY_BONUS = { Scientific: '宗主に科学 +3/ティック', Mercantile: '宗主に金 +4/ティック', Agrarian: '宗主の首都に食料 +2/ティック' };
export const RELATION = { self: 'あなた', peace: '平和', war: '戦争', nap: '不可侵', alliance: '同盟' };
/** Season phases; their start ticks come from the server (season.phases). */
export const PHASES = [['建国', 'FOUNDING'], ['拡大', 'EXPANSION'], ['競合', 'CONTENTION'], ['危機', 'CRISIS'], ['決着', 'RESOLUTION']];
const PHASE_STARTS = [0, 18, 60, 120, 162]; // only if a view has no season.phases
export const DIPLO_ACTION = {
  DeclareWar: '宣戦する', ProposePeace: '講和を申し入れる', ProposeNap: '不可侵条約を申し入れる', ProposeAlliance: '同盟を申し入れる',
  AcceptPeace: '講和を受け入れる', AcceptNap: '不可侵条約を受け入れる', AcceptAlliance: '同盟に加わる',
};

const CITY_NAMES = ['ラナ', 'ヴェル', 'オルト', 'セナ', 'カロ', 'ミラ', 'トーレ', 'ウルム', 'ネス', 'ハルカ', 'イゼル', 'ボラ', 'エダ', 'クオン', 'サイラ', 'ティモ', 'リュカ', 'ファロ', 'ジン', 'アルバ'];
export const cityName = id => CITY_NAMES[id % CITY_NAMES.length] + (id >= CITY_NAMES.length ? ` ${Math.floor(id / CITY_NAMES.length) + 1}` : '');
export const civName = name => CIV_NAMES[name] || name;
/** [start, name, NAME] of the phase `tick` is in. */
export function phaseOf(tick, starts = PHASE_STARTS) {
  let i = 0;
  while (i + 1 < PHASES.length && tick >= (starts[i + 1] ?? Infinity)) i++;
  return [starts[i] ?? 0, ...PHASES[i]];
}

export function blockedText(b) {
  if (!b) return '';
  const t = TECH[b.tech] || b.tech;
  switch (b.code) {
    case 'NeedsTech': return `「${t}」の研究が必要です`;
    case 'TooCloseToCity': return `都市から${b.distance}マス。${b.min}マス以上離す必要があります`;
    case 'TooCloseToCityState': return `都市国家から${b.distance}マス。${b.min}マス以上離す必要があります`;
    case 'NeedsPop': return `人口${b.need}以上が必要です（現在${b.have}）`;
    case 'InTruce': return `講和後の休戦中です。ティック${b.until}まで宣戦できません`;
    case 'BondTooSmall': return `保証金は${b.min}金以上が必要です`;
    case 'NotEnoughGold': return `金が足りません（必要${b.need}・所持${b.have}）`;
    case 'AllianceFull': return `同盟は${b.cap}文明までです`;
    case 'OutOfRange': return `射程外です（距離${b.distance}・射程${b.range}）`;
    case 'OverCap': return `上限${b.cap}を超えています`;
    case 'OutOfBounds': return `${b.min}〜${b.max}の範囲で指定してください`;
    case 'ProtectedCapital': return `首都の保護区域です${b.until !== null && b.until !== undefined ? `（ティック${b.until}まで）` : ''}`;
    default: return ({
      UnknownUnit: '部隊が見つかりません', UnknownCity: '都市が見つかりません', UnknownCiv: '文明が見つかりません',
      NotYours: 'あなたのものではありません', SameCiv: '自分の文明です', NotASettler: '開拓者ではありません',
      Impassable: '通行できない地形です', ForeignTerritory: '他の文明の領土です', InProtectedZone: '他の文明の保護区域内です',
      AlreadyBuilt: '建設済みです', AlreadyQueued: 'すでに生産予定です', StarGateInAnotherCity: 'スターゲートは1都市だけに建てられます',
      NeedsPreviousStage: '前の段階を先に完成させてください', InvalidTroopCount: '兵数が正しくありません', AlreadyResearched: '研究済みです',
      NotAtPeace: '平和な関係のときだけできます', AlreadyAtWar: 'すでに戦争中です', NotAtWar: '戦争中ではありません',
      UnderNap: '不可侵条約中です。宣戦するには先に条約を破棄します', Allied: '同盟国には宣戦できません', NoProposal: '申し入れがありません',
      AlreadyInAlliance: 'すでに同盟に加わっています', AllianceLeaving: '離脱手続き中の同盟には加われません',
      CivilianCannotAttack: '非戦闘ユニットは攻撃できません', TargetProtected: '保護区域内の相手は攻撃できません',
      TargetNotHostile: '戦争中の相手ではありません。先に宣戦が必要です', TargetGone: '目標がもういません',
      Frozen: '終盤のため取引は凍結中です', NothingToSell: '売れる在庫がありません',
      WrongStandingTarget: 'この対象には使えない継続命令です',
      BatchRejected: 'この役職の命令全体が受け付けられませんでした（枠・役職・献策の採用を確認）', WrongOffice: '担当外の役職の命令です',
      NeedsConsent: '宣戦には将軍か内政官（外交官とは別の人）の同意が必要です', AlreadyPurchased: '購入は1都市1ティックに1回までです',
      CannotBuyStarGate: 'スターゲートはお金で買えません', NothingQueued: '生産予定がありません', NotEnoughInfluence: '影響力が足りません',
      NoCounterparty: '自国・交戦中の国とは取引できません', NeedsSpendConsent: '一定額を超える支出には別の役職者の同意が必要です',
      NotEnoughUsdc: '国庫のUSDCが足りません',
    })[b.code] || b.code;
  }
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

/** Chronicle lines come from the server as `kind|English text`; translate the common forms. */
export function chronicleText(line) {
  const [kind, text = ''] = line.split('|');
  const n = s => civName(s.trim());
  let m;
  if ((m = text.match(/^(\w+) declares war on (\w+)(.*)$/))) return [kind, `${n(m[1])}が${n(m[2])}に宣戦${m[3].includes('pact') ? '（条約破棄）' : m[3].includes('casus') ? '（正当な理由あり）' : ''}`];
  if ((m = text.match(/^(\w+) and (\w+) make peace$/))) return [kind, `${n(m[1])}と${n(m[2])}が講和`];
  if ((m = text.match(/^(\w+) and (\w+) form an alliance$/))) return [kind, `${n(m[1])}と${n(m[2])}が同盟を結成`];
  if ((m = text.match(/^(\w+) and (\w+) sign a non-aggression pact$/))) return [kind, `${n(m[1])}と${n(m[2])}が不可侵条約を締結`];
  if ((m = text.match(/^The pact between (\w+) and (\w+) expires$/))) return [kind, `${n(m[1])}と${n(m[2])}の不可侵条約が満了`];
  if ((m = text.match(/^(\w+) and (\w+) end their alliance$/))) return [kind, `${n(m[1])}と${n(m[2])}の同盟が解消`];
  if ((m = text.match(/^(\w+) founds a new city$/))) return [kind, `${n(m[1])}が新しい都市を建設`];
  if ((m = text.match(/^(\w+) captures a city of (\w+)$/))) return [kind, `${n(m[1])}が${n(m[2])}の都市を占領`];
  if ((m = text.match(/^(\w+) captures a free city$/))) return [kind, `${n(m[1])}が自由都市を占領`];
  if ((m = text.match(/^(\w+) conquers a city-state$/))) return [kind, `${n(m[1])}が都市国家を征服`];
  if ((m = text.match(/^A city of (\w+) revolts and becomes free$/))) return [kind, `${n(m[1])}の都市が反乱し自由都市に`];
  if (text === 'A city is razed to a ruin') return [kind, '都市が破壊され遺跡になった'];
  if ((m = text.match(/^(\w+) completes Star Gate stage (\d)$/))) return [kind, `${n(m[1])}がスターゲート第${m[2]}段階を完成`];
  if ((m = text.match(/^(\w+) becomes suzerain of city-state (\d+)$/))) return [kind, `${n(m[1])}が都市国家${m[2]}の宗主に`];
  if ((m = text.match(/^(\w+) discovers (\w+)$/))) return [kind, `${n(m[1])}が「${TECH[m[2]] || m[2]}」を発見`];
  if ((m = text.match(/^(\w+): (.+) becomes (general|steward|science officer|diplomat) \(was (.+)\)$/))) return [kind, `${n(m[1])}：${actor(m[2])}が${ROLE_NAME[m[3]]}に${kind === 'recall' ? '（解任による交代）' : ''}（前任 ${actor(m[4])}）`];
  if ((m = text.match(/^First election — (\w+): (.+)$/))) return [kind, `${n(m[1])}の第1回選挙：${m[2].split(', ').map(x => { const r = Object.keys(ROLE_NAME).find(k => x.startsWith(`${k} `)); return r ? `${ROLE_NAME[r]} ${actor(x.slice(r.length + 1))}` : x; }).join('・')}`];
  if ((m = text.match(/^(\w+) adopts (\d+) proposals?$/))) return [kind, `${n(m[1])}が献策を${m[2]}件採用`];
  if ((m = text.match(/^(\w+) reaches (\w+) (\d)$/))) return [kind, `${n(m[1])}が${PATH_NAME[m[2]]}の第${m[3]}段階に到達`];
  if ((m = text.match(/^(\w+) loses (\w+) (\d)$/))) return [kind, `${n(m[1])}が${PATH_NAME[m[2]]}の第${m[3]}段階を失った`];
  if ((m = text.match(/^(\w+) enters era (\d)$/))) return [kind, `${n(m[1])}が第${m[2]}時代に入った`];
  return [kind, text];
}
export const KIND_GLYPH = { war: '⚔', capture: '⚑', raze: '✕', peace: '☮', ally: '⚭', diplo: '✉', science: '✦', revolt: '!', found: '⌂', tech: '✧', gov: '⚖', recall: '⚠', milestone: '◆', era: '✺' };
// Chronicle lines name offices and paths in English.
const ROLE_NAME = { general: ROLE_JA.General, steward: ROLE_JA.Steward, 'science officer': ROLE_JA.Science, diplomat: ROLE_JA.Diplomat };
const PATH_NAME = Object.fromEntries(PATHS.map((p, i) => [p, PATH_JA[i]]));
const actor = s => (s === 'the acting official' || s === 'acting' ? '代行' : s);
/** What earned merit (engine event names, V5 §7.3). */
export const MERIT_WHAT = { office: '役職者として活動', gold: '都市が稼いだ金（内政官）', growth: '人口の成長', building: '建物の完成', found_city: '都市の建設', tech: '技術の完成',
  star_gate: 'スターゲートの段階', capture: '都市の占領', troops: '敵部隊への損害', held: '攻撃に耐えた都市（将軍）', treaty: '条約の成立', suzerain: '都市国家の宗主', trade: '交易' };

// Standing rules (§13)
export const STANDING_GLYPH = { AutoDefend: '⛨', Retreat: '↩', Patrol: '⟳', QueueRepeat: '↻', AutoPurchase: '◆' };
export function standingText(r) {
  if (!r) return '';
  switch (r.kind) {
    case 'AutoDefend': return `自動防衛 · 半径${r.radius}`;
    case 'Retreat': return `撤退 · 相手が${(r.ratioBps / 10000).toFixed(1)}倍を超えたら`;
    case 'Patrol': return `巡回 · ${r.route.length}地点`;
    case 'QueueRepeat': return r.on ? '生産の繰り返し ON' : '生産の繰り返し OFF';
    case 'AutoPurchase': return r.maxGold ? `自動購入 · 毎ティック${r.maxGold}金まで` : '自動購入 OFF';
    case 'Clear': return '継続命令を解除';
    default: return r.kind;
  }
}
