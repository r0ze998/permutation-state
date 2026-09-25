// Milestones of the four paths and the eras (V5 §6), and my merit with the
// payout if the season ended now (V5 §7).
import * as T from '../i18n.mjs';
import { html, fmt, usdc } from '../util.mjs';
import { S, civN } from '../state.mjs';
import { activityWindows, equalSharePct, eraPathsNeeded } from '../rules.mjs';

export function drawerEra() {
  const v = S.view, A = v.achievements; if (!A) return '';
  const th = A.thresholds, mine = A.nations[S.myCiv], f = v.facts || {};
  const tierText = [
    k => `領土 ${th.hegemonyTiles[k]}${th.hegemonyCities[k] ? (k === 2 ? ` または占領都市${th.hegemonyCities[k]}` : ` ＋ 占領都市${th.hegemonyCities[k]}`) : ''}`,
    k => `人口 ${th.prosperityPop[k]}${th.prosperityWealth[k] ? ` ＋ 富 ${fmt(th.prosperityWealth[k])}` : ''}`,
    k => (k < 3 ? `技術 ${th.scienceTechs[k]}` : k === 3 ? 'スターゲート I' : 'スターゲート III'),
    k => ['条約相手1 または 使節', '条約相手1 ＋ 宗主経験', '宗主 ＋ 条約相手1', `宗主 ＋ 同盟 ＋ 交易 ${th.concordTrade[0]}`, `宗主2 ＋ 条約相手2 ＋ 交易 ${th.concordTrade[1]}`][k],
  ];
  const now = [`領土 ${f.tiles} · 占領都市 ${f.capturedHeld}`, `人口 ${f.pop} · 富 ${fmt(f.wealth)}`, `技術 ${f.techs} · スターゲート最高 ${f.starGateMax}`, `条約相手 ${f.partners} · 同盟 ${f.alliances} · 宗主 ${f.suzerainties}${f.everSuzerain ? '（経験あり）' : ''} · 交易 ${fmt(f.trade)}`];
  const next = mine.era + 1;
  const pr = v.projection;
  const tiers = [0, 1, 2, 3, 4];
  return html`<div class="eyebrow">ERAS · 4つの道と時代</div><h2>${civN(S.myCiv)} · 第${mine.era}時代 · ${mine.points}点</h2>
    <p class="drawer-intro">節目を1つ達成するごとに点が入り、同じ段階の節目を${eraPathsNeeded(1)}つの道（第5段階は${eraPathsNeeded(5)}つ）で達成すると新しい時代に入ります。領土・人口・宗主・条約・占領都市は<b>シーズン終了時の状態</b>で判定するので、失えば取り消されます（下は「今終わったら」）。賞金プールは国民のいる国の点の比で分けます。</p>
    <table class="grid era-grid"><thead><tr><th>段階</th>${T.PATH_JA.map(n => html`<th>${n}</th>`)}</tr></thead><tbody>
    ${tiers.map(k => html`<tr><td><b>${k + 1}</b><small> ${A.tierPoints[k]}点</small></td>${T.PATH_JA.map((_, p) => html`<td class="${mine.tiers[p] > k ? 'met' : mine.tiers[p] === k ? 'next' : ''}">${mine.tiers[p] > k ? '✓ ' : ''}${tierText[p](k)}</td>`)}</tr>`)}
    </tbody></table><div class="meta" style="margin:6px 0">いま：${now.map((t, i) => html`${i ? ' · ' : ''}<b>${T.PATH_JA[i]}</b> ${t}`)}</div>
    ${next <= 5 ? html`<div class="explanation">第${next}時代まで：段階${next}の節目を${eraPathsNeeded(next)}つの道で（いま${mine.tiers.filter(t => t >= next).length}つ）。</div>` : ''}
    <div class="section-title">国ごとの達成と賞金の見込み</div><table class="grid"><thead><tr><th>国</th><th>時代</th><th>節目</th><th>点</th><th>取り分</th></tr></thead><tbody>
    ${A.nations.map(n => html`<tr class="${n.civ === S.myCiv ? 'me' : ''}"><td><span class="swatch-s" style="background:${T.CIV_COLORS[n.civ]}"></span>${civN(n.civ)}${pr?.counted?.[n.civ] ? '' : html` <span class="meta" title="国民がいない・活動した国民がいない・都市がない国は配分の対象外">対象外</span>`}</td><td>${n.era}</td><td>${n.tiers.join('/')}</td><td>${n.points}</td><td>${pr ? usdc(pr.nationShare[n.civ]) : '—'}</td></tr>`)}
    </tbody></table>`;
}

const MERIT_BARS = ['hegemony', 'prosperity', 'science', 'concord', 'common'];
export function drawerMerit() {
  const v = S.view, m = v.member, pr = v.projection;
  if (!m) return html`<div class="eyebrow">MERIT</div><h2>功績</h2><p class="desc">観戦中は表示されません。</p>`;
  const share = pr?.nationShare?.[S.myCiv] ?? 0, eq = pr?.equalEach?.[S.myCiv] ?? 0;
  const max = Math.max(1, ...MERIT_BARS.map(b => m.merit[b]));
  return html`<div class="eyebrow">MERIT · 功績と見込み</div><h2>今終わったら ${usdc(m.projectedPayout)} USDC</h2>
    <p class="drawer-intro">国の取り分は ${usdc(share)} USDC。その${equalSharePct(v)}%は活動した国民で均等（1人 ${usdc(eq)}・上限は参加費の半分）、残りは功績の比で、国が点を取った道の功績ほど重く分けます。活動した国民＝${activityWindows(v)}区間のうち${m.windowsNeeded}区間以上で命令・献策・支持・投票をした人（あなた：${m.activeWindows}区間${m.active ? ' ✓' : ''}）。</p>
    <div class="section-title">あなたの功績 · 合計 ${m.merit.total.toFixed(1)}</div>
    ${MERIT_BARS.map(b => html`<div class="meter"><span>${T.MERIT_JA[b]} ${m.merit[b].toFixed(1)}</span><div class="bar"><i style="width:${(100 * m.merit[b] / max).toFixed(0)}%"></i></div></div>`)}
    <div class="section-title">直前のティックで得た功績</div>${m.meritLog.length ? m.meritLog.map(e => html`<div class="list-row" style="cursor:default"><div class="main"><div class="title">+${e.merit.toFixed(1)} ${T.MERIT_JA[e.path.toLowerCase()] || e.path}</div><div class="meta">${T.MERIT_WHAT[e.what] || e.what}</div></div></div>`)
      : html`<p class="desc">なし。都市の成長・建物の完成・研究・占領・条約・交易などで、命令を出した人（採用された献策は半分ずつ）に付きます。</p>`}`;
}
