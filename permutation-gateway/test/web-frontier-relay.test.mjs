// The web page's play actions against the real Frontier relay (W3-F;
// contract §8.3, §9.4, I-24, I-51): the relay (src/frontier, its public
// listener on 127.0.0.1:0) over a scripted chain, driven by the page's own
// write path (web/frontier/fplay.mjs over fchainio.mjs):
//  - every player shape the page builds — Join (wallet), SetSession (wallet),
//    SetVigil, FileTicket (1–3 sites over ≤ 3 provinces), Harvest, Build
//    (walls name the Province), Train, Muster, Dissolve, Garrison, Explore,
//    Depart at each tip preset — and both settle shapes (charged to the
//    requester's citizen) pass the relay's allowlist, signature, drain guard
//    and quota, with exactly the accounts the page recomputed;
//  - the answer's signature is the fee payer's signature of the signed
//    message, and a relay answering anything else is caught;
//  - no Reveal is ever a transaction: the material goes to POST /f/reveal
//    with no signature; a non-preset tip is refused (TipNotPreset);
//  - program errors come back by name and number and map to text; GET /f/tx
//    reports landed, failed (by program error) and expired.
import { test, after } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { Keypair, PublicKey } from '@solana/web3.js';
import * as io from '../../permutation-server/web/frontier/fchainio.mjs';
import * as fplay from '../../permutation-server/web/frontier/fplay.mjs';
import { failureText, errorText } from '../../permutation-server/web/frontier/fi18n.mjs';
import { keyFromSeed } from '../../permutation-server/web/session.mjs';
import { setLang } from '../../permutation-server/web/lang.mjs';
import { encode as base58 } from '../client/src/base58.mjs';
import { CLOCK_SYSVAR, FrontierAddresses } from '../client/src/frontier/addresses.mjs';
import { decodeAccount, encodeAccount, decodeIxData } from '../client/src/frontier/codec.mjs';
import { minTipLamports, rent, tipPresets } from '../client/src/frontier/fees.mjs';
import { parseTransaction, wireTransaction } from '../client/src/solana-tx.mjs';
import { verifyTalk } from '../client/src/talk-node.mjs';
import { createFrontierApps } from '../src/frontier/app.mjs';
import { InviteBook } from '../src/frontier/invites.mjs';
import { PayerPool } from '../src/frontier/payers.mjs';

const PROGRAM = Keypair.generate().publicKey.toBase58();
const SEASON_ID = 5n;
const A = new FrontierAddresses({ programId: PROGRAM, seasonId: SEASON_ID });
const GENESIS = 1_800_000_000;
const CLOCK_UNIX = GENESIS + 3_600;
const TIP_MIN = minTipLamports(433, 26_000, 1_048_576);
const PRESETS = tipPresets(TIP_MIN);
const BLOCKHASH = Keypair.generate().publicKey.toBase58();
const HOLDING_RENT = rent(1280), CITIZEN_RENT = rent(384);
const HOLD = { p: -3, q: 7, site: 11 };

const u8 = (n, v) => new Uint8Array(n).fill(v);
const wallet = await keyFromSeed(u8(32, 21));
const session = await keyFromSeed(u8(32, 22));

/**
 * The escrow a shape moves from the relay's fee payer (what the program
 * would): the drain guard's allowance, exactly. FileTicket moves the
 * Holding-rent shortfall of the Citizen's escrow (read from the chain).
 */
function debitOf(tx, chain) {
  const ix = tx.instructions.at(-1);
  if (ix.programId !== PROGRAM) return 0n;
  const d = decodeIxData(ix.data);
  if (d.name === 'Depart') return BigInt(d.tip) + 10_000n + 20_000n;
  if (d.name === 'FileTicket') {
    const c = chain.accounts.get(A.citizen(wallet.publicKey));
    const escrow = c ? BigInt(decodeAccount('Citizen', c.data).TICKET_ESCROW) : 0n;
    return escrow >= HOLDING_RENT ? 0n : HOLDING_RENT - escrow;
  }
  if (d.name === 'Join') return CITIZEN_RENT;
  return 0n;
}

