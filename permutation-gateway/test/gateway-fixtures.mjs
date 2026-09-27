// Shared fixtures for the gateway's HTTP tests: account data encoded like the
// program's (the inverse of codec.mjs decodeSeason / decodeMember, checked
// against the Rust vectors), a scripted connection that records what is
// simulated and sent, an app over them, and a request helper. No network.
import assert from 'node:assert/strict';
import { Readable } from 'node:stream';
import { Keypair, PublicKey, Transaction } from '@solana/web3.js';
import { Writer } from '../client/src/borsh.mjs';
import { ChainClient } from '../client/src/chain.mjs';
import { decodeMember, decodeSeason, MAGIC, SEASON_STATUS } from '../client/src/codec.mjs';
import { toBase64 } from '../client/src/bytes.mjs';
import { createApp, createApps } from '../src/app.mjs';
import { DEFAULTS } from '../src/config.mjs';
import { SealedStore } from '../src/sealed.mjs';
import { fromHex, vectors } from './vectors.mjs';

export const programId = DEFAULTS.programId;
const u8 = s => new TextEncoder().encode(s);
const bytes = k => (k instanceof Uint8Array ? k : new PublicKey(k).toBytes());

/**
 * A season account's data: the sample season (vectors.json) with `over`
 * fields replaced, in the v8 layout (`MAGIC.season`; `legacy: true` writes
 * `MAGIC.legacySeason`).
 */
export function seasonData(over = {}) {
  const sample = fromHex(vectors.accounts.season.hex);
  const s = { ...decodeSeason(sample), ...over };
  const status = name => SEASON_STATUS.indexOf(name);
  const i64 = (w, v) => w.u64(BigInt.asUintN(64, BigInt(v)));
  const w = new Writer().fixed(u8(s.legacy ? MAGIC.legacySeason : MAGIC.season), 8).u64(s.seasonId).u8(s.bump).u8(s.vaultBump).fixed(bytes(s.admin), 32).fixed(bytes(s.crank), 32)
    .fixed(bytes(s.usdcMint), 32).u8(s.usdcDecimals).u8(s.preset).u8(s.nations).u64(s.entryFee).u32(s.tickSeconds).bool(s.market)
    .u8(status(s.status)).fixed(s.worldSeed, 32).fixed(s.seasonSeed, 32).u32(s.memberCount).vec(s.nationMembers, (x, v) => x.u32(v))
    .u32(s.seated).u64(s.pool).u64(s.ops).bool(s.opsWithdrawn).vec(s.treasury, (x, v) => x.u64(v)).vec(s.treasuryFinal, (x, v) => x.u64(v))
    .vec(s.payouts, (x, v) => x.u64(v)).fixed(s.finalRoot, 32).u64(s.prevSeasonId).fixed(s.prevHistoryRoot, 32).fixed(s.historyRoot, 32)
    .u16(s.aiCount).fixed(s.rosterCommit, 32).u64(s.bountyEach).u64(s.bond).fixed(s.rosterAcc, 32).u16(s.rosterRevealed)
    .u8(['none', 'revealed', 'forfeited'].indexOf(s.rosterOutcome)).vec(s.bountyPaid, (x, v) => x.u64(v))
    // The v8 tail, in the contract's block order.
    .u32(s.delegated).fixed(s.rosterBlind, 32).vec(s.refundBase, (x, v) => x.u64(v)).bytes(s.refundInPayout)
    .u8(s.seedState).fixed(s.seedOracle, 32);
  i64(w, s.seedRequestedAt).u8(s.seedRequests).u64(s.deposit).u64(s.outstanding).bool(s.voided);
  i64(i64(w, s.startBy), s.stageAt).u32(s.rolledBack).u8(status(s.abortedFrom)).fixed(bytes(s.validator), 32)
    .u16(s.rulesVersion).fixed(s.rulesHash, 32).u16(s.logicVersion).u64(s.createdSlot);
  const b = w.toBytes();
  const out = new Uint8Array(Math.max(b.length, sample.length));
  out.set(b);
  return Buffer.from(out);
}

