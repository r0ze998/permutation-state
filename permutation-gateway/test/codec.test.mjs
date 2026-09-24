import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { Writer } from '../client/src/borsh.mjs';
import { encodeGov, encodeOrder, IX, NOBODY } from '../client/src/codec.mjs';

const vectors = JSON.parse(readFileSync(new URL('./vectors.json', import.meta.url), 'utf8'));
const hex = b => Buffer.from(b).toString('hex');
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
    createSeason: IX.createSeason({ seasonId: 42n, preset: 0, nations: 6, entryFee: 10_000_000n, tickSeconds: 30, worldSeed: k(7), crank: k(9), market: true }),
    allocWorld: IX.allocWorld(3),
    register: IX.register({ civ: 2, name: 'アステル', kind: 1, session: k(1), attestation: k(0), stand: 5, votes: [0, NOBODY, 3, NOBODY], deposit: 5_000_000n }),
    startSeason: IX.startSeason(),
    genesisStep: IX.genesisStep(50),
    delegate: IX.delegate(1003),
    submitOrders: IX.submitOrders({ role: 'Steward', tick: 17, decisionDigest: k(5), orders, adopt: [4, 9] }),
    resolveTick: IX.resolveTick(12),
    commit: IX.commit(),
    commitAndUndelegate: IX.commitAndUndelegate(),
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
  for (const v of vectors.instructions) assert.equal(hex(built[v.name]), v.hex, v.name);
  assert.equal(Object.keys(built).length, vectors.instructions.length, 'every instruction is covered');
});

test('program errors are named by their code', async () => {
  const { chainError, CHAIN_ERRORS } = await import('../client/src/codec.mjs');
  assert.equal(chainError('{"InstructionError":[1,{"Custom":27}]}'), 'TickFrozen');
  assert.equal(chainError('{"InstructionError":[2,{"Custom":16}]}'), 'TooEarly');
  assert.equal(chainError('{"InstructionError":[0,{"Custom":14}]}'), 'WrongTick');
  assert.equal(chainError('ProgramFailedToComplete'), null);
  assert.equal(CHAIN_ERRORS.at(-1), 'InputNotPublished');
});