/** A scripted chain (as frontier-relay.test.mjs): signatures verified at simulation, the fee payer's post-balance reported. */
function fakeChain() {
  const c = {
    accounts: new Map(), sent: [], statuses: new Map(), fail: () => null, height: 100,
    set(key, v) { c.accounts.set(String(key), { lamports: v.lamports ?? 1_000_000_000, data: Buffer.from(v.data ?? []) }); },
    getAccountInfo: async key => c.accounts.get(new PublicKey(key).toBase58()) ?? null,
    getMultipleAccountsInfo: async keys => keys.map(k => c.accounts.get(new PublicKey(k).toBase58()) ?? null),
    getBalance: async key => c.accounts.get(new PublicKey(key).toBase58())?.lamports ?? 0,
    getLatestBlockhash: async () => ({ blockhash: BLOCKHASH, lastValidBlockHeight: 150 }),
    getBlockHeight: async () => c.height,
    getSlot: async () => 77,
    simulateTransaction: async (vtx, cfg) => {
      const tx = parseTransaction(vtx.serialize());
      for (const [i, s] of tx.signers.entries()) {
        if (!verifyTalk(tx.message, tx.signatures[i], new PublicKey(s).toBytes())) return { value: { err: 'SignatureFailure', logs: [] } };
      }
      const err = c.fail(tx);
      if (err) return { value: { err, logs: [`Program ${PROGRAM} failed: custom program error`] } };
      const payer = tx.signers[0];
      const post = BigInt(c.accounts.get(payer)?.lamports ?? 0) - 5_000n * BigInt(tx.signers.length) - debitOf(tx, c);
      return { value: { err: null, logs: [], unitsConsumed: 9_000, accounts: cfg.accounts.addresses.map(a => (a === payer ? { lamports: Number(post), data: ['', 'base64'] } : null)) } };
    },
    sendRawTransaction: async raw => { const tx = parseTransaction(new Uint8Array(raw)); c.sent.push(tx); return base58(tx.signatures[0]); },
    getSignatureStatuses: async sigs => ({ value: sigs.map(s => c.statuses.get(s) ?? null) }),
  };
  return c;
}

const clockAccount = unix => { const b = Buffer.alloc(40); b.writeBigUInt64LE(77n, 0); b.writeBigInt64LE(BigInt(unix), 32); return b; };
const seasonAccount = (over = {}) => encodeAccount('Season', { SEASON_ID, STATUS: 2, GENESIS_TS: BigInt(GENESIS), MARCH_FEE: 10_000n, SEAL_BOND: 20_000n,
  MIN_REVEAL_PRIORITY_MILLI: 433, REVEAL_CU_LIMIT: 26_000, REVEAL_LOADED_LIMIT: 1_048_576, JOIN_GATE: new Uint8Array(32), ...over });

const citizenAccount = (over = {}) => encodeAccount('Citizen', { SEASON_ID, WALLET: wallet.publicKeyBytes, SESSION: session.publicKeyBytes,
  SESSION_EXPIRY: BigInt(CLOCK_UNIX + 86_400), FACTION: 0, TICKET_BELL: 0xffffffff, ...over });

const revealed = [];
const keeper = {
  reveal: async b => { revealed.push(b); return { status: 202, body: { accepted: true, track: 't1' } }; },
  nudge: async b => ({ status: 200, body: { queued: true, blocking: [], got: b } }),
};

async function startRelay({ season = {}, gateKey = null } = {}) {
  const chain = fakeChain();
  const pool = new PayerPool({ masterSeed: Buffer.alloc(32, 7), n: 150 });
  for (const k of pool.publicKeys()) chain.set(k, { lamports: 2_000_000_000 });
  chain.set(A.season, { data: seasonAccount(season) });
  chain.set(CLOCK_SYSVAR, { data: clockAccount(CLOCK_UNIX) });
  // The viewer's Citizen: its wallet and session key (a settle's requester is checked against it).
  chain.set(A.citizen(wallet.publicKey), { data: citizenAccount() });
  const store = { state: {}, save() {} };
  const invites = new InviteBook({ secret: Buffer.alloc(32, 3), seasonId: SEASON_ID, store });
  const cfg = { cluster: 'localnet', programId: PROGRAM, seasonId: String(SEASON_ID), heraldPeer: '127.0.0.1', heraldUrl: 'http://127.0.0.1:41040', minPoolSol: 1, operatorToken: 'op' };
  const apps = createFrontierApps({ cfg, connection: chain, pool, keeper, invites, gateKey, store, log: () => {} });
  const server = http.createServer(apps.public);
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  return { chain, pool, invites, apps, server, url: `http://127.0.0.1:${server.address().port}` };
}

