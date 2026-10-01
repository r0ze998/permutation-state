// People in the page's panels (design session, "people" request): a person
// chip (portrait and name), the leader card of a faction, and the
// spectator's highlights ("Kaito of Ember sets out"). Markup only; names
// carry data-name (proper names, not translated text).
import { html, raw } from '../../util.mjs';
import { L, fmtNum, lang } from '../../lang.mjs';
import { factionName, DOCTRINE_NAMES, TIERS } from '../fi18n.mjs';
import { hostParts } from '../faddr.mjs';
import { decode as fromBase58 } from '../../sdk/base58.mjs';
import { identityOf, displayName, tagOf, placeName } from './identity.mjs';
import { avatarSvg } from './avatar.mjs';
import { leaderSvg, LEADERS, DOCTRINE_PITCH } from './leaders.mjs';
import { ownerFaction } from './scene.mjs';

let uid = 0;
/** A portrait and a name: `<span class="person">`. */
export function personChip(identity, faction, { size = 28, full = false, note = '' } = {}) {
  if (!identity) return '';
  const name = displayName(identity, { full });
  return html`<span class="person">${raw(avatarSvg(identity, faction, { size, uid: `p${uid++ % 100000}` }))}<span class="person-name" data-name>${name}</span>${note ? html`<span class="person-note">${note}</span>` : ''}</span>`;
}

/** A holding's name: its place and tier, 「サフォードの町」 / "Saford's Town" (UI plan E5). */
export function holdingName(h, tier = h?.tier) {
  if (!h) return '';
  const n = placeName(h.p, h.q, h.site)[lang() === 'en' ? 'en' : 'ja'];
  return L`${n}の${TIERS[tier] ?? TIERS[0]}`;
}

/** The viewer's own citizen tag: a holding's owner, else the Citizen address of the wallet. */
export function ownTag(FS) {
  const h = FS.holdings?.find(x => x?.ownerCitizen);
  if (h) return tagOf(h.ownerCitizen);
  try {
    const a = FS.wallet && FS.pin?.addresses?.of('Citizen', { wallet: FS.wallet.address });
    return a ? tagOf(fromBase58(a)) : null;
  } catch { return null; }
}

/** The identity of a host's owner (its holding's holder in the roster), or null. */
export function hostOwner(roster, hostId) {
  const h = hostParts(hostId);
  const o = h && roster?.ownerOf(h.p, h.q, h.site);
  return o ? identityOf(o.tag) : null;
}

/** A faction's leader card: portrait, leader name and title, doctrine and its one-line pitch. */
export function leaderCard(f, { size = 96 } = {}) {
  const l = LEADERS[f];
  const name = lang() === 'en' ? l.name.en : l.name.ja;
  return html`<span class="leader-card">${raw(leaderSvg(f, { size }))}<span class="leader-text">
    <strong>${factionName(f)}</strong>
    <span class="leader-name"><span data-name>${name}</span> · ${l.title()}</span>
    <span class="leader-doctrine">${L`教義：${DOCTRINE_NAMES[f]}`}</span>
    <span class="muted">${DOCTRINE_PITCH[f]()}</span></span></span>`;
}

/**
 * The spectator's highlights from the chronicle, newest first:
 * `[{bell, kind, faction, identity, text}]` — departures (with the arrival
 * bell only: the destination is sealed), settlements, explores, clashes.
 */
export function highlights(chronicle, overviews, roster, { limit = 8, faction = null, bell = null } = {}) {
  const out = [];
  const list = chronicle ?? [];
  // the spectator's filters (UI plan G3): one faction (a clash counts for the factions holding land in its province), one bell
  const keep = x => (bell === null || x.bell === bell) && (faction === null || (x.factions ?? [x.faction]).includes(faction));
  const push = x => { if (keep(x)) out.push(x); };
  for (let i = list.length - 1; i >= 0 && out.length < limit; i--) {
    const r = list[i].record ?? list[i];
    const bell = Number(r.bell);
    if (r.name === 'DEPART' || r.name === 'EXPLORE') {
      const h = hostParts(r.host_id);
      const faction = h ? ownerFaction(overviews, h.p, h.q, h.site) : null;
      const id = hostOwner(roster, r.host_id);
      if (faction === null) continue;
      const who = id ? displayName(id) : L`名もない領主`;
      push({ bell, kind: r.name === 'DEPART' ? 'depart' : 'explore', faction, identity: id, p: h.p, q: h.q,
        text: r.name === 'DEPART' ? L`${factionName(faction)}の${who}が出陣（第${fmtNum(Number(r.arrive_bell))}鐘に到着）` : L`${factionName(faction)}の${who}が州 ${Number(r.p)},${Number(r.q)} を探索` });
    } else if (r.name === 'SETTLE' && (Number(r.outcome) === 0 || Number(r.outcome) === 1)) {
      const p = Number(r.p), q = Number(r.q), site = Number(r.site);
      const faction = ownerFaction(overviews, p, q, site);
      const id = identityOf(tagOf(BigInt(String(r.citizen_tag))));
      if (faction === null) continue;
      push({ bell, kind: 'settle', faction, identity: id, p, q, text: L`${factionName(faction)}の${displayName(id)}が州 ${p},${q} に入植` });
    } else if (r.name === 'CLASH') {
      const p = Number(r.p), q = Number(r.q);
      push({ bell, kind: 'clash', faction: null, factions: provinceFactions(overviews, p, q), identity: null, p, q, clash: true, text: L`州 ${p},${q} で衝突（第${fmtNum(bell)}鐘）` });
    }
  }
  return out;
}

/** The factions holding land in a province (the overviews). */
export function provinceFactions(overviews, p, q) {
  for (const o of overviews?.values?.() ?? []) {
    const r = o.provinces?.find(x => x.p === p && x.q === q);
    if (r) return [...new Set(r.owners.filter((f, j) => r.sites[j] === 1 && f < 6))];
  }
  return [];
}

/** The highlights list markup; with `go`, each item is a button to its place (a clash replays there). */
export function renderHighlights(items, { go = false } = {}) {
  if (!items.length) return html`<p class="muted">${L`まだ見どころはありません`}</p>`;
  const face = x => (x.identity ? raw(avatarSvg(x.identity, x.faction, { size: 24, uid: `h${uid++ % 100000}` })) : html`<span class="hl-dot" aria-hidden="true"></span>`);
  return html`<ol class="highlights">${items.map(x => html`<li class="hl-${x.kind}">${go && Number.isInteger(x.p)
    ? html`<button type="button" class="hl-go" data-act="${x.clash ? 'battle-play' : 'goto'}" data-p="${x.p}" data-q="${x.q}" data-bell="${x.bell}">${face(x)}<span>${x.text}</span></button>`
    : html`${face(x)}<span>${x.text}</span>`}</li>`)}</ol>`;
}
