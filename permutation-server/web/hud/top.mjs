// Top of the screen: member chip, resources, clock, the nation plate with
// its path tracker, and the leader ribbon (every nation).
import * as T from '../i18n.mjs';
import { $, html, setHtml, fmt, usdc } from '../util.mjs';
import { S, civN, myNation, held, membersOf, isWatching, secondsLeft, aiRoster } from '../state.mjs';
import { SUZERAIN_MIN, TECH_COUNT } from '../rules.mjs';
import { poolUsdc } from '../chain.mjs';
import { autoSeconds } from '../orders.mjs';
import { chainPhase } from '../chainplay.mjs';
import { mountLangToggle, langToggleHtml, L, lang, lazyTable } from '../lang.mjs';

/** The language toggle as markup, for screens outside the top bar (lobby, registration, spectator). */
export { langToggleHtml };

export function renderTop() {
  const v = S.view, e = v.economy;
  const offices = held();
  const badges = isWatching() ? html`<span class="badge">${L`観戦`}</span>`
    : offices.length ? offices.map(r => html`<span class="badge office">${T.ROLE_GLYPH[r]} ${T.ROLE_JA[r]}</span>`) : html`<span class="badge human">${L`メンバー`}</span>`;
  setHtml($('#civ-chip'), html`<span class="swatch" style="background:${T.CIV_COLORS[S.myCiv]}"></span><div><b>${v.member?.name ?? civN(S.myCiv)}</b><small class="nation">${civN(S.myCiv)}</small><div>${badges}</div></div>`);
  const ms = v.members || [], ai = aiRoster(v).aiCount;
  $('.prototype-label').textContent = L`${v.chain ? L`オンチェーン · MagicBlock ER` : L`ローカル`} · メンバー${ms.length}人${ai ? L`（うち運営のAI ${ai}人）` : ''} · 全員が同じ情報で判断 · 命令は締め切りまで封印 · USDCはテスト用`;
  $('#season-label').textContent = `ONE CIVILIZATION · SEASON ${v.chain?.seasonId ?? 0} · ${String(v.season?.preset ?? 'Blitz').toUpperCase()}`;
  if (e) renderResources(e);
  const pause = $('#pause-btn');
  pause.hidden = !!v.chain;
  pause.classList.toggle('on', v.paused);
  pause.textContent = v.paused ? '▶' : '❚❚';
  pause.title = v.paused ? L`時計を動かす` : L`時計を止める（このPCだけの試作）`;
  mountLangToggle($('.top-actions')); // "EN" while Japanese is shown, "日本語" while English is
  renderClock();
}

function renderResources(e) {
  const upkeep = e.upkeepUnits + e.upkeepCities;
  const net = e.goldIncome - upkeep;
  const signed = n => `${n >= 0 ? '+' : ''}${n}`;
  const draftTech = S.drafts.find(d => d.dto.type === 'SetResearch')?.dto.techs[0];
  const res = [
    { g: '◆', c: '#b08a3e', a: fmt(e.gold), n: L`金`, rate: L`${signed(net)} / ティック`, neg: net < 0,
      tip: [[L`都市の金`, `+${e.goldIncome}`], [L`部隊の維持費`, `−${e.upkeepUnits}`], [L`都市の維持費`, `−${e.upkeepCities}`], [L`差し引き`, signed(net), true]] },
    { g: '✧', c: '#7b7698', a: e.research ? html`${fmt(e.research.store)}<span class="name">/${fmt(e.research.cost)}</span>` : fmt(e.scienceStore), n: L`科学`,
      rate: e.research ? T.TECH[e.research.tech] : draftTech ? L`予定：${T.TECH[draftTech]}（未解決）` : L`研究を選んでください`, neg: !e.research && !draftTech,
      tip: [[L`研究中`, e.research ? T.TECH[e.research.tech] : L`なし`], [L`蓄積`, fmt(e.scienceStore)], [L`研究済み`, `${e.techs.length} / ${TECH_COUNT}`]] },
    { g: '☮', c: '#5b8278', a: fmt(e.influence), n: L`影響力`, rate: L`都市国家への使節に使う`, tip: [[L`用途`, L`都市国家への使節`], [L`宗主になる条件`, L`影響力${SUZERAIN_MIN}以上で最多`]] },
    { g: '⬡', c: '#a27151', a: fmt(e.iron), n: L`鉄`, rate: L`+${e.ironIncome} / ティック`, tip: [[L`用途`, L`長槍兵・弩兵（1兵につき1）`], [L`産出`, L`鉄鉱床のある土地を都市が使うと+1`]] },
    { g: '♞', c: '#8b6a48', a: fmt(e.horses), n: L`馬`, rate: L`+${e.horseIncome} / ティック`, tip: [[L`用途`, L`騎士（1兵につき2）`], [L`産出`, L`馬のいる土地を都市が使うと+1`]] },
  ];
  setHtml($('#resources'), res.map(r => html`<button class="res" type="button" style="--c:${r.c}" aria-label="${r.n}"><span class="glyph">${r.g}</span><span class="amount">${r.a}<span class="name">${r.n}</span></span><span class="rate ${r.neg ? 'negative' : ''}">${r.rate}</span>
    <span class="tip panel">${r.tip.map(([k, x, total]) => html`<div class="tip-row ${total ? 'total' : ''}"><span>${k}</span><span>${x}</span></div>`)}</span></button>`));
}

/** Chain mode after a tick's deadline: what the clock says instead of a countdown. */
const CLOSED_PHASE = lazyTable({ reveal: () => L`公開中`, frozen: () => L`解決中` });

