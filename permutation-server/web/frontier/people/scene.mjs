// The people layer's inputs from what a page already reads (design session,
// "people" request): the departures still on the road and this bell's
// explores from the chronicle's DEPART / EXPLORE records, and the holders'
// names from the roster (roster.mjs → identity.mjs). Pure functions of
// their arguments, shared by the game page and the replay page.
import { hostParts } from '../faddr.mjs';
import { identityOf, displayName, tagOf } from './identity.mjs';

/** The faction holding (P, Q, site) in the overviews (0–5), or null. */
export function ownerFaction(overviews, p, q, site) {
  for (const o of overviews?.values?.() ?? []) {
    const r = o.provinces?.find(x => x.p === p && x.q === q);
    if (r) return r.sites[site] === 1 && r.owners[site] < 6 ? r.owners[site] : null;
  }
  return null;
}

/**
 * Hosts on the road at `bell`: `[{p, q, tile, faction, troops, arriveBell, host}]`
 * from DEPART records (origin and arrival bell are public; the destination is
 * sealed and never part of this). A host is on the road from its depart bell
 * until its arrival bell.
 */
export function departuresAt(chronicle, overviews, bell) {
  const out = [];
  for (const x of chronicle ?? []) {
    const r = x.record ?? x;
    if (r.name !== 'DEPART') continue;
    const arrive = Number(r.arrive_bell), depart = Number(r.depart_bell);
    if (!(depart <= bell && bell < arrive)) continue;
    const h = hostParts(r.host_id);
    const faction = h ? ownerFaction(overviews, h.p, h.q, h.site) : null;
    if (faction === null) continue;
    out.push({ p: Number(r.origin_p), q: Number(r.origin_q), tile: Number(r.origin_tile), faction, troops: Number(r.dep_mass ?? 0), arriveBell: arrive, host: String(r.host_id) });
  }
  return out;
}

/** Explores of the bell before `bell` and of `bell`: `[{p, q, tiles, faction}]`. */
export function exploresAt(chronicle, overviews, bell) {
  const out = [];
  for (const x of chronicle ?? []) {
    const r = x.record ?? x;
    if (r.name !== 'EXPLORE' || !(x.bell === bell || x.bell === bell - 1 || r.bell === bell || r.bell === bell - 1)) continue;
    const h = hostParts(r.host_id);
    const faction = h ? ownerFaction(overviews, h.p, h.q, h.site) : null;
    const tiles = Array.from(r.tiles ?? []).slice(0, Number(r.n ?? 0));
    out.push({ p: Number(r.p), q: Number(r.q), tiles, faction });
  }
  return out;
}

/** `nameOf(p, q, site, faction)` for the name tags, from a roster store. */
export function namer(roster, { bell = null, language } = {}) {
  return (p, q, site, faction) => {
    const o = roster?.ownerOf(p, q, site, bell);
    if (!o) return null;
    const identity = identityOf(o.tag);
    return { identity, faction, name: displayName(identity, language ? { language } : {}) };
  };
}

/** Seed the roster with the viewer's own holdings (their Holding accounts name the owner). */
export function seedOwn(roster, holdings) {
  for (const h of holdings ?? []) if (h?.ownerCitizen) roster.put(h.p, h.q, h.site, tagOf(h.ownerCitizen), 0);
}
