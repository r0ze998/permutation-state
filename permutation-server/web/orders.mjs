// This tick's orders: drafting, costs, labels, sealing (commit) and ending
// the turn. Orders of offices I hold are sealed and sent; orders of other
// offices become proposals (献策) to their officers (V5 §5.4).
import * as T from './i18n.mjs';
import * as api from './api.mjs';
import { $, toast, usdc, usdcFixed } from './util.mjs';
import { S, civN, held, spendable, unitById, cityById, isWatching, invalidate } from './state.mjs';
import { map, keyOf, key } from './world.mjs';
import { poll } from './sync.mjs';

// ================================================================== costs and offices
const FREE = new Set(['ExchangeOrder', 'RevealRationale', 'ConsentWar', 'ConsentSpend']);
export const costOf = dto => (FREE.has(dto.type) ? 0 : 1);
/** Slots my drafts use, for one office or all. */
export const used = role => S.drafts.filter(d => !role || d.office === role).reduce((a, d) => a + costOf(d.dto), 0);

/** The office an order belongs to (V5 §5.1); settlers are the steward's. */
export function officeOf(dto) {
  if (dto.type === 'MoveUnit' || (dto.type === 'SetStanding' && dto.target.kind === 'Unit')) {
    const id = dto.type === 'MoveUnit' ? dto.unit : dto.target.id;
    return unitById(id)?.type === 'Settler' ? 'Steward' : 'General';
  }
  return ({ Attack: 'General', Raze: 'General', FoundCity: 'Steward', SetQueue: 'Steward', SetFocus: 'Steward', Purchase: 'Steward', SetStanding: 'Steward',
    SetResearch: 'Science', ConsentWar: 'General', ConsentSpend: 'General' })[dto.type] || 'Diplomat';
}

/** Identity of the slot an order occupies: one order per unit, per city field, per diplomatic target. */
export function slotOf(dto) {
  switch (dto.type) {
    case 'MoveUnit': case 'FoundCity': return `u${dto.unit ?? dto.settler}`;
    case 'Attack': return `u${dto.army}`;
    case 'SetQueue': return `q${dto.city}`; case 'SetFocus': return `f${dto.city}`; case 'Purchase': return `p${dto.city}`;
    case 'SetResearch': return 'research';
    case 'SendEnvoy': return `e${dto.cityState}`;
    case 'SetStanding': return dto.target.kind === 'Unit' ? `u${dto.target.id}` : `cs${dto.target.id}:${dto.rule.kind === 'Clear' ? 'all' : dto.rule.kind}`;
    case 'MarketTrade': case 'ExchangeOrder': return `m${Math.random()}`;
    default: return `${dto.type}${dto.civ ?? ''}`;
  }
}
/** Units with an order drafted this tick. */
export const draftedUnits = () => new Set(S.drafts.map(d => d.dto.unit ?? d.dto.army ?? d.dto.settler).filter(x => x !== undefined));