export function renderClock() {
  const v = S.view; if (!v) return;
  const el = $('#clock');
  const secs = secondsLeft();
  const frac = v.tickSeconds ? secs / v.tickSeconds : 1;
  const [, ph, phEn] = T.phaseOf(v.tick, v.season?.phases);
  // Chain mode: the commitments close at the deadline, then the sealed batches are revealed and the tick resolves.
  const closed = v.chain && !v.over ? CLOSED_PHASE[chainPhase(v)] : null;
  el.className = 'clock' + (v.paused ? ' paused' : closed ? '' : frac <= .1 ? ' red' : frac <= .25 ? ' amber' : '') + (!v.paused && !closed && secs <= 5 ? ' pulse' : '');
  const mm = Math.floor(secs / 60), ss = Math.floor(secs % 60);
  const time = v.over ? L`終了` : v.paused ? L`停止中` : closed || `${String(mm).padStart(2, '0')}:${String(ss).padStart(2, '0')}`;
  const auto = Math.round(autoSeconds(v));
  const rows = v.chain
    ? html`<div class="tip-row"><span>${L`締切で`}</span><span>${L`封印を閉じ、公開して全勢力を同時に解決`}</span></div><div class="tip-row"><span>${L`締切${auto}秒前`}</span><span>${L`下書きを自動確定（表示中のタブ）`}</span></div>`
    : html`<div class="tip-row"><span>${L`締切で`}</span><span>${L`全勢力を同時に解決`}</span></div><div class="tip-row"><span>${L`締切${auto}秒前`}</span><span>${L`下書きを自動確定`}</span></div>`;
  setHtml(el, html`<div class="clock-top"><span class="clock-tick">TICK ${String(v.tick).padStart(3, '0')}<span class="clock-of"> / ${v.ticks}</span></span><span class="clock-time">${time}</span></div>
    <div class="clock-phase">${lang() === 'en' ? phEn : `${ph} · ${phEn}`}</div><div class="clock-bar"><i style="width:${(v.paused ? 1 : closed ? 0 : frac) * 100}%"></i></div>
    <span class="tip panel"><div class="tip-row"><span>${L`1ティック`}</span><span>${L`${v.tickSeconds}秒`}</span></div>${rows}</span>`);
  $('#live-dot').classList.toggle('off', !S.online);
}

export function renderPlate() {
  const v = S.view, me = myNation();
  const ach = v.achievements?.nations?.[S.myCiv];
  $('#civ-title').textContent = `${civN(S.myCiv)}${ach ? ` · ${L`第${ach.era}時代`}` : ''}`;
  setHtml($('#civ-sub'), html`${L`${me.cities}都市 · 人口${me.pop ?? '?'} · 兵${me.troops ?? '?'} · メンバー${membersOf(S.myCiv).length}人`}${v.economy?.protectionLost ? '' : html` · <span title="${L`他の勢力はあなたの保護区域に入れません`}">${L`保護区域あり`}</span>`}`);
  renderTracker();
}

/** Four paths × five tiers for my nation, and the era (Civ VII legacy paths). */
function renderTracker() {
  const ach = S.view.achievements?.nations?.[S.myCiv];
  if (!ach) return;
  const pool = poolUsdc();
  setHtml($('#tracker'), html`<div class="head"><span>${L`4つの道 · 第${ach.era}時代 · ${ach.points}点`}</span>${pool ? html`<b>${L`賞金 ${fmt(pool)} USDC`}</b>` : ''}</div>${ach.tiers.map((t, i) => html`<div class="trk"><span class="n">${T.PATH_JA[i]}</span><span class="steps">${[1, 2, 3, 4, 5].map(k => html`<i class="${t >= k ? 'on' : ''}"></i>`)}</span><span class="rank">${t}/5</span></div>`)}`);
}

const REL_ICON = { war: '⚔', alliance: '🤝', nap: '☮', peace: '' };
/** Leader ribbon, as in Civ's top-right: every nation, its era, members, officers. */
export function renderRibbon() {
  const v = S.view;
  setHtml($('#ribbon'), v.civs.map(c => {
    const me = c.id === S.myCiv, rel = me ? '' : REL_ICON[c.relation] || '';
    const members = membersOf(c.id);
    const ach = v.achievements?.nations?.[c.id];
    const name = civN(c.id);
    return html`<button class="leader ${me ? 'me' : ''}" type="button" style="--c:${T.CIV_COLORS[c.id % T.CIV_COLORS.length]}" aria-label="${name}">
      ${name.slice(0, 1)}
      ${rel ? html`<span class="rel">${rel}</span>` : ''}<span class="era">${c.era}</span>
      <span class="kind ${members.length ? '' : 'bot'}">${members.length ? L`${members.length}人` : L`代行`}</span>
      <span class="card"><b>${name}</b>
        <div class="row2"><span>${ach ? L`第${c.era}時代 · ${ach.points}点` : L`第${c.era}時代`}</span><span>${me ? L`あなたの勢力` : T.RELATION[c.relation] || ''}</span></div>
        <div class="row2"><span>${L`メンバー ${members.length}人`}</span><span>${L`都市 ${c.cities}`}</span></div>
        <div class="row2"><span>${L`節目 ${c.tiers.map((t, i) => `${T.PATH_JA[i]}${t}`).join(' ')}`}</span></div>
        ${v.projection && members.length ? html`<div class="row2"><span>${L`今の取り分`}</span><span>${L`${usdc(v.projection.nationShare[c.id])} USDC（1人 ${usdc(v.projection.nationShare[c.id] / members.length)}）`}</span></div>` : ''}
      </span></button>`;
  }));
}
