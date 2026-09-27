// Offline: the largest transactions the gateway and clients build must fit
// one packet (1232 bytes), serialized and signed as they are sent.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Keypair, Transaction } from '@solana/web3.js';
import { ChainClient, CLOSE_BATCH } from '../client/src/chain.mjs';
import { BATCH_BYTES, DEGRADED, MAX_BATCH_ORDERS, MAX_REVEAL_BYTES, NATION_TARGET, NATIONS, NOBODY } from '../client/src/codec.mjs';
import { packBatch } from '../client/src/batch.mjs';
import { intentGroups } from '../src/crank.mjs';
import { SEAT_BATCH } from '../src/season.mjs';
import { DEFAULTS } from '../src/config.mjs';

const PACKET = 1232;
const chain = new ChainClient(DEFAULTS.programId, 1_790_000_000_123n);
const crank = Keypair.generate();
const session = Keypair.generate();
const blockhash = Keypair.generate().publicKey.toBase58();
const nations = NATIONS.length;

function size(ixs, signers = [crank]) {
  const tx = new Transaction().add(...ixs);
  tx.feePayer = signers[0].publicKey;
  tx.recentBlockhash = blockhash;
  tx.sign(...signers);
  return tx.serialize().length;
}
const fits = (name, ixs, signers) => {
  const n = size(ixs, signers);
  assert.ok(n <= PACKET, `${name}: ${n} > ${PACKET} bytes`);
  return n;
};

test('world-wide instructions (every world chunk, every nation) fit', () => {
  const members = Array.from({ length: SEAT_BATCH }, () => Keypair.generate().publicKey);
  fits(`seatMembers × ${SEAT_BATCH}`, chain.seatMembers({ authority: crank.publicKey, members }));
  fits(`resolveTick, ${nations} nations`, chain.resolveTick({ nations, to: 12 }));
  fits(`resolveTick (degraded), ${nations} nations`, chain.resolveTick({ nations, to: DEGRADED }));
  fits(`logTickInput, ${nations} nations`, chain.logTickInput({ nations, chunk: 0 }));
  fits(`closeCommits, ${nations} nations`, chain.closeCommits({ nations }));
  fits(`startClock, ${nations} nations`, chain.startClock({ crank: crank.publicKey, nations }));
  fits(`openGovernment, ${nations} nations`, chain.openGovernment({ authority: crank.publicKey, nations }));
  fits('genesisStep', chain.genesisStep({ work: 50 }));
  fits('finishSeason', chain.finishSeason());
  fits('finishSeason + roster', chain.finishSeason({ roster: true }));
  fits('abort', chain.abort({ caller: crank.publicKey }));
});

test('the randomness requests fit, the freeze with its first request in one transaction', () => {
  for (const delegated of [true, false]) {
    fits(`freezeTick + retryTickRandomness, ${nations} nations`, chain.freezeTick({ payer: crank.publicKey, nations, delegated, request: true }));
    fits('retryTickRandomness', chain.retryTickRandomness({ payer: crank.publicKey, delegated }));
  }
  fits('startSeason + retrySeasonSeed', chain.startSeason({ authority: crank.publicKey, request: true }));
  fits('retrySeasonSeed', chain.retrySeasonSeed({ payer: crank.publicKey }));
});

