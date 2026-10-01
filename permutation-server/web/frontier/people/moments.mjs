// Moments (docs/frontier/ui-shell/UNITS-REDESIGN.md, after Eternum's arrival punch and
// reveal-yield): short, one-shot effects for changes the page has just seen, each tied to
// a real record — never looping, never invented.
//   harvest   the resources of the worked tiles fly from the fields into the holding
//   built     a building finished: a gold burst and "✓ name" rising over the holding
//   depart    a host set out: a puff of mist and a seal where it stood (its road is sealed)
//   arrive    a revealed arrival: the mist it came out of clears
// `detectMoments(prev, next, now)` compares two small snapshots of the page's state.
import { RADIUS, FLATTEN, project } from '../../map.mjs';
import { tileHex } from '../fgeo.mjs';

export const MOMENT_SECS = { harvest: 2.2, built: 2.8, depart: 2.2, arrive: 1.8 };
export const MOMENT_MAX = 24;

/**
 * The page's state the moments compare: `{harvest: Map key→bell, builds: Map key→{item,label},
 * hosts: Map id→{p, q, tile, state, faction}, arrivals: Set "p,q,tile,host"}`.
 */
export function momentSnapshot({ life = new Map(), constructions = [], provinces = new Map() } = {}) {
  const harvest = new Map(), builds = new Map(), hosts = new Map(), arrivals = new Set();
  for (const [k, rec] of life) if (rec.harvest >= 0) harvest.set(k, rec.harvest);
  for (const c of constructions) builds.set(`${c.p},${c.q},${c.site}`, { label: c.name ?? c.label ?? '' });
  for (const env of provinces.values()) {
    const pv = env?.province;
    if (!pv) continue;
    for (const e of pv.entries ?? []) if (e.state >= 1 && e.state <= 3) hosts.set(String(e.id), { p: pv.p, q: pv.q, tile: e.tile, state: e.state, faction: e.faction });
    const pend = env.inputs;
    if (pend && !pend.resolvedTs) for (const a of pend.arrivals ?? []) if (a.present) arrivals.add(`${pv.p},${pv.q},${a.tile},${a.hostId}`);
  }
  return { harvest, builds, hosts, arrivals };
}

/** The moments between two snapshots (the first snapshot of a page gives none). */
export function detectMoments(prev, next, now) {
  if (!prev) return [];
  const out = [];
  for (const [k, b] of next.harvest) if ((prev.harvest.get(k) ?? -1) < b && prev.harvest.has(k)) { const [p, q, site] = k.split(',').map(Number); out.push({ kind: 'harvest', p, q, site, t0: now }); }
  for (const [k, v] of prev.builds) if (!next.builds.has(k)) { const [p, q, site] = k.split(',').map(Number); out.push({ kind: 'built', p, q, site, label: v.label, t0: now }); }
  for (const [id, h] of next.hosts) { const was = prev.hosts.get(id); if (was && was.state !== 3 && h.state === 3) out.push({ kind: 'depart', p: h.p, q: h.q, tile: h.tile, faction: h.faction, t0: now }); }
  for (const k of next.arrivals) if (!prev.arrivals.has(k)) { const [p, q, tile] = k.split(',').map(Number); out.push({ kind: 'arrive', p, q, tile, t0: now }); }
  return out;
}

/** Keep the moments still playing (and at most MOMENT_MAX). */
export const liveMoments = (list, now) => list.filter(m => now - m.t0 <= (MOMENT_SECS[m.kind] ?? 2)).slice(-MOMENT_MAX);   // (a moment still to come stays)

const ease = k => (k <= 0 ? 0 : k >= 1 ? 1 : k * k * (3 - 2 * k));
const centreOf = (p, q, tile) => { const h = tileHex(p, q, tile); return project(h.q, h.r); };

/** A small icon of a resource (1 wheat, 2 iron, 3 horses), centred at (x, y), radius r. */
function resourceIcon(ctx, x, y, r, res) {
  ctx.save();
  ctx.fillStyle = 'rgba(16,14,10,.78)'; ctx.beginPath(); ctx.arc(x, y, r, 0, Math.PI * 2); ctx.fill();
  ctx.strokeStyle = '#f3d58a'; ctx.lineWidth = r * 0.14; ctx.stroke();
  ctx.lineCap = 'round';
  if (res === 1) { ctx.strokeStyle = '#e8c35a'; ctx.lineWidth = r * 0.16; for (const dx of [-0.3, 0, 0.3]) { ctx.beginPath(); ctx.moveTo(x + dx * r * 0.4, y + r * 0.55); ctx.lineTo(x + dx * r, y - r * 0.45); ctx.stroke(); } }
  else if (res === 2) { ctx.fillStyle = '#b8c0c6'; ctx.beginPath(); ctx.moveTo(x - r * 0.55, y + r * 0.3); ctx.lineTo(x - r * 0.3, y - r * 0.3); ctx.lineTo(x + r * 0.55, y - r * 0.3); ctx.lineTo(x + r * 0.3, y + r * 0.3); ctx.closePath(); ctx.fill(); }
  else { ctx.strokeStyle = '#d9a46a'; ctx.lineWidth = r * 0.2; ctx.beginPath(); ctx.arc(x, y + r * 0.05, r * 0.45, Math.PI * 0.15, Math.PI * 0.85, true); ctx.stroke(); }
  ctx.restore();
}

/**
 * Draw the moments of a frame. `tiles` = the art's tiles (`{p, pq, idx, site, state, x, y, q, r, res}`),
 * `k` = 1 / zoom (screen sizing). Returns how many played.
 */
