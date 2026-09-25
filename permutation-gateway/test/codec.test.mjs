import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Writer } from '../client/src/borsh.mjs';
import {
  chainError, CHAIN_ERRORS, claimAmount, claimParts, decodeMember, decodeNationHeader, decodeSeason, decodeWorldHeader, encodeGov, encodeOrder, IX, IX_TAG, NOBODY,
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

// Whole-world intents the client no longer builds (superseded by commitPart / undelegatePart).
const NOT_BUILT = ['commit', 'commitAndUndelegate'];

test('every program instruction encodes exactly like ChainInstruction', () => {
  const orders = vectors.orders.slice(0, 6).map(v => v.dto);
  const built = {
    createSeason: IX.createSeason({ seasonId: 42n, preset: 0, nations: 6, entryFee: 10_000_000n, tickSeconds: 30, worldSeed: k(7), crank: k(9), market: true }),
    allocWorld: IX.allocWorld(3),
    register: IX.register({ civ: 2, name: 'アステル', kind: 1, session: k(1), attestation: k(0), stand: 5, votes: [0, NOBODY, 3, NOBODY], deposit: 5_000_000n }),
    startSeason: IX.startSeason(),
    genesisStep: IX.genesisStep(50),
    delegate: IX.delegate(1003),
    submitOrders: IX.submitOrders({ role: 'Steward', tick: 17, decisionDigest: k(5), orders, adopt: [4, 9] }),
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
  };
  for (const v of vectors.instructions) {
    if (NOT_BUILT.includes(v.name)) assert.equal(v.hex, hex([IX_TAG[v.name]]), `${v.name} carries no data`);
    else assert.equal(hex(built[v.name]), v.hex, v.name);
  }
  assert.deepEqual([...Object.keys(built), ...NOT_BUILT].sort(), vectors.instructions.map(v => v.name).sort(), 'every instruction is covered');
});

test('IX_TAG is the first byte of every instruction vector, and covers exactly the Rust enum', () => {
  for (const v of vectors.instructions) assert.equal(IX_TAG[v.name], fromHex(v.hex)[0], v.name);
  assert.deepEqual(Object.keys(IX_TAG).sort(), vectors.instructions.map(v => v.name).sort());
});

test('account decoders return what the Rust accounts hold', () => {
  const { season, member, nation, worldHeader } = vectors.accounts;
  assert.deepEqual(plain(decodeSeason(fromHex(season.hex))), season.decoded);
  assert.deepEqual(plain(decodeMember(fromHex(member.hex))), member.decoded);
  assert.deepEqual(plain(decodeNationHeader(fromHex(nation.hex))), nation.decoded);
  assert.deepEqual(plain(decodeWorldHeader(fromHex(worldHeader.hex))), worldHeader.decoded);
  assert.throws(() => decodeSeason(fromHex(member.hex)), /not a V5 season/);
  assert.throws(() => decodeMember(fromHex(season.hex)), /not a member/);
  assert.throws(() => decodeNationHeader(fromHex(season.hex)), /not a nation/);
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

test('claimAmount equals the program\'s claim_amount', () => {
  assert.ok(vectors.claims.length > 0);
  for (const v of vectors.claims) {
    const season = { payouts: v.payouts.map(BigInt), treasury: v.treasury.map(BigInt), treasuryFinal: v.treasuryFinal.map(BigInt) };
    const member = { index: v.index, civ: v.civ, shares: BigInt(v.shares) };
    assert.equal(claimAmount(season, member).toString(), v.amount, JSON.stringify(v));
    const { prize, refund, total } = claimParts(season, member);
    assert.equal(prize + refund, total);
  }
});