const R = await startRelay();
const servers = [R.server];
after(() => { for (const s of servers) s.close(); setLang('ja'); });
io.setRelay(R.url);
io.setPin({ programId: PROGRAM, cluster: 'localnet', seasonId: SEASON_ID });
const PA = io.pinned().addresses;
const v = (extra = {}) => ({ addresses: PA, wallet: wallet.publicKey, actor: session.publicKey, faction: 0, ...extra });

/** Send one shape through the page's path; the chain must have received exactly the instruction the page built. */
async function send(name, { extra = {}, fields = {}, signer = { session }, requester = null, citizen } = {}) {
  const before = R.chain.sent.length;
  const r = await fplay.submit({ name, accounts: fplay.accountsFor(name, v(extra)), fields, signer, requester, citizen });
  assert.equal(r.ok, true, `${name}: ${r.code} ${r.error}`);
  assert.equal(R.chain.sent.length, before + 1, `${name} was sent`);
  const tx = R.chain.sent.at(-1);
  assert.equal(base58(tx.signatures[0]), r.signature);
  assert.ok(verifyTalk(tx.message, tx.signatures[0], new PublicKey(r.feePayer).toBytes()), 'the fee payer signed the message the page built');
  assert.deepEqual(tx.message, r.message, 'the message sent is byte for byte the one the page signed');
  return { r, tx, ix: tx.instructions.at(-1) };
}

// ------------------------------------------------------------------ player shapes
test('every player shape the page builds passes the real relay, with the accounts it recomputed', async () => {
  // Session-signed shapes: the relay's fee payer, then the session key; nobody else.
  const sessionShapes = [
    ['SetVigil', {}, { start_min: 120 }],
    ['FileTicket', { sites: [{ p: 3, q: 0, site: 2 }] }, { sites: [{ p: 3, q: 0, site: 2 }] }],
    ['FileTicket', { sites: [{ p: 3, q: 0, site: 2 }, { p: 3, q: -1, site: 0 }, { p: 3, q: 0, site: 7 }] }, { sites: [{ p: 3, q: 0, site: 2 }, { p: 3, q: -1, site: 0 }, { p: 3, q: 0, site: 7 }] }],
    ['Harvest', { holding: HOLD }, {}],
    ['Build', { holding: HOLD, item: 1 }, { item: 1 }],
    ['Build', { holding: HOLD, item: fplay.ITEM_WALLS }, { item: fplay.ITEM_WALLS }],
    ['Train', { holding: HOLD }, { unit: 6, n: 200 }],
    ['Muster', { holding: HOLD }, { unit: 0, troops: 300, tile: 30 }],
    ['Dissolve', { holding: HOLD, province: { p: -3, q: 7 } }, { host_id: 77n }],
    ['Garrison', { holding: HOLD }, { delta: -100n }],
    ['Explore', { holding: HOLD, province: { p: -2, q: 7 } }, { host_id: 78n, n: 2, tiles: Uint8Array.of(3, 4) }],
  ];
  for (const [name, extra, fields] of sessionShapes) {
    const { tx, ix } = await send(name, { extra, fields });
    // The first FileTicket escrows the Holding's rent in the Citizen; the next one moves nothing (§5.9).
    if (name === 'FileTicket') R.chain.set(A.citizen(wallet.publicKey), { data: citizenAccount({ TICKET_ESCROW: HOLDING_RENT }) });
    assert.deepEqual(tx.signers.slice(1), [session.publicKey], `${name}: signed by the session key only`);
    const keys = ix.keys.map(k => k.pubkey);
    assert.equal(keys[0], session.publicKey);
    assert.equal(keys[2], A.season);
    assert.equal(keys[3], A.citizen(wallet.publicKey), `${name}: the page's Citizen address is the SDK's`);
    if (extra.holding) assert.equal(keys[4], A.holding(HOLD.p, HOLD.q, HOLD.site));
  }
  // The walls name the Province, other items do not; FileTicket names each distinct province once.
  const walls = R.chain.sent.find(t => decodeIxData(t.instructions.at(-1).data).name === 'Build' && decodeIxData(t.instructions.at(-1).data).item === fplay.ITEM_WALLS);
  assert.equal(walls.instructions.at(-1).keys.at(-1).pubkey, A.province(HOLD.p, HOLD.q));
  const ticket3 = R.chain.sent.filter(t => decodeIxData(t.instructions.at(-1).data).name === 'FileTicket').at(-1).instructions.at(-1);
  assert.deepEqual(ticket3.keys.slice(5, 7).map(k => k.pubkey), [A.province(3, 0), A.province(3, -1)]);
  const explore = R.chain.sent.find(t => decodeIxData(t.instructions.at(-1).data).name === 'Explore').instructions.at(-1);
  assert.equal(explore.keys[5].pubkey, A.province(-2, 7), 'Explore names the province where the host is');
});

