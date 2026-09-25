// Borsh encoding of PERMUTATION STATE orders and program instructions,
// decoding of the program's accounts and log records, program error names,
// and the constants the client shares with the Rust crates. The input shape
// of orders is the server's order DTO (JSON, camelCase).
//
// Everything here is checked against vectors the Rust types produce
// (permutation-server/tests/codec_vectors.rs → test/vectors.json): encoders
// byte for byte, decoders value for value, instruction tags, error codes,
// constants, magics and names (test/codec.test.mjs, test/constants.test.mjs).

import { createHash } from 'node:crypto';
import { Writer, Reader } from './borsh.mjs';

// ------------------------------------------------------------------ constants
// Mirrors of permutation-chain `state.rs` / `processor.rs`, permutation-rules
// and permutation-server `ledger.rs`.

/** World chunk accounts per season, and the size of each (bytes). */
export const WORLD_CHUNKS = 20;
export const CHUNK = 4 * 1024;
/** Chunk 0 header: magic (8) + body length (4) + `WorldMeta` padded to 64. */
export const WORLD_HEADER = 8 + 4 + 64;
/** `Delegate { target }` etc.: 0..WORLD_CHUNKS = a world chunk; NATION_TARGET + civ = a nation account. */
export const NATION_TARGET = 1000;
/** Bytes of tick input per `PS_INPUT` log record. */
export const INPUT_CHUNK = 6000;
export const MAX_NATIONS = 8;
/** Member names, in UTF-8 bytes. */
export const MAX_NAME = 24;
export const MAX_MEMBERS = 256;
/** Governance actions one signer may queue per tick. */
export const MAX_GOV_PER_SIGNER = 8;
/** Encoded orders that fit in one RevealOrders transaction (packet limit 1232 bytes); the server's `BATCH_BYTES`. */
export const BATCH_BYTES = 800;
/** Operator AI members per season (V5 §18.2). */
export const MAX_AI = 64;
/** Seconds after the last tick the operator has to reveal its roster (V5 §18.2). */
export const ROSTER_GRACE_SECONDS = 3600;
/** PDA seeds (first component). */
export const SEEDS = Object.freeze({ season: 'season', world: 'world', nation: 'nation', member: 'member', vault: 'vault', roster: 'roster' });
/** Account magics (first 8 bytes). `genesis` marks a world whose genesis is still being built. */
export const MAGIC = Object.freeze({ season: 'PSSEASN7', member: 'PSMEMBR6', nation: 'PSNATN07', world: 'PSWORLD5', genesis: 'PSGENJB1', roster: 'PSROSTR1' });
/** Nation names, in civ order (`permutation_rules::genesis::NATIONS`). */
export const NATIONS = Object.freeze(['Aster', 'Borealis', 'Cinder', 'Dunmar', 'Ember', 'Fjordal']);
/** Offices (V5 §5.1), in `Role` order. */
export const ROLES = Object.freeze(['General', 'Steward', 'Science', 'Diplomat']);
/** `SeasonStatus`, in tag order. */
export const SEASON_STATUS = Object.freeze(['Registering', 'Genesis', 'Seating', 'Running', 'Finalized']);
/** `MemberAccount.kind` (self-declared, V5 D16), as the gateway names it. */
export const MEMBER_KINDS = Object.freeze(['human', 'agent', 'undeclared']);
/** `u32::MAX`: no member (a vacant office, no vote). */
export const NOBODY = 0xffffffff;

const TECHS = ['Agriculture', 'BronzeWorking', 'Archery', 'HorsebackRiding', 'Masonry', 'Mysticism', 'Writing', 'Currency',
  'IronWorking', 'Mathematics', 'Chivalry', 'Philosophy', 'Engineering', 'Astronomy', 'Physics', 'CelestialMechanics'];
