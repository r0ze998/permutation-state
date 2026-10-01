// The clash report and "verify in this browser" (web design §4.3, §7.9;
// contract §5.11 ResolveFromInputs, §6 CLASH, §8.4 `/h/clash`, §9.5).
//
// A report is read from the herald (`/h/clash/{P},{Q}/{bell}`): the
// ClashInputs bytes after the resolve, the seed and its anchor, the digests
// the program logged. The page trusts none of the herald's decoded JSON for
// what it verifies. "Verify in this browser" re-runs the clash with the
// rules kernel (frontier.wasm) and checks, step by step:
//   kernel     the module's ruleset hash is the season's;
//   seed       the seed is THE anchor's: A from the BellAnchor bytes (or the
//              archive entry), S = the first round at or after A + W(b) + Δ,
//              a SeedCache of round S with that A (or the archive's seed);
//   inputs     the ClashInputs bytes are this province-bell's and resolved;
//   recompute  the kernel input rebuilt from the Province bytes before the
//              resolve and the ClashInputs bytes (§5.11), resolved by the
//              kernel; the page re-hashes the kernel's outcome itself;
//   digest     the outcome digest equals the one the Province recorded after
//              the resolve (its own bytes), else the CLASH log's;
//   fates      every present arrival's fate and troops in the ClashInputs
//              fate table (written by the program) equal the kernel's.
// Green only when every step passes; red when the digest, the fates or the
// seed disagree; grey ("could not check") when something is missing.
//
// The kernel input is built by the kernel's own `resolve_from_inputs` when
// the module answers it, else by `buildClashArgs` below, a transcription of
// the §5.11 text (the herald's `Provisional` builder follows the same text).
// The report names the builder it used. The codec of the `resolve_clash`
// export (frontier-wasm `ClashArgs` / `ClashOut`, borsh) is here, pinned
// against the recorded native call in frontier-wasm/vectors/wasm-vectors.json
// by web-frontier-practice.test.mjs.
import { html, raw } from '../../util.mjs';
import { L, fmtNum } from '../../lang.mjs';
import { sha256 } from '../../sdk/sha256.mjs';
import { toHex, fromBase64 } from '../../sdk/bytes.mjs';
import { Writer, Reader, OK } from '../wasm.mjs';
import { decode as decodeAccount } from '../fcodec.mjs';
import { hostId } from '../faddr.mjs';
import { regionOf } from '../fgeo.mjs';
import { seedRound, bellEnd, dayOf } from '../clock.mjs';
import { factionName, UNITS as UNIT_TEXT, STANCES as STANCE_TEXT, FATES as FATE_TEXT } from '../fi18n.mjs';
import { UNIT_ORDER } from '../fland.mjs';
import { swatch } from './shell.mjs';
import { personChip } from '../people/ui.mjs';

// ------------------------------------------------------------------ the resolve_clash codec (borsh)
/** Postures in borsh order: Stance(Hold|Assault|Flank|Brace), then Disarray. */
export const POSTURES = Object.freeze(['Hold', 'Assault', 'Flank', 'Brace', 'Disarray']);
export const DISARRAY = 4;
/** Fates in borsh order (ClashInputs `fate` = index + 1). */
export const FATE_KINDS = Object.freeze(['Stays', 'Withdrew', 'Bounced', 'Retreated', 'Destroyed']);
/** The camp garrison's id (`u64::MAX − gen`, §5.11 and the herald's builder). */
export const campId = gen => 0xffffffffffffffffn - BigInt(gen >>> 0);
/** NEUTRAL faction (camps, Free Cities). */
export const NEUTRAL = 6;
/** A host's (and a garrison's) troop cap, milli-troops (`host::MAX_HOST_TROOPS`). */
export const MAX_HOST_TROOPS = 30_000_000;
/** Garrisons a clash takes (`clash::MAX_GARRISONS`); the camp only below it. */
export const MAX_GARRISONS = 12;
/** Milli-troops per whole troop (a camp stores whole troops). */
export const MILLI = 1000;
/** Doctrine multipliers are stored with 0 reading as 1.0 (the program's `bps`). */
const bpsOf = v => (v === 0 ? 10_000 : v);
export const PROVINCE_TILES = 61;
export const SITES = 12;
/** Entries a Province stores (`Occupancy::STORAGE`). */
export const STORAGE = 56;
const NO_BELL = 0xffffffff;
const OUTCOME_DOMAIN = 'frontier/clash-outcome';

const writePosture = (w, p) => (p === DISARRAY ? w.u8(1) : w.u8(0).u8(p));
const readPosture = r => (r.u8() === 1 ? DISARRAY : r.u8());
function writeFighter(w, f) {
  w.u64(f.id).u8(f.faction).u8(f.unit).u32(f.troops).u16(f.stamina).u8(f.tile);
  writePosture(w, f.posture);
  if (f.retreatBps === null || f.retreatBps === undefined) w.u8(0); else w.u8(1).u32(f.retreatBps);
  return w.u32(f.dealtBps);
}
const readFighter = r => ({ id: r.u64(), faction: r.u8(), unit: r.u8(), troops: r.u32(), stamina: r.u16(), tile: r.u8(), posture: readPosture(r), retreatBps: r.option(x => x.u32()), dealtBps: r.u32() });
const writeGarrison = (w, g) => { w.u64(g.id).u8(g.faction).u8(g.tile).u32(g.troops).bool(g.walls); return writePosture(w, g.posture); };
const readGarrison = r => ({ id: r.u64(), faction: r.u8(), tile: r.u8(), troops: r.u32(), walls: r.bool(), posture: readPosture(r) });