test('Depart at each of the three tip presets is sponsored; any other tip is refused TipNotPreset', async () => {
  const fields = tip => ({ host_id: 99n, commit: u8(32, 1), seal: Uint8Array.of(0xa0, ...u8(164, 2)), arrive_bell: 20, tip, transit_slot: 0 });
  for (const tip of PRESETS) {
    const { tx, r } = await send('Depart', { extra: { holding: HOLD }, fields: fields(tip) });
    assert.ok(r.bytes <= 800, `Depart is ${r.bytes} B`);
    assert.equal(BigInt(decodeIxData(tx.instructions.at(-1).data).tip), tip);
  }
  const bad = await fplay.submit({ name: 'Depart', accounts: fplay.accountsFor('Depart', v({ holding: HOLD })), fields: fields(TIP_MIN + 1n), signer: { session } });
  assert.deepEqual([bad.ok, bad.code, bad.httpStatus], [false, 'TipNotPreset', 400]);
  setLang('en');
  assert.equal(failureText(bad), errorText('TipNotPreset'));
  setLang('ja');
  const zero = await fplay.submit({ name: 'Depart', accounts: fplay.accountsFor('Depart', v({ holding: HOLD })), fields: fields(0n), signer: { session } });
  assert.equal(zero.code, 'TipNotPreset', 'a zero tip never goes through the relay (the composer does not offer one either)');
});

