import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { Writer } from '../client/src/borsh.mjs';
import { encodeOrder, IX } from '../client/src/codec.mjs';

const vectors = JSON.parse(readFileSync(new URL('./vectors.json', import.meta.url), 'utf8'));
const hex = b => Buffer.from(b).toString('hex');
const k = b => new Uint8Array(32).fill(b);

test('every order DTO encodes exactly like the Rust Order', () => {
  for (const v of vectors.orders) {
    assert.equal(hex(encodeOrder(new Writer(), v.dto).toBytes()), v.hex, JSON.stringify(v.dto));
  }
});

test('every program instruction encodes exactly like ChainInstruction', () => {
  const orders = vectors.orders.slice(0, 6).map(v => v.dto);
  const built = {
    createSeason: IX.createSeason({ seasonId: 42n, preset: 0, maxCivs: 6, entryFee: 10_000_000n, exchangeCredit: 20_000_000n, tickSeconds: 30, worldSeed: k(7), crank: k(9) }),
    allocWorld: IX.allocWorld(3),
    joinSeason: IX.joinSeason({ name: 'アステル', kind: 1, session: k(1), payout: k(2) }),
    startSeason: IX.startSeason(),
    genesisStep: IX.genesisStep(50),
    delegate: IX.delegate(3),
    submitOrders: IX.submitOrders({ tick: 17, decisionDigest: k(5), orders }),
    resolveTick: IX.resolveTick(12),
    commit: IX.commit(),
    commitAndUndelegate: IX.commitAndUndelegate(),
    finishSeason: IX.finishSeason(),
    claim: IX.claim(4),
    undelegatePart: IX.undelegatePart([3, 1002, 0]),
  };
  for (const v of vectors.instructions) assert.equal(hex(built[v.name]), v.hex, v.name);
  assert.equal(Object.keys(built).length, vectors.instructions.length, 'every instruction is covered');
});