/** `ClashArgs` → the borsh input of `resolve_clash`. */
export function encodeClashArgs(a) {
  const w = new Writer();
  w.i32(a.province.p).i32(a.province.q).u32(a.bell).fixed(a.seed, 32);
  for (const t of a.terrain.terrain) w.u8(t);
  for (const x of a.terrain.resource) { if (x === null || x === undefined) w.u8(0); else w.u8(1).u8(x); }
  w.fixed(a.terrain.sites, SITES).u8(a.terrain.siteCount);
  w.vec(a.residents, writeFighter).vec(a.garrisons, writeGarrison).vec(a.arrivals, writeFighter);
  w.u64(a.relations ?? 0n);
  return w.fixed(a.occupancy.pending, 8).u8(a.occupancy.storageFree).bytes();
}

/** The borsh input of `resolve_clash` → `ClashArgs` (the recorded vector's reader). */
export function decodeClashArgs(bytes) {
  const r = new Reader(bytes);
  const province = { p: r.i32(), q: r.i32() }, bell = r.u32(), seed = r.fixed(32);
  const terrain = { terrain: Array.from({ length: PROVINCE_TILES }, () => r.u8()), resource: Array.from({ length: PROVINCE_TILES }, () => r.option(x => x.u8())), sites: Array.from(r.fixed(SITES)), siteCount: r.u8() };
  const out = { province, bell, seed, terrain, residents: r.vec(readFighter), garrisons: r.vec(readGarrison), arrivals: r.vec(readFighter), relations: r.u64(), occupancy: { pending: Array.from(r.fixed(8)), storageFree: r.u8() } };
  r.end();
  return out;
}

function writeOutcome(w, o) {
  w.i32(o.province.p).i32(o.province.q).u32(o.bell);
  w.vec(o.fighters, (x, f) => {
    x.u64(f.id).bool(f.arrival).u32(f.troops).u16(f.stamina).u8(FATE_KINDS.indexOf(f.fate.kind));
    if (f.fate.kind === 'Stays' || f.fate.kind === 'Withdrew') x.u8(f.fate.tile);
    x.bool(f.engaged);
  });
  w.vec(o.garrisons, (x, g) => x.u64(g.id).u32(g.troops).bool(g.attackersHold).u8(g.holders).bool(g.defenderPresent));
  return w.u32(o.engagements);
}
function readOutcome(r) {
  const province = { p: r.i32(), q: r.i32() }, bell = r.u32();
  const fighters = r.vec(x => {
    const f = { id: x.u64(), arrival: x.bool(), troops: x.u32(), stamina: x.u16() };
    const k = FATE_KINDS[x.u8()];
    if (!k) throw new Error('borsh: fate');
    f.fate = k === 'Stays' || k === 'Withdrew' ? { kind: k, tile: x.u8() } : { kind: k };
    f.engaged = x.bool();
    return f;
  });
  const garrisons = r.vec(x => ({ id: x.u64(), troops: x.u32(), attackersHold: x.bool(), holders: x.u8(), defenderPresent: x.bool() }));
  return { province, bell, fighters, garrisons, engagements: r.u32() };
}

/** The borsh answer of `resolve_clash` → `{outcome, digest}` (digest hex). */
export function decodeClashOut(bytes) {
  const r = new Reader(bytes);
  const outcome = readOutcome(r);
  const digest = toHex(r.fixed(32));
  r.end();
  return { outcome, digest };
}

/** `ClashOutcome::digest` recomputed by the page: sha256("frontier/clash-outcome" ‖ borsh(outcome)). */
export const outcomeDigest = outcome => toHex(sha256(OUTCOME_DOMAIN, writeOutcome(new Writer(), outcome).bytes()));

/**
 * Resolve `args` in the kernel: `{ok: true, outcome, digest}`, or
 * `{ok: false, why, kernelCode?, arg?, reason?}` (a refusal names the
 * kernel's code). The page re-hashes the outcome itself; a different hash is
 * `CodecMismatch`. Verification reasons are `why` (texts: `codeText`), not
 * failure codes of an action (fi18n.failureText).
 */
export function resolveArgs(kernel, args) {
  const r = kernel.call('resolve_clash', encodeClashArgs(args));
  if (!r.ok) return { ok: false, why: r.status === 2 ? 'Refused' : 'BadInput', arg: r.arg, kernelCode: r.code, reason: r.reason };
  const out = decodeClashOut(r.value);
  if (outcomeDigest(out.outcome) !== out.digest) return { ok: false, why: 'CodecMismatch' };
  return { ok: true, ...out };
}

// ------------------------------------------------------------------ the §5.11 builder
const hexOf = b => (typeof b === 'string' ? b.toLowerCase() : toHex(b));

/**
 * The kernel input of ResolveFromInputs (contract §5.11) from the Province
 * bytes before the resolve and the ClashInputs bytes (both decoded by
 * fcodec), for `bell` and `seed` — the program's `model::build` as the
 * native readers transcribe it (fclient `clash_model`, pinned against it by
 * the `clash_model` vectors): residents in state 1 with `from_bell ≤ bell`
 * at their values at the bell, fighting at Hold, `dealt` 0 reading 1.0;
 * garrisons from the site mirror (pending changes with a non-zero delta of
 * earlier bells must be settled) capped at MAX_HOST_TROOPS, with walls
 * effective at the bell; the camp, if present and fewer than MAX_GARRISONS
 * garrisons stand, as a NEUTRAL garrison of `troops × 1,000` (capped);
 * the present arrival records with their revealed stance and retreat
 * ("never" = 0); residents and arrivals in id order; relations from the
 * Province; the storage room from the entry states. `{ok: true, args,
 * certain}` — `certain` is false whenever the day's camp check runs at this
 * bell (`day(bell) ≥ next_check_day`: the program may spawn or respawn the
 * camp first, I-56; this builder does not model it) — or `{ok: false, why,
 * detail}`.
 */