test('Join and SetSession are wallet-signed; a gated season needs the gate and an invite', async () => {
  const signTransaction = async wire => {
    const tx = parseTransaction(wire);
    const sigs = [...tx.signatures];
    sigs[tx.signers.indexOf(wallet.publicKey)] = await wallet.sign(tx.message);
    return wireTransaction(tx.message, sigs);
  };
  const w = { signTransaction };
  const joinFields = { faction: 0, session: session.publicKeyBytes, session_expiry: BigInt(CLOCK_UNIX + 86_000) };
  let { tx } = await send('Join', { extra: { faction: 0 }, fields: joinFields, signer: { wallet: w } });
  assert.deepEqual(tx.signers.slice(1), [wallet.publicKey]);
  assert.equal(tx.instructions.at(-1).keys[5].pubkey, A.joinShardFor(0, wallet.publicKey));
  ({ tx } = await send('SetSession', { fields: { session: session.publicKeyBytes, expiry: BigInt(CLOCK_UNIX + 86_000) }, signer: { wallet: w } }));
  assert.equal(tx.instructions.at(-1).keys[0].pubkey, wallet.publicKey, 'SetSession: the actor is the wallet');
  // A wallet that signs something else is refused before sending.
  const liar = { signTransaction: async wire => { const t = parseTransaction(wire); return wireTransaction(Uint8Array.from([...t.message.slice(0, -1), 9]), t.signatures); } };
  assert.equal((await fplay.submit({ name: 'Join', accounts: fplay.accountsFor('Join', v()), fields: joinFields, signer: { wallet: liar } })).code, 'WalletAlteredMessage');
  // Gated: the page names the gate (from the Season), the relay co-signs with a valid invite.
  const gate = Keypair.generate();
  const G = await startRelay({ season: { JOIN_GATE: gate.publicKey.toBytes() }, gateKey: gate });
  servers.push(G.server);
  io.setRelay(G.url);
  try {
    const accounts = fplay.accountsFor('Join', v({ joinGate: gate.publicKey.toBase58() }));
    const noInvite = await fplay.submit({ name: 'Join', accounts, fields: joinFields, signer: { wallet: w } });
    assert.equal(noInvite.code, 'InviteRequired');
    const [invite] = G.invites.issue(1);
    const ok = await fplay.submit({ name: 'Join', accounts, fields: joinFields, signer: { wallet: w }, invite });
    assert.equal(ok.ok, true, `${ok.code} ${ok.error}`);
    const sent = G.chain.sent.at(-1);
    const gi = sent.signers.indexOf(gate.publicKey.toBase58());
    assert.ok(gi > 0 && verifyTalk(sent.message, sent.signatures[gi], gate.publicKey.toBytes()), 'the relay co-signed with the gate');
  } finally {
    io.setRelay(R.url);
  }
});

// ------------------------------------------------------------------ settle shapes
test('settle shapes carry no authority signature and are charged to the requester\'s citizen', async () => {
  const citizen = A.citizen(wallet.publicKey);
  const left = async () => (await io.quota(citizen)).left;
  const before = await left();
  const settle = { seed: A.seedCache(40, 3, 0), anchor: A.bellAnchor(40, 3) };
  const { tx } = await send('SettleExplore', { extra: { holding: HOLD, settle }, signer: null, requester: session, citizen });
  assert.deepEqual(tx.signers.length, 1, 'the fee payer alone signs a settle shape');
  assert.equal(await left(), before - 1, 'charged to the viewer\'s citizen, verified on chain');
  // SettleTransit: its beneficiary is the relay's fee payer (a function of it), the logged commit and seal.
  const r = await fplay.submit({ name: 'SettleTransit', signer: null, requester: session, citizen,
    accounts: fp => fplay.accountsFor('SettleTransit', v({ holding: HOLD, settle: { dest: { p: -2, q: 7 }, arriveBell: 44, faction: 0, slotIndex: 1, anchor: A.bellAnchor(44, 5),
      slotBeneficiary: fp, resolver: fp, rentPayer: fp, beneficiary: fp } })),
    fields: fp => ({ transit_slot: 1, commit: u8(32, 4), seal: u8(165, 5), beneficiary: new PublicKey(fp).toBytes() }) });
  assert.equal(r.ok, true, `${r.code} ${r.error}`);
  const ix = R.chain.sent.at(-1).instructions.at(-1);
  assert.equal(ix.keys[5].pubkey, A.arrivalSlot(-2, 7, 44, 0, 1));
  assert.equal(ix.keys[4].pubkey, A.clashInputs(-2, 7, 44));
  assert.equal(ix.keys[11].pubkey, r.feePayer, 'settle_beneficiary = the data\'s beneficiary = the fee payer');
  assert.equal(await left(), before - 2);
  // v1.7 (I-56, integ-W4 hand-over to W5-E): the camp's winner names its owner's Citizen as the 14th account; the relay takes it.
  const c = await fplay.submit({ name: 'SettleTransit', signer: null, requester: session, citizen,
    accounts: fp => fplay.accountsFor('SettleTransit', v({ holding: HOLD, settle: { dest: { p: -2, q: 7 }, arriveBell: 44, faction: 0, slotIndex: 1, anchor: A.bellAnchor(44, 5),
      slotBeneficiary: fp, resolver: fp, rentPayer: fp, beneficiary: fp, campCitizen: citizen } })),
    fields: fp => ({ transit_slot: 1, commit: u8(32, 4), seal: u8(165, 5), beneficiary: new PublicKey(fp).toBytes() }) });
  assert.equal(c.ok, true, `${c.code} ${c.error}`);
  const cix = R.chain.sent.at(-1).instructions.at(-1);
  assert.equal(cix.keys.length, 14);
  assert.deepEqual([cix.keys[13].pubkey, cix.keys[13].isWritable, cix.keys[13].isSigner], [citizen, true, false]);
});

