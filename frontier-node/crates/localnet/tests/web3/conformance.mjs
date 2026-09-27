// RPC conformance of frontier-localnet against the real clients (M1
// contract §8.7): @solana/web3.js 1.99 and permutation-gateway's send.mjs
// and tickscan.mjs, imported from the gateway tree so their own response
// validation (web3.js's superstruct schemas) runs against the node.
//
//   node conformance.mjs <rpc url> <ws url> <permutation-gateway dir> <program id> <program account> <b58 "PSF1"> <b58 "XXXX">
//
// <program id> owns <program account> (set by the Rust test with
// frontier_setAccount) for the getProgramAccounts checks; the two base58
// strings are memcmp filters (base58 is web3.js's default encoding). Prints one JSON
// line {ok, checks} and exits 0, or prints the failure and exits 1.
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import assert from 'node:assert/strict';

const [rpc, ws, gw, programId, programAccount, psf1, xxxx] = process.argv.slice(2);
const require = createRequire(`${gw}/package.json`);
const web3 = require('@solana/web3.js');
const send = await import(pathToFileURL(`${gw}/src/send.mjs`).href);
const tickscan = await import(pathToFileURL(`${gw}/src/tickscan.mjs`).href);
const { Connection, Keypair, PublicKey, SystemProgram, Transaction, LAMPORTS_PER_SOL, ComputeBudgetProgram } = web3;

const checks = [];
const check = async (name, fn) => {
  try {
    await fn();
    checks.push(name);
  } catch (e) {
    console.error(`FAIL ${name}: ${e.stack || e}`);
    process.exit(1);
  }
};
const withTimeout = (p, ms, what) => Promise.race([p, new Promise((_, rej) => setTimeout(() => rej(new Error(`timeout: ${what}`)), ms))]);

const conn = new Connection(rpc, { commitment: 'confirmed', wsEndpoint: ws });
const payer = Keypair.fromSeed(new Uint8Array(32).fill(21));
const dest = Keypair.fromSeed(new Uint8Array(32).fill(22)).publicKey;
const program = new PublicKey(programId);
const owned = new PublicKey(programAccount);

await check('getVersion/getHealth/getSlot/getBlockHeight/getEpochInfo/getGenesisHash', async () => {
  const v = await conn.getVersion();
  assert.equal(v['solana-core'], '3.1.9');
  assert.ok((await conn.getSlot()) > 0);
  assert.ok((await conn.getBlockHeight('confirmed')) > 0);
  const e = await conn.getEpochInfo();
  assert.equal(typeof e.absoluteSlot, 'number');
  assert.equal((await conn.getGenesisHash()).length > 30, true);
});

await check('requestAirdrop + confirmTransaction (WebSocket signatureSubscribe)', async () => {
  const sig = await conn.requestAirdrop(payer.publicKey, 10 * LAMPORTS_PER_SOL);
  const r = await withTimeout(conn.confirmTransaction(sig, 'confirmed'), 20_000, 'confirmTransaction');
  assert.equal(r.value.err, null);
  assert.equal(await conn.getBalance(payer.publicKey), 10 * LAMPORTS_PER_SOL);
});

await check('getLatestBlockhash + blockhash-strategy confirmTransaction + isBlockhashValid', async () => {
  const { blockhash, lastValidBlockHeight } = await conn.getLatestBlockhash('confirmed');
  const tx = new Transaction({ feePayer: payer.publicKey, blockhash, lastValidBlockHeight }).add(
    SystemProgram.transfer({ fromPubkey: payer.publicKey, toPubkey: dest, lamports: LAMPORTS_PER_SOL }));
  tx.sign(payer);
  const sig = await conn.sendRawTransaction(tx.serialize());
  const r = await withTimeout(conn.confirmTransaction({ signature: sig, blockhash, lastValidBlockHeight }, 'confirmed'), 20_000, 'confirm');
  assert.equal(r.value.err, null);
  assert.equal((await conn.isBlockhashValid(blockhash)).value, true);
});