export function buildClashArgs({ province: pv, inputs: ci, bell, seed }) {
  if (ci.p !== pv.p || ci.q !== pv.q || ci.bell !== bell) return { ok: false, why: 'WrongKey', detail: 'inputs and province are different province-bells' };
  const residents = [], garrisons = [], arrivals = [];
  const pending = [0, 0, 0, 0, 0, 0, 0, 0];
  let used = 0;
  for (const e of pv.entries) {
    if (e.state === 1) {
      used++;
      if (e.fromBell > bell) continue;
      // Host::values_at: a pending change issued before this bell must have been settled.
      if (e.pendOp >= 1 && e.pendOp <= 4 && bell > e.pendBell) return { ok: false, why: 'Unsettled', detail: `host ${e.id}` };
      if (e.unit >= UNIT_ORDER.length) return { ok: false, why: 'BadUnit', detail: `host ${e.id}` };
      const stamina = bell <= e.staminaBell ? e.staminaValue : Math.min(120, e.staminaValue + (bell - e.staminaBell));
      residents.push({ id: BigInt(e.id), faction: e.faction, unit: e.unit, troops: e.troops, stamina, tile: e.tile, posture: 0, retreatBps: null, dealtBps: bpsOf(e.dealtBps) });
    } else if (e.state === 2) {
      used++;
      if (e.faction < 8) pending[e.faction] = Math.min(255, pending[e.faction] + 1);
    } else if (e.state === 3) used++;
  }
  for (let i = 0; i < Math.min(pv.siteCount, SITES); i++) {
    const m = pv.siteMirror[i];
    if (m.state !== 1) continue;
    // GarrisonState::at: a pending change (delta ≠ 0) of an earlier bell must have been settled.
    for (const [pb, d] of [[m.pend0Bell, m.pend0Delta], [m.pend1Bell, m.pend1Delta]]) {
      if (pb !== NO_BELL && Number(d ?? 0) !== 0 && pb < bell) return { ok: false, why: 'Unsettled', detail: `garrison ${i}` };
    }
    const walls = m.wallsCommitted > 0 || [[m.wallItem0Bell, m.wallItem0Delta], [m.wallItem1Bell, m.wallItem1Delta]].some(([eff, d]) => d > 0 && eff !== NO_BELL && eff <= bell);
    garrisons.push({ id: hostId({ p: pv.p, q: pv.q, site: i, gen: m.gen, seq: 0 }), faction: m.faction, tile: pv.sites[i], troops: Math.min(m.garrison, MAX_HOST_TROOPS), walls, posture: 0 });
  }
  if (pv.camp.state === 1 && garrisons.length < MAX_GARRISONS) {
    garrisons.push({ id: campId(pv.camp.gen), faction: NEUTRAL, tile: pv.camp.tile, troops: Math.min(pv.camp.troops * MILLI, MAX_HOST_TROOPS), walls: false, posture: 0 });
  }
  for (const a of ci.arrivals) {
    if (a.present !== 1) continue;
    if (a.unit >= UNIT_ORDER.length || a.stance > 3) return { ok: false, why: 'BadRecord', detail: `arrival ${a.hostId}` };
    arrivals.push({ id: BigInt(a.hostId), faction: a.faction, unit: a.unit, troops: a.troops, stamina: a.stamina, tile: a.tile, posture: a.stance, retreatBps: a.retreat === 0 ? null : a.retreat, dealtBps: bpsOf(a.dealt) });
  }
  // The kernel takes residents and arrivals in id order.
  const byId = (x, y) => (x.id < y.id ? -1 : x.id > y.id ? 1 : 0);
  residents.sort(byId);
  arrivals.sort(byId);
  const terrain = {
    terrain: Array.from(pv.terrain),
    resource: Array.from(pv.resource, x => (x === 0 ? null : x - 1)),
    sites: Array.from(pv.sites),
    siteCount: pv.siteCount,
  };
  // The day's check runs at the first resolve (or skip) of a day with day ≥ next_check_day.
  const certain = dayOf(bell) < (pv.camp.nextCheckDay ?? 0);
  return {
    ok: true, certain,
    args: { province: { p: pv.p, q: pv.q }, bell, seed: Uint8Array.from(seed), terrain, residents, garrisons, arrivals,
      relations: BigInt(pv.relations ?? 0n), occupancy: { pending, storageFree: Math.max(0, STORAGE - used) } },
  };
}

/** The borsh input the page offers the kernel's own builder: (Province bytes, ClashInputs bytes, bell, seed). */
export const fromInputsRequest = ({ provinceBytes, inputsBytes, bell, seed }) =>
  new Writer().bytesVec(provinceBytes).bytesVec(inputsBytes).u32(bell).fixed(seed, 32).bytes();

// ------------------------------------------------------------------ the seed of THE anchor
/**
 * THE anchor's seed for (bell, region) from the herald's bell-region answer
 * (`{record, anchor}`, the anchor decoded from its bytes) and the season
 * clock: `{ok: true, a, s, seed (hex), source: {cache: nonce} | {archive}}`
 * or `{ok: false, why}`. Only a SeedCache whose round is S = seed_round(A +
 * W(bell) + Δ) and whose A is THE anchor's counts; after archiving, the
 * archive entry's seed.
 */