/** A member account's data: the sample member with `over` fields replaced. */
export function memberData(over = {}) {
  const sample = fromHex(vectors.accounts.member.hex);
  const m = { ...decodeMember(sample), ...over };
  const w = new Writer().fixed(u8(MAGIC.member), 8).u64(m.seasonId).u8(m.bump).u32(m.index).u16(m.civ).fixed(bytes(m.wallet), 32)
    .fixed(bytes(m.session), 32).u8(m.kind).string(m.name).fixed(m.attestation, 32).u8(m.stand);
  for (const v of m.votes) w.u32(v);
  const b = w.u64(m.shares).bool(m.claimed).fixed(m.tag, 32).toBytes();
  const out = new Uint8Array(Math.max(b.length, sample.length));
  out.set(b);
  return Buffer.from(out);
}

// The encoders reproduce the samples byte for byte.
assert.deepEqual(new Uint8Array(seasonData()), fromHex(vectors.accounts.season.hex));
{
  // A legacy account is the v8 layout with a zero tail (padded to the sample's length here).
  const legacy = fromHex(vectors.accounts.legacySeason.hex);
  const data = new Uint8Array(seasonData(decodeSeason(legacy)));
  assert.deepEqual(data.slice(0, legacy.length), legacy);
  assert.ok(data.slice(legacy.length).every(b => b === 0));
}
assert.deepEqual(new Uint8Array(memberData()), fromHex(vectors.accounts.member.hex));

/**
 * A member as the gateway's registry lists it (`registryMembers`), for a
 * key the test generated: the relay accepts `SubmitGov` only from members
 * signing for themselves and their own nation.
 */
export const member = (key, { index = 3, civ = 1 } = {}) => ({ index, civ, session: key.publicKey.toBase58() });

export const blockhash = Keypair.generate().publicKey.toBase58();

/**
 * A scripted connection: `accounts` (base58 → {data, owner?}) answers
 * getAccountInfo; `members` (Member account data) answers getProgramAccounts;
 * `simulate(bytes)` → {err, logs} answers simulateTransaction (recorded in
 * `simulated`, with the config); sends are recorded in `sent`.
 */
export function fakeConnection({ accounts = new Map(), members = [], simulate = () => ({ err: null, logs: [] }), balance = 50e9, tokenAccounts = [], ...over } = {}) {
  const c = {
    accounts, members, sent: [], simulated: [], calls: {},
    count(name) { c.calls[name] = (c.calls[name] ?? 0) + 1; },
    getLatestBlockhash: async () => { c.count('getLatestBlockhash'); return { blockhash, lastValidBlockHeight: 1234 }; },
    getAccountInfo: async key => { c.count('getAccountInfo'); return accounts.get(new PublicKey(key).toBase58()) ?? null; },
    getMultipleAccountsInfo: async keys => keys.map(k => accounts.get(new PublicKey(k).toBase58()) ?? null),
    getProgramAccounts: async () => { c.count('getProgramAccounts'); return c.members.map((data, i) => ({ pubkey: Keypair.generate().publicKey, account: { data, i } })); },
    simulateTransaction: async (vtx, config) => {
      const wire = vtx.serialize();
      c.simulated.push({ wire: new Uint8Array(wire), config });
      return { context: { slot: 1 }, value: { unitsConsumed: 1, ...simulate(wire) } };
    },
    sendRawTransaction: async raw => { c.sent.push(new Uint8Array(raw)); return `sig${c.sent.length}`.padEnd(64, '1'); },
    getSignatureStatuses: async () => ({ value: [{ confirmationStatus: 'confirmed', err: null }] }),
    getTransaction: async () => ({ meta: { logMessages: [], computeUnitsConsumed: 1 } }),
    getBlockHeight: async () => 1,
    getBalance: async () => { c.count('getBalance'); return balance; },
    getTokenAccountsByOwner: async (owner, filter) => { c.count('getTokenAccountsByOwner'); return { value: tokenAccounts.filter(a => a.owner === new PublicKey(owner).toBase58()).map(a => ({ pubkey: new PublicKey(a.address), account: { data: a.data } })) }; },
    ...over,
  };
  return c;
}

/** Token account data (SPL layout) holding `amount` of `mint` for `owner`. */
export function tokenAccountData({ mint, owner, amount }) {
  const b = Buffer.alloc(165);
  Buffer.from(new PublicKey(mint).toBytes()).copy(b, 0);
  Buffer.from(new PublicKey(owner).toBytes()).copy(b, 32);
  b.writeBigUInt64LE(BigInt(amount), 64);
  return b;
}

