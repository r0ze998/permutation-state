// The automatic site ticket (owner decision V2: joining is choosing a faction; the first
// holding is placed automatically). Pure planning, the controller (autoTicket) does the
// reads and the send. Review of 2026-10-02 (PLAM), findings 2–4:
//   - every citizen gets its own order: the candidate provinces of a ring and the free
//     sites of a province are shuffled by a hash of (citizen, bell, p, q, site), so the
//     joiners of one bell do not all file the same three sites;
//   - a province whose ticket cohorts have no reusable record for now_bell is skipped
//     (the program would refuse the ticket with CohortFull);
//   - the decision rests on the decoded Provinces (siteCount, siteMirror), never on the
//     overview's site codes alone (sites past site_count read as "free" there); when no
//     home-wedge Province gives a site, the adjacent wedges' outermost ring is read (§5.9);
//   - read failures are told apart from "no free site".
import { candidateProvinces, freeSitesOf, COHORT_BELLS, homeWedge } from './fland.mjs';
import { adjacentWedges } from './onboarding.mjs';
import { ringOf, wedgeOf, provinceIndex } from './fgeo.mjs';

export const AUTO_MAX_SITES = 3;
/** Provinces read per pass, and at most in all (each read is one herald request). */
export const AUTO_BATCH = 6, AUTO_READ_MAX = 30;

/** A 32-bit FNV-1a hash of a string (stable across browsers). */
export function hash32(s) {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 0x01000193); }
  return h >>> 0;
}

/** Whether FileTicket can count a ticket in this Province's cohorts at `nowBell` (the program's cohort step, I-47). */
export function cohortRoom(province, nowBell) {
  const recs = province?.ticketCohorts ?? [];
  if (!recs.length) return true;
  return recs.some(c => (c.bell === nowBell && c.filed > 0) || c.settled >= c.filed || nowBell >= c.bell + COHORT_BELLS);
}

/** Candidates in this citizen's order: nearest ring first, shuffled within a ring by the seed. */
export function seededOrder(cands, seed) {
  return [...cands].sort((a, b) => (a.ring - b.ring) || (hash32(`${seed}|${a.p},${a.q}`) - hash32(`${seed}|${b.p},${b.q}`)) || (provinceIndex(a.p, a.q) - provinceIndex(b.p, b.q)));
}

/** The adjacent wedges' provinces of the outermost open ring (§5.9), whatever the overview's site codes say. */
export function overflowCandidates(overviews, faction, ringsOpen) {
  const w = homeWedge(faction), ring = Math.max(0, Number(ringsOpen) - 1), wedges = adjacentWedges(w), out = [];
  for (const ov of overviews.values()) {
    if (!ov?.provinces || ov.ring !== ring) continue;
    for (const pr of ov.provinces) if (wedges.includes(wedgeOf(pr.p, pr.q)) && ringOf(pr.p, pr.q) === ring) out.push({ p: pr.p, q: pr.q, ring });
  }
  return out;
}

/**
 * Up to `max` sites from decoded Provinces (already in this citizen's order): free sites
 * shuffled per province by the seed, one per province in turn; provinces without cohort
 * room skipped.
 */
export function pickSites(provinces, { seed = '', nowBell = 0, max = AUTO_MAX_SITES } = {}) {
  const lists = provinces.filter(pv => cohortRoom(pv, nowBell))
    .map(pv => freeSitesOf(pv).sort((a, b) => hash32(`${seed}|${a.p},${a.q},${a.site}`) - hash32(`${seed}|${b.p},${b.q},${b.site}`)))
    .filter(l => l.length);
  const out = [];
  for (let round = 0; out.length < max && lists.some(l => l.length > round); round++) {
    for (const l of lists) { if (out.length >= max) break; if (l[round]) out.push({ p: l[round].p, q: l[round].q, site: l[round].site }); }
  }
  return out;
}

/**
 * Plan a ticket: `{ok: true, sites, source: 'home'|'overflow'}` or `{ok: false, why: 'nofree'|'readfail', reads}`.
 * `read(p, q)` → Promise<Province|null> (null = the read failed). The home wedge first
 * (batches until three sites or the candidates run out), then the overflow ring.
 */
export async function planTicket({ overviews, faction, ringsOpen, seed, nowBell, read }) {
  let reads = 0, failures = 0;
  const run = async list => {
    const got = [];
    for (let i = 0; i < list.length && reads < AUTO_READ_MAX; i += AUTO_BATCH) {
      for (const c of list.slice(i, i + AUTO_BATCH)) {
        if (reads >= AUTO_READ_MAX) break;
        reads++;
        const pv = await read(c.p, c.q);
        if (pv) got.push(pv); else failures++;
      }
      const sites = pickSites(got, { seed, nowBell });
      if (sites.length >= AUTO_MAX_SITES) return sites;
    }
    return pickSites(got, { seed, nowBell });
  };
  // the home wedge: the overview's free count is an upper bound (sites past site_count read as free), so a
  // province it shows full is full; the others are read and decided on their decoded sites
  const home = seededOrder(candidateProvinces(overviews, faction), seed);
  const a = await run(home);
  if (a.length) return { ok: true, sites: a, source: 'home' };
  const b = await run(seededOrder(overflowCandidates(overviews, faction, ringsOpen), seed));
  if (b.length) return { ok: true, sites: b, source: 'overflow' };
  return failures && failures === reads ? { ok: false, why: 'readfail', reads } : { ok: false, why: 'nofree', reads };
}
