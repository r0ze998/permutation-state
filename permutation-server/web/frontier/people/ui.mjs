// People in the page's panels (design session, "people" request): a person
// chip (portrait and name), the leader card of a faction, and the
// spectator's highlights ("Kaito of Ember sets out"). Markup only; names
// carry data-name (proper names, not translated text).
import { html, raw } from '../../util.mjs';
import { L, fmtNum, lang } from '../../lang.mjs';
import { factionName, DOCTRINE_NAMES } from '../fi18n.mjs';
import { hostParts } from '../faddr.mjs';
import { decode as fromBase58 } from '../../sdk/base58.mjs';
import { identityOf, displayName, tagOf } from './identity.mjs';
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
export function highlights(chronicle, overviews, roster, { limit = 8 } = {}) {
  const out = [];
  const list = chronicle ?? [];
  for (let i = list.length - 1; i >= 0 && out.length < limit; i--) {
    const r = list[i].record ?? list[i];
    const bell = Number(r.bell);
    if (r.name === 'DEPART' || r.name === 'EXPLORE') {
      const h = hostParts(r.host_id);
      const faction = h ? ownerFaction(overviews, h.p, h.q, h.site) : null;
      const id = hostOwner(roster, r.host_id);
      if (faction === null) continue;
      const who = id ? displayName(id) : L`名もない領主`;
      out.push({ bell, kind: r.name === 'DEPART' ? 'depart' : 'explore', faction, identity: id,
        text: r.name === 'DEPART' ? L`${factionName(faction)}の${who}が出陣（第${fmtNum(Number(r.arrive_bell))}鐘に到着）` : L`${factionName(faction)}の${who}が州 ${Number(r.p)},${Number(r.q)} を探索` });
    } else if (r.name === 'SETTLE' && (Number(r.outcome) === 0 || Number(r.outcome) === 1)) {
      const p = Number(r.p), q = Number(r.q), site = Number(r.site);
      const faction = ownerFaction(overviews, p, q, site);
      const id = identityOf(tagOf(BigInt(String(r.citizen_tag))));
      if (faction === null) continue;
      out.push({ bell, kind: 'settle', faction, identity: id, text: L`${factionName(faction)}の${displayName(id)}が州 ${p},${q} に入植` });
    } else if (r.name === 'CLASH') {
      out.push({ bell, kind: 'clash', faction: null, identity: null, text: L`州 ${Number(r.p)},${Number(r.q)} で衝突（第${fmtNum(bell)}鐘）` });
    }
  }
  return out;
}

/** The highlights list markup. */
export function renderHighlights(items) {
  if (!items.length) return html`<p class="muted">${L`まだ見どころはありません`}</p>`;
  return html`<ol class="highlights">${items.map(x => html`<li class="hl-${x.kind}">${x.identity ? raw(avatarSvg(x.identity, x.faction, { size: 24, uid: `h${uid++ % 100000}` })) : html`<span class="hl-dot" aria-hidden="true"></span>`}<span>${x.text}</span></li>`)}</ol>`;
}
