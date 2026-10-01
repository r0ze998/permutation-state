// Player portraits (design session, "people" request): a small bust drawn
// from an identity's face (identity.mjs) in the faction's colours — the
// garment, the frame and the sigil badge carry the faction, the face the
// person. Pure SVG markup (attributes only, no style: the page CSP allows
// no inline style), readable from 20 px (the map's name tag) to 96 px (the
// panel). `avatarSvg` returns markup for the page; `avatarImage` a cached
// HTMLImageElement for canvases (map, replay; `img-src data:` is allowed).
import { SKIN, HAIR, EYES } from './identity.mjs';

/** Faction colours (fi18n CIV_COLORS order) and their dark trims. */
export const FACTION_FILL = Object.freeze(['#c1504a', '#2f8f84', '#c28f2c', '#7a5fb0', '#3f78c2', '#b5527f']);
export const FACTION_DARK = Object.freeze(['#7d2c27', '#1b5a53', '#7d5a14', '#4b3874', '#24497b', '#6f2d4c']);
export const FACTION_LIGHT = Object.freeze(['#f1d6d2', '#d3ebe7', '#f2e4c5', '#e2daf0', '#d6e2f3', '#f0d6e2']);
const NEUTRAL = { fill: '#8a8a80', dark: '#55554e', light: '#e6e4dc' };
const col = f => (Number.isInteger(f) && f >= 0 && f < 6 ? { fill: FACTION_FILL[f], dark: FACTION_DARK[f], light: FACTION_LIGHT[f] } : NEUTRAL);

/** The sigil of a faction as an SVG path centred on (0, 0), radius 1 (map/layers.mjs SIGILS: circle, triangle, square, diamond, cross, hexagon). */
export function sigilPath(f, r = 1) {
  const k = n => (n * r).toFixed(2);
  switch (f) {
    case 0: return `M ${k(-1)} 0 A ${k(1)} ${k(1)} 0 1 0 ${k(1)} 0 A ${k(1)} ${k(1)} 0 1 0 ${k(-1)} 0 Z`;
    case 1: return `M 0 ${k(-1.05)} L ${k(1)} ${k(0.8)} L ${k(-1)} ${k(0.8)} Z`;
    case 2: return `M ${k(-0.85)} ${k(-0.85)} H ${k(0.85)} V ${k(0.85)} H ${k(-0.85)} Z`;
    case 3: return `M 0 ${k(-1.1)} L ${k(1)} 0 L 0 ${k(1.1)} L ${k(-1)} 0 Z`;
    case 4: return `M ${k(-0.33)} ${k(-1)} H ${k(0.33)} V ${k(-0.33)} H ${k(1)} V ${k(0.33)} H ${k(0.33)} V ${k(1)} H ${k(-0.33)} V ${k(0.33)} H ${k(-1)} V ${k(-0.33)} H ${k(-0.33)} Z`;
    case 5: return `M ${k(-0.5)} ${k(-0.87)} H ${k(0.5)} L ${k(1)} 0 L ${k(0.5)} ${k(0.87)} H ${k(-0.5)} L ${k(-1)} 0 Z`;
    default: return `M ${k(-0.8)} 0 A ${k(0.8)} ${k(0.8)} 0 1 0 ${k(0.8)} 0 A ${k(0.8)} ${k(0.8)} 0 1 0 ${k(-0.8)} 0 Z`;
  }
}

/** Shade a #rrggbb colour by `k` (−1 darker … +1 lighter). */
export function shade(hex, k) {
  const n = parseInt(hex.slice(1), 16);
  const c = [n >> 16, (n >> 8) & 255, n & 255].map(v => Math.round(k >= 0 ? v + (255 - v) * k : v * (1 + k)));
  return `#${c.map(v => Math.max(0, Math.min(255, v)).toString(16).padStart(2, '0')).join('')}`;
}