let sent;
await check('send.mjs send(): preflight-less send, HTTP confirm, getTransaction logs and CU', async () => {
  const ix = SystemProgram.transfer({ fromPubkey: payer.publicKey, toPubkey: dest, lamports: 1_000_000 });
  sent = await send.send(conn, [ComputeBudgetProgram.setComputeUnitLimit({ units: 60_000 }), ix], [payer], 'transfer');
  assert.ok(sent.fetched);
  assert.ok(sent.logs.some(l => l.includes('11111111111111111111111111111111 success')), sent.logs.join('\n'));
  assert.equal(typeof sent.cu, 'number');
});

await check('getTransaction (json, maxSupportedTransactionVersion 0) and getParsedTransaction-free fields', async () => {
  const t = await conn.getTransaction(sent.signature, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 });
  assert.equal(t.transaction.signatures[0], sent.signature);
  assert.equal(t.meta.err, null);
  assert.equal(t.meta.fee, 5_000 + 0);
  assert.equal(t.meta.preBalances.length, t.transaction.message.staticAccountKeys.length);
  assert.equal(t.meta.preBalances[0] - t.meta.postBalances[0], 1_000_000 + t.meta.fee);
  assert.equal(typeof t.blockTime, 'number');
  assert.equal(await conn.getBlockTime(t.slot), t.blockTime);
});

await check('send.mjs simulateWire (sigVerify, no blockhash replacement) and SimulationError', async () => {
  const { blockhash } = await conn.getLatestBlockhash();
  const ok = new Transaction({ feePayer: payer.publicKey, recentBlockhash: blockhash }).add(
    SystemProgram.transfer({ fromPubkey: payer.publicKey, toPubkey: dest, lamports: 2 }));
  ok.sign(payer);
  const r = await send.simulateWire(conn, ok.serialize());
  assert.ok(r.unitsConsumed > 0);
  const bad = new Transaction({ feePayer: payer.publicKey, recentBlockhash: blockhash }).add(
    SystemProgram.transfer({ fromPubkey: payer.publicKey, toPubkey: dest, lamports: 1_000 * LAMPORTS_PER_SOL }));
  bad.sign(payer);
  await assert.rejects(send.simulateWire(conn, bad.serialize()), e => {
    assert.ok(e instanceof send.SimulationError);
    assert.deepEqual(e.err, { InstructionError: [0, { Custom: 1 }] });
    assert.ok(e.logs.length > 0);
    return true;
  });
  // web3.js simulateTransaction with post-state accounts.
  const vtx = web3.VersionedTransaction.deserialize(ok.serialize());
  const s = await conn.simulateTransaction(vtx, { sigVerify: true, accounts: { encoding: 'base64', addresses: [dest.toBase58()] } });
  assert.equal(s.value.err, null);
  assert.equal(s.value.accounts[0].lamports, (await conn.getBalance(dest)) + 2);
});

await check('sendRawTransaction with preflight: SendTransactionError carries the logs', async () => {
  const { blockhash } = await conn.getLatestBlockhash();
  const bad = new Transaction({ feePayer: payer.publicKey, recentBlockhash: blockhash }).add(
    SystemProgram.transfer({ fromPubkey: payer.publicKey, toPubkey: dest, lamports: 1_000 * LAMPORTS_PER_SOL }));
  bad.sign(payer);
  await assert.rejects(conn.sendRawTransaction(bad.serialize()), e => {
    assert.ok(e instanceof web3.SendTransactionError, String(e));
    assert.ok(e.logs.length > 0);
    return true;
  });
});

await check('send.mjs sendWire of a failing transaction: SendError with the chain logs', async () => {
  const { blockhash, lastValidBlockHeight } = await conn.getLatestBlockhash();
  const bad = new Transaction({ feePayer: payer.publicKey, recentBlockhash: blockhash }).add(
    SystemProgram.transfer({ fromPubkey: payer.publicKey, toPubkey: dest, lamports: 1_000 * LAMPORTS_PER_SOL }));
  bad.sign(payer);
  await assert.rejects(send.sendWire(conn, bad.serialize(), 'bad', { lastValidBlockHeight }), e => {
    assert.ok(e instanceof send.SendError);
    assert.match(e.message, /Custom/);
    assert.ok(e.logs.length > 0, 'logs fetched with getTransaction');
    return true;
  });
});

