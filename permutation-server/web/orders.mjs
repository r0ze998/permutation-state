// This tick's orders: drafting, costs, labels, sealing (commit) and ending
// the turn. Orders of offices I hold are sealed and sent; orders of other
// offices become proposals (献策) to their officers (V5 §5.4).
//
// Local mode: the play server seals them (/api/orders, /api/gov,
// /api/control). Chain mode: the play server only checks the drafts
// (/api/validate); this browser seals each office's batch, hands it to the
// gateway to reveal and commits it on chain with its session key, and sends
// proposals and other governance as SubmitGov (chainplay.mjs).
import * as T from './i18n.mjs';
import * as api from './api.mjs';
import { $, toast, usdc, usdcFixed } from './util.mjs';
import { S, civN, held, spendable, unitById, cityById, isWatching, invalidate } from './state.mjs';
import { map, keyOf, key } from './world.mjs';
import { poll } from './sync.mjs';
import * as chainplay from './chainplay.mjs';
import { AUTO_RETRY_MS, autoCommitSeconds, autoCommitSettled, autoCommitStarted, autoCommitStep, checkValidation, retryable } from './sealbook.mjs';
import { AUTO_COMMIT_SECONDS } from './rules.mjs';

/** Chain mode, after a tick's deadline: nothing more is taken until the next tick. */
const CLOSED = 'このティックの受付は締め切られました（封印を公開して解決しています）。次のティックで出してください。';

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

/**
 * The first view after a (re)load: show again what was sealed earlier this
 * tick — the play server's copy (local mode) or this browser's own (chain
 * mode: orders, proposals adopted and the rationale).
 */
