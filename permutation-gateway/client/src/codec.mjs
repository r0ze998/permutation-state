// Borsh encoding of PERMUTATION STATE orders and program instructions, and
// decoding of the program's accounts. The input shape is the server's order
// DTO (JSON, camelCase). Byte-for-byte equality with the Rust types is
// checked by test/codec.test.mjs against vectors produced by the Rust side.

import { Writer, Reader } from './borsh.mjs';

const TECHS = ['Agriculture', 'BronzeWorking', 'Archery', 'HorsebackRiding', 'Masonry', 'Mysticism', 'Writing', 'Currency',
  'IronWorking', 'Mathematics', 'Chivalry', 'Philosophy', 'Engineering', 'Astronomy', 'Physics', 'CelestialMechanics'];
const BUILDINGS = ['Granary', 'Workshop', 'Temple', 'Market', 'Academy', 'Barracks', 'Walls', 'StarGate1', 'StarGate2', 'StarGate3'];
const UNITS = ['Spearman', 'Archer', 'Horseman', 'Pikeman', 'Crossbowman', 'Knight', 'Scout', 'Settler'];
const FOCUS = ['Balanced', 'Food', 'Production', 'Gold', 'Science'];
const ORDER = ['MoveUnit', 'Attack', 'FoundCity', 'SetQueue', 'SetFocus', 'Purchase', 'SetResearch', 'DeclareWar', 'ProposePeace',
  'AcceptPeace', 'ProposeNap', 'AcceptNap', 'BreakNap', 'ProposeAlliance', 'AcceptAlliance', 'LeaveAlliance', 'SendEnvoy',
  'Transfer', 'MarketTrade', 'ExchangeOrder', 'Raze', 'SetStanding', 'RevealRationale'];
const STANDING = ['Clear', 'AutoDefend', 'Retreat', 'Patrol', 'QueueRepeat', 'AutoPurchase'];

const index = (list, name, what) => { const i = list.indexOf(name); if (i < 0) throw new Error(`unknown ${what} ${name}`); return i; };
const hex = (w, [q, r]) => w.i32(q).i32(r);
const fromHex = h => Uint8Array.from((h.match(/../g) || []).map(b => parseInt(b, 16)));

function good(w, g) {
  switch (g.kind) {
    case 'Gold': return w.u8(0);
    case 'Iron': return w.u8(1);
    case 'Horses': return w.u8(2);
    case 'Food': return w.u8(3).u32(g.city);
    case 'Production': return w.u8(4).u32(g.city);
    default: throw new Error(`unknown good ${g.kind}`);
  }
}
const side = (w, s) => w.u8(index(['Buy', 'Sell'], s, 'side'));

function item(w, it) {
  switch (it.kind) {
    case 'Building': return w.u8(0).u8(index(BUILDINGS, it.building, 'building'));
    case 'Troops': return w.u8(1).u8(index(UNITS, it.unit, 'unit')).u8(it.n);
    case 'Scout': return w.u8(2);
    case 'Settler': return w.u8(3);
    default: throw new Error(`unknown item ${it.kind}`);
  }
}

function standing(w, r) {
  w.u8(index(STANDING, r.kind, 'standing rule'));
  switch (r.kind) {
    case 'AutoDefend': return w.u8(r.radius);
    case 'Retreat': return w.u32(r.ratioBps);
    case 'Patrol': return w.vec(r.route, hex);
    case 'QueueRepeat': return w.bool(r.on);
    case 'AutoPurchase': return w.u32(r.maxGold);
    default: return w;
  }
}

/** Encode one order DTO (as the server's /api/orders accepts it). */
export function encodeOrder(w, o) {
  w.u8(index(ORDER, o.type, 'order'));
  switch (o.type) {
    case 'MoveUnit': return w.u32(o.unit).vec(o.path, hex);
    case 'Attack': return w.u32(o.army).u8(index(['Unit', 'City', 'CityState'], o.target.kind, 'target'))[o.target.kind === 'CityState' ? 'u16' : 'u32'](o.target.id);
    case 'FoundCity': return w.u32(o.settler);
    case 'SetQueue': return w.u32(o.city).vec(o.items, item);
    case 'SetFocus': return w.u32(o.city).u8(index(FOCUS, o.focus, 'focus'));
    case 'Purchase': return w.u32(o.city).u32(o.gold);
    case 'SetResearch': return w.vec(o.techs, (w2, t) => w2.u8(index(TECHS, t, 'tech')));
    case 'DeclareWar': case 'ProposePeace': case 'AcceptPeace': case 'BreakNap': case 'ProposeAlliance': case 'AcceptAlliance': return w.u16(o.civ);
    case 'ProposeNap': case 'AcceptNap': return w.u16(o.civ).u32(o.bond);
    case 'LeaveAlliance': return w;
    case 'SendEnvoy': return w.u16(o.cityState).u32(o.influence);
    case 'Transfer': good(w.u16(o.civ), o.good); return w.u32(o.amount);
    case 'MarketTrade': good(w, o.good); side(w, o.side); return w.u32(o.amount).u32(o.limitGold);
    case 'ExchangeOrder': good(w, o.good); side(w, o.side); return w.u32(o.amount).u64(o.price);
    case 'Raze': return w.u32(o.city);
    case 'SetStanding': w.u8(o.target.kind === 'Unit' ? 0 : 1).u32(o.target.id); return standing(w, o.rule);
    case 'RevealRationale': return w.u16(o.tick).bytes(new TextEncoder().encode(o.policy)).fixed(fromHex(o.salt), 16).bytes(new TextEncoder().encode(o.text));
    default: throw new Error(`unknown order ${o.type}`);
  }
}

