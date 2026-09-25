// Top of the screen: member chip, resources, clock, the nation plate with
// its path tracker, and the leader ribbon (every nation).
import * as T from '../i18n.mjs';
import { $, html, setHtml, fmt, usdc } from '../util.mjs';
import { S, civN, myNation, held, membersOf, isWatching, secondsLeft } from '../state.mjs';
import { SUZERAIN_MIN, TECH_COUNT, AUTO_COMMIT_SECONDS } from '../rules.mjs';
import { poolUsdc } from '../chain.mjs';

export function renderTop() {
  const v = S.view, e = v.economy;
  const offices = held();
  const badges = isWatching() ? html`<span class="badge">観戦</span>`
    : offices.length ? offices.map(r => html`<span class="badge office">${T.ROLE_GLYPH[r]} ${T.ROLE_JA[r]}</span>`) : html`<span class="badge human">国民</span>`;
  setHtml($('#civ-chip'), html`<span class="swatch" style="background:${T.CIV_COLORS[S.myCiv]}"></span><div><b>${v.member?.name ?? civN(S.myCiv)}</b><small class="nation">${civN(S.myCiv)}</small><div>${badges}</div></div>`);
  const ms = v.members || [];
  $('.prototype-label').textContent = `${v.chain ? 'オンチェーン · MagicBlock ER' : 'ローカル'} · 国民${ms.length}人（人間${ms.filter(m => m.host === 'human').length}） · 同じ霧の中で判断 · USDCはテスト用`;
  $('#season-label').textContent = `ONE WORLD · SEASON ${v.chain?.seasonId ?? 0} · ${String(v.season?.preset ?? 'Blitz').toUpperCase()}`;
  if (e) renderResources(e);
  const pause = $('#pause-btn');
  pause.hidden = !!v.chain;
  pause.classList.toggle('on', v.paused);
  pause.textContent = v.paused ? '▶' : '❚❚';
  pause.title = v.paused ? '時計を動かす' : '時計を止める（このPCだけの試作）';
  renderClock();
}

function renderResources(e) {
  const upkeep = e.upkeepUnits + e.upkeepCities;
  const net = e.goldIncome - upkeep;
  const signed = n => `${n >= 0 ? '+' : ''}${n}`;
  const draftTech = S.drafts.find(d => d.dto.type === 'SetResearch')?.dto.techs[0];
  const res = [
    { g: '◆', c: '#b08a3e', a: fmt(e.gold), n: '金', rate: `${signed(net)} / ティック`, neg: net < 0,
      tip: [['都市の金', `+${e.goldIncome}`], ['部隊の維持費', `−${e.upkeepUnits}`], ['都市の維持費', `−${e.upkeepCities}`], ['差し引き', signed(net), true]] },
    { g: '✧', c: '#7b7698', a: e.research ? html`${fmt(e.research.store)}<span class="name">/${fmt(e.research.cost)}</span>` : fmt(e.scienceStore), n: '科学',
      rate: e.research ? T.TECH[e.research.tech] : draftTech ? `予定：${T.TECH[draftTech]}（未解決）` : '研究を選んでください', neg: !e.research && !draftTech,
      tip: [['研究中', e.research ? T.TECH[e.research.tech] : 'なし'], ['蓄積', fmt(e.scienceStore)], ['研究済み', `${e.techs.length} / ${TECH_COUNT}`]] },
    { g: '☮', c: '#5b8278', a: fmt(e.influence), n: '影響力', rate: '都市国家への使節に使う', tip: [['用途', '都市国家への使節'], ['宗主になる条件', `影響力${SUZERAIN_MIN}以上で最多`]] },
    { g: '⬡', c: '#a27151', a: fmt(e.iron), n: '鉄', rate: `+${e.ironIncome} / ティック`, tip: [['用途', '長槍兵・弩兵（1兵につき1）'], ['産出', '鉄鉱床のある土地を都市が使うと+1']] },
    { g: '♞', c: '#8b6a48', a: fmt(e.horses), n: '馬', rate: `+${e.horseIncome} / ティック`, tip: [['用途', '騎士（1兵につき2）'], ['産出', '馬のいる土地を都市が使うと+1']] },
  ];
  setHtml($('#resources'), res.map(r => html`<button class="res" type="button" style="--c:${r.c}" aria-label="${r.n}"><span class="glyph">${r.g}</span><span class="amount">${r.a}<span class="name">${r.n}</span></span><span class="rate ${r.neg ? 'negative' : ''}">${r.rate}</span>
    <span class="tip panel">${r.tip.map(([k, x, total]) => html`<div class="tip-row ${total ? 'total' : ''}"><span>${k}</span><span>${x}</span></div>`)}</span></button>`));
}

