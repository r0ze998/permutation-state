// Player identity (design session, "people" request). The chain keeps no
// name and no picture: a Citizen has a wallet, a faction and holdings. The
// page derives a name and a face from the owner's `citizen_tag` (the first
// 8 bytes of the Citizen address, a function of the wallet; contract §4.1)
// — the same answer on every device, every time, for a human and for a
// shade alike (nothing here can tell them apart, by design).
//
//   identityOf(tag) → {tag, given: {ja, en}, house: {ja, en}, face}
//   displayName(id, {full}) → the name in the page's language
//
// IDENTITY_VERSION freezes the tables below: changing a syllable renames
// everyone, so a new table is a new version (and a migration note).
// A player may replace the derived name with a signed off-chain profile
// (profile.mjs); `withProfile` applies one that verified.
import { lang } from '../../lang.mjs';

export const IDENTITY_VERSION = 1;

// ------------------------------------------------------------------ the seed
/** The u64 citizen tag (BigInt) from 8+ bytes (little-endian, contract §4.1), a hex string, or a bigint/number. */
export function tagOf(x) {
  if (typeof x === 'bigint') return BigInt.asUintN(64, x);
  if (typeof x === 'number') return BigInt.asUintN(64, BigInt(Math.trunc(x)));
  if (typeof x === 'string') return BigInt.asUintN(64, BigInt(/^0x/i.test(x) ? x : `0x${x || '0'}`));
  if (x && x.length >= 8) { let t = 0n; for (let i = 7; i >= 0; i--) t = (t << 8n) | BigInt(x[i]); return t; }
  return 0n;
}
/** The hex key of a tag (16 digits): the cache and profile key. */
export const tagKey = t => tagOf(t).toString(16).padStart(16, '0');

/** A small deterministic generator from a 64-bit tag and a salt (splitmix-style mixing, then mulberry32). */
export function rngOf(tag, salt = 0) {
  const t = tagOf(tag);
  let s = Number((t ^ (t >> 32n)) & 0xffffffffn) ^ Math.imul(salt + 0x9e3779b9, 0x85ebca6b);
  s = Math.imul(s ^ (s >>> 16), 0x7feb352d); s = Math.imul(s ^ (s >>> 15), 0x846ca68b); s ^= s >>> 16;
  return () => {
    s = (s + 0x6d2b79f5) | 0;
    let r = Math.imul(s ^ (s >>> 15), 1 | s);
    r = (r + Math.imul(r ^ (r >>> 7), 61 | r)) ^ r;
    return ((r ^ (r >>> 14)) >>> 0) / 4294967296;
  };
}
const pick = (rnd, list) => list[Math.floor(rnd() * list.length) % list.length];

// ------------------------------------------------------------------ names
// [romaji, katakana] pieces (the katakana written as \u escapes: proper-name
// material, not page text for the translation check). A given name is a start, an optional middle and
// an ending ("Ka"+"i"+"to" → Kaito / カイト); a house name a start and a
// house suffix ("Sa"+"ford" → Saford / サフォード). Every piece reads in both
// languages; the English name is the romaji with its first letter capital.
const START = [
  ['ka', '\u30ab'], ['ki', '\u30ad'], ['ko', '\u30b3'], ['sa', '\u30b5'], ['se', '\u30bb'], ['so', '\u30bd'], ['ta', '\u30bf'], ['to', '\u30c8'],
  ['na', '\u30ca'], ['no', '\u30ce'], ['ha', '\u30cf'], ['hi', '\u30d2'], ['mi', '\u30df'], ['mo', '\u30e2'], ['ra', '\u30e9'], ['ri', '\u30ea'],
  ['ro', '\u30ed'], ['yu', '\u30e6'], ['a', '\u30a2'], ['e', '\u30a8'], ['i', '\u30a4'], ['o', '\u30aa'], ['lu', '\u30eb'], ['fe', '\u30d5\u30a7'],
  ['va', '\u30f4\u30a1'], ['ze', '\u30bc'], ['da', '\u30c0'], ['jo', '\u30b8\u30e7'], ['el', '\u30a8\u30eb'], ['is', '\u30a4\u30b9'], ['ma', '\u30de'], ['ne', '\u30cd'],
];
const MIDDLE = [
  ['', ''], ['', ''], ['', ''], ['i', '\u30a4'], ['ra', '\u30e9'], ['ri', '\u30ea'], ['ro', '\u30ed'], ['na', '\u30ca'], ['ki', '\u30ad'],
  ['ka', '\u30ab'], ['me', '\u30e1'], ['ve', '\u30f4\u30a7'], ['da', '\u30c0'], ['ma', '\u30de'], ['ya', '\u30e4'], ['sa', '\u30b5'],
];
const END = [
  ['n', '\u30f3'], ['ra', '\u30e9'], ['to', '\u30c8'], ['ko', '\u30b3'], ['ka', '\u30ab'], ['ri', '\u30ea'], ['ya', '\u30e4'], ['ren', '\u30ec\u30f3'],
  ['rik', '\u30ea\u30af'], ['el', '\u30a8\u30eb'], ['in', '\u30a4\u30f3'], ['os', '\u30aa\u30b9'], ['a', '\u30a2'], ['e', '\u30a8'], ['o', '\u30aa'], ['mi', '\u30df'],
  ['sha', '\u30b7\u30e3'], ['ris', '\u30ea\u30b9'], ['dan', '\u30c0\u30f3'], ['wen', '\u30a6\u30a7\u30f3'],
];
const HOUSE = [
  ['ford', '\u30d5\u30a9\u30fc\u30c9'], ['wyn', '\u30a6\u30a3\u30f3'], ['mar', '\u30de\u30fc\u30eb'], ['dor', '\u30c9\u30fc\u30eb'], ['vale', '\u30f4\u30a7\u30a4\u30eb'], ['holt', '\u30db\u30eb\u30c8'],
  ['mere', '\u30e1\u30a2'], ['croft', '\u30af\u30ed\u30d5\u30c8'], ['ridge', '\u30ea\u30c3\u30b8'], ['well', '\u30a6\u30a7\u30eb'], ['lund', '\u30eb\u30f3\u30c9'], ['hall', '\u30db\u30fc\u30eb'],
  ['wick', '\u30a6\u30a3\u30c3\u30af'], ['bourne', '\u30dc\u30fc\u30f3'], ['stead', '\u30b9\u30c6\u30c3\u30c9'], ['by', '\u30d3\u30fc'],
];
const cap = s => s.charAt(0).toUpperCase() + s.slice(1);
/** Join romaji pieces without a doubled vowel cluster that reads badly ("aa" → "a"). */
const joinRomaji = parts => parts.join('').replace(/([aeiou])\1+/g, '$1');