const BUILDINGS = ['Granary', 'Workshop', 'Temple', 'Market', 'Academy', 'Barracks', 'Walls', 'StarGate1', 'StarGate2', 'StarGate3'];
const UNITS = ['Spearman', 'Archer', 'Horseman', 'Pikeman', 'Crossbowman', 'Knight', 'Scout', 'Settler'];
const FOCUS = ['Balanced', 'Food', 'Production', 'Gold', 'Science'];
const ORDER = ['MoveUnit', 'Attack', 'FoundCity', 'SetQueue', 'SetFocus', 'Purchase', 'SetResearch', 'DeclareWar', 'ProposePeace',
  'AcceptPeace', 'ProposeNap', 'AcceptNap', 'BreakNap', 'ProposeAlliance', 'AcceptAlliance', 'LeaveAlliance', 'SendEnvoy',
  'Transfer', 'MarketTrade', 'ExchangeOrder', 'Raze', 'SetStanding', 'RevealRationale', 'ConsentWar', 'ConsentSpend',
  'OfferContract', 'AcceptContract', 'CancelContract'];
/** `ContractTerm` (V5 §18.6), in tag order. */
export const CONTRACT_TERMS = Object.freeze(['Peace', 'LeaveAlliance', 'KeepNap', 'Capture']);
const GOV = ['Stand', 'Vote', 'Propose', 'Support', 'Recall'];
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