export function anchorSeed(clock, bell, bellRegion) {
  const j = bellRegion?.record;
  if (!j) return { ok: false, why: 'NoBellRecord' };
  let a = bellRegion.anchor ? Number(bellRegion.anchor.a) : null;
  if (a === null && j.archived && j.archive?.aOff !== undefined && j.archive?.aOff !== null) a = bellEnd(clock.genesisTs, bell) + Number(j.archive.aOff);
  if (a === null) return { ok: false, why: 'NoAnchor' };
  const s = seedRound(clock.drand, a + clock.window(bell), clock.margin);
  if (j.archived && typeof j.archive?.seed === 'string') return { ok: true, a, s, seed: j.archive.seed.toLowerCase(), source: { archive: true } };
  // The cache must name THE anchor's A (a cache without one is not trusted).
  const c = (j.caches ?? []).find(x => String(x.round) === String(s) && x.A !== undefined && x.A !== null && Number(x.A) === a && typeof x.seed === 'string');
  return c ? { ok: true, a, s, seed: c.seed.toLowerCase(), source: { cache: c.nonce } } : { ok: false, why: 'SeedNotReady', a, s };
}

// ------------------------------------------------------------------ verify in this browser
export const VERIFY_STEPS = Object.freeze(['kernel', 'seed', 'inputs', 'recompute', 'digest', 'fates']);

/**
 * Verify one clash in this browser. `deps = {herald, kernel (or a thunk
 * that loads it), clock (seasonClock), season (decoded)}`; `clash` = the
 * herald's `clash()` answer `{report, inputs}` for (p, q, bell). Answer:
 * `{result: 'match'|'mismatch'|'unchecked', steps: {id: {ok: true|false|null,
 * why?, detail?}}, args?, outcome?, digest?, builder?}`. Never rejects.
 */
export async function verifyClash({ p, q, bell, clash, herald, kernel, clock, season }) {
  const steps = Object.fromEntries(VERIFY_STEPS.map(id => [id, { ok: null }]));
  const out = { result: 'unchecked', steps, args: null, outcome: null, digest: null, builder: null, seed: null };
  const done = () => {
    const vals = VERIFY_STEPS.map(id => steps[id].ok);
    if (vals.every(v => v === true)) out.result = 'match';
    else if ((steps.digest.ok === false || steps.fates.ok === false) && out.certain !== false) out.result = 'mismatch';
    else if (steps.seed.ok === false && steps.seed.why === 'SeedMismatch') out.result = 'mismatch';
    else out.result = 'unchecked';
    return out;
  };
  try {
    // kernel: the module and the season run the same rules.
    let k = null;
    try { k = typeof kernel === 'function' ? await kernel() : kernel; } catch (e) { steps.kernel = { ok: null, why: e?.code ?? 'NoWasm' }; }
    if (k) {
      const want = season?.rulesetHash ? hexOf(season.rulesetHash) : null;
      steps.kernel = want && k.rulesetHash() !== want ? { ok: false, why: 'RulesetMismatch' } : { ok: true };
    }
    // seed: THE anchor's S round.
    const report = clash?.report ?? {};
    const br = await herald.bellRegion(bell, regionOf(p, q));
    const sd = br.ok ? anchorSeed(clock, bell, br) : { ok: false, why: br.code ?? 'NoBellRecord' };
    if (!sd.ok) steps.seed = { ok: null, why: sd.why };
    else if (typeof report.seed === 'string' && report.seed.toLowerCase() !== sd.seed) steps.seed = { ok: false, why: 'SeedMismatch' };
    else if (report.anchor?.A !== undefined && report.anchor?.A !== null && Number(report.anchor.A) !== sd.a) steps.seed = { ok: false, why: 'SeedMismatch' };
    else steps.seed = { ok: true, detail: sd };
    out.seed = sd.ok ? sd : null;
    // inputs: this province-bell's, resolved.
    const ci = clash?.inputs ?? null;
    if (!ci) steps.inputs = { ok: null, why: 'NoInputs' };
    else if (ci.p !== p || ci.q !== q || ci.bell !== bell) steps.inputs = { ok: false, why: 'WrongKey' };
    else if (!(ci.flags & 2)) steps.inputs = { ok: null, why: 'NotResolved' };
    else steps.inputs = { ok: true };
    // recompute: the kernel input rebuilt, resolved by the kernel.
    const pvB64 = report.province_before_b64 ?? null;
    if (!k || steps.kernel.ok !== true || !sd.ok || steps.inputs.ok !== true) steps.recompute = { ok: null, why: 'Prerequisite' };
    else if (!pvB64) steps.recompute = { ok: null, why: 'NoProvinceBefore' };
    else {
      const pvBytes = fromBase64(pvB64);
      let pv = null;
      try { pv = decodeAccount('Province', pvBytes, { seasonId: season?.seasonId }); } catch (e) { steps.recompute = { ok: null, why: e.code ?? 'BadRecord' }; }
      if (pv && (pv.p !== p || pv.q !== q || pv.resolvedNext !== bell)) steps.recompute = { ok: null, why: 'WrongProvinceBefore' };
      else if (pv) {
        const seed = Uint8Array.from(sd.seed.match(/../g), x => parseInt(x, 16));
        // The kernel's own builder first (frontier-wasm resolve_from_inputs), else the §5.11 transcription.
        const own = k.call('resolve_from_inputs', fromInputsRequest({ provinceBytes: pvBytes, inputsBytes: fromBase64(report.inputs_b64), bell, seed }));
        let res = null;
        if (own.ok && own.status === undefined) {
          try { const d = decodeClashOut(own.value); res = outcomeDigest(d.outcome) === d.digest ? { ok: true, ...d } : { ok: false, why: 'CodecMismatch' }; } catch { res = null; }
          if (res) { out.builder = 'kernel'; out.certain = true; }
        }
        if (res?.ok) {
          // The page's own transcription too (W6-D): its args drive "what if", and it is kept only when it agrees with the kernel's builder.
          const b = buildClashArgs({ province: pv, inputs: ci, bell, seed });
          const mine = b.ok ? resolveArgs(k, b.args) : null;
          out.pageBuilder = mine?.ok && mine.digest === res.digest ? 'agrees' : b.ok && b.certain === false ? 'uncertain' : 'differs';
          if (out.pageBuilder === 'agrees') out.args = b.args;
        }
        if (!res) {
          const b = buildClashArgs({ province: pv, inputs: ci, bell, seed });
          if (!b.ok) res = { ok: false, why: b.why };
          else {
            out.args = b.args; out.certain = b.certain; out.builder = 'page';
            res = resolveArgs(k, b.args);
          }
        }
        if (res.ok) { steps.recompute = { ok: true }; out.outcome = res.outcome; out.digest = res.digest; } else steps.recompute = { ok: null, why: res.why };
      }
    }
    // digest: the Province's own record after the resolve, else the CLASH log's.
    if (out.digest) {
      const env = await herald.province(p, q, bell);
      const post = env.ok ? env.province : null;
      if (post && post.resolveSummary?.bell === bell) {
        steps.digest = hexOf(post.lastDigest) === out.digest ? { ok: true, detail: 'province' } : { ok: false, why: 'DigestMismatch', detail: 'province' };
      } else if (typeof report.outcomeDigest === 'string') {
        steps.digest = report.outcomeDigest.toLowerCase() === out.digest ? { ok: true, detail: 'log' } : { ok: false, why: 'DigestMismatch', detail: 'log' };
      } else steps.digest = { ok: null, why: 'NoDigest' };
    } else steps.digest = { ok: null, why: 'Prerequisite' };
    // fates: the program's fate table against the kernel's outcome.
    if (out.outcome && ci) {
      const bad = [];
      for (const a of ci.arrivals.filter(x => x.present === 1)) {
        const f = out.outcome.fighters.find(x => x.arrival && x.id === BigInt(a.hostId));
        if (!f || FATE_KINDS.indexOf(f.fate.kind) + 1 !== a.fate || f.troops !== a.troopsAfter) bad.push(String(a.hostId));
      }
      steps.fates = bad.length ? { ok: false, why: 'FateMismatch', detail: bad } : { ok: true };
    } else steps.fates = { ok: null, why: 'Prerequisite' };
  } catch (e) {
    out.error = String(e?.message ?? e);
  }
  return done();
}