export function restoreTick(v) {
  if (!v.chain) {
    const orders = (v.member?.committed || []).flatMap(c => c.orders);
    if (orders.length) restoreCommitted(orders);
    return;
  }
  const r = chainplay.restore(v);
  if (!r) return;
  if (r.orders.length) restoreCommitted(r.orders);
  S.adopt = Object.fromEntries(Object.entries(r.adopt).map(([role, ids]) => [role, [...ids]]));
  S.committedAdopt = adoptJson();
  S.committedRationale = r.rationale;
  const el = $('#rationale');
  if (el) el.value = r.rationale;
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

/** Seconds before the deadline at which a dirty draft is sealed automatically. */
export const autoSeconds = (v = S.view) => (v?.chain ? autoCommitSeconds(v.tickSeconds) : AUTO_COMMIT_SECONDS);

export function addDraft(dto) {
  if (isWatching()) { toast('観戦中は命令を出せません。', 'error'); return false; }
  if (S.view?.chain) {
    if (!S.session) { toast(chainplay.NO_KEY, 'error'); return false; }
    if (chainplay.chainPhase() !== 'commit') { toast(CLOSED, 'error'); return false; }
  }
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
  else if (!S.hintedDock) {
    S.hintedDock = true;
    toast(S.view?.chain
      ? `命令は下のドックに貯まります。「確定する」（Ctrl+Enter）で封印してチェーンへ送ります。締切の約${Math.round(autoSeconds())}秒前には自動で確定します（このタブを表示している間）。`
      : '命令は下のドックに貯まります。「確定する」（Ctrl+Enter）で封印して送信、締切3秒前には自動で確定します。');
  }
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
 * proposals to their offices (one proposal per office, V5 §5.4). Chain
 * mode resolves `{ok, retry, errors}` (commitChain); with `quiet`, its
 * error messages are returned in `errors` instead of shown.
 */
export async function commit(auto = false, { quiet = false } = {}) {
  if (S.committing) return undefined;
  if (S.view?.chain) return commitChain(auto, quiet);
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
  if (S.view?.chain) return endTurnChain({ commitFirst });
  if (commitFirst && isDirty()) await commit(true);
  const r = await api.post('/api/control', { advance: true });
  if (r.error) toast(`${commitFirst ? '送信できませんでした' : '手番を終えられませんでした'}：${api.translateError(r.error)}`, 'error');
  else if (!commitFirst) toast('命令なしで手番を終えました。');
  poll();
}

/** This member's turn is over for the open tick: every office it holds has sealed (possibly nothing). */
export const turnEnded = () => (S.view?.chain ? chainplay.turnEnded() : !!S.view?.member?.ready);

/** The dock's button: seal the drafts, or end the turn when there is nothing to seal. */
export function commitOrEndTurn() {
  if (!isDirty() && held().length && !turnEnded()) endTurn();
  else commit();
}

/** Shown once when an auto-commit failed in a way that may pass and is tried again. */
const AUTO_RETRY_NOTE = `下書きの自動確定が一時的なエラーで届きませんでした。${Math.round(AUTO_RETRY_MS / 1000)}秒ほどでもう一度送ります。`;

/** Every applied view: auto-commit a dirty draft just before the deadline (never lose orders silently). */
export function maybeAutoCommit(v) {
  if (v.paused) return;
  if (!v.chain) {
    if (!S.committing && isDirty() && v.secondsLeft < AUTO_COMMIT_SECONDS) commit(true);
    return;
  }
  // Chain mode (sealbook.mjs autoCommitStep): early enough for the seal and
  // the commitment to land, only while commitments are open, once the drafts
  // have settled, once per draft state (the rationale alone does not count:
  // typing a memo must not use up the office's commits), again after a
  // failure that may pass, and never past an office's commit cap.
  if (!chainplay.inChain() || !S.session) return;
  const a = (S.autoCommit ??= {});
  const key = `${v.tick}|${draftsJson(S.drafts)}|${adoptJson()}`;
  const { fire, report } = autoCommitStep(a, {
    key, now: Date.now(), dirty: isDirty(), busy: S.committing, phase: chainplay.chainPhase(v), secondsLeft: v.secondsLeft,
    window: autoSeconds(v), counts: chainplay.officeStates().map(s => s.count),
  });
  for (const e of report ?? []) toast(e, 'error');
  if (!fire) return;
  autoCommitStarted(a);
  const settle = out => {
    const r = autoCommitSettled(a, key, out, Date.now());
    if (r === 'retry' && a.attempts === 1) toast(AUTO_RETRY_NOTE);
    else if (r === 'failed') for (const e of out?.errors ?? []) toast(e, 'error');
  };
  commit(true, { quiet: true }).then(settle, () => settle({ ok: false, retry: false, errors: [] }));
}

// ================================================================== chain mode
/** Every applied view in chain mode: keep in step with the chain, send governance that waited for this phase. */
export function afterView(v) {
  if (v.chain) {
    chainplay.onView(v);
    if (chainplay.inChain() && S.session && chainplay.chainPhase(v) === 'commit' && S.govQueue.length && !flushing && !S.committing) flushGov();
  }
  maybeAutoCommit(v);
}

let flushing = false;
async function flushGov() {
  flushing = true;
  try { reportGov(await chainplay.submitGov()); } finally { flushing = false; }
}

export const GOV_VERB = { Vote: '投票', Support: '支持', Stand: '立候補', Recall: 'リコールに賛成', Propose: '献策' };
const verbs = list => [...new Set(list.map(a => GOV_VERB[a.type] || a.type))].join('・');
const govError = f => (f.code === 'InboxFull' ? 'このティックに送れる国の操作の上限に達しました。次のティックで送ってください' : api.translateError(f));

const showError = text => toast(text, 'error');

/**
 * Say what became of governance actions sent on chain (chainplay.mjs
 * submitGov's answer); the ones that went out are left to the caller for
 * proposals when `proposals` is false; failures go to `say`. Returns
 * whether nothing failed.
 */
export function reportGov(r, { proposals = true, say = showError } = {}) {
  for (const a of [...r.sent, ...(r.requeued || [])]) if (a.type === 'Vote') S.myVotes[a.role] = a.candidate;
  const sent = r.sent.filter(a => proposals || a.type !== 'Propose'), late = r.requeued || [];
  if (sent.length) toast(`${verbs(sent)}をチェーンに送りました。次のティックの解決で反映されます。`);
  if (late.length) toast(`締切を過ぎたため、${verbs(late)}は次のティックの受付が始まったら送ります。`);
  for (const f of r.failed) say(`${verbs(f.actions)}できませんでした：${govError(f)}`);
  if (r.tooLarge?.length) say(`${verbs(r.tooLarge)}が大きすぎて1つの取引に収まりません。命令を減らしてください。`);
  invalidate('drawer', 'nextTurn', 'notifs');
  return !r.failed.length && !r.tooLarge?.length;
}

/** One message for office results that failed (a shared reason once), to `say`. */
function reportOffices(failed, what = '確定できませんでした', say = showError) {
  if (!failed.length) return;
  const same = failed.every(r => r.code === failed[0].code && r.stage === null);
  if (same) { say(`${what}：${api.translateError(failed[0])}`); return; }
  for (const r of failed) {
    const where = r.stage === 'seal' ? '封印をゲートウェイに預けられず、送っていません' : r.stage === 'commit' ? 'チェーンに送れませんでした' : '';
    say(`${what}（${T.ROLE_JA[r.role]}${where ? `・${where}` : ''}）：${api.translateError(r)}`);
  }
}

/**
 * Chain mode: check my offices' drafts with the play server, then seal and
 * commit every office's batch and send the proposals, all at once. Nothing
 * is sent when the drafts do not pass (or the play server's answer does
 * not match what was sent). Resolves `{ok, retry, errors}`: `retry` when
 * every failure may pass if sent again (sealbook.mjs retryable); `errors`
 * the error messages, shown here unless `quiet`.
 */
async function commitChain(auto, quiet = false) {
  const v = S.view;
  const errors = [];
  const say = text => { errors.push(text); if (!quiet) toast(text, 'error'); };
  const result = (ok, retry = false) => ({ ok, retry: !ok && retry, errors });
  if (!S.session) { if (!auto) say(chainplay.NO_KEY); return result(false); }
  if (v.over || chainplay.chainPhase(v) !== 'commit') { if (!auto) say(CLOSED); return result(false); }
  S.committing = true;
  invalidate('dock', 'nextTurn');
  try {
    const tick = v.tick, offices = held();
    const mine = S.drafts.filter(d => !d.proposal), props = S.drafts.filter(d => d.proposal);
    const orders = mine.map(d => d.dto);
    const rationale = rationaleText();
    const adopt = {};
    for (const [role, ids] of Object.entries(S.adopt)) if (ids.length && offices.includes(role)) adopt[role] = [...ids];
    const adoptSent = adoptJson();
    const nAdopt = Object.values(adopt).reduce((n, l) => n + l.length, 0);
    // 1. My offices' drafts, checked as the engine will check each batch.
    let plans = [];
    if (offices.length) {
      const res = await api.post('/api/validate', { member: S.memberId, orders, adopt });
      if (!Array.isArray(res?.offices)) {
        say(`確定できませんでした：${api.translateError(res?.error ?? 'network')}`);
        return result(false, retryable(res ?? { error: 'network' }));
      }
      const chk = checkValidation(res, { orders, adopt, tick });
      if (chk.problem) {
        say(chk.problem === 'tick' ? 'ティックが進みました。もう一度確定してください。' : 'ゲームサーバーの確認結果が送った命令と合わないため、何も送っていません。');
        return result(false);
      }
      if (chk.refused.length) {
        say(`担当の役職の命令として受け付けられない命令があります（何も送っていません）：${chk.refused.map(r => describeOrder(r.order).label).join('、')}`);
        return result(false);
      }
      const bad = chk.offices.find(o => o.error);
      if (bad) { say(`確定できませんでした（${T.ROLE_JA[bad.role]}）：${api.translateError(bad.error)}`); return result(false); }
      plans = chk.offices.map(o => ({ role: o.role, orders: o.orders, adopt: o.adopt }));
    }
    // 2. Proposals to the offices I do not hold, as SubmitGov (with any
    // governance that waited for this phase), and my offices' batches.
    const byRole = {};
    for (const d of props) (byRole[d.office] ||= []).push(d.dto);
    const prop = chainplay.proposalActions(byRole);
    const [results, gov] = await Promise.all([
      plans.length ? chainplay.commitOffices(plans, { rationale, tick }) : [],
      prop.actions.length || S.govQueue.length ? chainplay.submitGov(prop.actions) : null,
    ]);
    S.sealed = results;
    const done = results.filter(r => r.ok), failed = results.filter(r => !r.ok);
    reportOffices(failed, undefined, say);
    const msg = [];
    if (done.length) msg.push(orders.length ? `命令${orders.length}件を封印して確定（${done.map(r => T.ROLE_JA[r.role]).join('・')}）` : nAdopt ? '' : '担当の命令なしで確定');
    if (nAdopt && done.length && !failed.length) msg.push(`献策${nAdopt}件の採用を確定`);
    // Proposals sent (or waiting for the next tick) leave the dock.
    let govOk = true;
    if (gov) {
      govOk = reportGov({ ...gov, tooLarge: [...gov.tooLarge, ...prop.tooLarge] }, { proposals: false, say });
      const out = [...gov.sent, ...gov.requeued].filter(a => a.type === 'Propose');
      const gone = new Set(out.flatMap(a => a.orders));
      if (gone.size) setDrafts(S.drafts.filter(d => !(d.proposal && gone.has(d.dto))));
      const now = gov.sent.filter(a => a.type === 'Propose');
      if (now.length) msg.push(`${[...new Set(now.map(a => T.ROLE_JA[a.role]))].join('・')}に献策（${now.reduce((n, a) => n + a.orders.length, 0)}件）`);
    } else if (prop.tooLarge.length) {
      govOk = false;
      say('献策が大きすぎて1つの取引に収まりません。命令を減らしてください。');
    }
    const ok = !failed.length && govOk;
    if (ok) {
      S.committedRationale = rationale;
      S.committedAdopt = adoptSent;
      S.committedJson = JSON.stringify(orders);
      if (!auto) toast(`${msg.filter(Boolean).join(' / ') || '確定しました'}。締切まで確定し直せます。`);
      else if (S.drafts.length) toast('締切が近いため、下書きを封印してチェーンへ送りました。');
    }
    if (results.length) chainplay.refreshFlags();
    // Worth sending again only when everything that failed may pass.
    const tooLarge = !!(gov?.tooLarge?.length || prop.tooLarge.length);
    return result(ok, !tooLarge && failed.every(retryable) && (gov?.failed ?? []).every(retryable));
  } finally {
    S.committing = false;
    invalidate('dock', 'nextTurn');
  }
}

/**
 * Chain mode: end this member's turn — an empty sealed batch for every
 * office held that has sent nothing this tick. With commitFirst, unsent
 * drafts are committed before (and nothing more happens if that fails).
 */
async function endTurnChain({ commitFirst }) {
  if (!S.session) { toast(chainplay.NO_KEY, 'error'); return; }
  if (chainplay.chainPhase() !== 'commit') { toast(CLOSED, 'error'); return; }
  if (commitFirst && isDirty()) {
    await commit(true);
    if (isDirty()) return; // the commit failed and said why
  }
  const tick = S.view.tick;
  const todo = chainplay.officeStates().filter(s => !s.committed && !s.sending).map(s => s.role);
  if (!todo.length || S.committing) { if (!commitFirst && !S.committing) toast('すでに手番を終えています（締切までは確定し直せます）。'); poll(); return; }
  S.committing = true;
  invalidate('dock', 'nextTurn');
  let results = [];
  try {
    results = await chainplay.commitOffices(todo.map(role => ({ role, orders: [], adopt: [] })), { rationale: rationaleText(), tick });
  } finally {
    S.committing = false;
    invalidate('dock', 'nextTurn');
  }
  const failed = results.filter(r => !r.ok);
  reportOffices(failed, commitFirst ? '送信できませんでした' : '手番を終えられませんでした');
  if (!failed.length && !commitFirst) toast('命令なしで手番を終えました。');
  chainplay.refreshFlags();
  poll();
}
