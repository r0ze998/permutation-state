// The web client's chain actions (permutation-server/web/chainio.mjs)
// against a fake gateway (a stubbed fetch): the x402 registration with the
// pins checked before any wallet popup, the message the wallet returns
// checked byte for byte, the Register it signs (kind 2, votes NOBODY, the
// public deposit), the retry on an expired blockhash, session-key relays,
// and claims (ATA created when the wallet has no token account).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createPublicKey, verify as nodeVerify, randomBytes } from 'node:crypto';
import bs58 from 'bs58';
import * as chainio from '../../permutation-server/web/chainio.mjs';
import { keyFromSeed } from '../../permutation-server/web/session.mjs';
import { compileMessage, parseTransaction, wireTransaction } from '../client/src/solana-tx.mjs';
import { ata, pda } from '../client/src/player.mjs';
import { IX_TAG, NOBODY, roleMask } from '../client/src/codec.mjs';

const SPKI = Buffer.from('302a300506032b6570032100', 'hex');
const nodeOk = (address, msg, sig) => nodeVerify(null, Buffer.from(msg), createPublicKey({ key: Buffer.concat([SPKI, bs58.decode(address)]), format: 'der', type: 'spki' }), Buffer.from(sig));
const key = () => bs58.encode(randomBytes(32));

const PROGRAM = key(), MINT = key(), CRANK = key(), BLOCKHASH = key();
const SEASON_ID = '7', FEE = 10_000_000n;

/** A wallet handle (wallet.mjs connected() shape) backed by a key; `tamper` edits the returned wire. */
async function testWallet(tamper = null) {
  const k = await keyFromSeed(randomBytes(32));
  const calls = [];
  return {
    address: k.publicKey, calls,
    async signTransaction(wire) {
      calls.push(wire);
      const tx = parseTransaction(wire);
      const i = tx.signers.indexOf(k.publicKey);
      const sigs = tx.signatures.slice();
      sigs[i] = await k.sign(tx.message);
      const out = wireTransaction(tx.message, sigs);
      return tamper ? tamper(out, tx, k) : out;
    },
  };
}

/** Decode Register's data (codec.mjs IX.register). */
function decodeRegister(d) {
  const v = new DataView(d.buffer, d.byteOffset, d.byteLength);
  let o = 0;
  const tag = d[o++]; const civ = v.getUint16(o, true); o += 2;
  const len = v.getUint32(o, true); o += 4;
  const name = new TextDecoder().decode(d.subarray(o, o + len)); o += len;
  const kind = d[o++]; const session = bs58.encode(d.subarray(o, o + 32)); o += 32; o += 32;
  const stand = d[o++]; const votes = [0, 1, 2, 3].map(i => v.getUint32(o + 4 * i, true)); o += 16;
  const deposit = v.getBigUint64(o, true); o += 8; o += 32;
  return { tag, civ, name, kind, session, stand, votes, deposit, rest: d.length - o };
}

