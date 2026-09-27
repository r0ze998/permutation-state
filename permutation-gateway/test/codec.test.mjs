import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Writer } from '../client/src/borsh.mjs';
import {
  chainError, CHAIN_ERRORS, claimAmount, claimParts, decodeMember, decodeRoster, parseRecord, rosterChain, rosterCommit, rosterTag, encodeBatch, orderCommitment,
  RETIRED_IX, decodeNationHeader, decodeSeason, decodeWorldHeader, encodeGov, encodeOrder, IX, IX_TAG, NOBODY, MAX_NAME, MEMBER_NAMES, memberName,
  govSlots, govQuota, bondFloor, forfeitPenalty, checkAbort, runningDeadline, abortRefund, opsAfterAbort, escrow, degradeAfter, effectiveTo,
  isFree, orderOutOfRange, NATION_HEAD_LEN, RAND, SEED,
} from '../client/src/codec.mjs';
import { fromHex, hex, plain, vectors } from './vectors.mjs';

const k = b => new Uint8Array(32).fill(b);

test('every order DTO encodes exactly like the Rust Order', () => {
  for (const v of vectors.orders) {
    assert.equal(hex(encodeOrder(new Writer(), v.dto).toBytes()), v.hex, JSON.stringify(v.dto));
  }
});

test('every governance action encodes exactly like the Rust GovAction', () => {
  for (const v of vectors.gov) {
    assert.equal(hex(encodeGov(new Writer(), v.dto).toBytes()), v.hex, JSON.stringify(v.dto));
  }
});