// ------------------------------------------------------------------ parts (viewBox 0 0 64 64; head centre ≈ (32, 27))
const JAW = [
  'M20 25 C20 14 44 14 44 25 C44 33 41 40 32 42 C23 40 20 33 20 25 Z', // oval
  'M20 24 C20 13 44 13 44 24 L44 31 C44 37 39 41 32 42 C25 41 20 37 20 31 Z', // square
  'M21 25 C21 14 43 14 43 25 C43 33 38 41 32 43 C26 41 21 33 21 25 Z', // narrow
];
/** Hair behind the head (long styles), or ''. */
const HAIR_BACK = [
  '', '', '',
  'M17 26 C15 12 49 12 47 26 L48 44 C44 46 41 44 40 40 L24 40 C23 44 20 46 16 44 Z', // long
  'M18 24 C16 11 48 11 46 24 L47 36 C45 38 43 37 42 35 L22 35 C21 37 19 38 17 36 Z', // shoulder
  '', 'M19 22 C19 10 45 10 45 22 L46 30 C44 31 20 31 18 30 Z', // bun base
  'M17 26 C14 11 50 11 47 27 L50 48 C46 49 43 47 42 43 L22 43 C21 47 18 49 14 48 Z', // very long
  '', '',
];
/** Hair over the forehead. */
const HAIR_FRONT = [
  'M19 24 C18 11 46 11 45 24 C41 18 36 17 32 19 C28 17 23 18 19 24 Z', // short parted
  'M19 23 C19 12 45 12 45 23 C42 21 40 18 38 16 C34 20 26 21 19 23 Z', // swept
  'M20 22 C21 13 43 13 44 22 C40 20 24 20 20 22 Z', // cropped
  'M18 25 C17 11 47 11 46 25 C43 19 38 16 32 18 C26 16 21 19 18 25 Z', // long, parted
  'M19 23 C18 12 46 12 45 23 C40 19 24 19 19 23 Z', // shoulder fringe
  'M22 19 C24 12 40 12 42 19 C38 17 26 17 22 19 Z', // receding
  'M19 23 C19 12 45 12 45 23 C40 20 24 20 19 23 Z M28 9 C28 4 36 4 36 9 C36 12 28 12 28 9 Z', // bun
  'M18 25 C17 10 47 10 46 25 C42 17 22 17 18 25 Z', // very long
  'M19 24 C18 10 46 10 45 24 L42 20 L39 23 L36 19 L32 22 L28 19 L25 23 L22 20 Z', // spiky
  'M20 21 C22 15 42 15 44 21 C40 19 24 19 20 21 Z', // shaven
];
const BEARD = [
  '',
  'M23 34 C25 41 39 41 41 34 C40 40 36 44 32 44 C28 44 24 40 23 34 Z', // short
  'M21 30 C22 42 42 42 43 30 C44 42 39 49 32 49 C25 49 20 42 21 30 Z', // full
  'M27 37 C29 39 35 39 37 37 C37 41 35 45 32 45 C29 45 27 41 27 37 Z', // goatee
];
const BROW = [['M23.5 23.2 L29 22.6', 'M35 22.6 L40.5 23.2'], ['M23.5 23.6 L29 22.2', 'M35 22.2 L40.5 23.6'], ['M23.5 22.8 L29 23.1', 'M35 23.1 L40.5 22.8']];
const MOUTH = ['M28.5 35.4 C30.5 36.6 33.5 36.6 35.5 35.4', 'M29 35.6 L35 35.6', 'M28.5 35 C30.5 37.2 33.5 37.2 35.5 35'];

/**
 * The portrait as SVG markup. `size` sets width/height; `id` makes gradient
 * ids unique when several portraits share a page; `title` names it (else
 * it is decorative: aria-hidden).
 */