/** A fake gateway: routes by method + path; records calls. */
function gateway({ members = [], deposit = '0', usdc = null, requirements = r => r, join = null, claimRelay = {}, relayFeePayer = CRANK } = {}) {
  const calls = [];
  const season = {
    programId: PROGRAM, cluster: 'localnet',
    season: { seasonId: SEASON_ID, entryFee: String(FEE), usdcMint: MINT, crank: CRANK, status: 'Registering', payouts: ['0', '5000000'], treasury: ['0', '1000000'], treasuryFinal: ['0', '500000'] },
    members, nations: ['Aster', 'Borealis'], accounts: { season: pda.season(PROGRAM, SEASON_ID), vault: pda.vault(PROGRAM, SEASON_ID) },
    registration: { members: members.length, aiCount: 1, openedAt: 1000, closesAt: 1600, serverNow: 1200, entryFee: String(FEE), deposit },
    endpoints: { base: 'http://127.0.0.1:18899' },
  };
  const req = () => requirements({
    scheme: 'exact', network: 'solana-localnet', maxAmountRequired: String(FEE + BigInt(deposit)), payTo: pda.vault(PROGRAM, SEASON_ID), asset: MINT,
    extra: { feePayer: CRANK, programId: PROGRAM, seasonId: SEASON_ID, accounts: { season: pda.season(PROGRAM, SEASON_ID), vault: pda.vault(PROGRAM, SEASON_ID) }, recentBlockhash: BLOCKHASH, aiCount: 1 },
  });
  const reply = (status, body) => ({ ok: status >= 200 && status < 300, status, json: async () => body });
  globalThis.fetch = async (url, init = {}) => {
    const u = new URL(url, 'http://page.test');
    const method = init.method || 'GET';
    const body = init.body ? JSON.parse(init.body) : null;
    calls.push({ method, path: u.pathname, query: u.search, body, headers: init.headers || {} });
    const route = `${method} ${u.pathname.replace(/^\/gw/, '')}`;
    if (route === 'GET /season') return reply(200, season);
    if (route === 'GET /usdc') return reply(200, usdc ?? { mint: MINT, decimals: 6, accounts: [{ address: ata(u.searchParams.get('owner'), MINT), amount: String(FEE) }] });
    if (route === 'POST /x402/join') {
      const header = (init.headers || {})['X-PAYMENT'];
      if (!header) return reply(402, { x402Version: 1, error: 'X-PAYMENT header is required', accepts: [req()] });
      const payment = JSON.parse(Buffer.from(header, 'base64').toString('utf8'));
      if (join) return join(payment, reply);
      return reply(200, { ok: true, member: 5, civ: body.civ, name: body.name, signature: 'sig' });
    }
    if (route === 'GET /relay') return reply(200, { feePayer: relayFeePayer, blockhash: BLOCKHASH, lastValidBlockHeight: 99 });
    if (route === 'POST /relay') return reply(200, { ok: true, signature: 'relaysig' });
    if (route === 'GET /claim-relay') return reply(200, { feePayer: CRANK, blockhash: BLOCKHASH, lastValidBlockHeight: 77, programId: PROGRAM, mint: MINT, status: 'Finalized', ...claimRelay });
    if (route === 'POST /claim-relay') return reply(200, { ok: true, signature: 'claimsig' });
    return reply(404, { error: 'not found', code: 'NotFound' });
  };
  return { calls, season };
}

chainio.setGateway('/gw/');
chainio.setPin({ programId: PROGRAM, cluster: 'localnet', seasonId: SEASON_ID, entryFee: Number(FEE), accounts: { season: pda.season(PROGRAM, SEASON_ID), vault: pda.vault(PROGRAM, SEASON_ID) } });

test('setPin derives the season accounts and refuses others', () => {
  const p = chainio.pinned();
  assert.equal(p.seasonId, '7');
  assert.equal(p.entryFee, FEE);
  assert.equal(p.vault, pda.vault(PROGRAM, SEASON_ID));
  assert.equal(chainio.gateway(), '/gw');
  assert.throws(() => chainio.setPin({ programId: PROGRAM, cluster: 'localnet', seasonId: SEASON_ID, entryFee: 1, accounts: { vault: key() } }), e => e.code === 'PinMismatch');
  assert.equal(chainio.pinned(), p); // unchanged
});

test('request: {ok, httpStatus, code, error} from the HTTP status and body; a body status is kept', async () => {
  gateway();
  const r = await chainio.request('GET', '/nothing');
  assert.deepEqual([r.ok, r.httpStatus, r.code, r.error], [false, 404, 'NotFound', 'not found']);
  globalThis.fetch = async () => { throw new Error('down'); };
  assert.deepEqual(await chainio.season(), { ok: false, httpStatus: 0, code: 'network', error: 'network' });
  globalThis.fetch = async () => ({ ok: false, status: 429, json: async () => { throw new Error('not json'); } });
  const rl = await chainio.faucet('x');
  assert.equal(rl.code, 'RateLimited');
  globalThis.fetch = async () => ({ ok: true, status: 200, json: async () => ({ status: 'Finalized', mint: 'm' }) });
  const st = await chainio.claimStatus('3');
  assert.deepEqual([st.ok, st.httpStatus, st.status, st.code], [true, 200, 'Finalized', null]);
});