/** Deterministic named keys (no files). */
export function keyring() {
  const ring = new Map();
  return name => { if (!ring.has(name)) ring.set(name, Keypair.generate()); return ring.get(name); };
}

/**
 * A gateway over fake connections, season `seasonId` of the default
 * program, the crank `crankKey`. `season` overrides the season account
 * fields (default: Registering, no AI members). Returns the handlers
 * (`operator`, `public`), the context, both connections and the store.
 */
export function gateway({ seasonId = 42n, season = {}, state = {}, crank = {}, cfg = {}, base: baseOver = {}, er: erOver = {}, registryMembers = [], keys = keyring(), now = Date.now, funds } = {}) {
  const chain = new ChainClient(programId, seasonId);
  const crankKey = keys('crank');
  const mint = keys('usdc-mint').publicKey;
  const seasonAccount = { data: seasonData({ seasonId, status: 'Registering', aiCount: 0, memberCount: 0, crank: crankKey.publicKey, usdcMint: mint, ...season }) };
  const base = fakeConnection(baseOver);
  base.accounts.set(chain.season.toBase58(), seasonAccount);
  const er = fakeConnection(erOver);
  const logs = [];
  const store = { state: { seasonId: seasonId.toString(), programId, members: [], mint: mint.toBase58(), cluster: 'localnet', lineage: [], ...state }, saves: 0, save() { this.saves++; } };
  const theCrank = { crank: crankKey, phase: 'registering', snapshot: null, refresh: async () => null, fresh: async () => null, tickRecords: from => [{ tick: from }], sealed: new SealedStore(), ...crank };
  const config = { programId, cluster: 'localnet', port: 4191, publicPort: 4194, publicHost: '127.0.0.1', baseRpc: 'b', erRpc: 'e', operatorToken: 'op-token', minCrankSol: 0.3, ...cfg };
  const apps = createApps({ cfg: config, base, er, store, crank: theCrank, keys, now, funds, log: m => logs.push(m), registry: { list: async () => registryMembers, invalidate() {} } });
  return { ...apps, chain, base, er, store, crank: theCrank, crankKey, mint, keys, logs, cfg: config, setSeason: over => base.accounts.set(chain.season.toBase58(), { data: seasonData({ seasonId, status: 'Registering', aiCount: 0, memberCount: 0, crank: crankKey.publicKey, usdcMint: mint, ...season, ...over }) }) };
}

export { createApp };

/** One request through a handler; `ip` is the socket's peer address. */
export async function call(handle, method, url, { body, headers = {}, ip = '127.0.0.1' } = {}) {
  const req = Readable.from(body === undefined ? [] : [Buffer.from(typeof body === 'string' ? body : JSON.stringify(body))]);
  Object.assign(req, { method, url, headers, socket: { remoteAddress: ip } });
  const res = { headers: {} };
  await new Promise(resolve => {
    res.writeHead = (status, h) => { res.status = status; Object.assign(res.headers, h); };
    res.end = data => { res.body = data; resolve(); };
    handle(req, res);
  });
  const text = res.body === undefined ? '' : Buffer.from(res.body).toString();
  return { status: res.status, headers: res.headers, json: text && res.headers['Content-Type'] === 'application/json' ? JSON.parse(text) : null };
}

/** A web3 transaction's wire bytes (unsigned slots zero), base64. */
export const b64 = tx => tx.serialize({ requireAllSignatures: false, verifySignatures: false }).toString('base64');

/** A legacy transaction of `ixs` paid by `feePayer`, signed by `signers` (Keypairs). */
export function tx(ixs, feePayer, signers = []) {
  const t = new Transaction().add(...ixs);
  t.feePayer = feePayer;
  t.recentBlockhash = blockhash;
  if (signers.length) t.partialSign(...signers);
  return t;
}

/** An X-PAYMENT header carrying `t` (base64 JSON). */
export const payment = (t, network = 'solana-localnet') => ({ 'x-payment': toBase64(new TextEncoder().encode(JSON.stringify({ x402Version: 1, scheme: 'exact', network, payload: { transaction: b64(t) } }))) });
