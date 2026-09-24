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
  'Transfer', 'MarketTrade', 'ExchangeOrder', 'Raze', 'SetStanding', 'RevealRationale', 'ConsentWar', 'ConsentSpend'];
/** Offices (V5 §5.1), in `Role` order. */
export const ROLES = ['General', 'Steward', 'Science', 'Diplomat'];
const GOV = ['Stand', 'Vote', 'Propose', 'Support', 'Recall'];
/** `u32::MAX`: no member (a vacant office, no vote). */
export const NOBODY = 0xffffffff;
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
    case 'ConsentWar': return w.u16(o.civ);
    case 'ConsentSpend': return w.u64(o.usdc);
    default: throw new Error(`unknown order ${o.type}`);
  }
}

export const roleIndex = r => index(ROLES, r, 'office');
/** `Role::bit` mask of office names. */
export const roleMask = roles => roles.reduce((m, r) => m | (1 << roleIndex(r)), 0);

/** Encode one governance action DTO (as the server's /api/gov takes it). */
export function encodeGov(w, a) {
  w.u8(index(GOV, a.type, 'governance action'));
  switch (a.type) {
    case 'Stand': return w.u8(roleMask(a.roles || []));
    case 'Vote': return w.u8(roleIndex(a.role)).u32(a.candidate);
    case 'Propose': return w.u8(roleIndex(a.role)).vec(a.orders, encodeOrder);
    case 'Support': return w.u32(a.proposal);
    case 'Recall': return w.u8(roleIndex(a.role));
    default: throw new Error(`unknown governance action ${a.type}`);
  }
}

/** Program instruction data (enum `ChainInstruction`). Keys are 32-byte arrays. */
const votes4 = (w, v) => { for (let i = 0; i < 4; i++) w.u32(v?.[i] ?? NOBODY); return w; };
export const IX = {
  createSeason: a => new Writer().u8(0).u64(a.seasonId).u8(a.preset).u8(a.nations).u64(a.entryFee)
    .u32(a.tickSeconds).fixed(a.worldSeed, 32).fixed(a.crank, 32).bool(a.market ?? true).toBytes(),
  allocWorld: chunk => Uint8Array.of(1, chunk),
  register: a => votes4(new Writer().u8(2).u16(a.civ).string(a.name).u8(a.kind).fixed(a.session, 32).fixed(a.attestation ?? new Uint8Array(32), 32)
    .u8(a.stand ?? 0), a.votes).u64(a.deposit ?? 0n).toBytes(),
  startSeason: () => Uint8Array.of(3),
  genesisStep: work => new Writer().u8(4).u32(work).toBytes(),
  delegate: target => new Writer().u8(5).u16(target).toBytes(),
  submitOrders: a => new Writer().u8(6).u8(roleIndex(a.role)).u16(a.tick).fixed(a.decisionDigest, 32).vec(a.orders, encodeOrder).vec(a.adopt ?? [], (w, id) => w.u32(id)).toBytes(),
  resolveTick: (to = 12) => Uint8Array.of(7, to),
  commit: () => Uint8Array.of(8),
  commitAndUndelegate: () => Uint8Array.of(9),
  finishSeason: () => Uint8Array.of(10),
  claim: () => Uint8Array.of(11),
  undelegatePart: targets => new Writer().u8(12).vec(targets, (w, t) => w.u16(t)).toBytes(),
  updateMember: a => votes4(new Writer().u8(13).u8(a.stand ?? 0), a.votes).toBytes(),
  allocNation: civ => new Writer().u8(14).u16(civ).toBytes(),
  seatMembers: () => Uint8Array.of(15),
  openGovernment: () => Uint8Array.of(16),
  submitGov: a => encodeGov(new Writer().u8(17).u32(a.member), a.action).toBytes(),
  withdrawOps: () => Uint8Array.of(18),
  logTickInput: chunk => new Writer().u8(19).u16(chunk).toBytes(),
};

// ------------------------------------------------------------------ accounts

const STATUS = ['Registering', 'Genesis', 'Seating', 'Running', 'Finalized'];
const magicOf = r => new TextDecoder().decode(r.fixed(8));

export function decodeSeason(data) {
  const r = new Reader(data);
  if (magicOf(r) !== 'PSSEASN5') throw new Error('not a V5 season account');
  const s = { seasonId: r.u64(), bump: r.u8(), vaultBump: r.u8(), admin: r.fixed(32), crank: r.fixed(32), usdcMint: r.fixed(32), usdcDecimals: r.u8(),
    preset: r.u8(), nations: r.u8(), entryFee: r.u64(), tickSeconds: r.u32(), market: r.bool(), status: STATUS[r.u8()],
    worldSeed: r.fixed(32), seasonSeed: r.fixed(32), memberCount: r.u32() };
  s.nationMembers = r.vec(x => x.u32()); s.seated = r.u32(); s.pool = r.u64(); s.ops = r.u64(); s.opsWithdrawn = r.bool();
  s.treasury = r.vec(x => x.u64()); s.treasuryFinal = r.vec(x => x.u64()); s.payouts = r.vec(x => x.u64()); s.finalRoot = r.fixed(32);
  return s;
}