function contractTerm(w, t) {
  w.u8(index(CONTRACT_TERMS, t.kind, 'contract term'));
  switch (t.kind) {
    case 'Peace': return w;
    case 'LeaveAlliance': return w.u16(t.with);
    case 'KeepNap': return w.u16(t.every).u8(t.installments);
    case 'Capture': return w.u32(t.city);
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
    case 'OfferContract': contractTerm(w.option(o.to, (x, c) => x.u16(c)), o.term); return w.u64(o.usdc).u16(o.deadline);
    case 'AcceptContract': case 'CancelContract': return w.u32(o.id);
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

/**
 * `ChainInstruction` tags (the first byte of instruction data). Security
 * filters (the gateway's relays and x402) match on these. `commit` and
 * `commitAndUndelegate` (whole-world intents) are superseded by
 * `commitPart` / `undelegatePart`; the client builds neither.
 */
export const IX_TAG = Object.freeze({
  createSeason: 0, allocWorld: 1, register: 2, startSeason: 3, genesisStep: 4, delegate: 5, submitOrders: 6, resolveTick: 7,
  commit: 8, commitAndUndelegate: 9, finishSeason: 10, claim: 11, undelegatePart: 12, updateMember: 13, allocNation: 14,
  seatMembers: 15, openGovernment: 16, submitGov: 17, withdrawOps: 18, logTickInput: 19, commitPart: 20,
  closeCommits: 21, commitOrders: 22, revealOrders: 23, revealRoster: 24, anchorTalk: 25,
});

/** Instructions the program still decodes but always refuses (`Retired`); the client builds none. */
export const RETIRED_IX = Object.freeze(['submitOrders']);

/** Program instruction data (enum `ChainInstruction`). Keys are 32-byte arrays. */
const votes4 = (w, v) => { for (let i = 0; i < 4; i++) w.u32(v?.[i] ?? NOBODY); return w; };
const ixWriter = name => { if (!(name in IX_TAG)) throw new Error(`unknown instruction ${name}`); return new Writer().u8(IX_TAG[name]); };
const targets = (w, list) => w.vec(list, (x, t) => x.u16(t));
export const IX = {
  createSeason: a => ixWriter('createSeason').u64(a.seasonId).u8(a.preset).u8(a.nations).u64(a.entryFee)
    .u32(a.tickSeconds).fixed(a.worldSeed, 32).fixed(a.crank, 32).bool(a.market ?? true).u64(a.prevSeasonId ?? 0n)
    .u16(a.aiCount ?? 0).fixed(a.rosterChain ?? new Uint8Array(32), 32).u64(a.bountyEach ?? 0n).u64(a.bond ?? 0n).toBytes(),
  allocWorld: chunk => ixWriter('allocWorld').u8(chunk).toBytes(),
  register: a => votes4(ixWriter('register').u16(a.civ).string(a.name).u8(a.kind).fixed(a.session, 32).fixed(a.attestation ?? new Uint8Array(32), 32)
    .u8(a.stand ?? 0), a.votes).u64(a.deposit ?? 0n).fixed(a.tag, 32).toBytes(),
  startSeason: () => ixWriter('startSeason').toBytes(),
  genesisStep: work => ixWriter('genesisStep').u32(work).toBytes(),
  delegate: target => ixWriter('delegate').u16(target).toBytes(),
  resolveTick: (to = 12) => ixWriter('resolveTick').u8(to).toBytes(),
  finishSeason: () => ixWriter('finishSeason').toBytes(),
  claim: () => ixWriter('claim').toBytes(),
  undelegatePart: list => targets(ixWriter('undelegatePart'), list).toBytes(),
  updateMember: a => votes4(ixWriter('updateMember').u8(a.stand ?? 0), a.votes).toBytes(),
  allocNation: civ => ixWriter('allocNation').u16(civ).toBytes(),
  seatMembers: () => ixWriter('seatMembers').toBytes(),
  openGovernment: () => ixWriter('openGovernment').toBytes(),
  submitGov: a => encodeGov(ixWriter('submitGov').u32(a.member), a.action).toBytes(),
  withdrawOps: () => ixWriter('withdrawOps').toBytes(),
  logTickInput: chunk => ixWriter('logTickInput').u16(chunk).toBytes(),
  commitPart: list => targets(ixWriter('commitPart'), list).toBytes(),
  closeCommits: () => ixWriter('closeCommits').toBytes(),
  commitOrders: a => ixWriter('commitOrders').u8(roleIndex(a.role)).u16(a.tick).fixed(a.commitment, 32).toBytes(),
  revealOrders: a => ixWriter('revealOrders').u8(roleIndex(a.role)).u16(a.tick).fixed(a.decisionDigest, 32).vec(a.orders, encodeOrder)
    .vec(a.adopt ?? [], (w, id) => w.u32(id)).fixed(a.salt, 32).toBytes(),
  revealRoster: salts => ixWriter('revealRoster').vec(salts, (w, x) => w.fixed(x, 32)).toBytes(),
  anchorTalk: a => ixWriter('anchorTalk').u16(a.tick).u32(a.count).fixed(a.root, 32).toBytes(),
};

/**
 * Sealed orders (commit–reveal): borsh `OrderBatch` and its commitment
 * `sha256("permutation-rules/orders" ‖ borsh(batch) ‖ salt)`, exactly as
 * `permutation_rules::orders::order_commitment`. `b` = {civ, tick, role,
 * member, decisionDigest, orders (DTOs), adopt}; `salt` 32 bytes.
 */
export const encodeBatch = b => new Writer().u16(b.civ).u16(b.tick).u8(roleIndex(b.role)).u32(b.member).fixed(b.decisionDigest, 32)
  .vec(b.orders, encodeOrder).vec(b.adopt ?? [], (w, id) => w.u32(id)).toBytes();
const sha256 = (...parts) => { const h = createHash('sha256'); for (const p of parts) h.update(p); return new Uint8Array(h.digest()); };
export const orderCommitment = (b, salt) => sha256(new TextEncoder().encode('permutation-rules/orders'), encodeBatch(b), salt);

/**
 * Operator AI members (V5 §18.2), as `permutation_rules::roster`: the tag an
 * AI registers with, and the chain over the tags committed at creation.
 */
const u64le = v => { const b = new Uint8Array(8); new DataView(b.buffer).setBigUint64(0, BigInt(v), true); return b; };
export const rosterTag = (seasonId, wallet, salt) => sha256(new TextEncoder().encode('permutation-rules/ai'), u64le(seasonId), wallet, salt);
export const rosterLink = (prev, tag) => sha256(new TextEncoder().encode('permutation-rules/roster'), prev, tag);
export const rosterChain = tags => tags.reduce((acc, t) => rosterLink(acc, t), new Uint8Array(32));

// ------------------------------------------------------------------ accounts

const magicOf = r => new TextDecoder().decode(r.fixed(8));

export function decodeSeason(data) {
  const r = new Reader(data);
  if (magicOf(r) !== MAGIC.season) throw new Error('not a V5 season account');
  const s = { seasonId: r.u64(), bump: r.u8(), vaultBump: r.u8(), admin: r.fixed(32), crank: r.fixed(32), usdcMint: r.fixed(32), usdcDecimals: r.u8(),
    preset: r.u8(), nations: r.u8(), entryFee: r.u64(), tickSeconds: r.u32(), market: r.bool(), status: SEASON_STATUS[r.u8()],
    worldSeed: r.fixed(32), seasonSeed: r.fixed(32), memberCount: r.u32() };
  s.nationMembers = r.vec(x => x.u32()); s.seated = r.u32(); s.pool = r.u64(); s.ops = r.u64(); s.opsWithdrawn = r.bool();
  s.treasury = r.vec(x => x.u64()); s.treasuryFinal = r.vec(x => x.u64()); s.payouts = r.vec(x => x.u64()); s.finalRoot = r.fixed(32);
  s.prevSeasonId = r.u64(); s.prevHistoryRoot = r.fixed(32); s.historyRoot = r.fixed(32);
  s.aiCount = r.u16(); s.rosterChain = r.fixed(32); s.bountyEach = r.u64(); s.bond = r.u64();
  s.rosterAcc = r.fixed(32); s.rosterRevealed = r.u16(); s.rosterOutcome = ['none', 'revealed', 'forfeited'][r.u8()]; s.bountyPaid = r.vec(x => x.u64());
  return s;
}

export function decodeMember(data) {
  const r = new Reader(data);
  if (magicOf(r) !== MAGIC.member) throw new Error('not a member account');
  const m = { seasonId: r.u64(), bump: r.u8(), index: r.u32(), civ: r.u16(), wallet: r.fixed(32), session: r.fixed(32), kind: r.u8(), name: r.string(),
    attestation: r.fixed(32), stand: r.u8() };
  m.votes = [r.u32(), r.u32(), r.u32(), r.u32()];
  m.shares = r.u64(); m.claimed = r.bool(); m.tag = r.fixed(32);
  return m;
}

/** The operator's revealed AI roster (V5 §18.2). */
export function decodeRoster(data) {
  const r = new Reader(data);
  if (magicOf(r) !== MAGIC.roster) throw new Error('not a roster account');
  return { seasonId: r.u64(), bump: r.u8(), entries: r.vec(x => ({ member: x.u32(), civ: x.u16(), salt: x.fixed(32) })) };
}

/** The fixed front of a nation account: office holders, keys, budgets, who submitted. */
export function decodeNationHeader(data) {
  const r = new Reader(data);
  if (magicOf(r) !== MAGIC.nation) throw new Error('not a nation account');
  const n = { seasonId: r.u64(), civ: r.u16(), bump: r.u8(), preset: r.u8(), market: r.bool(), crank: r.fixed(32), openTick: r.u16() };
  n.officers = [r.u32(), r.u32(), r.u32(), r.u32()];
  n.keys = [r.fixed(32), r.fixed(32), r.fixed(32), r.fixed(32)];
  n.spendable = [r.u32(), r.u32(), r.u32(), r.u32()];
  n.submitted = [r.u16(), r.u16(), r.u16(), r.u16()];
  n.frozen = r.bool();
  n.revealing = r.bool();
  n.revealDeadline = Number(r.i64());
  n.committed = [r.u16(), r.u16(), r.u16(), r.u16()];
  n.commits = [r.fixed(32), r.fixed(32), r.fixed(32), r.fixed(32)];
  n.salts = [r.fixed(32), r.fixed(32), r.fixed(32), r.fixed(32)];
  return n;
}

/** World header (chunk 0): magic (`MAGIC.world`, or `MAGIC.genesis` while genesis runs), body length, `WorldMeta`. */
export function decodeWorldHeader(data) {
  const r = new Reader(data);
  const magic = magicOf(r);
  const len = r.u32();
  const meta = { seasonId: r.u64(), preset: r.u8(), civs: r.u8(), tickSeconds: r.u32(), deadline: Number(r.i64()), finished: r.bool(), market: r.bool(),
    frozen: r.bool(), vrf: r.fixed(32), inputChunks: r.u16(), inputLogged: r.u16(), revealing: r.bool() };
  return { magic, len, meta, bodyOffset: WORLD_HEADER };
}

/** The program's log records (`sol_log_data`, logged as `Program data:`). */
export const RECORD_TAGS = Object.freeze(['PS_TICK', 'PS_INPUT', 'PS_GENESIS', 'PS_SEAT', 'PS_OPEN', 'PS_COMMITS', 'PS_SALTS', 'PS_HISTORY', 'PS_TALK']);

/** Parse a `Program data:` log of one of the `RECORD_TAGS` records (fields already base64-decoded). */
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
  // Commitments closed: borsh Vec<(civ u16, role u8, member u32, commitment [32])>.
  if (tag === 'PS_COMMITS') {
    const r = new Reader(fields[2]);
    return { tag, tick: u16(fields[1]), commits: r.vec(x => ({ civ: x.u16(), role: ROLES[x.u8()], member: x.u32(), commitment: x.fixed(32) })) };
  }
  // Input frozen: the pre-state root and the revealed salts, borsh Vec<(civ u16, role u8, salt [32])>.
  if (tag === 'PS_SALTS') {
    const r = new Reader(fields[3]);
    return { tag, tick: u16(fields[1]), preRoot: fields[2], salts: r.vec(x => ({ civ: x.u16(), role: ROLES[x.u8()], salt: x.fixed(32) })) };
  }
  // A finalized season's history (permutation_rules::history): its record, borsh SeasonRecord.
  if (tag === 'PS_HISTORY') {
    const r = new Reader(fields[4]);
    const hex32 = () => Buffer.from(r.fixed(32)).toString('hex');
    const finalRoot = hex32();
    const nations = r.vec(x => ({ points: x.u64(), era: x.u8(), tiers: [x.u8(), x.u8(), x.u8(), x.u8()], share: x.u64(), cities: x.u32(), members: x.u32() }));
    const opt = (x, f) => (x.u8() ? f(x) : null);
    const cities = r.vec(x => ({ q: x.i32(), r: x.i32(), founder: x.u16(), foundedTick: x.u16(), owner: opt(x, y => y.u16()), capturedFrom: opt(x, y => y.u16()), pop: x.u32(), alive: x.bool() }));
    const ruins = r.vec(x => ({ q: x.i32(), r: x.i32(), peak: x.u16() }));
    return { tag, seasonId: new DataView(fields[1].buffer, fields[1].byteOffset).getBigUint64(0, true), prevHistoryRoot: fields[2], historyRoot: fields[3], record: { finalRoot, nations, cities, ruins } };
  }
  // A tick's relayed messages (V5 §18.7): season, tick, count, Merkle root.
  if (tag === 'PS_TALK') {
    return { tag, seasonId: new DataView(fields[1].buffer, fields[1].byteOffset).getBigUint64(0, true), tick: u16(fields[2]),
      count: new DataView(fields[3].buffer, fields[3].byteOffset).getUint32(0, true), root: fields[4] };
  }
  return { tag };
}

/**
 * What a member can claim once the season is finalized: its prize plus its
 * share of what is left in its nation's treasury (V5 §7.5), exactly as the
 * program's `claim_amount`. `season` and `member` as decoded above.
 */
export function claimParts(season, member) {
  const prize = BigInt(season.payouts[member.index] ?? 0n);
  const deposited = BigInt(season.treasury[member.civ] ?? 0n);
  const left = BigInt(season.treasuryFinal[member.civ] ?? 0n);
  const refund = deposited === 0n ? 0n : (BigInt(member.shares) * left) / deposited;
  return { prize, refund, total: prize + refund };
}
export const claimAmount = (season, member) => claimParts(season, member).total;

/** Program errors (`ChainError`, surfaced as `{"Custom": code}`), in code order from 1. */
export const CHAIN_ERRORS = Object.freeze(['InvalidInstruction', 'MissingSignature', 'WrongPda', 'AlreadyInitialized', 'NotInitialized', 'WrongStatus',
  'Unauthorized', 'SeasonFull', 'InvalidName', 'WrongMint', 'WrongTokenAccount', 'WorldTooSmall', 'Rules', 'WrongTick', 'OverBudget', 'TooEarly',
  'MissingNation', 'WrongDelegationProgram', 'WrongMagicProgram', 'AlreadyClaimed', 'NothingToClaim', 'SeasonNotOver', 'WrongOffice',
  'InvalidParams', 'WrongWorld', 'InboxFull', 'TickFrozen', 'InputNotPublished', 'WrongPhase', 'CommitMismatch', 'Retired',
  'RosterPending', 'RosterMismatch']);

/**
 * The program error named in a transaction error message, or null. Reads
 * both the JSON form (`{"Custom":27}`) and the log form
 * (`custom program error: 0x1b`).
 */
export function chainError(message) {
  const text = String(message);
  const m = /"Custom":\s*(\d+)/.exec(text) ?? /custom program error: (0x[0-9a-f]+|\d+)/i.exec(text);
  if (!m) return null;
  const code = Number(m[1]);
  return CHAIN_ERRORS[code - 1] ?? `Custom(${code})`;
}