// ------------------------------------------------------------------ the report's rows
const troopsOf = milli => Math.floor(Number(milli) / 1000);
const unitName = u => UNIT_TEXT[UNIT_ORDER[u]] ?? `#${u}`;
const postureName = p => STANCE_TEXT[POSTURES[p]] ?? '';

/**
 * Rows of a resolved clash: `[{id, kind: 'resident'|'arrival'|'garrison'|'camp',
 * faction, unit?, before, after, stamina?, posture, fate?, tile?, engaged?,
 * walls?, mine}]` (troops in whole troops), sides first by faction.
 * `mine(id)` marks the viewer's own hosts and holdings.
 */
export function outcomeRows(args, outcome, mine = () => false) {
  const rows = [];
  const res = new Map(outcome.fighters.map(f => [`${f.arrival ? 'a' : 'r'}${f.id}`, f]));
  for (const [kind, list] of [['resident', args.residents], ['arrival', args.arrivals]]) {
    for (const f of list) {
      const r = res.get(`${kind === 'arrival' ? 'a' : 'r'}${f.id}`);
      rows.push({ id: String(f.id), kind, faction: f.faction, unit: f.unit, before: troopsOf(f.troops), after: r ? troopsOf(r.troops) : null, stamina: r?.stamina ?? null,
        posture: f.posture, retreatBps: f.retreatBps, fate: r?.fate.kind ?? null, tile: r?.fate.tile ?? f.tile, engaged: !!r?.engaged, mine: !!mine(f.id) });
    }
  }
  const gres = new Map(outcome.garrisons.map(g => [String(g.id), g]));
  for (const g of args.garrisons) {
    const r = gres.get(String(g.id));
    rows.push({ id: String(g.id), kind: g.faction === NEUTRAL ? 'camp' : 'garrison', faction: g.faction, before: troopsOf(g.troops), after: r ? troopsOf(r.troops) : null,
      posture: g.posture, walls: g.walls, tile: g.tile, fate: r && r.troops === 0 && g.troops > 0 ? 'Destroyed' : null, mine: !!mine(g.id) });
  }
  return rows.sort((a, b) => a.faction - b.faction || ['garrison', 'camp', 'resident', 'arrival'].indexOf(a.kind) - ['garrison', 'camp', 'resident', 'arrival'].indexOf(b.kind));
}

/** Rows before verification, from the ClashInputs bytes alone (the arrivals the program wrote). */
export function inputRows(ci, mine = () => false) {
  return (ci?.arrivals ?? []).filter(a => a.present === 1).map(a => ({
    id: String(a.hostId), kind: 'arrival', faction: a.faction, unit: a.unit, before: troopsOf(a.troops), after: a.fate ? troopsOf(a.troopsAfter) : null,
    posture: a.stance, retreatBps: a.retreat || null, fate: FATE_KINDS[a.fate - 1] ?? null, tile: a.tile, mine: !!mine(a.hostId),
  })).sort((a, b) => a.faction - b.faction);
}

// ------------------------------------------------------------------ the headline
/** Stances that beat another (the triangle): Assault > Flank > Brace > Assault. */
const BEATS = { 1: 2, 2: 3, 3: 1 };
const lossOf = r => (r.after === null ? 0 : Math.max(0, r.before - r.after));

/**
 * The headline of a clash for the viewer (UI plan D1): `{mine, lost, before,
 * result: 'won'|'held'|'fell'|'turned'|'none', reached, fates}` from the
 * rows; `mine` false for a spectator (then `leader` is the faction with the
 * most troops left on the field, or null).
 */