test('the operator\'s season steps fit: createSeason with AI members, postBond, the roster reveal, the escape hatches', () => {
  const k = () => Keypair.generate().publicKey;
  fits('createSeason (AI members, a predecessor)', chain.createSeason({ admin: crank.publicKey, mint: k(), entryFee: 10n, worldSeed: new Uint8Array(32), crank: k(), prevSeasonId: 7n,
    aiCount: 12, rosterCommit: new Uint8Array(32), bountyEach: 5n, bond: 9n, adminToken: k(), deposit: 1n, startBy: 1_790_000_000, validator: k() }));
  fits('postBond', chain.postBond({ authority: crank.publicKey, source: k(), mint: k(), amount: 5n }));
  fits('revealRoster × 12', chain.revealRoster({ authority: crank.publicKey, from: 0, members: Array.from({ length: 12 }, k),
    salts: Array.from({ length: 12 }, () => new Uint8Array(32)), blind: new Uint8Array(32) }));
  fits('requestUndelegation', chain.requestUndelegation({ operator: crank.publicKey, target: NATION_TARGET }));
  fits('rollbackUndelegation', chain.rollbackUndelegation({ target: 3, rentPayer: crank.publicKey }));
  const targets = Array.from({ length: CLOSE_BATCH }, (_, i) => (i < 10 ? i : NATION_TARGET + i - 10));
  fits(`closeSeasonAccounts × ${CLOSE_BATCH}`, chain.closeSeasonAccounts({ operator: crank.publicKey, targets }));
});

test('every commit and undelegate intent fits', () => {
  for (const targets of intentGroups({ nations })) {
    fits(`commitPart ${targets}`, chain.commitPart({ payer: crank.publicKey, targets }));
    fits(`undelegatePart ${targets}`, chain.undelegatePart({ payer: crank.publicKey, targets }));
  }
});

/** A MoveUnit of `steps` hexes: 9 + 8·steps bytes. */
const move = steps => ({ type: 'MoveUnit', unit: 7, path: Array.from({ length: steps }, (_, i) => [i, -i]) });

test('a full RevealOrders batch fits, signed by the session key and the fee payer; the fullest packable batch is within MAX_REVEAL_BYTES', () => {
  // As many bytes as a packed batch can hold: BATCH_BYTES in fewer than MAX_BATCH_ORDERS orders.
  const orders = Array.from({ length: 19 }, () => move(4)); // 19 × 41 = 779 bytes
  const batch = packBatch({ orders });
  assert.ok(batch.fits && batch.used > BATCH_BYTES - 41 && orders.length + 3 <= MAX_BATCH_ORDERS, JSON.stringify(batch.reason));
  assert.ok(BATCH_BYTES <= MAX_REVEAL_BYTES, 'what packBatch lets through, the program takes');
  fits(`revealOrders (${batch.used} bytes of orders)`, chain.revealOrders({ signer: session.publicKey, civ: 5, role: 'Diplomat', tick: 179,
    decisionDigest: new Uint8Array(32).fill(9), orders: batch.orders, adopt: [1, 2, 3], salt: new Uint8Array(32).fill(4) }), [crank, session]);
  // A batch of 912 bytes (the largest the gateway's seal route takes, self-paid by the session key) fits too.
  const big = [...Array.from({ length: 21 }, () => move(4)), move(1), move(1), move(1)]; // 21 × 41 + 3 × 17 = 912, 24 orders
  const packed = packBatch({ orders: big, maxBytes: MAX_REVEAL_BYTES });
  assert.deepEqual([packed.used, packed.fits, big.length], [912, true, MAX_BATCH_ORDERS]);
  fits('revealOrders (912 bytes, self-paid)', chain.revealOrders({ signer: session.publicKey, civ: 5, role: 'Diplomat', tick: 179,
    decisionDigest: new Uint8Array(32).fill(9), orders: big, adopt: [], salt: new Uint8Array(32).fill(4) }), [session]);
});

test('a CommitOrders fits, signed by the session key and the fee payer', () => {
  fits('commitOrders', chain.commitOrders({ signer: session.publicKey, civ: 5, role: 'Diplomat', tick: 179, commitment: new Uint8Array(32).fill(3) }), [crank, session]);
});

test('an x402 Register with the longest name fits, signed by the facilitator, the wallet and the session key', () => {
  const wallet = Keypair.generate();
  fits('register', chain.register({ wallet: wallet.publicKey, feePayer: crank.publicKey, civ: 5, walletToken: Keypair.generate().publicKey,
    mint: Keypair.generate().publicKey, name: 'x'.repeat(24), kind: 1, session: session.publicKey, stand: 15, votes: [1, 2, NOBODY, 4], deposit: 5n }), [crank, wallet, session]);
});