test('join: registers with kind 2, votes NOBODY and the public deposit; the wallet signs once; nothing but X-PAYMENT goes along', async () => {
  const g = gateway({ deposit: '0' });
  const w = await testWallet();
  const session = await keyFromSeed(randomBytes(32));
  const steps = [];
  const r = await chainio.join({ wallet: w, session, civ: 1, name: 'Ada K.', stand: ['Science', 'Diplomat'], onStep: s => steps.push(s) });
  assert.equal(r.ok, true, JSON.stringify(r));
  assert.equal(r.member, 5);
  assert.deepEqual(steps, ['balance', 'quote', 'sign', 'send']);
  assert.equal(w.calls.length, 1);
  const pay = g.calls.filter(c => c.path === '/gw/x402/join');
  assert.equal(pay.length, 2);
  assert.equal(pay[0].headers['X-PAYMENT'], undefined);
  assert.deepEqual(Object.keys(pay[1].headers).sort(), ['Content-Type', 'X-PAYMENT']);
  assert.ok(g.calls.every(c => !Object.keys(c.headers).some(h => /member-token|authorization/i.test(h))));
  const payment = JSON.parse(Buffer.from(pay[1].headers['X-PAYMENT'], 'base64').toString('utf8'));
  assert.equal(payment.scheme, 'exact');
  assert.equal(payment.network, 'solana-localnet');
  const tx = parseTransaction(Buffer.from(payment.payload.transaction, 'base64'));
  // Fee payer first (its signature left for the gateway), the wallet a read-only signer.
  assert.deepEqual(tx.signers, [CRANK, w.address]);
  assert.equal(tx.header.numReadonlySignedAccounts, 1);
  assert.ok(tx.signatures[0].every(x => x === 0));
  assert.ok(nodeOk(w.address, tx.message, tx.signatures[1]));
  assert.equal(tx.recentBlockhash, BLOCKHASH);
  assert.equal(tx.instructions.length, 1); // no compute budget instruction
  const ix = tx.instructions[0];
  assert.equal(ix.programId, PROGRAM);
  assert.deepEqual(ix.keys.map(k => k.pubkey), [w.address, CRANK, pda.season(PROGRAM, SEASON_ID), pda.member(PROGRAM, SEASON_ID, w.address), ata(w.address, MINT),
    pda.vault(PROGRAM, SEASON_ID), MINT, 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA', '11111111111111111111111111111111']);
  const d = decodeRegister(ix.data);
  assert.deepEqual({ ...d }, { tag: IX_TAG.register, civ: 1, name: 'Ada K.', kind: 2, session: session.publicKey, stand: roleMask(['Science', 'Diplomat']),
    votes: [NOBODY, NOBODY, NOBODY, NOBODY], deposit: 0n, rest: 0 });
});

test('join: a public deposit is paid by everyone alike', async () => {
  gateway({ deposit: '2000000', usdc: null });
  const w = await testWallet();
  const session = await keyFromSeed(randomBytes(32));
  // The wallet's ATA holds exactly the fee: not enough with the deposit.
  const short = await chainio.join({ wallet: w, session, civ: 0, name: 'Bo', stand: ['General'] });
  assert.equal(short.code, 'InsufficientFunds');
  assert.equal(w.calls.length, 0);
  const g = gateway({ deposit: '2000000', usdc: { mint: MINT, decimals: 6, accounts: [{ address: ata(w.address, MINT), amount: '12000000' }] } });
  const r = await chainio.join({ wallet: w, session, civ: 0, name: 'Bo', stand: ['General'] });
  assert.equal(r.ok, true);
  const payment = JSON.parse(Buffer.from(g.calls.at(-1).headers['X-PAYMENT'], 'base64').toString('utf8'));
  assert.equal(decodeRegister(parseTransaction(Buffer.from(payment.payload.transaction, 'base64')).instructions[0].data).deposit, 2_000_000n);
});

test('join: a payment request that does not match the pinned season is refused before any popup', async () => {
  const cases = {
    payTo: r => ({ ...r, payTo: key() }),
    amount: r => ({ ...r, maxAmountRequired: String(FEE + 1n) }),
    asset: r => ({ ...r, asset: key() }),
    programId: r => ({ ...r, extra: { ...r.extra, programId: key() } }),
    seasonId: r => ({ ...r, extra: { ...r.extra, seasonId: '8' } }),
    feePayer: r => ({ ...r, extra: { ...r.extra, feePayer: key() } }),
    accounts: r => ({ ...r, extra: { ...r.extra, accounts: { ...r.extra.accounts, vault: key() } } }),
    network: r => ({ ...r, network: 'solana' }),
  };
  for (const [what, requirements] of Object.entries(cases)) {
    const g = gateway({ requirements });
    const w = await testWallet();
    const r = await chainio.join({ wallet: w, session: await keyFromSeed(randomBytes(32)), civ: 0, name: 'Bo', stand: ['General'] });
    assert.equal(r.code, 'X402Mismatch', what);
    assert.match(r.error, new RegExp(what === 'feePayer' ? 'feePayer' : what));
    assert.equal(w.calls.length, 0, what);
    assert.equal(g.calls.filter(c => c.path.endsWith('/x402/join')).length, 1, what);
  }
  // The fee payer may never be the paying wallet.
  const w = await testWallet();
  const r = chainio.paymentProblems({ scheme: 'exact', network: 'solana-localnet', payTo: pda.vault(PROGRAM, SEASON_ID), asset: MINT, maxAmountRequired: String(FEE),
    extra: { feePayer: w.address, programId: PROGRAM, seasonId: SEASON_ID, accounts: { season: pda.season(PROGRAM, SEASON_ID), vault: pda.vault(PROGRAM, SEASON_ID) }, recentBlockhash: BLOCKHASH } },
  { mint: MINT, crank: w.address, need: FEE, wallet: w.address });
  assert.deepEqual(r, ['feePayer is the wallet']);
});

test('join: refuses before signing when the wallet or the session key is already a member', async () => {
  const w = await testWallet();
  const session = await keyFromSeed(randomBytes(32));
  gateway({ members: [{ index: 0, civ: 0, wallet: w.address, session: key() }] });
  assert.equal((await chainio.join({ wallet: w, session, civ: 0, name: 'Bo', stand: ['General'] })).code, 'AlreadyMember');
  gateway({ members: [{ index: 0, civ: 0, wallet: key(), session: session.publicKey }] });
  assert.equal((await chainio.join({ wallet: w, session, civ: 0, name: 'Bo', stand: ['General'] })).code, 'SessionInUse');
  assert.equal(w.calls.length, 0);
});

test('join: a wallet that changes the message, or signs badly, is caught and nothing is sent', async () => {
  const other = key();
  const variants = {
    WalletModified: out => { const tx = parseTransaction(out); return wireTransaction(compileMessage({ feePayer: CRANK, recentBlockhash: other, instructions: tx.instructions.map(i => ({ programId: i.programId, keys: i.keys, data: i.data })) }), tx.signatures); },
    WalletBadSignature: (out, tx) => { const sigs = tx.signatures.slice(); sigs[1] = new Uint8Array(64).fill(1); return wireTransaction(tx.message, sigs); },
  };
  for (const [code, tamper] of Object.entries(variants)) {
    const g = gateway();
    const r = await chainio.join({ wallet: await testWallet(tamper), session: await keyFromSeed(randomBytes(32)), civ: 0, name: 'Bo', stand: ['General'] });
    assert.equal(r.code, code);
    assert.equal(g.calls.filter(c => c.headers['X-PAYMENT']).length, 0, code);
  }
  // A wallet that says no.
  const g = gateway();
  const no = { address: (await testWallet()).address, signTransaction: async () => { throw Object.assign(new Error('rejected'), { code: 'WalletRejected' }); } };
  assert.equal((await chainio.join({ wallet: no, session: await keyFromSeed(randomBytes(32)), civ: 0, name: 'Bo', stand: ['General'] })).code, 'WalletRejected');
  assert.equal(g.calls.filter(c => c.headers['X-PAYMENT']).length, 0);
});

test('join: settlement failed with no member → retry; failed answer but the member exists → registered', async () => {
  gateway({ join: (p, reply) => reply(402, { x402Version: 1, error: 'settlement failed: Blockhash not found', accepts: [] }) });
  const w = await testWallet();
  const r = await chainio.join({ wallet: w, session: await keyFromSeed(randomBytes(32)), civ: 0, name: 'Bo', stand: ['General'] });
  assert.equal(r.ok, false);
  assert.equal(r.code, 'BlockhashExpired');
  assert.equal(r.retry, true);
  const g = gateway({ join: (p, reply) => { g.season.members.push({ index: 9, civ: 0, name: 'Bo', wallet: w.address, session: key() }); return reply(500, { error: 'timeout' }); } });
  const ok = await chainio.join({ wallet: w, session: await keyFromSeed(randomBytes(32)), civ: 0, name: 'Bo', stand: ['General'] });
  assert.equal(ok.ok, true);
  assert.equal(ok.member, 9);
  assert.equal(ok.recovered, true);
  gateway({ join: (p, reply) => reply(409, { error: 'the session key is in use', code: 'SessionInUse' }) });
  assert.equal((await chainio.join({ wallet: w, session: await keyFromSeed(randomBytes(32)), civ: 0, name: 'Bo', stand: ['General'] })).code, 'SessionInUse');
});

test('relay: the session key signs; the fee payer must be the season crank', async () => {
  const g = gateway();
  await chainio.season();
  const session = await keyFromSeed(randomBytes(32));
  const ixs = [{ programId: PROGRAM, keys: [{ pubkey: session.publicKey, isSigner: true, isWritable: false }, { pubkey: pda.nation(PROGRAM, SEASON_ID, 0), isSigner: false, isWritable: true }], data: new Uint8Array([17]) }];
  const r = await chainio.relay(ixs, session);
  assert.equal(r.ok, true);
  const post = g.calls.find(c => c.method === 'POST' && c.path === '/gw/relay');
  assert.equal(post.body.lastValidBlockHeight, 99);
  const tx = parseTransaction(Buffer.from(post.body.tx, 'base64'));
  assert.deepEqual(tx.signers, [CRANK, session.publicKey]);
  assert.ok(nodeOk(session.publicKey, tx.message, tx.signatures[1]));
  const g2 = gateway({ relayFeePayer: key() });
  assert.equal((await chainio.relay(ixs, session)).code, 'RelayMismatch');
  assert.equal(g2.calls.some(c => c.method === 'POST'), false);
});

test('claim: Finalized only; the ATA is created when the wallet has no token account; the wallet signs Claim', async () => {
  const w = await testWallet();
  gateway({ claimRelay: { status: 'Running' } });
  const early = await chainio.claim({ wallet: w });
  assert.equal(early.code, 'NotFinalized');
  assert.equal(early.seasonStatus, 'Running');
  assert.equal(w.calls.length, 0);

  const g = gateway({ usdc: { mint: MINT, decimals: 6, accounts: [] } });
  const r = await chainio.claim({ wallet: w });
  assert.equal(r.ok, true, JSON.stringify(r));
  assert.equal(r.dest, ata(w.address, MINT));
  assert.equal(g.calls.find(c => c.path === '/gw/claim-relay' && c.method === 'GET').query, '?season=7');
  const post = g.calls.find(c => c.path === '/gw/claim-relay' && c.method === 'POST');
  const tx = parseTransaction(Buffer.from(post.body.tx, 'base64'));
  assert.deepEqual(tx.signers, [CRANK, w.address]);
  assert.equal(tx.header.numReadonlySignedAccounts, 1);
  assert.deepEqual(tx.instructions.map(i => i.programId), ['ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL', PROGRAM]);
  assert.deepEqual(tx.instructions[0].keys.map(k => k.pubkey), [CRANK, ata(w.address, MINT), w.address, MINT, '11111111111111111111111111111111', 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA']);
  assert.deepEqual(tx.instructions[1].keys.map(k => k.pubkey), [w.address, pda.season(PROGRAM, SEASON_ID), pda.member(PROGRAM, SEASON_ID, w.address), pda.vault(PROGRAM, SEASON_ID), ata(w.address, MINT), MINT, 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA']);
  assert.deepEqual([...tx.instructions[1].data], [IX_TAG.claim]);
  assert.ok(nodeOk(w.address, tx.message, tx.signatures[1]));

  // An existing token account (not the ATA) is used as it is; an earlier season names its own id.
  const acct = key();
  const g2 = gateway({ usdc: { mint: MINT, decimals: 6, accounts: [{ address: acct, amount: '0' }] } });
  const r2 = await chainio.claim({ wallet: w, seasonId: '6' });
  assert.equal(r2.dest, acct);
  assert.equal(g2.calls.find(c => c.path === '/gw/claim-relay').query, '?season=6');
  const tx2 = parseTransaction(Buffer.from(g2.calls.find(c => c.method === 'POST').body.tx, 'base64'));
  assert.equal(tx2.instructions.length, 1);
  assert.equal(tx2.instructions[0].keys[1].pubkey, pda.season(PROGRAM, '6'));
});

test('helpers: registration times in s or ms, paying account, claim amounts, explorer links', () => {
  const s = { registration: { openedAt: 1000, closesAt: 1600, serverNow: 1200, deposit: '5' }, season: { entryFee: '10' }, members: [1, 2] };
  const r = chainio.registrationOf(s, 1_200_000 - 500);
  assert.deepEqual([r.openedAt, r.closesAt, r.serverNow, r.offsetMs, r.deposit, r.entryFee, r.members], [1_000_000, 1_600_000, 1_200_000, 500, 5n, 10n, 2]);
  assert.equal(chainio.registrationOf({ registration: { closesAt: 1_700_000_000_000 } }).closesAt, 1_700_000_000_000);
  assert.equal(chainio.registrationOf({}).closesAt, null);
  const owner = key();
  const u = { mint: MINT, accounts: [{ address: 'big', amount: '50' }, { address: ata(owner, MINT), amount: '20' }, { address: 'small', amount: '5' }] };
  assert.equal(chainio.payingAccount(u, { owner, mint: MINT, need: 10n }), ata(owner, MINT));
  assert.equal(chainio.payingAccount(u, { owner, mint: MINT, need: 30n }), 'big');
  assert.equal(chainio.payingAccount(u, { owner, mint: MINT, need: 60n }), null);
  assert.equal(chainio.bestBalance(u), 50n);
  assert.deepEqual(chainio.claimDestination({ mint: key(), accounts: [{ address: 'x' }] }, { owner, mint: MINT }), { dest: ata(owner, MINT), create: true });
  const g = { season: { payouts: ['0', '7'], treasury: ['100', '0'], treasuryFinal: ['40', '0'] } };
  assert.deepEqual(chainio.claimOf(g, { index: 1, civ: 0, shares: '50' }), { prize: 7n, refund: 20n, total: 27n });
  assert.equal(chainio.explorerTx('abc', { cluster: 'devnet' }), 'https://explorer.solana.com/tx/abc?cluster=devnet');
  assert.equal(chainio.explorerTx('abc', { cluster: 'localnet', base: 'http://127.0.0.1:18899' }), 'https://explorer.solana.com/tx/abc?cluster=custom&customUrl=http%3A%2F%2F127.0.0.1%3A18899');
  assert.equal(chainio.explorerTx('abc', { cluster: 'localnet' }), null);
  assert.equal(chainio.devWalletAllowed({ devWallet: true }), true);
  assert.equal(chainio.devWalletAllowed({ registration: {} }), false);
  assert.equal(chainio.x402Network('devnet'), 'solana-devnet');
});