export function summaryOf(rows) {
  const own = rows.filter(r => r.mine);
  const known = rows.every(r => r.after !== null);
  if (!own.length) {
    const left = new Map();
    for (const r of rows) if (r.after !== null && r.kind !== 'camp' && (r.fate === null || r.fate === 'Stays')) left.set(r.faction, (left.get(r.faction) ?? 0) + r.after);
    let leader = null, best = 0;
    for (const [f, n] of left) if (n > best) { best = n; leader = f; }
    return { mine: false, known, leader, lost: rows.reduce((a, r) => a + lossOf(r), 0), factions: new Set(rows.map(r => r.faction)).size };
  }
  const lost = own.reduce((a, r) => a + lossOf(r), 0), before = own.reduce((a, r) => a + r.before, 0);
  const arrivals = own.filter(r => r.kind === 'arrival');
  const reached = arrivals.length ? arrivals.some(r => r.fate === 'Stays') : null;
  const fell = own.every(r => r.fate === 'Destroyed' || r.after === 0);
  const turned = own.every(r => ['Withdrew', 'Bounced', 'Retreated', 'Routed'].includes(r.fate));
  const foes = rows.filter(r => !r.mine && r.faction !== own[0].faction);
  const foesLeft = foes.filter(r => r.after !== null && r.after > 0 && (r.fate === null || r.fate === 'Stays')).length;
  const result = !known ? 'none' : fell ? 'fell' : turned ? 'turned' : foesLeft === 0 ? 'won' : 'held';
  return { mine: true, known, lost, before, result, reached, fates: own.map(r => r.fate).filter(Boolean) };
}

/**
 * Per-tile detail (UI plan D2, spec §7.9): `[{tile, sides: [{faction, lost,
 * dealt, stance, mine}], edges: [{winner, loser}], retaliation}]` — `dealt`
 * is the losses of the other sides on that tile (the rules share them out;
 * the rows do not say who struck whom), `edges` the stance matches the
 * triangle favours, `retaliation` the losses the attackers took on a tile a
 * garrison or a camp defended.
 */
export function tileDetail(rows) {
  const tiles = new Map();
  for (const r of rows) {
    if (r.tile === undefined || r.tile === null || r.after === null) continue;
    const t = tiles.get(r.tile) ?? { tile: r.tile, rows: [] };
    t.rows.push(r);
    tiles.set(r.tile, t);
  }
  const out = [];
  for (const t of tiles.values()) {
    const by = new Map();
    for (const r of t.rows) {
      const s = by.get(r.faction) ?? { faction: r.faction, lost: 0, before: 0, stance: null, mine: false };
      s.lost += lossOf(r); s.before += r.before; s.mine ||= r.mine;
      if ((r.kind === 'arrival' || r.kind === 'resident') && s.stance === null) s.stance = r.posture;
      by.set(r.faction, s);
    }
    if (by.size < 2) continue;
    const sides = [...by.values()];
    const total = sides.reduce((a, s) => a + s.lost, 0);
    for (const s of sides) s.dealt = total - s.lost;
    const edges = [];
    for (const a of sides) for (const b of sides) if (a !== b && BEATS[a.stance] === b.stance) edges.push({ winner: a.faction, loser: b.faction, stance: a.stance, against: b.stance });
    const held = t.rows.filter(r => r.kind === 'garrison' || r.kind === 'camp');
    const retaliation = held.length ? t.rows.filter(r => r.kind === 'arrival' && !held.some(h => h.faction === r.faction)).reduce((a, r) => a + lossOf(r), 0) : 0;
    out.push({ tile: t.tile, sides, edges, retaliation, walls: held.some(h => h.walls) });
  }
  return out.sort((a, b) => a.tile - b.tile);
}

// ------------------------------------------------------------------ rendering
const KIND_TEXT = { resident: () => L`駐留`, arrival: () => L`到着`, garrison: () => L`守備隊`, camp: () => L`蛮族の野営地` };
const STEP_TEXT = {
  kernel: () => L`ルール（frontier.wasm）がシーズンと同じ`,
  seed: () => L`乱数がこの鐘の公式ビーコンのもの`,
  inputs: () => L`衝突の入力がこの州とこの鐘のもの`,
  recompute: () => L`このブラウザで衝突を計算し直した`,
  digest: () => L`結果の要約がチェーンの記録と一致`,
  fates: () => L`到着軍勢の結末がチェーンの記録と一致`,
};
const CODE_TEXT = {
  NoWasm: () => L`ルールのモジュール（frontier.wasm）を読み込めません`,
  RulesetMismatch: () => L`モジュールのルールがシーズンと違います`,
  NoBellRecord: () => L`この鐘の記録をまだ読めません`,
  NoAnchor: () => L`この鐘のビーコンの記録がありません`,
  SeedNotReady: () => L`この鐘の乱数がまだありません`,
  SeedMismatch: () => L`報告の乱数が公式ビーコンのものと違います`,
  NoInputs: () => L`衝突の入力がありません`,
  NotResolved: () => L`衝突はまだ決着していません`,
  WrongKey: () => L`入力が別の州か鐘のものです`,
  NoProvinceBefore: () => L`決着前の州の記録が報告にありません（ヘラルドの対応待ち）`,
  WrongProvinceBefore: () => L`決着前の州の記録がこの鐘のものではありません`,
  Unsettled: () => L`前の鐘の変更が未精算の記録です`,
  Refused: () => L`ルールが入力を受け付けませんでした`,
  BadInput: () => L`ルールが入力を読めませんでした`,
  BadUnit: () => L`記録に知らない兵種があります`,
  BadRecord: () => L`記録を読めませんでした`,
  CodecMismatch: () => L`結果の読み取りが一致しません`,
  DigestMismatch: () => L`結果の要約がチェーンの記録と違います`,
  FateMismatch: () => L`到着軍勢の結末がチェーンの記録と違います`,
  NoDigest: () => L`比べるチェーンの記録がありません`,
  Prerequisite: () => L`前の確認が済んでいません`,
};
export const codeText = why => CODE_TEXT[why]?.() ?? String(why ?? '');