test('the camp winner (fclient camp_winner) and camp_mask read from the raw ClashInputs bytes', async () => {
  assert.equal(fplay.campWinner(0b1000, 0, 3, 1), true, 'faction 0, slot 3 is bit 3');
  assert.equal(fplay.campWinner(0b1000 | (1 << 9), 2, 1, 1), false, 'only the lowest set bit wins');
  assert.equal(fplay.campWinner(1 << 9, 2, 1, 1), true, 'faction 2, slot 1 is bit 9');
  assert.equal(fplay.campWinner(1 << 9, 2, 1, 2), false, 'only a host that Stays');
  assert.equal(fplay.campWinner(0, 0, 0, 1), false, 'no camp');
  assert.equal(fplay.campWinner(1 << 23, 5, 3, 1), true, 'the last position');
  const raw = new Uint8Array(1280);
  new DataView(raw.buffer).setUint32(fplay.CAMP_MASK_OFFSET, (1 << 9) | (1 << 12), true);
  assert.equal(fplay.campMaskOf(raw), (1 << 9) | (1 << 12));
  assert.equal(fplay.campMaskOf(new Uint8Array(10)), 0);
  const { campCitizenOf } = await import('../../permutation-server/web/frontier/controller.mjs');
  const owner = new Uint8Array(32).fill(9);
  const arrivals = Array.from({ length: 24 }, () => ({ present: 0, hostId: 0n, fate: 0 }));
  arrivals[2 * 4 + 1] = { present: 1, hostId: 77n, fate: 1 };
  const m = { env: { inputs: { arrivals } }, dest: { p: 3, q: -1 }, transit: { arriveBell: 50 } };
  const heraldClient = { clash: async (p, q, b) => ({ ok: p === 3 && q === -1 && b === 50, report: { inputs_b64: Buffer.from(raw).toString('base64') } }) };
  const got = await campCitizenOf(m, 77n, { heraldClient, holding: { ownerCitizen: owner }, faction: 2 });
  assert.equal(got, new PublicKey(owner).toBase58(), 'the Holding\'s owner Citizen');
  assert.equal(await campCitizenOf(m, 78n, { heraldClient, holding: { ownerCitizen: owner }, faction: 2 }), null, 'another host');
  // ABI v1.8: the decoded envelope carries campMask; no clash report is fetched then.
  const noFetch = { clash: async () => { throw new Error('fetched /h/clash although the envelope has campMask'); } };
  const md = { ...m, env: { inputs: { arrivals, campMask: 1 << (2 * 4 + 1) } } };
  assert.equal(await campCitizenOf(md, 77n, { heraldClient: noFetch, holding: { ownerCitizen: owner }, faction: 2 }), new PublicKey(owner).toBase58(), 'decoded campMask');
  assert.equal(await campCitizenOf({ ...md, env: { inputs: { arrivals, campMask: 1 } } }, 77n, { heraldClient: noFetch, holding: { ownerCitizen: owner }, faction: 2 }), null, 'a lower position won the camp');
  arrivals[2 * 4 + 1].fate = 3;
  assert.equal(await campCitizenOf(m, 77n, { heraldClient, holding: { ownerCitizen: owner }, faction: 2 }), null, 'not Stays');
});