export function avatarSvg(identity, faction, { size = 48, title = null, uid = identity?.tag ?? 'x', badge = size >= 28 } = {}) {
  const f = identity?.face ?? { skin: 1, hair: 1, hairStyle: 0, eyes: 0, brow: 0, mouth: 0, jaw: 0, beard: 0, headwear: 'none', bg: 0 };
  const c = col(faction);
  const skin = SKIN[f.skin] ?? SKIN[1], skinD = shade(skin, -0.22), hair = HAIR[f.hair] ?? HAIR[1], hairD = shade(hair, -0.35);
  const eye = EYES[f.eyes] ?? EYES[0];
  const g = `av${uid}`;
  const a11y = title ? `role="img" aria-label="${String(title).replace(/[<>&"]/g, '')}"` : 'aria-hidden="true" focusable="false"';
  const bgPattern = [
    '',
    `<path d="M0 50 L64 30 L64 64 L0 64 Z" fill="${c.fill}" opacity=".10"/>`,
    `<circle cx="50" cy="12" r="16" fill="${c.fill}" opacity=".10"/>`,
    `<path d="M0 12 L64 0 L64 8 L0 22 Z" fill="${c.fill}" opacity=".08"/>`,
  ][f.bg % 4];
  const hood = f.headwear === 'hood';
  const parts = [
    `<defs><radialGradient id="${g}b" cx=".5" cy=".35" r=".75"><stop offset="0" stop-color="#fffdf6"/><stop offset="1" stop-color="${c.light}"/></radialGradient>`,
    `<linearGradient id="${g}g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${shade(c.fill, 0.12)}"/><stop offset="1" stop-color="${c.dark}"/></linearGradient>`,
    `<clipPath id="${g}c"><rect x="2" y="2" width="60" height="60" rx="14"/></clipPath></defs>`,
    `<g clip-path="url(#${g}c)"><rect x="0" y="0" width="64" height="64" fill="url(#${g}b)"/>${bgPattern}<g transform="translate(32 40) scale(1.16) translate(-32 -38)">`,
    hood ? `<path d="M12 64 C12 40 18 12 32 10 C46 12 52 40 52 64 Z" fill="${c.dark}"/>` : '',
    HAIR_BACK[f.hairStyle] && !hood ? `<path d="${HAIR_BACK[f.hairStyle]}" fill="${hairD}"/>` : '',
    // shoulders and garment
    `<path d="M6 64 C7 51 16 46 26 44 L38 44 C48 46 57 51 58 64 Z" fill="url(#${g}g)"/>`,
    `<path d="M24 44 L32 53 L40 44 L37 43 L32 48 L27 43 Z" fill="${shade(c.light, -0.05)}"/>`,
    `<path d="M6 64 C7 51 16 46 26 44 L27 46 C18 48 11 53 10 64 Z" fill="${shade(c.dark, -0.15)}" opacity=".55"/>`,
    // neck
    `<path d="M27 37 L37 37 L38 45 C35 47 29 47 26 45 Z" fill="${skinD}"/>`,
    // ears and head
    `<ellipse cx="20" cy="28" rx="2.6" ry="4" fill="${skinD}"/><ellipse cx="44" cy="28" rx="2.6" ry="4" fill="${skinD}"/>`,
    `<path d="${JAW[f.jaw % 3]}" fill="${skin}"/>`,
    `<path d="M22 31 C23 37 27 40 32 41 C28 39 25 36 24 31 Z" fill="${skinD}" opacity=".35"/>`,
    f.freckles ? `<g fill="${shade(skin, -0.3)}" opacity=".6"><circle cx="25.5" cy="31" r=".6"/><circle cx="27.5" cy="32" r=".6"/><circle cx="36.5" cy="32" r=".6"/><circle cx="38.5" cy="31" r=".6"/></g>` : '',
    // eyes, brows, nose, mouth
    `<ellipse cx="26.5" cy="27" rx="2.4" ry="1.7" fill="#fbf8f2"/><ellipse cx="37.5" cy="27" rx="2.4" ry="1.7" fill="#fbf8f2"/>`,
    `<circle cx="26.8" cy="27.1" r="1.25" fill="${eye}"/><circle cx="37.8" cy="27.1" r="1.25" fill="${eye}"/>`,
    `<circle cx="27.2" cy="26.7" r=".4" fill="#fff"/><circle cx="38.2" cy="26.7" r=".4" fill="#fff"/>`,
    `<g stroke="${hairD}" stroke-width="1.5" stroke-linecap="round" fill="none"><path d="${BROW[f.brow % 3][0]}"/><path d="${BROW[f.brow % 3][1]}"/></g>`,
    `<path d="M32 28 C31.4 30.6 30.6 32 31 32.6 C31.6 33 32.6 33 33.4 32.6" stroke="${shade(skin, -0.35)}" stroke-width=".9" fill="none" stroke-linecap="round"/>`,
    `<path d="${MOUTH[f.mouth % 3]}" stroke="${shade(skin, -0.45)}" stroke-width="1.2" fill="none" stroke-linecap="round"/>`,
    f.scar ? `<path d="M38 22 L41 31" stroke="${shade(skin, -0.3)}" stroke-width=".9" stroke-linecap="round"/>` : '',
    f.beard ? `<path d="${BEARD[f.beard]}" fill="${hair}"/>` : '',
    // hair and headwear
    hood ? `<path d="M15 34 C14 18 22 11 32 11 C42 11 50 18 49 34 C47 24 41 18 32 18 C23 18 17 24 15 34 Z" fill="${c.fill}"/><path d="M17 33 C17 21 24 15 32 15 C40 15 47 21 47 33 C45 24 39 19 32 19 C25 19 19 24 17 33 Z" fill="${c.dark}" opacity=".45"/>`
      : `<path d="${HAIR_FRONT[f.hairStyle % HAIR_FRONT.length]}" fill="${hair}"/>`,
    f.headwear === 'band' ? `<path d="M19 20.5 C26 18 38 18 45 20.5 L45 23 C38 20.6 26 20.6 19 23 Z" fill="${c.fill}"/>` : '',
    f.headwear === 'circlet' ? `<path d="M19.5 19.5 C26 17.5 38 17.5 44.5 19.5 L44.5 21 C38 19.2 26 19.2 19.5 21 Z" fill="#d9b44a"/><path d="M30.6 17.4 L32 15 L33.4 17.4 L32 19.2 Z" fill="${c.fill}" stroke="#d9b44a" stroke-width=".5"/>` : '',
    f.headwear === 'cap' ? `<path d="M18 21 C18 9 46 9 46 21 C40 18 24 18 18 21 Z" fill="${c.fill}"/><path d="M18 21 C24 18.5 40 18.5 46 21 L46 22.6 C40 20.2 24 20.2 18 22.6 Z" fill="${c.dark}"/>` : '',
    `</g></g>`,
    // frame and sigil badge
    `<rect x="2" y="2" width="60" height="60" rx="14" fill="none" stroke="${c.dark}" stroke-width="2.5"/>`,
    // the badge only where it reads (a 20-px name tag keeps the frame colour)
    badge ? `<g transform="translate(52.5 52.5)"><circle r="8" fill="#fffdf6" stroke="${c.dark}" stroke-width="1.6"/><path d="${sigilPath(faction, 4.4)}" fill="${c.fill}"/></g>` : '',
  ];
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="${size}" height="${size}" class="avatar" ${a11y}>${parts.join('')}</svg>`;
}

// ------------------------------------------------------------------ canvas
const images = new Map();
/** A decoded <img> of the portrait for canvases (null until it loads; `onLoad` once it has). */
export function avatarImage(identity, faction, onLoad = () => {}) {
  const key = `${identity?.tag ?? 'x'}:${faction}`;
  const hit = images.get(key);
  if (hit) return hit.ready ? hit.img : null;
  if (typeof globalThis.Image !== 'function') return null;
  const img = new globalThis.Image();
  const rec = { img, ready: false };
  img.onload = () => { rec.ready = true; onLoad(); };
  img.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(avatarSvg(identity, faction, { size: 96, uid: 'c' }))}`;
  if (images.size > 4000) images.clear();
  images.set(key, rec);
  return null;
}