export function paintMoments(ctx, moments, { tiles = [], k = 1, now }) {
  let n = 0;
  const byHex = new Map(tiles.map(u => [`${u.q},${u.r}`, u]));
  for (const m of moments) {
    const age = now - m.t0, dur = MOMENT_SECS[m.kind] ?? 2, u = Math.max(0, Math.min(1, age / dur));
    if (age < 0 || u >= 1) continue;
    n++;
    if (m.kind === 'harvest' || m.kind === 'built') {
      const home = tiles.find(x => x.p === m.p && x.pq === m.q && x.site === m.site && x.state === 1);
      if (!home) continue;
      if (m.kind === 'harvest') {
        // the worked tiles around the holding (wheat, iron, horses) send their yield in
        const fields = [];
        for (let dq = -2; dq <= 2; dq++) for (let dr = Math.max(-2, -dq - 2); dr <= Math.min(2, -dq + 2); dr++) { const w = byHex.get(`${home.q + dq},${home.r + dr}`); if (w && w.res > 0) fields.push(w); }
        const list = fields.length ? fields.slice(0, 4) : [{ x: home.x, y: home.y + RADIUS * 0.5, res: 1 }];
        list.forEach((w, i) => {
          const v = ease(Math.max(0, Math.min(1, (age - i * 0.15) / 1.2)));
          if (v <= 0) return;
          const x = w.x + (home.x - w.x) * v, y = w.y + (home.y - w.y) * v - Math.sin(v * Math.PI) * RADIUS * 0.6;
          ctx.globalAlpha = v < 1 ? 1 : 1 - Math.min(1, (age - 1.2 - i * 0.15) / 0.4);
          resourceIcon(ctx, x, y, 12 * k, w.res);
        });
        ctx.globalAlpha = 1;
      } else {
        const r = RADIUS * (0.3 + ease(u) * 0.6);
        ctx.save();
        ctx.globalAlpha = 1 - u; ctx.strokeStyle = '#f3d58a'; ctx.lineWidth = 3 * k;
        ctx.beginPath(); ctx.ellipse(home.x, home.y + RADIUS * 0.1, r, r * FLATTEN, 0, 0, Math.PI * 2); ctx.stroke();
        for (let i = 0; i < 8; i++) { const a = (i / 8) * Math.PI * 2; ctx.beginPath(); ctx.moveTo(home.x + Math.cos(a) * r * 0.6, home.y - RADIUS * 0.3 + Math.sin(a) * r * 0.4); ctx.lineTo(home.x + Math.cos(a) * r * 0.9, home.y - RADIUS * 0.3 + Math.sin(a) * r * 0.6); ctx.stroke(); }
        const text = `✓ ${m.label ?? ''}`.trim();
        ctx.font = `800 ${14 * k}px system-ui, -apple-system, sans-serif`; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
        const y = home.y - RADIUS * (0.7 + ease(u) * 0.4), w = ctx.measureText(text).width + 16 * k, h = 22 * k;
        ctx.globalAlpha = u < 0.75 ? 1 : 1 - (u - 0.75) / 0.25;
        ctx.fillStyle = 'rgba(31,26,16,.92)'; ctx.beginPath(); ctx.roundRect?.(home.x - w / 2, y - h / 2, w, h, 10 * k); ctx.fill();
        ctx.strokeStyle = '#f3d58a'; ctx.lineWidth = 1.5 * k; ctx.stroke();
        ctx.fillStyle = '#f3d58a'; ctx.fillText(text, home.x, y + 0.5 * k);
        ctx.restore();
      }
      continue;
    }
    // depart / arrive: mist where the host stood or came out
    const c = centreOf(m.p, m.q, m.tile);
    ctx.save();
    for (let i = 0; i < 7; i++) {
      const a = (i / 7) * Math.PI * 2 + age * 0.5, spread = m.kind === 'depart' ? 0.2 + ease(u) * 0.35 : 0.45 - ease(u) * 0.25;
      ctx.globalAlpha = (m.kind === 'depart' ? Math.sin(u * Math.PI) : 1 - u) * 0.55; ctx.fillStyle = '#eef1f4';
      ctx.beginPath(); ctx.ellipse(c.x + Math.cos(a) * RADIUS * spread, c.y + RADIUS * 0.2 + Math.sin(a) * RADIUS * spread * FLATTEN, RADIUS * 0.26, RADIUS * 0.13, 0, 0, Math.PI * 2); ctx.fill();
    }
    if (m.kind === 'depart' && u > 0.3) {
      // the seal stamps down: the destination is hidden until the arrival bell
      const v = ease(Math.min(1, (u - 0.3) / 0.25)), r = 12 * k * (1.6 - v * 0.6);
      ctx.globalAlpha = u < 0.85 ? 1 : 1 - (u - 0.85) / 0.15;
      ctx.fillStyle = '#8f2418'; ctx.strokeStyle = '#3a0c05'; ctx.lineWidth = 1.6 * k;
      ctx.beginPath(); for (let i = 0; i < 12; i++) { const a = (i / 12) * Math.PI * 2, rr = r * (i % 2 ? 0.86 : 1); i ? ctx.lineTo(c.x + Math.cos(a) * rr, c.y - RADIUS * 0.5 + Math.sin(a) * rr) : ctx.moveTo(c.x + Math.cos(a) * rr, c.y - RADIUS * 0.5 + Math.sin(a) * rr); }
      ctx.closePath(); ctx.fill(); ctx.stroke();
      ctx.fillStyle = '#f3d58a'; ctx.beginPath(); ctx.arc(c.x, c.y - RADIUS * 0.5 - r * 0.15, r * 0.22, 0, Math.PI * 2); ctx.fill();
      ctx.fillRect(c.x - r * 0.1, c.y - RADIUS * 0.5 - r * 0.05, r * 0.2, r * 0.45);
    }
    ctx.restore();
  }
  return n;
}
