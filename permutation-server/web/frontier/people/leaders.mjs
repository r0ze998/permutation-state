// The six factions' faces (design session, "people" request): a leader
// portrait per faction whose costume, prop and backdrop read its doctrine
// (DESIGN.md §4.1, kernel `frontier::doctrine`):
//   0 Aster    · A Wardens of Stone — stone plate, a pike, the wall behind
//   1 Borealis · B Tide              — sea cloak, a spyglass, waves and a sail
//   2 Cinder   · C Flame             — lamellar, a flame crown, the war horn
//   3 Dunmar   · D Verdant           — leaf mantle, antler circlet, a bow
//   4 Ember    · E Lumen             — surveyor's robe, a compass, road lines
//   5 Fjordal  · F Iron              — iron helm, fur collar, a horse crest
// Leaders are presentation: the chain has no leader. Names are fixed per
// faction (proper names: data-name in markup). SVG only (attributes, no
// style), viewBox 240 × 300, legible from 72 px (standings) to 320 px
// (the join cards, the demo's title cards).
import { L } from '../../lang.mjs';
import { FACTION_FILL, FACTION_DARK, FACTION_LIGHT, sigilPath, shade } from './avatar.mjs';

/** The leaders' names (both languages) and titles. */
export const LEADERS = Object.freeze([
  { faction: 0, name: { en: 'Oriane Vell', ja: '\u30aa\u30ea\u30a2\u30fc\u30cc\u30fb\u30f4\u30a7\u30eb' }, title: () => L`石の守り手の元帥` },
  { faction: 1, name: { en: 'Kaito Marrow', ja: '\u30ab\u30a4\u30c8\u30fb\u30de\u30ed\u30a6' }, title: () => L`潮の船長` },
  { faction: 2, name: { en: 'Sedra Ashfane', ja: '\u30bb\u30c9\u30e9\u30fb\u30a2\u30c3\u30b7\u30e5\u30d5\u30a7\u30a4\u30f3' }, title: () => L`炎の伝令` },
  { faction: 3, name: { en: 'Brannoc Elm', ja: '\u30d6\u30e9\u30ce\u30c3\u30af\u30fb\u30a8\u30eb\u30e0' }, title: () => L`新緑の森番の長` },
  { faction: 4, name: { en: 'Ilse Ardent', ja: '\u30a4\u30eb\u30bc\u30fb\u30a2\u30fc\u30c7\u30f3\u30c8' }, title: () => L`光明の測量長` },
  { faction: 5, name: { en: 'Torvald Hride', ja: '\u30c8\u30fc\u30f4\u30a1\u30eb\u30fb\u30d5\u30ea\u30fc\u30c7' }, title: () => L`鉄のヤール` },
]);

/** One-line doctrine pitch per faction (join cards, standings). */
export const DOCTRINE_PITCH = Object.freeze([
  () => L`城壁が安く、長槍兵は迎撃に強い。本拠を落とすには時間がかかる。`,
  () => L`隊商が速く、軽騎兵で動き回る。街道の道標を多く持てる。`,
  () => L`到着の鐘に一撃が重い突撃歩兵。角笛で戦を告げる。`,
  () => L`補給の消耗が半分。側撃に強い森番が遠くまで行く。`,
  () => L`進軍が速い。工兵が道を倍に延ばし、探索で周りも見える。`,
  () => L`兵の維持費が安い重騎兵。奪った自由都市は城壁を残す。`,
]);

const SKIN = ['#e9c3a0', '#c99a72', '#f0d2b6', '#a8724c', '#ddb08a', '#e4bc98'];
const HAIR = ['#3a2a22', '#1e2a33', '#7a2e1c', '#5b4a2a', '#c9a35a', '#b7b0a2'];