export function renderClock() {
  const v = S.view; if (!v) return;
  const el = $('#clock');
  const secs = secondsLeft();
  const frac = v.tickSeconds ? secs / v.tickSeconds : 1;
  const [, ph, phEn] = T.phaseOf(v.tick, v.season?.phases);
  el.className = 'clock' + (v.paused ? ' paused' : frac <= .1 ? ' red' : frac <= .25 ? ' amber' : '') + (!v.paused && secs <= 5 ? ' pulse' : '');
  const mm = Math.floor(secs / 60), ss = Math.floor(secs % 60);
  const time = v.over ? '終了' : v.paused ? '停止中' : `${String(mm).padStart(2, '0')}:${String(ss).padStart(2, '0')}`;
  setHtml(el, html`<div class="clock-top"><span class="clock-tick">TICK ${String(v.tick).padStart(3, '0')}<span class="clock-of"> / ${v.ticks}</span></span><span class="clock-time">${time}</span></div>
    <div class="clock-phase">${ph} · ${phEn}</div><div class="clock-bar"><i style="width:${(v.paused ? 1 : frac) * 100}%"></i></div>
    <span class="tip panel"><div class="tip-row"><span>1ティック</span><span>${v.tickSeconds}秒</span></div><div class="tip-row"><span>締切で</span><span>全文明を同時に解決</span></div><div class="tip-row"><span>締切${AUTO_COMMIT_SECONDS}秒前</span><span>下書きを自動確定</span></div></span>`);
  $('#live-dot').classList.toggle('off', !S.online);
}

export function renderPlate() {
  const v = S.view, me = myNation();
  const ach = v.achievements?.nations?.[S.myCiv];
  $('#civ-title').textContent = `${civN(S.myCiv)}${ach ? ` · 第${ach.era}時代` : ''}`;
  setHtml($('#civ-sub'), html`${me.cities}都市 · 人口${me.pop ?? '?'} · 兵${me.troops ?? '?'} · 国民${membersOf(S.myCiv).length}人${v.economy?.protectionLost ? '' : html` · <span title="他の国はあなたの保護区域に入れません">保護区域あり</span>`}`);
  renderTracker();
}

/** Four paths × five tiers for my nation, and the era (Civ VII legacy paths). */
function renderTracker() {
  const ach = S.view.achievements?.nations?.[S.myCiv];
  if (!ach) return;
  const pool = poolUsdc();
  setHtml($('#tracker'), html`<div class="head"><span>4つの道 · 第${ach.era}時代 · ${ach.points}点</span>${pool ? html`<b>賞金 ${fmt(pool)} USDC</b>` : ''}</div>${ach.tiers.map((t, i) => html`<div class="trk"><span class="n">${T.PATH_JA[i]}</span><span class="steps">${[1, 2, 3, 4, 5].map(k => html`<i class="${t >= k ? 'on' : ''}"></i>`)}</span><span class="rank">${t}/5</span></div>`)}`);
}

const REL_ICON = { war: '⚔', alliance: '🤝', nap: '☮', peace: '' };
/** Leader ribbon, as in Civ's top-right: every nation, its era, members, officers. */
export function renderRibbon() {
  const v = S.view;
  setHtml($('#ribbon'), v.civs.map(c => {
    const me = c.id === S.myCiv, rel = me ? '' : REL_ICON[c.relation] || '';
    const members = membersOf(c.id);
    const humans = members.filter(m => m.host === 'human').length;
    const ach = v.achievements?.nations?.[c.id];
    const name = civN(c.id);
    return html`<button class="leader ${me ? 'me' : ''}" type="button" style="--c:${T.CIV_COLORS[c.id % T.CIV_COLORS.length]}" aria-label="${name}">
      ${name.slice(0, 1)}
      ${rel ? html`<span class="rel">${rel}</span>` : ''}<span class="era">${c.era}</span>
      <span class="kind ${members.length ? (humans ? '' : 'ai') : 'bot'}">${members.length ? `${members.length}人` : '代行'}</span>
      <span class="card"><b>${name}</b>
        <div class="row2"><span>第${c.era}時代${ach ? ` · ${ach.points}点` : ''}</span><span>${me ? 'あなたの国' : T.RELATION[c.relation] || ''}</span></div>
        <div class="row2"><span>国民 ${members.length}人（人間${humans}）</span><span>都市 ${c.cities}</span></div>
        <div class="row2"><span>節目 ${c.tiers.map((t, i) => `${T.PATH_JA[i]}${t}`).join(' ')}</span></div>
        ${v.projection && members.length ? html`<div class="row2"><span>今の取り分</span><span>${usdc(v.projection.nationShare[c.id])} USDC（1人 ${usdc(v.projection.nationShare[c.id] / members.length)}）</span></div>` : ''}
      </span></button>`;
  }));
}