/** The verification's headline. */
export function resultText(v) {
  if (!v) return '';
  if (v.result === 'match') return L`一致：このブラウザで計算し直した結果はチェーンの記録と同じです`;
  if (v.result === 'mismatch') return L`不一致：計算し直した結果がチェーンの記録と違います`;
  return L`確かめられませんでした（足りない記録があります）`;
}

function rowHtml(r, ownerOf = null) {
  const stance = r.kind === 'arrival' || r.kind === 'resident' ? postureName(r.posture) : r.walls ? L`城壁あり` : '';
  const fate = r.fate ? FATE_TEXT[r.fate] ?? r.fate : r.kind === 'garrison' || r.kind === 'camp' ? L`守った` : '—';
  const who = ownerOf && r.kind !== 'camp' ? ownerOf(r.id) : null;
  return html`<tr class="${r.mine ? 'mine' : ''}"><th scope="row">${who ? raw(personChip(who, r.faction, { size: 24 })) : ''}${swatch(r.faction)}${factionName(r.faction)} · ${KIND_TEXT[r.kind]()}${r.unit !== undefined ? html` · ${unitName(r.unit)}` : ''}${r.mine ? html` <span class="mine-mark">${L`（あなた）`}</span>` : ''}</th>
    <td>${fmtNum(r.before)} → ${r.after === null ? '—' : fmtNum(r.after)}</td><td>${stance}</td><td>${fate}</td></tr>`;
}

/** The fighters table (shared with practice results). */
export function renderRows(rows, caption, ownerOf = null) {
  if (!rows.length) return html`<p class="muted">${L`戦った軍勢はいません`}</p>`;
  return html`<table class="stores"><caption class="visually-hidden">${caption}</caption>
    <thead><tr><th scope="col">${L`軍勢`}</th><th scope="col">${L`兵（前 → 後）`}</th><th scope="col">${L`構え`}</th><th scope="col">${L`結末`}</th></tr></thead>
    <tbody>${rows.map(r => rowHtml(r, ownerOf))}</tbody></table>`;
}

function renderSteps(v) {
  return html`<ol class="list">${VERIFY_STEPS.map(id => {
    const s = v.steps[id];
    const mark = s.ok === true ? L`済` : s.ok === false ? L`違う` : L`未確認`;
    return html`<li><strong>${mark}</strong> ${STEP_TEXT[id]()}${s.why ? html` <span class="muted">${codeText(s.why)}</span>` : ''}</li>`;
  })}</ol>`;
}

/**
 * The report panel: `FS.report = {p, q, bell, loading?, error?, clash?,
 * verify?, verifying?}`; `mine(id)` marks the viewer's own; `whatIf` offers
 * the practice "what if" (not on the spectator page: it has no own hosts).
 */
export function render(FS, mine = () => false, { whatIf = true, ownerOf = null } = {}) {
  const r = FS.report;
  if (!r) return '';
  const title = L`衝突の報告：州 ${r.p},${r.q} · 第${fmtNum(r.bell)}鐘`;
  const head = html`<h3 id="report-title">${title}</h3><button type="button" class="btn small" data-act="report-close">${L`閉じる`}</button>`;
  if (r.loading) return html`<section aria-labelledby="report-title">${head}<p class="muted">${L`読み込み中…`}</p></section>`;
  if (r.error) return html`<section aria-labelledby="report-title">${head}<p class="notice error" role="alert">${L`報告を読めませんでした（${r.error}）`}</p></section>`;
  const rep = r.clash?.report ?? {};
  const v = r.verify;
  const rows = v?.outcome && v.args ? outcomeRows(v.args, v.outcome, mine) : inputRows(r.clash?.inputs, mine);
  const seed = v?.seed;
  const provenance = html`<dl class="facts">
    <div class="row"><dt>${L`乱数のラウンド`}</dt><dd>${seed ? fmtNum(seed.s) : '—'}</dd></div>
    <div class="row"><dt>${L`ビーコン`}</dt><dd>${rep.anchor?.key ?? '—'}</dd></div>
    <div class="row"><dt>${L`乱数の記録`}</dt><dd>${rep.cache?.key ?? rep.archive?.key ?? '—'}</dd></div>
    <div class="row"><dt>${L`入力の要約`}</dt><dd><code>${String(rep.inputDigest ?? '—').slice(0, 16)}</code></dd></div>
    <div class="row"><dt>${L`結果の要約`}</dt><dd><code>${String(rep.outcomeDigest ?? '—').slice(0, 16)}</code></dd></div>
    <div class="row"><dt>${L`交戦`}</dt><dd>${fmtNum(v?.outcome?.engagements ?? rep.decoded?.engagements ?? 0)}</dd></div></dl>`;
  const verdict = v ? html`<p class="notice ${v.result === 'match' ? 'ok' : v.result === 'mismatch' ? 'error' : ''}" role="status" data-verify-result="${v.result}">${resultText(v)}</p>` : '';
  const steps = v ? html`${renderSteps(v)}${v.builder === 'page' ? html`<p class="muted">${L`入力はこのページが契約の手順どおりに組み立てました。`}</p>` : ''}` : '';
  return html`<section aria-labelledby="report-title" class="report">${head}
    ${renderHeadline(summaryOf(rows), r)}
    ${v?.outcome ? '' : html`<p class="muted">${L`確かめる前は、チェーンに書かれた到着軍勢の結末だけを表示しています。`}</p>`}
    ${renderRows(rows, title, ownerOf)}
    ${renderTiles(tileDetail(rows))}
    <p><button type="button" class="btn primary" data-act="report-verify" ${raw(r.verifying ? 'disabled' : '')}>${r.verifying ? L`確かめています…` : L`このブラウザで確かめる`}</button>
      ${whatIf ? html`<button type="button" class="btn" data-act="report-whatif" ${raw(v?.args ? '' : 'disabled')}>${L`練習で試す（もしも）`}</button>` : ''}</p>
    ${verdict}
    ${whatIf && !v?.args ? html`<p class="muted">${L`「練習で試す」は、このブラウザで確かめた後に使えます。`}</p>` : ''}
    <details class="report-proof"><summary>${L`検証の詳細（乱数・要約・手順）`}</summary>${provenance}${steps}</details>
  </section>`;
}