/** The shared bust: neck, head, ears, eyes, brows, nose, mouth, shading (centre x 120). */
function face(f, { skin, hair, beard = false, brow = 0 }) {
  const s = skin, sd = shade(s, -0.2), sdd = shade(s, -0.38);
  return [
    `<path d="M100 150 L140 150 L144 186 C134 194 106 194 96 186 Z" fill="${sd}"/>`,
    `<path d="M100 160 C110 168 130 168 140 160 L141 172 C130 178 110 178 99 172 Z" fill="${sdd}" opacity=".35"/>`,
    `<ellipse cx="80" cy="112" rx="8" ry="13" fill="${sd}"/><ellipse cx="160" cy="112" rx="8" ry="13" fill="${sd}"/>`,
    `<path d="M82 100 C82 62 158 62 158 100 C158 128 150 150 120 160 C90 150 82 128 82 100 Z" fill="${s}"/>`,
    `<path d="M86 118 C90 140 102 152 120 158 C106 150 96 138 94 118 Z" fill="${sd}" opacity=".45"/>`,
    `<ellipse cx="97" cy="124" rx="9" ry="5" fill="#e2766a" opacity=".09"/><ellipse cx="143" cy="124" rx="9" ry="5" fill="#e2766a" opacity=".09"/>`,
    // eyes
    `<path d="M93 106 C97 101 107 101 111 106 C107 110 97 110 93 106 Z" fill="#fbf7ef"/><path d="M129 106 C133 101 143 101 147 106 C143 110 133 110 129 106 Z" fill="#fbf7ef"/>`,
    `<circle cx="102.5" cy="105.8" r="3.6" fill="#3a2f28"/><circle cx="138.5" cy="105.8" r="3.6" fill="#3a2f28"/>`,
    `<circle cx="103.6" cy="104.6" r="1.1" fill="#fff"/><circle cx="139.6" cy="104.6" r="1.1" fill="#fff"/>`,
    `<path d="M92 105.5 C97 100 107 100 112 105" stroke="${sdd}" stroke-width="1.6" fill="none"/><path d="M128 105 C133 100 143 100 148 105.5" stroke="${sdd}" stroke-width="1.6" fill="none"/>`,
    // brows (0 level, 1 stern, 2 raised)
    [`<path d="M90 96 L112 93" stroke="${hair}" stroke-width="4" stroke-linecap="round"/><path d="M128 93 L150 96" stroke="${hair}" stroke-width="4" stroke-linecap="round"/>`,
      `<path d="M90 93 L112 96" stroke="${hair}" stroke-width="4.4" stroke-linecap="round"/><path d="M128 96 L150 93" stroke="${hair}" stroke-width="4.4" stroke-linecap="round"/>`,
      `<path d="M90 95 C96 90 106 89 112 92" stroke="${hair}" stroke-width="3.6" fill="none" stroke-linecap="round"/><path d="M128 92 C134 89 144 90 150 95" stroke="${hair}" stroke-width="3.6" fill="none" stroke-linecap="round"/>`][brow],
    // nose and mouth
    `<path d="M120 108 C118 118 114 126 116 129 C118 131 123 131 126 129" stroke="${sdd}" stroke-width="2" fill="none" stroke-linecap="round"/>`,
    beard ? '' : `<path d="M108 140 C114 143 126 143 132 140" stroke="${shade(s, -0.5)}" stroke-width="2.4" fill="none" stroke-linecap="round"/><path d="M111 144 C116 146 124 146 129 144" stroke="${sd}" stroke-width="1.6" fill="none" stroke-linecap="round" opacity=".7"/>`,
  ].join('');
}

/** The torso base: shoulders in the faction's garment (gradient `g`). */
const torso = (g, extra = '') => `<path d="M14 300 C16 236 52 206 96 196 L144 196 C188 206 224 236 226 300 Z" fill="url(#${g})"/>${extra}`;