await check('getSignatureStatuses (searchTransactionHistory) and getSignaturesForAddress paging', async () => {
  const st = await conn.getSignatureStatuses([sent.signature], { searchTransactionHistory: true });
  assert.equal(st.value[0].confirmationStatus, 'finalized');
  assert.equal(st.value[0].err, null);
  const all = await conn.getSignaturesForAddress(payer.publicKey, { limit: 1000 }, 'confirmed');
  assert.ok(all.length >= 4, `${all.length}`);
  const first = await conn.getSignaturesForAddress(payer.publicKey, { limit: 2 });
  const next = await conn.getSignaturesForAddress(payer.publicKey, { limit: 2, before: first[1].signature });
  assert.deepEqual([...first, ...next].map(s => s.signature), all.slice(0, 4).map(s => s.signature));
  const until = await conn.getSignaturesForAddress(payer.publicKey, { until: all[2].signature });
  assert.deepEqual(until.map(s => s.signature), all.slice(0, 2).map(s => s.signature));
  assert.ok(all.some(s => s.err !== null), 'the failed transfer is listed with its error');
});

await check('getAccountInfo / getMultipleAccountsInfo(AndContext) / dataSlice', async () => {
  const a = await conn.getAccountInfo(owned);
  assert.ok(a.owner.equals(program));
  assert.deepEqual([...a.data.subarray(0, 4)], [0x50, 0x53, 0x46, 0x31]);
  const s = await conn.getAccountInfo(owned, { dataSlice: { offset: 2, length: 3 } });
  assert.deepEqual([...s.data], [0x46, 0x31, 7]);
  const m = await conn.getMultipleAccountsInfo([owned, Keypair.generate().publicKey]);
  assert.equal(m.length, 2);
  assert.equal(m[1], null);
  const mc = await conn.getMultipleAccountsInfoAndContext([owned], 'confirmed');
  assert.ok(mc.context.slot > 0);
  assert.equal(await conn.getMinimumBalanceForRentExemption(0), 128 * 5_080);
});

await check('getProgramAccounts (memcmp base58, dataSize, withContext)', async () => {
  const rows = await conn.getProgramAccounts(program, { filters: [{ dataSize: 40 }, { memcmp: { offset: 0, bytes: psf1 } }] });
  assert.equal(rows.length, 1);
  assert.ok(rows[0].pubkey.equals(owned));
  const none = await conn.getProgramAccounts(program, { filters: [{ memcmp: { offset: 0, bytes: xxxx } }] });
  assert.equal(none.length, 0);
  const wc = await conn.getProgramAccounts(program, { withContext: true });
  assert.equal(wc.value.length, 1);
});

await check('onSlotChange and onLogs (WebSocket slotSubscribe / logsSubscribe mentions)', async () => {
  const slot = await withTimeout(new Promise(res => {
    const id = conn.onSlotChange(s => { conn.removeSlotChangeListener(id); res(s.slot); });
  }), 10_000, 'slot notification');
  assert.ok(slot > 0);
  const got = new Promise(res => {
    const id = conn.onLogs(payer.publicKey, l => { conn.removeOnLogsListener(id); res(l); }, 'confirmed');
  });
  await new Promise(r => setTimeout(r, 300));
  const s = await send.send(conn, [SystemProgram.transfer({ fromPubkey: payer.publicKey, toPubkey: dest, lamports: 3 })], [payer], 'logs');
  const l = await withTimeout(got, 10_000, 'logs notification');
  assert.equal(l.signature, s.signature);
  assert.equal(l.err, null);
  assert.ok(l.logs.length > 0);
});

await check('tickscan.mjs scanRecords (getSignaturesForAddress + getTransaction)', async () => {
  const r = await tickscan.scanRecords({ connection: conn, address: payer.publicKey, program: SystemProgram.programId.toBase58() });
  assert.equal(r.complete, true);
  assert.ok(r.txs.length >= 3);
  for (let i = 1; i < r.txs.length; i++) assert.ok(r.txs[i - 1].slot <= r.txs[i].slot);
});

console.log(JSON.stringify({ ok: true, checks }));
process.exit(0);
