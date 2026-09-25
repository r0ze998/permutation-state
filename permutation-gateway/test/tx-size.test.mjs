// Offline: the largest transactions the gateway and clients build must fit
// one packet (1232 bytes), serialized and signed as they are sent.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Keypair, Transaction } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { BATCH_BYTES, NATIONS, NOBODY } from '../client/src/codec.mjs';
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
  fits(`logTickInput, ${nations} nations`, chain.logTickInput({ nations, chunk: 0 }));
  fits(`openGovernment, ${nations} nations`, chain.openGovernment({ authority: crank.publicKey, nations }));
  fits('startSeason', chain.startSeason({ authority: crank.publicKey }));
  fits('genesisStep', chain.genesisStep({ work: 50 }));
  fits('finishSeason', chain.finishSeason());
});

test('every commit and undelegate intent fits', () => {
  for (const targets of intentGroups({ nations })) {
    fits(`commitPart ${targets}`, chain.commitPart({ payer: crank.publicKey, targets }));
    fits(`undelegatePart ${targets}`, chain.undelegatePart({ payer: crank.publicKey, targets }));
  }
});

test('a full RevealOrders batch fits, signed by the session key and the fee payer', () => {
  const order = { type: 'SendEnvoy', cityState: 1, influence: 5 }; // 7 bytes
  const orders = Array.from({ length: Math.floor(BATCH_BYTES / 7) }, () => order);
  const batch = packBatch({ orders });
  assert.ok(batch.fits && batch.used > BATCH_BYTES - 7);
  fits(`revealOrders (${batch.used} bytes of orders)`, chain.revealOrders({ signer: session.publicKey, civ: 5, role: 'Diplomat', tick: 179,
    decisionDigest: new Uint8Array(32).fill(9), orders: batch.orders, adopt: [1, 2, 3], salt: new Uint8Array(32).fill(4) }), [crank, session]);
});

test('a CommitOrders fits, signed by the session key and the fee payer', () => {
  fits('commitOrders', chain.commitOrders({ signer: session.publicKey, civ: 5, role: 'Diplomat', tick: 179, commitment: new Uint8Array(32).fill(3) }), [crank, session]);
});

test('an x402 Register with the longest name fits', () => {
  const wallet = Keypair.generate();
  fits('register', chain.register({ wallet: wallet.publicKey, feePayer: crank.publicKey, civ: 5, walletToken: Keypair.generate().publicKey,
    mint: Keypair.generate().publicKey, name: 'x'.repeat(24), kind: 1, session: session.publicKey, stand: 15, votes: [1, 2, NOBODY, 4], deposit: 5n }), [crank, wallet]);
});