function backdrop(f, c) {
  switch (f) {
    case 0: return `<g fill="${shade(c.light, -0.12)}"><rect x="0" y="150" width="240" height="150"/><path d="M0 150 V128 H26 V150 H52 V128 H78 V150 H162 V128 H188 V150 H214 V128 H240 V150 Z"/></g>
      <g stroke="${shade(c.light, -0.24)}" stroke-width="2" fill="none"><path d="M0 182 H240 M0 214 H240 M0 246 H240 M40 150 V182 M110 150 V182 M180 150 V182 M75 182 V214 M145 182 V214 M215 182 V214"/></g>`;
    case 1: return `<path d="M0 0 H240 V300 H0 Z" fill="${c.light}"/><path d="M170 30 L170 140 L222 140 Z" fill="#fffaf0" opacity=".9"/><path d="M168 28 V150" stroke="${shade(c.dark, 0.1)}" stroke-width="3"/>
      <g fill="none" stroke="${shade(c.fill, 0.25)}" stroke-width="5" stroke-linecap="round" opacity=".7"><path d="M-10 168 C20 156 40 180 70 168 S120 156 150 168 S200 180 250 166"/><path d="M-10 196 C20 184 40 208 70 196 S120 184 150 196 S200 208 250 194" opacity=".6"/></g>`;
    case 2: return `<radialGradient id="L2r" cx=".5" cy=".9" r=".8"><stop offset="0" stop-color="#f6c065"/><stop offset=".55" stop-color="#e88a3a"/><stop offset="1" stop-color="#5a2410"/></radialGradient><rect width="240" height="300" fill="url(#L2r)"/>
      <g fill="#ffe2a0">${[[30, 60, 2.5], [200, 40, 2], [60, 30, 1.6], [180, 100, 2.4], [24, 140, 1.8], [214, 160, 2.2], [140, 24, 1.4]].map(([x, y, r]) => `<circle cx="${x}" cy="${y}" r="${r}"/>`).join('')}</g>`;
    case 3: return `<rect width="240" height="300" fill="${c.light}"/><g fill="${shade(c.fill, 0.45)}" opacity=".7"><path d="M10 200 L34 120 L58 200 Z"/><path d="M40 210 L70 100 L100 210 Z"/><path d="M160 210 L190 96 L220 210 Z"/><path d="M196 200 L222 130 L248 200 Z"/></g>
      <g fill="${shade(c.fill, 0.25)}" opacity=".55"><path d="M-6 230 L24 140 L54 230 Z"/><path d="M186 232 L216 150 L246 232 Z"/></g>`;
    case 4: return `<rect width="240" height="300" fill="${c.light}"/><g stroke="${shade(c.fill, 0.45)}" stroke-width="1.4" fill="none"><path d="M0 40 H240 M0 80 H240 M0 120 H240 M0 160 H240 M40 0 V300 M80 0 V300 M160 0 V300 M200 0 V300"/></g>
      <path d="M0 230 C60 200 80 150 140 130 S220 90 240 60" stroke="#d9b44a" stroke-width="5" fill="none" stroke-dasharray="10 7"/><circle cx="120" cy="96" r="78" fill="#fff6d6" opacity=".55"/>`;
    case 5: return `<rect width="240" height="300" fill="${c.light}"/><path d="M0 190 L50 110 L80 150 L120 80 L160 150 L190 120 L240 190 Z" fill="${shade(c.fill, 0.5)}" opacity=".8"/><path d="M50 110 L62 128 L42 126 Z M120 80 L134 104 L108 102 Z" fill="#fff"/>
      <path d="M0 190 H240 V300 H0 Z" fill="${shade(c.fill, 0.62)}"/>`;
    default: return '';
  }
}

