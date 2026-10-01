// Map pins (UI plan C5; Civ VII's map pins, the feature most missed at its
// launch): the viewer marks a tile or a province, sees the mark on the map
// and the minimap, and finds it again in More or by searching "pin". Kept
// on this device per season (players and spectators alike); at most PIN_MAX.
import { html } from '../../util.mjs';
import { L } from '../../lang.mjs';
import { tileHex } from '../fgeo.mjs';
import { project } from '../../map.mjs';
import { provincePixel } from '../map/layers.mjs';

export const PIN_MAX = 20;
export const PIN_KEY = 'ps-fpins:';

const valid = x => x && Number.isInteger(x.p) && Number.isInteger(x.q) && (x.tile === null || (Number.isInteger(x.tile) && x.tile >= 0 && x.tile < 61));
export const pinKey = x => `${x.p},${x.q},${x.tile ?? '-'}`;

/** The stored pins of a season (damaged records read as none). */
export function loadPins(storage, season) {
  try { const v = JSON.parse(storage.get(`${PIN_KEY}${season}`) ?? '[]'); return Array.isArray(v) ? v.filter(valid).slice(0, PIN_MAX).map(({ p, q, tile, at }) => ({ p, q, tile: tile ?? null, at: Number(at) || 0 })) : []; } catch { return []; }
}
export const savePins = (storage, season, pins) => storage.set(`${PIN_KEY}${season}`, JSON.stringify(pins.slice(0, PIN_MAX)));

/** Add or remove the pin of a place; the newest first, the oldest dropped past PIN_MAX. */
export function togglePin(pins, place, at = Date.now()) {
  const k = pinKey(place);
  if (pins.some(x => pinKey(x) === k)) return pins.filter(x => pinKey(x) !== k);
  return [{ p: place.p, q: place.q, tile: place.tile ?? null, at }, ...pins].slice(0, PIN_MAX);
}
export const hasPin = (pins, place) => !!place && (pins ?? []).some(x => pinKey(x) === pinKey({ p: place.p, q: place.q, tile: place.tile ?? null }));

/** A pin's world point (a tile's centre, or the province's). */
export const pinPoint = x => (x.tile === null ? provincePixel(x.p, x.q) : (h => project(h.q, h.r))(tileHex(x.p, x.q, x.tile)));

/** Draw the pins (screen sized): a gold map pin with a dark rim. */
export function paintPins(ctx, pins, zoom) {
  if (!pins?.length) return;
  const k = 1 / zoom;
  ctx.save();
  for (const x of pins) {
    const c = pinPoint(x), r = 7 * k, top = c.y - 22 * k;
    ctx.fillStyle = 'rgba(0,0,0,.3)'; ctx.beginPath(); ctx.ellipse(c.x, c.y, 5 * k, 2 * k, 0, 0, Math.PI * 2); ctx.fill();
    ctx.fillStyle = '#f3d58a'; ctx.strokeStyle = '#3a2a08'; ctx.lineWidth = 1.6 * k;
    ctx.beginPath(); ctx.moveTo(c.x, c.y); ctx.bezierCurveTo(c.x - r * 1.2, top + r * 1.4, c.x - r * 1.3, top, c.x, top - r * 0.2); ctx.bezierCurveTo(c.x + r * 1.3, top, c.x + r * 1.2, top + r * 1.4, c.x, c.y); ctx.closePath(); ctx.fill(); ctx.stroke();
    ctx.fillStyle = '#3a2a08'; ctx.beginPath(); ctx.arc(c.x, top + r * 0.55, r * 0.38, 0, Math.PI * 2); ctx.fill();
  }
  ctx.restore();
}

/** The list (More): each pin goes there or comes off. */
export function renderPins(pins) {
  return html`<section aria-labelledby="pins-title"><h3 id="pins-title">${L`ピン`}</h3>
    ${pins.length ? html`<ul class="list pins">${pins.map(x => html`<li><button type="button" class="btn small" data-act="goto-pin" data-p="${x.p}" data-q="${x.q}" data-tile="${x.tile ?? ''}">📍 ${x.tile === null ? L`州 ${x.p},${x.q}` : L`州 ${x.p},${x.q} · マス ${x.tile + 1}`}</button>
      <button type="button" class="btn small" data-act="pin-toggle" data-p="${x.p}" data-q="${x.q}" data-tile="${x.tile ?? ''}" aria-label="${L`ピンを外す`}">×</button></li>`)}</ul>`
      : html`<p class="muted">${L`地図でマスや州を選び、「ピンを立てる」で印を付けられます（この端末に保存）。`}</p>`}</section>`;
}