export function decodeMember(data) {
  const r = new Reader(data);
  if (magicOf(r) !== 'PSMEMBR5') throw new Error('not a member account');
  const m = { seasonId: r.u64(), bump: r.u8(), index: r.u32(), civ: r.u16(), wallet: r.fixed(32), session: r.fixed(32), kind: r.u8(), name: r.string(),
    attestation: r.fixed(32), stand: r.u8() };
  m.votes = [r.u32(), r.u32(), r.u32(), r.u32()];
  m.shares = r.u64(); m.claimed = r.bool();
  return m;
}

/** The fixed front of a nation account: office holders, keys, budgets, who submitted. */
export function decodeNationHeader(data) {
  const r = new Reader(data);
  if (magicOf(r) !== 'PSNATN06') throw new Error('not a nation account');
  const n = { seasonId: r.u64(), civ: r.u16(), bump: r.u8(), preset: r.u8(), market: r.bool(), crank: r.fixed(32), openTick: r.u16() };
  n.officers = [r.u32(), r.u32(), r.u32(), r.u32()];
  n.keys = [r.fixed(32), r.fixed(32), r.fixed(32), r.fixed(32)];
  n.spendable = [r.u32(), r.u32(), r.u32(), r.u32()];
  n.submitted = [r.u16(), r.u16(), r.u16(), r.u16()];
  n.frozen = r.bool();
  return n;
}

/** World header: magic, body length, `WorldMeta`. */
export function decodeWorldHeader(data) {
  const r = new Reader(data);
  const magic = new TextDecoder().decode(r.fixed(8));
  const len = r.u32();
  const meta = { seasonId: r.u64(), preset: r.u8(), civs: r.u8(), tickSeconds: r.u32(), deadline: Number(r.i64()), finished: r.bool(), market: r.bool(),
    frozen: r.bool(), vrf: r.fixed(32), inputChunks: r.u16(), inputLogged: r.u16() };
  return { magic, len, meta, bodyOffset: 76 };
}

/** Parse a `Program data:` log of the PS_TICK / PS_INPUT / PS_GENESIS / PS_SEAT / PS_OPEN records. */
export function parseRecord(fields) {
  const tag = new TextDecoder().decode(fields[0]);
  const u16 = f => new DataView(f.buffer, f.byteOffset).getUint16(0, true);
  if (tag === 'PS_TICK') {
    return { tag, tick: u16(fields[1]), to: fields[2][0], preRoot: fields[3], root: fields[4], inputHash: fields[5] };
  }
  // One chunk of a tick's input (borsh TickInput); `hash` is the whole input's sha256.
  if (tag === 'PS_INPUT') return { tag, tick: u16(fields[1]), chunk: u16(fields[2]), total: u16(fields[3]), hash: fields[4], bytes: fields[5] ?? new Uint8Array() };
  if (tag === 'PS_GENESIS') return { tag, root: fields[1], seasonSeed: fields[2] };
  if (tag === 'PS_SEAT') return { tag, root: fields[1], members: fields[2] };
  if (tag === 'PS_OPEN') return { tag, root: fields[1] };
  return { tag };
}

/** Program errors (`ChainError`, surfaced as `{"Custom": code}`), in code order from 1. */
export const CHAIN_ERRORS = ['InvalidInstruction', 'MissingSignature', 'WrongPda', 'AlreadyInitialized', 'NotInitialized', 'WrongStatus', 'Unauthorized', 'SeasonFull', 'InvalidName', 'WrongMint', 'WrongTokenAccount', 'WorldTooSmall', 'Rules', 'WrongTick', 'OverBudget', 'TooEarly', 'MissingNation', 'WrongDelegationProgram', 'WrongMagicProgram', 'AlreadyClaimed', 'NothingToClaim', 'SeasonNotOver', 'WrongOffice', 'InvalidParams', 'WrongWorld', 'InboxFull', 'TickFrozen', 'InputNotPublished'];

/** The program error named in a transaction error message, or null. */
export function chainError(message) {
  const m = /"Custom":(\d+)/.exec(String(message));
  return m ? CHAIN_ERRORS[Number(m[1]) - 1] ?? `Custom(${m[1]})` : null;
}