function costume(f, c, g) {
  const fill = c.fill, dark = c.dark, light = c.light;
  switch (f) {
    case 0: return {
      back: `<path d="M196 300 L206 20" stroke="#6b5640" stroke-width="7" stroke-linecap="round"/><path d="M206 20 L198 0 L214 0 Z M200 22 L212 22 L206 46 Z" fill="#b8bcc0" stroke="#6d7277" stroke-width="2"/>`,
      body: torso(g, `<path d="M84 196 L120 300 L156 196 Z" fill="${fill}"/><path d="M110 230 m-14 0 a14 14 0 1 0 28 0 a14 14 0 1 0 -28 0" fill="${light}"/><path d="M14 300 C16 250 40 216 74 204 L92 232 C66 244 50 270 48 300 Z" fill="#9aa0a6"/><path d="M226 300 C224 250 200 216 166 204 L148 232 C174 244 190 270 192 300 Z" fill="#9aa0a6"/><path d="M60 214 C70 206 84 202 92 204 L96 214 C86 214 72 218 62 226 Z M180 214 C170 206 156 202 148 204 L144 214 C154 214 168 218 178 226 Z" fill="#c6cbd0"/>`),
      head: `<path d="M78 98 C76 50 164 50 162 98 L162 108 L154 108 C152 82 88 82 86 108 L78 108 Z" fill="#a3a9ae"/><path d="M78 98 C80 66 104 52 120 52 C136 52 160 66 162 98 C150 74 90 74 78 98 Z" fill="#c4c9cd"/><path d="M116 40 L124 40 L126 56 L114 56 Z" fill="${fill}"/><path d="M120 22 C110 30 112 40 120 42 C128 40 130 30 120 22 Z" fill="${fill}"/>`,
      opts: { brow: 1 } };
    case 1: return {
      back: `<path d="M30 300 C30 220 60 190 96 184 L144 184 C180 190 210 220 210 300 Z" fill="${dark}"/>`,
      body: torso(g, `<path d="M96 196 L120 248 L144 196 Z" fill="#f4efe3"/><path d="M104 200 L120 232 L136 200" stroke="${dark}" stroke-width="3" fill="none"/><path d="M40 300 L70 214 L90 210 L64 300 Z" fill="${shade(fill, 0.15)}"/><g transform="translate(150 246) rotate(-28)"><rect x="-6" y="-38" width="12" height="76" rx="4" fill="#c99a4a" stroke="#7a5a26" stroke-width="2"/><rect x="-8" y="-40" width="16" height="10" rx="3" fill="#8a6a30"/></g>`),
      head: `<path d="M76 102 C70 56 170 56 164 102 C150 80 90 80 76 102 Z" fill="${fill}"/><path d="M70 104 C90 92 150 92 170 104 L170 112 C150 100 90 100 70 112 Z" fill="${dark}"/><path d="M96 66 C104 58 112 66 120 58 C128 66 136 58 144 66" stroke="#f4efe3" stroke-width="3" fill="none" stroke-linecap="round"/>`,
      hair: `<path d="M84 104 C84 128 80 150 74 160 C86 150 90 132 92 112 Z M156 104 C156 128 160 150 166 160 C154 150 150 132 148 112 Z" fill="${HAIR[1]}"/>`,
      opts: { brow: 0 } };
    case 2: return {
      back: '',
      body: torso(g, `<g fill="${shade(fill, -0.15)}" opacity=".9">${Array.from({ length: 5 }, (_, r) => Array.from({ length: 9 }, (_, i) => `<rect x="${44 + i * 18 + (r % 2) * 9}" y="${214 + r * 16}" width="14" height="12" rx="2"/>`).join('')).join('')}</g><path d="M96 196 L120 220 L144 196 Z" fill="#7a2a18"/><g transform="translate(70 262) rotate(-18)"><path d="M-30 -8 C-10 -18 20 -16 34 -2 L40 -8 L44 10 L28 8 C14 -2 -10 -2 -30 8 Z" fill="#e8d6b0" stroke="#7a5a26" stroke-width="2"/><path d="M-30 -8 L-30 8" stroke="#7a5a26" stroke-width="3"/></g>`),
      head: `<path d="M84 74 L92 44 L102 66 L110 34 L120 62 L130 34 L138 66 L148 44 L156 74 C140 66 100 66 84 74 Z" fill="#f2b33d" stroke="#a8641c" stroke-width="2"/><path d="M84 74 C100 66 140 66 156 74 L156 82 C140 74 100 74 84 82 Z" fill="#a8641c"/><path d="M110 60 C112 50 128 50 130 60 C126 56 114 56 110 60 Z" fill="#ff7a3a"/>`,
      hair: `<path d="M82 98 C80 120 70 160 60 190 C78 176 88 140 92 110 Z M158 98 C160 120 170 160 180 190 C162 176 152 140 148 110 Z" fill="${HAIR[2]}"/>`,
      opts: { brow: 2 } };
    case 3: return {
      back: `<path d="M206 40 C238 110 238 200 200 270" stroke="#6b4a2a" stroke-width="7" fill="none" stroke-linecap="round"/><path d="M206 40 L200 270" stroke="#e8e0c8" stroke-width="1.6"/>`,
      body: torso(g, `<g fill="${shade('#5c8a3a', 0)}">${[[40, 230], [70, 214], [100, 206], [140, 206], [170, 214], [200, 230], [56, 252], [184, 252]].map(([x, y], i) => `<path d="M${x} ${y} C${x - 14} ${y - 6} ${x - 16} ${y + 14} ${x} ${y + 20} C${x + 16} ${y + 14} ${x + 14} ${y - 6} ${x} ${y} Z" fill="${i % 2 ? '#6f9a46' : '#4f7a32'}"/>`).join('')}</g><path d="M104 200 L120 222 L136 200 Z" fill="${light}"/>`),
      head: `<g stroke="#8a6436" stroke-width="5" stroke-linecap="round" fill="none"><path d="M98 80 C90 60 80 48 66 40 M86 58 C80 56 74 58 70 62 M80 50 C80 42 76 36 72 32 M142 80 C150 60 160 48 174 40 M154 58 C160 56 166 58 170 62 M160 50 C160 42 164 36 168 32"/></g><path d="M84 84 C100 76 140 76 156 84 L156 90 C140 82 100 82 84 90 Z" fill="#4f7a32"/>`,
      hair: `<path d="M84 104 C80 132 78 140 82 150 C88 140 90 124 92 110 Z M156 104 C160 132 162 140 158 150 C152 140 150 124 148 110 Z" fill="${HAIR[3]}"/><path d="M100 136 C104 160 136 160 140 136 C138 170 128 188 120 190 C112 188 102 170 100 136 Z" fill="${HAIR[3]}"/>`,
      opts: { brow: 0, beard: true } };
    case 4: return {
      back: '',
      body: torso(g, `<path d="M92 196 L120 300 L148 196 Z" fill="${light}"/><path d="M106 196 L120 248 L134 196" stroke="#d9b44a" stroke-width="4" fill="none"/><g transform="translate(64 256)"><circle r="22" fill="#e6d29a" stroke="#8a6a20" stroke-width="3"/><circle r="16" fill="none" stroke="#8a6a20" stroke-width="1.6"/><path d="M0 -15 L4 0 L0 15 L-4 0 Z" fill="#b8402e"/><circle r="2.6" fill="#8a6a20"/></g>`),
      head: `<circle cx="120" cy="96" r="70" fill="none" stroke="#f2cf5a" stroke-width="3" opacity=".8"/>`,
      hair: `<path d="M82 100 C78 60 162 60 158 100 C150 78 136 72 120 74 C104 72 90 78 82 100 Z" fill="${HAIR[4]}"/><path d="M82 100 C80 128 84 160 96 176 C92 150 90 126 92 108 Z M158 100 C160 128 156 160 144 176 C148 150 150 126 148 108 Z" fill="${HAIR[4]}"/><path d="M96 84 C110 78 130 78 144 84" stroke="#f2cf5a" stroke-width="3" fill="none"/>`,
      opts: { brow: 2 } };
    case 5: return {
      back: '',
      body: torso(g, `<path d="M40 232 C60 206 90 196 120 196 C150 196 180 206 200 232 C180 222 160 226 150 236 C140 222 100 222 90 236 C80 226 60 222 40 232 Z" fill="#cdbfa8"/><path d="M50 228 C70 214 100 206 120 206 C140 206 170 214 190 228" stroke="#a89a82" stroke-width="3" fill="none"/><g transform="translate(120 266)"><path d="M-16 18 C-18 0 -10 -16 4 -20 C10 -22 16 -18 16 -12 L10 -10 C12 -2 10 10 4 18 Z" fill="#b7b0a2" stroke="#55514a" stroke-width="2.4"/><circle cx="6" cy="-12" r="2" fill="#55514a"/></g>`),
      head: `<path d="M78 104 C74 52 166 52 162 104 L162 116 L150 112 C148 80 92 80 90 112 L78 116 Z" fill="#6c6f73"/><path d="M80 96 C82 64 104 52 120 52 C136 52 158 64 160 96 C150 72 90 72 80 96 Z" fill="#8c9095"/><path d="M115 84 H125 V114 C123 118 117 118 115 114 Z" fill="#55585c"/><path d="M80 98 C100 92 140 92 160 98" stroke="#43464a" stroke-width="3" fill="none"/>`,
      hair: `<path d="M94 126 C96 168 112 196 120 198 C128 196 144 168 146 126 C138 146 128 150 120 150 C112 150 102 146 94 126 Z" fill="${HAIR[5]}"/><path d="M104 140 C110 144 130 144 136 140" stroke="${shade(HAIR[5], -0.3)}" stroke-width="2" fill="none"/>`,
      opts: { brow: 1, beard: true } };
    default: return { back: '', body: torso(g), head: '', opts: {} };
  }
}