test('every program instruction encodes exactly like ChainInstruction', () => {
  const orders = vectors.orders.slice(0, 6).map(v => v.dto);
  const built = {
    createSeason: IX.createSeason({ seasonId: 42n, preset: 0, nations: 6, entryFee: 10_000_000n, tickSeconds: 30, worldSeed: k(7), crank: k(9), market: true, prevSeasonId: 41n,
      aiCount: 3, rosterCommit: k(12), bountyEach: 5_000_000n, bond: 60_000_000n, deposit: 1_000_000n, startBy: 1_790_003_600, validator: k(17) }),
    allocWorld: IX.allocWorld(3),
    register: IX.register({ civ: 2, name: 'アステル', kind: 1, session: k(1), attestation: k(0), stand: 5, votes: [0, NOBODY, 3, NOBODY], deposit: 5_000_000n, tag: k(13) }),
    startSeason: IX.startSeason(),
    genesisStep: IX.genesisStep(50),
    delegate: IX.delegate(1003),
    resolveTick: IX.resolveTick(12),
    finishSeason: IX.finishSeason(),
    claim: IX.claim(),
    undelegatePart: IX.undelegatePart([3, 1002, 0]),
    updateMember: IX.updateMember({ stand: 3, votes: [1, 1, NOBODY, 2] }),
    allocNation: IX.allocNation(5),
    seatMembers: IX.seatMembers(),
    openGovernment: IX.openGovernment(),
    submitGov: IX.submitGov({ member: 11, action: vectors.gov[2].dto }),
    withdrawOps: IX.withdrawOps(),
    logTickInput: IX.logTickInput(2),
    commitPart: IX.commitPart([1000, 1001, 7]),
    closeCommits: IX.closeCommits(),
    commitOrders: IX.commitOrders({ role: 'Science', tick: 17, commitment: k(6) }),
    revealOrders: IX.revealOrders({ role: 'Steward', tick: 17, decisionDigest: k(5), orders, adopt: [4, 9], salt: k(7) }),
    revealRoster: IX.revealRoster({ from: 1, salts: [k(14), k(15)], blind: k(18) }),
    anchorTalk: IX.anchorTalk({ tick: 33, count: 12, root: k(16) }),
    startClock: IX.startClock(),
    postBond: IX.postBond(7_000_000n),
    freezeTick: IX.freezeTick(),
    consumeTickRandomness: IX.consumeTickRandomness({ randomness: k(19), seasonId: 42n, tick: 17 }),
    retryTickRandomness: IX.retryTickRandomness(),
    consumeSeasonSeed: IX.consumeSeasonSeed({ randomness: k(20), seasonId: 42n }),
    retrySeasonSeed: IX.retrySeasonSeed(),
    abort: IX.abort(),
    requestUndelegation: IX.requestUndelegation(1002),
    rollbackUndelegation: IX.rollbackUndelegation(7),
    closeSeasonAccounts: IX.closeSeasonAccounts([1, 2, 1000]),
  };
  for (const v of vectors.instructions) {
    if (RETIRED_IX.includes(v.name)) assert.equal(fromHex(v.hex)[0], IX_TAG[v.name], `${v.name} keeps its tag`);
    else assert.equal(hex(built[v.name]), v.hex, v.name);
  }
  assert.deepEqual([...Object.keys(built), ...RETIRED_IX].sort(), vectors.instructions.map(v => v.name).sort(), 'every instruction is covered');
  // A negative start_by is borsh i64 (two's complement), as the program reads it.
  assert.deepEqual([...IX.createSeason({ seasonId: 1n, preset: 0, nations: 2, entryFee: 0n, tickSeconds: 1, worldSeed: k(0), crank: k(0), startBy: -2, validator: k(1) }).slice(-40, -32)],
    [0xfe, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
});

test('sealed orders: the batch and its commitment match permutation_rules::orders::order_commitment', () => {
  const c = vectors.commitment;
  const b = { civ: c.civ, tick: c.tick, role: c.role, member: c.member, decisionDigest: fromHex(c.decisionDigest), orders: c.orders, adopt: c.adopt };
  assert.equal(hex(encodeBatch(b)), c.batchHex);
  assert.equal(hex(orderCommitment(b, fromHex(c.salt))), c.commitment);
});

test('IX_TAG is the first byte of every instruction vector, and covers exactly the Rust enum', () => {
  for (const v of vectors.instructions) assert.equal(IX_TAG[v.name], fromHex(v.hex)[0], v.name);
  assert.deepEqual(Object.keys(IX_TAG).sort(), vectors.instructions.map(v => v.name).sort());
});

test('account decoders return what the Rust accounts hold', () => {
  const { season, legacySeason, member, nation, worldHeader, roster } = vectors.accounts;
  assert.deepEqual(plain(decodeSeason(fromHex(season.hex))), season.decoded);
  assert.deepEqual(plain(decodeRoster(fromHex(roster.hex))), roster.decoded);
  assert.deepEqual(plain(decodeMember(fromHex(member.hex))), member.decoded);
  assert.deepEqual(plain(decodeNationHeader(fromHex(nation.hex))), nation.decoded);
  assert.deepEqual(plain(decodeWorldHeader(fromHex(worldHeader.hex))), worldHeader.decoded);
  assert.throws(() => decodeSeason(fromHex(member.hex)), /not a V5 season/);
  assert.throws(() => decodeMember(fromHex(season.hex)), /not a member/);
  assert.throws(() => decodeNationHeader(fromHex(season.hex)), /not a nation/);
  // The nation header is the fixed front: it reads nothing past NATION_HEAD_LEN.
  assert.equal(nation.headLen, NATION_HEAD_LEN);
  assert.deepEqual(decodeNationHeader(fromHex(nation.hex).slice(0, NATION_HEAD_LEN)), decodeNationHeader(fromHex(nation.hex)));
  assert.throws(() => decodeNationHeader(fromHex(nation.hex).slice(0, NATION_HEAD_LEN - 1)), /out of data/);
});

test('a legacy PSSEASN7 season decodes with its v8 fields zero (claims of seasons from before this program)', () => {
  const { legacySeason } = vectors.accounts;
  const s = decodeSeason(fromHex(legacySeason.hex));
  assert.deepEqual(plain(s), legacySeason.decoded);
  assert.equal(s.legacy, true);
  assert.deepEqual([s.deposit, s.outstanding, s.voided, s.refundBase.length, s.refundInPayout.length], [0n, 0n, false, 0, 0]);
  assert.equal(decodeSeason(fromHex(vectors.accounts.season.hex)).legacy, false);
  const older = fromHex(legacySeason.hex);
  older.set(new TextEncoder().encode('PSSEASN6'));
  assert.throws(() => decodeSeason(older), /not a V5 season/);
});

test('program errors: names and codes match ChainError', () => {
  assert.deepEqual(CHAIN_ERRORS.map((name, i) => ({ code: i + 1, name })), vectors.errors);
});

test('chainError reads the JSON and the log form of a custom program error', () => {
  assert.equal(chainError('{"InstructionError":[1,{"Custom":27}]}'), 'TickFrozen');
  assert.equal(chainError('{"InstructionError":[2,{"Custom":16}]}'), 'TooEarly');
  assert.equal(chainError('{"InstructionError":[0,{"Custom":14}]}'), 'WrongTick');
  assert.equal(chainError('Program J4aZ… failed: custom program error: 0x1b'), 'TickFrozen');
  assert.equal(chainError('custom program error: 0x7'), 'Unauthorized');
  assert.equal(chainError('custom program error: 0x15'), 'NothingToClaim');
  assert.equal(chainError('{"Custom":99}'), 'Custom(99)');
  assert.equal(chainError('ProgramFailedToComplete'), null);
  assert.equal(chainError(undefined), null);
});

/** A claim vector's season as decodeSeason returns it (the fields claimParts reads). */
const claimSeason = v => ({ ...v, entryFee: BigInt(v.entryFee), bountyEach: BigInt(v.bountyEach), bond: BigInt(v.bond), payouts: v.payouts.map(BigInt),
  treasury: v.treasury.map(BigInt), treasuryFinal: v.treasuryFinal.map(BigInt), refundBase: v.refundBase.map(BigInt), refundInPayout: fromHex(v.refundInPayout) });

test('claimAmount equals the program\'s claim_amount (legacy rule)', () => {
  const legacy = vectors.claims.filter(v => !v.v9);
  assert.ok(legacy.length >= 7);
  for (const v of legacy) {
    const member = { index: v.index, civ: v.civ, shares: BigInt(v.shares) };
    assert.equal(claimAmount(claimSeason(v.season), member).toString(), v.amount, JSON.stringify(v));
    const { prize, refund, total } = claimParts(claimSeason(v.season), member);
    assert.equal(prize + refund, total);
  }
});

// The v9 rules (WP10 refund base and flags, WP14 refunds after an abort, WP08
// saturation) land in the program's claim_amount with unit P3; until then
// the vectors carry no amount for these cases (`claimRuleV9` false).
test('claimAmount equals the program\'s claim_amount: refund base, refund flags, Aborted, saturation', { todo: vectors.claimRuleV9 ? false : 'PENDING until P3 lands the v9 claim_amount (regenerate the vectors)' }, () => {
  const cases = vectors.claims.filter(v => v.v9);
  assert.ok(cases.length >= 10);
  for (const v of cases) {
    assert.notEqual(v.amount, null, 'the vectors come from a program with the v9 claim rules');
    const member = { index: v.index, civ: v.civ, shares: BigInt(v.shares) };
    assert.equal(claimAmount(claimSeason(v.season), member).toString(), v.amount, JSON.stringify(v));
  }
});

test('claimParts: flagged members get their payout alone, others their refund over refundBase; Aborted pays refunds', () => {
  // These hold whatever the vectors say (they are the design's rules, WP10 and WP14).
  const season = { status: 'Finalized', payouts: [12n, 30n, 5n], treasury: [100n], treasuryFinal: [80n], refundBase: [40n], refundInPayout: Uint8Array.of(0b010) };
  assert.deepEqual(claimParts(season, { index: 1, civ: 0, shares: 60n }), { prize: 30n, refund: 0n, total: 30n }, 'flagged: the refund is inside the payout');
  assert.deepEqual(claimParts(season, { index: 0, civ: 0, shares: 20n }), { prize: 12n, refund: 40n, total: 52n }, '20 × 80 / 40');
  assert.deepEqual(claimParts({ ...season, refundBase: [], refundInPayout: new Uint8Array() }, { index: 0, civ: 0, shares: 20n }),
    { prize: 12n, refund: 16n, total: 28n }, 'no refund base: the legacy rule, over the deposits');
  assert.equal(claimAmount({ ...season, payouts: [2n ** 64n - 1n] }, { index: 0, civ: 0, shares: 20n }), 2n ** 64n - 1n, 'saturates');
  const aborted = { status: 'Aborted', abortedFrom: 'Seating', entryFee: 10n, memberCount: 4, aiCount: 2, bountyEach: 3n, bond: 7n, payouts: [99n] };
  assert.deepEqual(claimParts(aborted, { index: 0, civ: 0, shares: 5n }), { prize: 0n, refund: 15n, forfeit: 3n, total: 18n }, '(2×3 + 7) / 4 = 3');
  assert.deepEqual(claimParts({ ...aborted, abortedFrom: 'Registering' }, { index: 0, civ: 0, shares: 5n }), { prize: 0n, refund: 15n, forfeit: 0n, total: 15n });
});

test('after an abort: escrow, refunds and the operator\'s remainder equal lifecycle.rs', () => {
  assert.ok(vectors.lifecycle.refunds.length >= 5);
  for (const v of vectors.lifecycle.refunds) {
    const season = claimSeason(v.season);
    const member = { index: 0, civ: 0, shares: BigInt(v.shares) };
    assert.equal(escrow(season).toString(), v.escrow, JSON.stringify(v));
    assert.equal(abortRefund(season, member).total.toString(), v.refund, JSON.stringify(v));
    assert.equal(opsAfterAbort(season).toString(), v.opsAfterAbort, JSON.stringify(v));
    assert.equal(claimAmount(season, member).toString(), v.refund, 'Claim of an Aborted season is its refund');
  }
});

test('checkAbort and runningDeadline equal lifecycle::check_abort / running_deadline', () => {
  const { ticksPerSeason, abort } = vectors.lifecycle;
  assert.ok(abort.length >= 20);
  const seen = new Set();
  for (const v of abort) {
    const s = { ...v.season, startBy: BigInt(v.season.startBy), stageAt: BigInt(v.season.stageAt) };
    assert.equal(runningDeadline(s, ticksPerSeason).toString(), v.runningDeadline, JSON.stringify(v));
    assert.equal(checkAbort(s, v.view, v.operator, BigInt(v.now), ticksPerSeason), v.result, JSON.stringify(v));
    seen.add(v.result);
  }
  assert.deepEqual([...seen].sort(), [null, 'TooEarly', 'WorldTooSmall', 'WrongStatus'].sort(), 'every verdict is covered');
  // `now` may be a Number (unix seconds).
  assert.equal(checkAbort({ status: 'Registering', startBy: 1000 }, null, false, 1000 + 86_400), null);
});

test('governance slots and quotas equal state::gov_slots / gov_quota', () => {
  assert.ok(vectors.govSlots.length >= 9);
  for (const v of vectors.govSlots) {
    assert.equal(encodeGov(new Writer(), v.dto).toBytes().length, v.bytes, JSON.stringify(v.dto));
    assert.equal(govSlots(v.dto), v.slots, JSON.stringify(v.dto));
  }
  assert.ok(vectors.govSlots.some(v => v.slots === null), 'an action over the size limit is refused');
  for (const [members, here, quota] of vectors.govQuota) assert.equal(govQuota(members, here), quota, `${members} members, ${here} here`);
});

test('bond floor and forfeit penalty equal state::bond_floor / forfeit_penalty', () => {
  for (const v of vectors.bond) {
    const s = { entryFee: BigInt(v.entryFee), memberCount: v.memberCount, aiCount: v.aiCount, bountyEach: BigInt(v.bountyEach), pool: BigInt(v.pool), treasury: v.treasury.map(BigInt) };
    assert.equal(forfeitPenalty(s).toString(), v.forfeitPenalty, JSON.stringify(v));
    assert.equal(bondFloor(s).toString(), v.bondFloor, JSON.stringify(v));
  }
});

test('free orders and value bounds follow the rules crate', () => {
  for (const v of vectors.offices) assert.equal(isFree(v.dto), v.free, JSON.stringify(v.dto));
  for (const v of vectors.orders) assert.equal(orderOutOfRange(v.dto), null, JSON.stringify(v.dto));
  const c = vectors.constants;
  assert.equal(orderOutOfRange({ type: 'ExchangeOrder', good: { kind: 'Iron' }, side: 'Buy', amount: c.MAX_TRADE_AMOUNT, price: BigInt(c.EXCHANGE_MAX_PRICE) }), null);
  assert.match(orderOutOfRange({ type: 'ExchangeOrder', good: { kind: 'Iron' }, side: 'Buy', amount: 1, price: BigInt(c.EXCHANGE_MAX_PRICE) + 1n }), /price/);
  assert.match(orderOutOfRange({ type: 'Transfer', civ: 1, good: { kind: 'Gold' }, amount: c.MAX_TRADE_AMOUNT + 1 }), /amount/);
  assert.match(orderOutOfRange({ type: 'MarketTrade', good: { kind: 'Gold' }, side: 'Sell', amount: c.MAX_TRADE_AMOUNT + 1, limitGold: 1 }), /amount/);
  assert.match(orderOutOfRange({ type: 'MoveUnit', unit: 1, path: [[0, 0], [c.MAX_ORDER_COORD + 1, 0]] }), /coordinate/);
  assert.match(orderOutOfRange({ type: 'SetStanding', target: { kind: 'Unit', id: 1 }, rule: { kind: 'Patrol', route: [[0, -c.MAX_ORDER_COORD - 1]] } }), /coordinate/);
});

test('operator AI roster: tags and their chain match permutation_rules::roster', () => {
  const tags = vectors.roster.tags.map(v => {
    const t = rosterTag(BigInt(v.seasonId), fromHex(v.wallet), fromHex(v.salt));
    assert.equal(hex(t), v.tag);
    return t;
  });
  assert.equal(hex(rosterChain(tags)), vectors.roster.chain);
  for (const c of vectors.roster.commits) {
    assert.equal(c.chain, vectors.roster.chain);
    assert.equal(hex(rosterCommit(fromHex(c.blind), fromHex(c.chain))), c.commit);
  }
});

const le = (n, v) => { const b = new Uint8Array(n); const d = new DataView(b.buffer); if (n === 8) d.setBigUint64(0, BigInt.asUintN(64, BigInt(v)), true); else if (n === 4) d.setUint32(0, v, true); else if (n === 2) d.setUint16(0, v, true); else b[0] = v; return b; };
const tag = t => new TextEncoder().encode(t);

test('PS_TALK records parse (season, tick, count, root)', () => {
  const r = parseRecord([tag('PS_TALK'), le(8, 77), le(2, 33), le(4, 12), k(16)]);
  assert.deepEqual({ ...r, root: hex(r.root) }, { tag: 'PS_TALK', seasonId: 77n, tick: 33, count: 12, root: hex(k(16)) });
});

test('PS_TICK: v8 records (6 fields) and v9 records (9 fields, degraded bit, randomness)', () => {
  const v8 = parseRecord([tag('PS_TICK'), le(2, 17), le(1, 200), k(1), k(2), k(3)]);
  assert.deepEqual(plain(v8), { tag: 'PS_TICK', tick: 17, to: 200, stop: 12, degraded: false, preRoot: hex(k(1)), root: hex(k(2)), inputHash: hex(k(3)), version: 8 });
  const v9 = parseRecord([tag('PS_TICK'), le(2, 17), le(1, 0x83), k(1), k(2), k(3), le(1, RAND.vrf), k(4), k(5)]);
  assert.deepEqual(plain(v9), { tag: 'PS_TICK', tick: 17, to: 0x83, stop: 3, degraded: true, preRoot: hex(k(1)), root: hex(k(2)), inputHash: hex(k(3)), version: 9,
    randState: RAND.vrf, randPre: hex(k(4)), randOut: hex(k(5)) });
  assert.deepEqual(effectiveTo(12), { stop: 12, degraded: false });
  assert.deepEqual(effectiveTo(0xff), { stop: 12, degraded: true });
  assert.deepEqual(effectiveTo(0x83, 6), { stop: 12, degraded: false }, 'v8: bit 7 was never a flag');
  for (const [t, after] of vectors.constants.degradeAfter) assert.equal(degradeAfter(t), after, `tick ${t}`);
});

test('the randomness and escape-hatch records parse (PS_SALTS, PS_FREEZE, PS_RAND, PS_SEED, PS_ABORT, PS_ROLLBACK)', () => {
  const salts = new Writer().vec([[2, 1, k(9)]], (w, [c, r, s]) => w.u16(c).u8(r).fixed(s, 32)).toBytes();
  assert.deepEqual(plain(parseRecord([tag('PS_SALTS'), le(2, 4), k(8), salts])), { tag: 'PS_SALTS', tick: 4, randPre: hex(k(8)), salts: [{ civ: 2, role: 'Steward', salt: hex(k(9)) }] });
  const lapsed = new Writer().vec([[1, 3, 7]], (w, [c, r, m]) => w.u16(c).u8(r).u32(m)).toBytes();
  assert.deepEqual(plain(parseRecord([tag('PS_FREEZE'), le(2, 4), k(8), lapsed])), { tag: 'PS_FREEZE', tick: 4, randPre: hex(k(8)), lapsed: [{ civ: 1, role: 'Diplomat', member: 7 }] });
  assert.deepEqual(plain(parseRecord([tag('PS_RAND'), le(2, 4), le(1, RAND.fallback), k(1), k(2)])), { tag: 'PS_RAND', tick: 4, state: RAND.fallback, oracle: hex(k(1)), vrf: hex(k(2)) });
  assert.deepEqual(plain(parseRecord([tag('PS_SEED'), le(8, 42), le(1, SEED.vrf), k(3), k(4)])), { tag: 'PS_SEED', seasonId: '42', state: SEED.vrf, oracle: hex(k(3)), seasonSeed: hex(k(4)) });
  assert.deepEqual(plain(parseRecord([tag('PS_ABORT'), le(8, 42), le(1, 2), le(8, 1_790_000_000)])), { tag: 'PS_ABORT', seasonId: '42', abortedFrom: 'Seating', at: 1_790_000_000 });
  assert.deepEqual(plain(parseRecord([tag('PS_ROLLBACK'), le(8, 42), le(2, 1003), k(6)])), { tag: 'PS_ROLLBACK', seasonId: '42', target: 1003, hash: hex(k(6)) });
});

test('memberName: one generator for everyone, from a large pool, always a valid name', () => {
  assert.ok(MEMBER_NAMES.length >= 300, `${MEMBER_NAMES.length} given names`);
  assert.equal(new Set(MEMBER_NAMES).size, MEMBER_NAMES.length, 'no duplicates');
  for (const n of MEMBER_NAMES) assert.match(n, /^[A-Z][a-z]+$/, n);
  // Deterministic in its bytes; the first u32 picks the name, the second the initial (0: none).
  const at = (i, s) => { const b = new Uint8Array(32); const d = new DataView(b.buffer); d.setUint32(0, i, true); d.setUint32(4, s, true); return b; };
  assert.equal(memberName(at(0, 0)), MEMBER_NAMES[0]);
  assert.equal(memberName(at(1, 1)), `${MEMBER_NAMES[1]} A.`);
  assert.equal(memberName(at(MEMBER_NAMES.length + 2, 26)), `${MEMBER_NAMES[2]} Z.`);
  assert.equal(memberName(at(3, 27)), MEMBER_NAMES[3]);
  assert.equal(memberName([...at(5, 3)]), `${MEMBER_NAMES[5]} C.`);
  assert.throws(() => memberName(new Uint8Array(7)), /8 random bytes/);
  const seen = new Set();
  for (let i = 0; i < 2000; i++) {
    const n = memberName(crypto.getRandomValues(new Uint8Array(32)));
    assert.match(n, /^[A-Z][a-z]+( [A-Z]\.)?$/, n);
    const len = new TextEncoder().encode(n).length;
    assert.ok(len >= 1 && len <= MAX_NAME, `${n}: ${len} bytes`);
    // The name round-trips through Register's borsh string.
    assert.equal(IX.register({ civ: 0, name: n, kind: 2, session: k(1), tag: k(2) })[3], len);
    seen.add(n);
  }
  assert.ok(seen.size > 1750, `2000 draws gave ${seen.size} distinct names (about 1830 expected)`);
});