/** Program instruction data (enum `ChainInstruction`). Keys are 32-byte arrays. */
export const IX = {
  createSeason: a => new Writer().u8(0).u64(a.seasonId).u8(a.preset).u8(a.maxCivs).u64(a.entryFee).u64(a.exchangeCredit)
    .u32(a.tickSeconds).fixed(a.worldSeed, 32).fixed(a.crank, 32).toBytes(),
  allocWorld: chunk => Uint8Array.of(1, chunk),
  joinSeason: a => new Writer().u8(2).string(a.name).u8(a.kind).fixed(a.session, 32).fixed(a.payout, 32).toBytes(),
  startSeason: () => Uint8Array.of(3),
  genesisStep: work => new Writer().u8(4).u32(work).toBytes(),
  delegate: target => new Writer().u8(5).u16(target).toBytes(),
  submitOrders: a => new Writer().u8(6).u16(a.tick).fixed(a.decisionDigest, 32).vec(a.orders, encodeOrder).toBytes(),
  resolveTick: (to = 12) => Uint8Array.of(7, to),
  commit: () => Uint8Array.of(8),
  commitAndUndelegate: () => Uint8Array.of(9),
  finishSeason: () => Uint8Array.of(10),
  claim: civ => new Writer().u8(11).u16(civ).toBytes(),
  undelegatePart: targets => new Writer().u8(12).vec(targets, (w, t) => w.u16(t)).toBytes(),
};

// ------------------------------------------------------------------ accounts

const STATUS = ['Registering', 'Genesis', 'Running', 'Finalized'];

export function decodeSeason(data) {
  const r = new Reader(data);
  const magic = new TextDecoder().decode(r.fixed(8));
  if (magic !== 'PSSEASN1') throw new Error('not a season account');
  const s = { seasonId: r.u64(), bump: r.u8(), vaultBump: r.u8(), admin: r.fixed(32), crank: r.fixed(32), usdcMint: r.fixed(32), usdcDecimals: r.u8(),
    preset: r.u8(), maxCivs: r.u8(), entryFee: r.u64(), exchangeCredit: r.u64(), tickSeconds: r.u32(), status: STATUS[r.u8()],
    worldSeed: r.fixed(32), seasonSeed: r.fixed(32) };
  s.civs = r.vec(x => ({ player: x.fixed(32), session: x.fixed(32), payout: x.fixed(32), kind: x.u8(), name: x.string() }));
  s.pool = r.u64(); s.payouts = r.vec(x => x.u64()); s.claimed = r.vec(x => x.bool()); s.rollover = r.u64(); s.finalRoot = r.fixed(32);
  return s;
}

export function decodeOrdersHeader(data) {
  const r = new Reader(data);
  const magic = new TextDecoder().decode(r.fixed(8));
  if (magic !== 'PSORDER1') throw new Error('not an orders account');
  const o = { seasonId: r.u64(), civ: r.u16(), bump: r.u8(), preset: r.u8(), player: r.fixed(32), session: r.fixed(32), openTick: r.u16(), spendable: r.u32() };
  o.hasBatch = r.u8() === 1;
  if (o.hasBatch) { o.batchCiv = r.u16(); o.batchTick = r.u16(); o.batchDigest = r.fixed(32); }
  return o;
}

/** World header: magic, body length, `WorldMeta`. */
export function decodeWorldHeader(data) {
  const r = new Reader(data);
  const magic = new TextDecoder().decode(r.fixed(8));
  const len = r.u32();
  const meta = { seasonId: r.u64(), preset: r.u8(), civs: r.u8(), tickSeconds: r.u32(), deadline: Number(r.i64()), finished: r.bool() };
  return { magic, len, meta, bodyOffset: 76 };
}

/** Parse a `Program data:` log of the PS_TICK / PS_GENESIS records. */
export function parseRecord(fields) {
  const tag = new TextDecoder().decode(fields[0]);
  if (tag === 'PS_TICK') {
    return { tag, tick: new DataView(fields[1].buffer, fields[1].byteOffset).getUint16(0, true), to: fields[2][0], vrf: fields[3], preRoot: fields[4], root: fields[5], batches: fields[6] };
  }
  if (tag === 'PS_GENESIS') return { tag, root: fields[1], seasonSeed: fields[2] };
  return { tag };
}