/** A leader portrait as SVG markup (`size` = width; height 1.25 ×). */
export function leaderSvg(faction, { size = 160, title = null } = {}) {
  const f = Number.isInteger(faction) && faction >= 0 && faction < 6 ? faction : 0;
  const c = { fill: FACTION_FILL[f], dark: FACTION_DARK[f], light: FACTION_LIGHT[f] };
  const g = `L${f}g`;
  const k = costume(f, c, g);
  const a11y = title ? `role="img" aria-label="${String(title).replace(/[<>&"]/g, '')}"` : 'aria-hidden="true" focusable="false"';
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 300" width="${size}" height="${Math.round(size * 1.25)}" class="leader" ${a11y}>
    <defs><linearGradient id="${g}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${shade(c.fill, 0.1)}"/><stop offset="1" stop-color="${c.dark}"/></linearGradient>
    <clipPath id="L${f}c"><rect x="0" y="0" width="240" height="300" rx="18"/></clipPath></defs>
    <g clip-path="url(#L${f}c)">${backdrop(f, c)}${k.back}${k.body}${k.hair && f === 2 ? k.hair : ''}${face(f, { skin: SKIN[f], hair: HAIR[f], ...k.opts })}${k.hair && f !== 2 ? k.hair : ''}${k.head}
    <rect x="0" y="262" width="240" height="38" fill="${c.dark}" opacity=".0"/></g>
    <rect x="3" y="3" width="234" height="294" rx="16" fill="none" stroke="${c.dark}" stroke-width="6"/>
    <g transform="translate(210 30)"><circle r="20" fill="#fffdf6" stroke="${c.dark}" stroke-width="3.5"/><path d="${sigilPath(f, 11)}" fill="${c.fill}"/></g>
  </svg>`;
}