// ================================================================== labels
/** Human label, glyph, office and map hints for an order DTO. */
export function describe(dto) {
  const d = describeOrder(dto);
  d.office = officeOf(dto);
  d.proposal = S.view ? !held().includes(d.office) : false;
  return d;
}
/** Label (plain text), glyph and map hints of an order. */
export function describeOrder(dto) {
  const u = unitById, city = cityById;
  const unitName = x => T.UNIT[x?.type] || '部隊';
  switch (dto.type) {
    case 'MoveUnit': { const x = u(dto.unit); const last = dto.path[dto.path.length - 1];
      return { dto, glyph: '→', label: `${unitName(x)}を移動（${dto.path.length}マス）`, focus: key(last[0], last[1]), from: x, path: dto.path }; }
    case 'Attack': { const x = u(dto.army); const t = dto.target; let pos = null, what = '';
      if (t.kind === 'Unit') { const y = u(t.id); pos = y; what = `${civN(y?.owner)}の${unitName(y)}`; }
      if (t.kind === 'City') { pos = city(t.id); what = T.cityName(t.id); }
      if (t.kind === 'CityState') { pos = S.view.cityStates.find(c => c.id === t.id); what = `都市国家${t.id + 1}`; }
      return { dto, glyph: '⚔', label: `${unitName(x)}で${what}を攻撃`, focus: keyOf(pos), from: x, attackAt: pos }; }
    case 'FoundCity': return { dto, glyph: '⌂', label: '開拓者が都市を建設', focus: keyOf(u(dto.settler)) };
    case 'SetQueue': return { dto, glyph: '▤', label: `${T.cityName(dto.city)}：${dto.items.map(T.itemName).join(' → ') || '生産なし'}`, focus: keyOf(city(dto.city)) };
    case 'SetFocus': return { dto, glyph: '◐', label: `${T.cityName(dto.city)}の方針：${T.FOCUS[dto.focus]}`, focus: keyOf(city(dto.city)) };
    case 'Purchase': return { dto, glyph: '◆', label: `${T.cityName(dto.city)}で生産を購入（${dto.gold}金）`, focus: keyOf(city(dto.city)) };
    case 'SetResearch': return { dto, glyph: '✧', label: `研究：${dto.techs.map(t => T.TECH[t]).join(' → ')}` };
    case 'SetStanding': {
      const unit = dto.target.kind === 'Unit';
      const who = unit ? unitName(u(dto.target.id)) : T.cityName(dto.target.id);
      return { dto, glyph: T.STANDING_GLYPH[dto.rule.kind] || '⚙', label: `${who}：${T.standingText(dto.rule)}`, focus: keyOf(unit ? u(dto.target.id) : city(dto.target.id)) };
    }
    case 'DeclareWar': return { dto, glyph: '⚔', label: `${civN(dto.civ)}に宣戦` };
    case 'ProposePeace': return { dto, glyph: '☮', label: `${civN(dto.civ)}に講和を申し入れ` };
    case 'AcceptPeace': return { dto, glyph: '☮', label: `${civN(dto.civ)}の講和を受諾` };
    case 'ProposeNap': return { dto, glyph: '✉', label: `${civN(dto.civ)}に不可侵条約（保証金${dto.bond}）` };
    case 'AcceptNap': return { dto, glyph: '✉', label: `${civN(dto.civ)}の不可侵条約を受諾` };
    case 'BreakNap': return { dto, glyph: '✕', label: `${civN(dto.civ)}との条約を破棄（宣戦）` };
    case 'ProposeAlliance': return { dto, glyph: '⚭', label: `${civN(dto.civ)}に同盟を申し入れ` };
    case 'AcceptAlliance': return { dto, glyph: '⚭', label: `${civN(dto.civ)}の同盟に加わる` };
    case 'LeaveAlliance': return { dto, glyph: '⚭', label: '同盟から離脱' };
    case 'SendEnvoy': return { dto, glyph: '✉', label: `都市国家${dto.cityState + 1}に使節（影響力${dto.influence}）`, focus: keyOf(S.view.cityStates.find(c => c.id === dto.cityState)) };
    case 'MarketTrade': return { dto, glyph: '⇄', label: `金の市場：${T.goodName(dto.good)}を${dto.amount}${dto.side === 'Buy' ? '購入' : '売却'}` };
    case 'ExchangeOrder': return { dto, glyph: '$', usdc: true, label: `USDC取引所：${T.goodName(dto.good)}${dto.amount}を${dto.side === 'Buy' ? '買い' : '売り'} @${usdcFixed(dto.price)}` };
    case 'Raze': return { dto, glyph: '✕', label: `${T.cityName(dto.city)}を破壊` };
    case 'ConsentWar': return { dto, glyph: '⚖', label: `${civN(dto.civ)}への宣戦に同意` };
    case 'ConsentSpend': return { dto, glyph: '⚖', label: `国庫から${usdc(dto.usdc)} USDCまでの支出に同意` };
    default: return { dto, glyph: '•', label: dto.type };
  }
}

// ================================================================== drafts
export const rationaleText = () => ($('#rationale')?.value || '').trim();
const draftsJson = list => JSON.stringify(list.map(d => d.dto));
/** Proposals to adopt, normalized (offices with an empty list do not count). */
const adoptJson = () => JSON.stringify(Object.entries(S.adopt).filter(([, ids]) => ids.length).map(([r, ids]) => [r, [...ids].sort((a, b) => a - b)]).sort());
/** Something changed since the last seal: drafts, the rationale, or proposals to adopt. */
export function isDirty() {
  return draftsJson(S.drafts) !== S.committedJson || rationaleText() !== S.committedRationale || adoptJson() !== S.committedAdopt;
}

function setDrafts(list) {
  S.drafts = list;
  map.setDrafts(list);
  invalidate('top', 'dock', 'inspector', 'drawer', 'nextTurn', 'notifs');
}

/** Orders sealed earlier this tick, restored after a reload. */
export function restoreCommitted(orders) {
  setDrafts(orders.map(describe));
  S.committedJson = draftsJson(S.drafts);
}

/** A new tick: nothing is drafted or sealed yet. */
export function resetTick() {
  S.adopt = {};
  setDrafts([]);
  S.committedJson = draftsJson([]);
  S.committedAdopt = adoptJson();
  S.committedRationale = '';
  $('#rationale').value = '';
}

