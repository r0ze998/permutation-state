// send(): a transaction the ER refused only because it had not loaded the
// program yet is rebuilt and sent again; any other failure is not.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Keypair, SystemProgram } from '@solana/web3.js';
import { send } from '../src/send.mjs';

function er(statuses) {
  const sent = [];
  return {
    sent,
    getLatestBlockhash: async () => ({ blockhash: Keypair.generate().publicKey.toBase58(), lastValidBlockHeight: 99 }),
    sendRawTransaction: async bytes => { sent.push(bytes); return `sig${sent.length}`.padEnd(64, '1'); },
    getSignatureStatuses: async () => ({ value: [statuses[sent.length - 1]] }),
    getTransaction: async () => ({ meta: { logMessages: [], computeUnitsConsumed: 5 } }),
    getBlockHeight: async () => 1,
  };
}

const payer = Keypair.generate();
const ix = SystemProgram.transfer({ fromPubkey: payer.publicKey, toPubkey: Keypair.generate().publicKey, lamports: 1 });
const ok = { confirmationStatus: 'confirmed', err: null };
const unloaded = { confirmationStatus: 'confirmed', err: { InstructionError: [1, 'UnsupportedProgramId'] } };
const refused = { confirmationStatus: 'confirmed', err: { InstructionError: [1, { Custom: 27 }] } };

test('UnsupportedProgramId is retried with a fresh transaction', async () => {
  const c = er([unloaded, unloaded, ok]);
  const r = await send(c, [ix], [payer], 'test');
  assert.equal(c.sent.length, 3);
  assert.notDeepEqual(c.sent[0], c.sent[2], 'rebuilt, not resent');
  assert.equal(r.cu, 5);
});

test('program errors and a lasting UnsupportedProgramId still fail', async () => {
  await assert.rejects(send(er([refused]), [ix], [payer], 'test'), /Custom/);
  const c = er([unloaded, unloaded, unloaded, unloaded, unloaded]);
  await assert.rejects(send(c, [ix], [payer], 'test'), /UnsupportedProgramId/);
  assert.equal(c.sent.length, 4);
});