/** `{ja, en}` of a given name from a generator. */
/** A name that reads badly: a repeated syllable ("rara"), three vowels in a row, or two at the start. */
const awkward = r => /(..)\1/.test(r) || /[aeiou]{3}/.test(r) || /^[aeiou]{2}/.test(r) || /([aeiou])\1/.test(r);
function givenName(rnd) {
  let out = null;
  for (let i = 0; i < 6; i++) {
    const s = pick(rnd, START), m = pick(rnd, MIDDLE), e = pick(rnd, END);
    const r = joinRomaji([s[0], m[0], e[0]]);
    out = { en: cap(r), ja: s[1] + m[1] + e[1] };
    if (!awkward(r) && r.length >= 3) break;
  }
  return out;
}
function houseName(rnd) {
  const s = pick(rnd, START), h = pick(rnd, HOUSE);
  return { en: cap(joinRomaji([s[0], h[0]])), ja: s[1] + h[1] };
}

// ------------------------------------------------------------------ faces
/** Skin, hair and eye palettes of the generated portraits (avatar.mjs draws them). */
export const SKIN = Object.freeze(['#f2d3b8', '#e8bf9a', '#d6a27a', '#b9805a', '#8f5c3e', '#6a412b']);
export const HAIR = Object.freeze(['#1f1a17', '#3b2a20', '#6b4426', '#a8672f', '#d8b36a', '#8c8c8c', '#e8e2d6', '#7a2e22']);
export const EYES = Object.freeze(['#2b2622', '#4a3626', '#3d5a6b', '#3f5a3a']);
export const HAIR_STYLES = 10;
export const HEADWEAR = Object.freeze(['none', 'none', 'none', 'band', 'hood', 'circlet', 'cap']);

/** The portrait's features: indices into the tables above (avatar.mjs). */
function faceOf(rnd) {
  const beard = rnd() < 0.3;
  return {
    skin: Math.floor(rnd() * SKIN.length), hair: Math.floor(rnd() * HAIR.length), hairStyle: Math.floor(rnd() * HAIR_STYLES),
    eyes: Math.floor(rnd() * EYES.length), brow: Math.floor(rnd() * 3), mouth: Math.floor(rnd() * 3), jaw: Math.floor(rnd() * 3),
    beard: beard ? 1 + Math.floor(rnd() * 3) : 0, headwear: pick(rnd, HEADWEAR), scar: rnd() < 0.08, freckles: rnd() < 0.12,
    bg: Math.floor(rnd() * 4),
  };
}

// ------------------------------------------------------------------ identity
const cache = new Map();
/** The derived identity of a citizen tag (cached; 0 is "no one"). */
export function identityOf(tag) {
  const key = tagKey(tag);
  const hit = cache.get(key);
  if (hit) return hit;
  const rnd = rngOf(tag, IDENTITY_VERSION);
  const id = Object.freeze({ tag: key, given: givenName(rnd), house: houseName(rnd), face: Object.freeze(faceOf(rnd)), profile: false });
  if (cache.size > 20_000) cache.clear();
  cache.set(key, id);
  return id;
}

/** An identity with a verified profile's name (profile.mjs checks the signature first). */
export function withProfile(id, profile) {
  if (!profile?.name) return id;
  const n = String(profile.name).slice(0, 24);
  return Object.freeze({ ...id, given: { ja: n, en: n }, house: { ja: '', en: '' }, profile: true });
}

/** The name in the page's language: "Kaito" or, `full`, "Kaito Saford" / "カイト・サフォード". */
export function displayName(id, { full = false, language = lang() } = {}) {
  if (!id) return '';
  const l = language === 'en' ? 'en' : 'ja';
  const g = id.given[l];
  if (!full || !id.house[l]) return g;
  return l === 'ja' ? `${g}\u30fb${id.house.ja}` : `${g} ${id.house.en}`;
}