export function addDraft(dto) {
  if (isWatching()) { toast('観戦中は命令を出せません。', 'error'); return false; }
  const d = describe(dto);
  const slot = slotOf(dto);
  const others = S.drafts.filter(x => slotOf(x.dto) !== slot);
  const replacing = others.length !== S.drafts.length;
  const mine = held().includes(d.office);
  if (mine && !replacing && used(d.office) + costOf(dto) > spendable(d.office)) {
    toast(`${T.ROLE_JA[d.office]}の命令の枠が足りません（${used(d.office)}/${spendable(d.office)}）。先に他の命令を取り消してください。`, 'error'); return false;
  }
  setDrafts([...others, d]);
  S.pulseChip = S.drafts.length - 1;
  if (!mine && !S.hintedPropose) { S.hintedPropose = true; toast(`${T.ROLE_JA[d.office]}はあなたの担当ではないので、この命令は「献策」になります。確定すると${T.ROLE_JA[d.office]}に提案され、採用されれば功績を半分ずつ分けます。`); }
  else if (!S.hintedDock) { S.hintedDock = true; toast('命令は下のドックに貯まります。「確定する」（Ctrl+Enter）で封印して送信、締切3秒前には自動で確定します。'); }
  return true;
}
export function removeDraft(i) { setDrafts(S.drafts.filter((_, j) => j !== i)); }

/** Adopt (or un-adopt) a proposal to one of my offices; sent with the next commit. */
export function toggleAdopt(role, id) {
  const list = S.adopt[role] ||= [];
  const i = list.indexOf(id);
  if (i >= 0) list.splice(i, 1); else list.push(id);
  invalidate('drawer', 'dock', 'nextTurn', 'notifs');
}

// ================================================================== sealing
/**
 * Seal and send the orders of the offices held, and turn the rest into
 * proposals to their offices (one proposal per office, V5 §5.4).
 */
export async function commit(auto = false) {
  if (S.committing) return;
  S.committing = true;
  const mine = S.drafts.filter(d => !d.proposal), props = S.drafts.filter(d => d.proposal);
  const orders = mine.map(d => d.dto);
  const rationale = rationaleText();
  const adopt = {};
  for (const [role, ids] of Object.entries(S.adopt)) if (ids.length && held().includes(role)) adopt[role] = ids;
  const adoptSent = adoptJson();
  let ok = true, msg = [];
  // Proposals first: once every office's batch is in, the tick closes and
  // later governance actions wait for the next tick.
  const byRole = {};
  for (const d of props) (byRole[d.office] ||= []).push(d.dto);
  for (const [role, list] of Object.entries(byRole)) {
    for (let i = 0; i < list.length; i += 4) {
      const batch = list.slice(i, i + 4);
      const res = await api.post('/api/gov', { action: { type: 'Propose', role, orders: batch } });
      if (res.ok) msg.push(`${T.ROLE_JA[role]}に献策（${batch.length}件）`);
      else { ok = false; toast(`献策できませんでした：${api.translateError(res.error)}`, 'error'); }
    }
  }
  if (held().length) {
    const res = await api.post('/api/orders', { orders, rationale, adopt });
    const nAdopt = Object.values(adopt).reduce((n, l) => n + l.length, 0);
    if (res.ok) {
      S.sealed = res.offices;
      msg.push(orders.length ? `命令${orders.length}件を封印して確定（${res.offices.map(o => T.ROLE_JA[o.role]).join('・')}）` : nAdopt ? '' : '担当の命令なしで確定');
      if (nAdopt) msg.push(`献策${nAdopt}件の採用を確定`);
      msg = msg.filter(Boolean);
    } else { ok = false; toast(`確定できませんでした：${api.translateError(res.error)}`, 'error'); }
  }
  S.committing = false;
  if (ok) {
    S.committedRationale = rationale;
    S.committedAdopt = adoptSent;
    if (props.length) setDrafts(S.drafts.filter(d => !props.includes(d))); // proposals are sent: they leave the dock
    S.committedJson = JSON.stringify(orders); // what was sealed (drafts added meanwhile stay unsent)
    if (!auto) toast(`${msg.join(' / ') || '確定しました'}。締切（全役職の確定）まで変更できます。`);
    else if (S.drafts.length) toast('締切が近いため、下書きを自動で確定しました。');
  }
  invalidate('dock');
}

/**
 * End this member's turn now: every office held seals what it has (possibly
 * nothing). With commitFirst, unsent drafts are committed before.
 */
export async function endTurn({ commitFirst = false } = {}) {
  if (commitFirst && isDirty()) await commit(true);
  const r = await api.post('/api/control', { advance: true });
  if (r.error) toast(`${commitFirst ? '送信できませんでした' : '手番を終えられませんでした'}：${api.translateError(r.error)}`, 'error');
  else if (!commitFirst) toast('命令なしで手番を終えました。');
  poll();
}

/** The dock's button: seal the drafts, or end the turn when there is nothing to seal. */
export function commitOrEndTurn() {
  if (!isDirty() && held().length && !S.view.member?.ready) endTurn();
  else commit();
}