// ------------------------------------------------------------------ reveals, tampering, errors
test('a Reveal is material posted to /f/reveal, never a transaction; the keeper gets exactly the five fields', async () => {
  const material = { holding: A.holding(HOLD.p, HOLD.q, HOLD.site), transit_slot: 2, plain_b64: Buffer.from(u8(37, 1)).toString('base64'),
    salt_b64: Buffer.from(u8(32, 2)).toString('base64'), ct_hash_b64: Buffer.from(u8(32, 3)).toString('base64') };
  const r = await io.reveal(material);
  assert.deepEqual([r.ok, r.httpStatus, r.accepted], [true, 202, true]);
  assert.deepEqual(Object.keys(revealed.at(-1)).sort(), ['ct_hash_b64', 'holding', 'plain_b64', 'salt_b64', 'transit_slot']);
  assert.throws(() => fplay.accountsFor('Reveal', v()), /not a shape/);
  const n = await io.nudge(-3, 7, 41);
  assert.equal(n.ok, true);
});

test('a relay that answers with anything but the fee payer\'s signature of the signed message is caught', async () => {
  const liar = http.createServer(async (req, res) => {
    const chunks = [];
    for await (const c of req) chunks.push(c);
    if (req.method === 'POST') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      return res.end(JSON.stringify({ ok: true, signature: base58(u8(64, 9)) }));
    }
    const upstream = await fetch(`${R.url}${req.url}`);
    res.writeHead(upstream.status, { 'Content-Type': 'application/json' });
    res.end(await upstream.text());
  });
  await new Promise(r => liar.listen(0, '127.0.0.1', r));
  servers.push(liar);
  io.setRelay(`http://127.0.0.1:${liar.address().port}`);
  try {
    const r = await fplay.submit({ name: 'Harvest', accounts: fplay.accountsFor('Harvest', v({ holding: HOLD })), signer: { session } });
    assert.equal(r.code, 'RelayMessageChanged');
    setLang('en');
    assert.match(failureText(r), /relay changed the transaction/i);
    setLang('ja');
  } finally {
    io.setRelay(R.url);
  }
  // A relay naming another program is refused before anything is built.
  io.setPin({ programId: Keypair.generate().publicKey.toBase58(), cluster: 'localnet', seasonId: SEASON_ID });
  const other = await fplay.submit({ name: 'Harvest', accounts: {}, signer: { session } });
  assert.equal(other.code, 'PinMismatch');
  io.setPin({ programId: PROGRAM, cluster: 'localnet', seasonId: SEASON_ID });
});

test('program refusals come back by name and number; GET /f/tx reports landed, failed and expired', async () => {
  R.chain.fail = tx => (decodeIxData(tx.instructions.at(-1).data).name === 'Dissolve' ? { InstructionError: [3, { Custom: 58 }] } : null);
  try {
    const r = await fplay.submit({ name: 'Dissolve', accounts: fplay.accountsFor('Dissolve', v({ holding: HOLD })), fields: { host_id: 5n }, signer: { session } });
    assert.deepEqual([r.ok, r.code, r.programCode, r.httpStatus], [false, 'HostInTransit', 58, 409]);
    setLang('en');
    assert.equal(failureText(r), errorText(58));
    assert.match(failureText(r), /settled/);
    setLang('ja');
  } finally {
    R.chain.fail = () => null;
  }
  const { r } = await send('Harvest', { extra: { holding: HOLD } });
  const quick = { tries: 3, every: 1, wait: async () => {} };
  assert.deepEqual(await fplay.track(r.signature, quick), { ok: false, state: 'unknown', code: 'Unconfirmed' });
  R.chain.statuses.set(r.signature, { slot: 88, confirmationStatus: 'confirmed', err: null });
  assert.deepEqual(await fplay.track(r.signature, quick), { ok: true, state: 'landed', slot: 88 });
  R.chain.statuses.set(r.signature, { slot: 89, confirmationStatus: 'confirmed', err: { InstructionError: [3, { Custom: 51 }] } });
  const failed = await fplay.track(r.signature, quick);
  assert.deepEqual([failed.state, failed.code, failed.programCode], ['failed', 'TipTooLow', 51]);
  R.chain.statuses.delete(r.signature);
  R.chain.height = 1_000;
  assert.equal((await fplay.track(r.signature, quick)).code, 'Expired');
  R.chain.height = 100;
});