const RESULT_TEXT = {
  won: () => L`勝利：相手は退いた`, held: () => L`持ちこたえた`, fell: () => L`壊滅した`, turned: () => L`退いた`, none: () => L`結末はまだ確かめていません`,
};
const RESULT_GLYPH = { won: '★', held: '◆', fell: '✕', turned: '↩', none: '…' };

/** The headline: the viewer's result, losses and whether the march got there; replay and map buttons. */
function renderHeadline(sum, r) {
  const buttons = html`<div class="actions"><button type="button" class="btn primary" data-act="battle-play" data-p="${r.p}" data-q="${r.q}" data-bell="${r.bell}">▶ ${L`戦いを再生`}</button>
    <button type="button" class="btn" data-act="goto" data-p="${r.p}" data-q="${r.q}">${L`地図で見る`}</button></div>`;
  if (!sum.mine) {
    return html`<div class="report-head report-head-watch"><p class="report-result"><strong>${sum.leader !== null ? html`${swatch(sum.leader)}${L`${factionName(sum.leader)}が戦場に残った`}` : L`${fmtNum(sum.factions)}つの陣営がぶつかった`}</strong></p>
      <p class="muted">${L`全体の損害 ${fmtNum(sum.lost)}`}</p>${buttons}</div>`;
  }
  return html`<div class="report-head report-${sum.result}"><p class="report-result"><span class="report-glyph" aria-hidden="true">${RESULT_GLYPH[sum.result]}</span><strong>${RESULT_TEXT[sum.result]()}</strong></p>
    <p>${L`あなたの損害 ${fmtNum(sum.lost)} / ${fmtNum(sum.before)}`}${sum.reached === null ? '' : sum.reached ? html` · ${L`行き先に着いた`}` : html` · ${L`行き先に残れなかった`}`}</p>
    ${sum.fates.length ? html`<p class="muted">${[...new Set(sum.fates)].map(f => FATE_TEXT[f] ?? f).join(' · ')}</p>` : ''}${buttons}</div>`;
}

/** Per-tile detail: who fought where, losses taken and dealt, the stance edge, the defenders' retaliation. */
function renderTiles(list) {
  if (!list.length) return '';
  return html`<section class="report-tiles" aria-labelledby="report-tiles-title"><h4 id="report-tiles-title">${L`マスごとの戦い`}</h4><ul class="list">${list.map(t => html`<li>
    <strong>${L`マス ${t.tile + 1}`}</strong>${t.walls ? html` <span class="muted">${L`城壁あり`}</span>` : ''}
    <ul class="report-sides">${t.sides.map(s => html`<li class="${s.mine ? 'mine' : ''}">${swatch(s.faction)}${factionName(s.faction)}${s.stance !== null ? html` · ${postureName(s.stance)}` : ''} — ${L`受けた損害 ${fmtNum(s.lost)} · 与えた損害 ${fmtNum(s.dealt)}`}</li>`)}</ul>
    ${t.edges.map(e => html`<p class="report-edge">▲ ${L`${factionName(e.winner)}の${postureName(e.stance)}が${postureName(e.against)}に有利`}</p>`)}
    ${t.retaliation ? html`<p class="muted">${L`守り手の反撃で攻め手が ${fmtNum(t.retaliation)} を失った`}</p>` : ''}</li>`)}</ul>
    <p class="muted">${L`与えた損害は、そのマスで相手側が失った兵の合計です（誰が誰を討ったかは記録にありません）。`}</p></section>`;
}

/** The report links of resolved clashes: `[{p, q, bell}]` → buttons. */
export function renderLinks(list, title) {
  if (!list.length) return '';
  return html`<section aria-labelledby="reports-title"><h3 id="reports-title">${title}</h3><ul class="list">${list.map(x => html`<li><button type="button" class="btn" data-act="report-open" data-p="${x.p}" data-q="${x.q}" data-bell="${x.bell}">${L`州 ${x.p},${x.q} · 第${fmtNum(x.bell)}鐘`}${x.mine ? html` <span class="muted">${L`（あなた）`}</span>` : ''}</button></li>`)}</ul></section>`;
}

/** Resolved clashes from the chronicle (CLASH records), newest first, distinct, at most `limit`. */
export function clashesFrom(chronicle, limit = 12) {
  const out = [];
  for (let i = (chronicle ?? []).length - 1; i >= 0 && out.length < limit; i--) {
    const r = chronicle[i].record;
    if (r.name !== 'CLASH') continue;
    if (!out.some(x => x.p === r.p && x.q === r.q && x.bell === r.bell)) out.push({ p: r.p, q: r.q, bell: r.bell });
  }
  return out;
}
