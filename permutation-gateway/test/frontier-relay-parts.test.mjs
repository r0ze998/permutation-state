// The Frontier relay's parts on their own: the payer pool (derivation, no
// round-robin, χ² uniformity, the floor), quotas (daily allowance,
// carry-over, lamport cap), invites (HMAC, one-time, reservation), the
// configuration's port rule, the keeper-link body checks, refusal mapping,
// and the server over a real socket on 127.0.0.1:0.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { Keypair, PublicKey } from '@solana/web3.js';
import { CLOCK_SYSVAR, FrontierAddresses } from '../client/src/frontier/addresses.mjs';
import { encodeAccount } from '../client/src/frontier/codec.mjs';
import { rent } from '../client/src/frontier/fees.mjs';
import { decodeClock, frontierRefusal } from '../src/frontier/chain.mjs';
import { checkFrontierConfig, loadFrontierConfig, portProblem, RESERVED_PORTS } from '../src/frontier/config.mjs';
import { InviteBook } from '../src/frontier/invites.mjs';
import { nudgeBody, revealBody } from '../src/frontier/keeperlink.mjs';
import { derivePayer, PayerPool, payerSeed } from '../src/frontier/payers.mjs';
import { dailyTxs, gameDay, QuotaBook } from '../src/frontier/quota.mjs';
import { startFrontierRelay } from '../src/frontier/server.mjs';
import { CITIZEN_RENT, HOLDING_RENT, lamportsPerDay } from '../src/frontier/shapes.mjs';

const MASTER = Buffer.alloc(32, 9);

test('payers: payer_i = ed25519(sha256("PS-FRONTIER-PAYER-v1" ‖ master ‖ "relay" ‖ le32(i))), 150 distinct keys', () => {
  const seed = createHash('sha256').update(Buffer.concat([Buffer.from('PS-FRONTIER-PAYER-v1'), MASTER, Buffer.from('relay'), Buffer.from([7, 0, 0, 0])])).digest();
  assert.deepEqual(payerSeed(MASTER, 'relay', 7), seed);
  assert.equal(derivePayer(MASTER, 'relay', 7).publicKey.toBase58(), Keypair.fromSeed(seed).publicKey.toBase58());
  assert.notEqual(derivePayer(MASTER, 'reveal', 7).publicKey.toBase58(), derivePayer(MASTER, 'relay', 7).publicKey.toBase58(), 'pools are disjoint');
  // Rust's side (solana-keypair =3.1.2 `Keypair::new_from_array`, as fclient `payers::derive`), master = [9; 32];
  // computed once with a scratch binary (W2-D notes) until fclient's vectors carry payers.
  for (const [pool, i, key] of [['relay', 0, 'F4Nd24wbyjRKs71cJMN7widrNRmHndDLdqneDoi4oHwB'], ['relay', 7, 'DJowmcm1WB9w7tkDfNn2BSNzCvVjrZ2ChUS1HJpBmG1A'],
    ['relay', 149, 'DtjbAwFvgkgSaKfzsYcTWQiF1u9pw1GLc7akF1a1jaJr'], ['reveal', 7, '2NQmiSXJJi1fugkoKgrwGSoYm4nu41frKEFENKFtA1c7']]) {
    assert.equal(derivePayer(MASTER, pool, i).publicKey.toBase58(), key, `${pool} ${i}`);
  }
  const pool = new PayerPool({ masterSeed: MASTER });
  assert.equal(pool.size, 150);
  assert.equal(new Set(pool.publicKeys()).size, 150);
  assert.equal(new PayerPool({ masterSeed: MASTER }).publicKeys()[149], pool.publicKeys()[149], 'derived again after a restart');
  assert.throws(() => new PayerPool({ masterSeed: MASTER, n: 149 }), /at least 150/);
  assert.equal(new PayerPool({ masterSeed: MASTER, n: 3, dev: true }).size, 3);
});

test('payers: the draw is uniform over the eligible payers (χ²) and never picks one below the floor', () => {
  const pool = new PayerPool({ masterSeed: MASTER, minLamports: 1_000 });
  pool.balances = pool.balances.map((_, i) => (i === 17 ? 999 : 5_000));
  const n = 149 * 200;
  const counts = new Map();
  for (let i = 0; i < n; i++) {
    const k = pool.draw().publicKey.toBase58();
    counts.set(k, (counts.get(k) ?? 0) + 1);
  }
  assert.equal(counts.get(pool.publicKeys()[17]), undefined, 'the payer under the floor is never drawn');
  assert.equal(counts.size, 149);
  const expected = n / 149;
  const chi2 = [...counts.values()].reduce((s, c) => s + (c - expected) ** 2 / expected, 0);
  // df = 148: mean 148, sd ≈ 17.2; 240 is beyond p = 10⁻⁶, so a fair draw essentially never fails.
  assert.ok(chi2 < 240, `χ² = ${chi2.toFixed(1)}`);
  // Not a round-robin: consecutive draws repeat sometimes.
  const seq = Array.from({ length: 2_000 }, () => pool.draw().publicKey.toBase58());
  assert.ok(seq.some((k, i) => i > 0 && k === seq[i - 1]));
  // With every payer under the floor the pool still answers (the funds breaker is what stops sponsoring).
  pool.balances = pool.balances.map(() => 0);
  assert.ok(pool.has(pool.draw().publicKey.toBase58()));
});

test('quota: daily allowance, carry-over capped at the burst, lamport cap, game days from the Clock', () => {
  assert.equal(dailyTxs(0), 40);
  assert.equal(dailyTxs(6), 40);
  assert.equal(dailyTxs(7), 20);
  assert.equal(gameDay(1000, 2000), 0);
  assert.equal(gameDay(2000 + 86_399, 2000), 0);
  assert.equal(gameDay(2000 + 86_400, 2000), 1);
  const q = new QuotaBook();
  const frame = { day: 0, lamportsCap: 1_000n, genesisTs: 0 };
  q.check('k', { ...frame, lamports: 1_000 });
  assert.throws(() => q.check('k', { ...frame, lamports: 1_001 }), e => e.code === 'QuotaExceeded' && e.extra.retryAt === 86_400);
  q.charge('k', { day: 0, lamports: 600 });
  assert.throws(() => q.check('k', { ...frame, lamports: 401 }), e => e.code === 'QuotaExceeded');
  assert.deepEqual(q.status('k', frame), { left: 39, lamportsLeft: '400', resetsAt: 86_400 });
  // The next day the lamports reset and the unused transactions carry over (39 + 40 → capped at 60).
  assert.deepEqual(q.status('k', { ...frame, day: 1 }), { left: 60, lamportsLeft: '1000', resetsAt: 172_800 });
  for (let i = 0; i < 39; i++) q.charge('k', { day: 0 });
  assert.throws(() => q.check('k', frame), e => e.code === 'QuotaExceeded');
  assert.equal(q.status('k', { ...frame, day: 1 }).left, 40);
  assert.equal(q.status('k', { ...frame, day: 9 }).left, 60);
  // The lamport allowance per day: 24 Depart escrows at 2 × tip_min + the two rents.
  const season = { MIN_REVEAL_PRIORITY_MILLI: 433, REVEAL_CU_LIMIT: 26_000, REVEAL_LOADED_LIMIT: 1_048_576, MARCH_FEE: 10_000n, SEAL_BOND: 20_000n };
  assert.equal(lamportsPerDay(season), 24n * (2n * 14_441n + 30_000n) + rent(384) + rent(1280));
  assert.equal(CITIZEN_RENT, 2_600_960n);
  assert.equal(HOLDING_RENT, 7_152_640n);
});

test('invites: HMAC tokens of this season, one use each, one Join in flight at a time', () => {
  const store = { state: {}, save() {} };
  const book = new InviteBook({ secret: Buffer.alloc(32, 1), seasonId: 5n, store });
  const [a, b] = book.issue(2);
  assert.notEqual(a, b);
  assert.ok(book.verify(a));
  assert.equal(book.verify(`${a.slice(0, -2)}AA`), null, 'a changed MAC');
  assert.equal(book.verify('!!'), null);
  assert.equal(book.verify(null), null);
  assert.equal(new InviteBook({ secret: Buffer.alloc(32, 1), seasonId: 6n }).verify(a), null, 'another season\'s');
  assert.equal(new InviteBook({ secret: Buffer.alloc(32, 2), seasonId: 5n }).verify(a), null, 'another secret\'s');
  const n = book.reserve(a);
  assert.ok(n);
  assert.equal(book.reserve(a), null, 'held by a Join in flight');
  book.release(n);
  const n2 = book.reserve(a);
  book.consume(n2);
  assert.equal(book.reserve(a), null, 'used');
  assert.ok(Object.hasOwn(store.state.invitesUsed, n2));
  assert.ok(book.reserve(b));
  assert.throws(() => book.issue(0), RangeError);
});

test('config: M1 ports only (41000–41999, never a reserved one), public listener on loopback, pool ≥ 150', () => {
  for (const p of RESERVED_PORTS) assert.match(portProblem(p), /reserved/);
  assert.match(portProblem(40999), /outside/);
  assert.match(portProblem(42000), /outside/);
  assert.equal(portProblem(41033), null);
  assert.equal(portProblem(0), null);
  const cfg = loadFrontierConfig(['--program', Keypair.generate().publicKey.toBase58(), '--season', '5']);
  assert.equal(cfg.port, 41030);
  assert.equal(cfg.publicPort, 41033);
  assert.equal(cfg.poolSize, 150);
  const base = { ...cfg };
  assert.ok(checkFrontierConfig({ ...base, publicHost: '0.0.0.0' }).some(e => /loopback only/.test(e)));
  assert.ok(checkFrontierConfig({ ...base, port: 4191 }).some(e => /reserved/.test(e)));
  assert.ok(checkFrontierConfig({ ...base, poolSize: 10 }).some(e => /150/.test(e)));
  assert.deepEqual(checkFrontierConfig({ ...base, poolSize: 10, dev: true }), []);
  assert.ok(checkFrontierConfig({ ...base, heraldPeer: '10.0.0.2' }).some(e => /loopback/.test(e)));
  assert.throws(() => loadFrontierConfig(['--season', '5']), /--program/);
  assert.throws(() => loadFrontierConfig(['--program', 'x', '--season', '5', '--port', '4190']), /reserved/);
});

test('keeper link bodies and refusals are checked before anything is forwarded or charged', () => {
  const ok = { holding: Keypair.generate().publicKey.toBase58(), transit_slot: 0, plain_b64: Buffer.alloc(37).toString('base64'), salt_b64: Buffer.alloc(32).toString('base64'),
    ct_hash_b64: Buffer.alloc(32).toString('base64'), extra: 'dropped' };
  const { extra, ...want } = ok;
  assert.deepEqual(revealBody(ok), want);
  for (const bad of [{ ...ok, transit_slot: -1 }, { ...ok, transit_slot: 1.5 }, { ...ok, plain_b64: Buffer.alloc(38).toString('base64') }, { ...ok, ct_hash_b64: 7 }, { ...ok, holding: '1' }, null]) {
    assert.throws(() => revealBody(bad), e => e.status === 400);
  }
  assert.deepEqual(nudgeBody({ province: [-3, 7], bell: 0 }), { province: [-3, 7], bell: 0 });
  assert.throws(() => nudgeBody({ province: [40_000, 0], bell: 1 }), e => e.status === 400);
  assert.throws(() => nudgeBody({ province: [0, 0], bell: -1 }), e => e.status === 400);
  const r = frontierRefusal({ err: { InstructionError: [3, { Custom: 17 }] }, logs: ['x'] });
  assert.deepEqual([r.status, r.code], [429, 'Bucket']);
  assert.deepEqual([frontierRefusal({ err: { InstructionError: [3, { Custom: 51 }] } }).code, frontierRefusal({ err: 'BlockhashNotFound' }).code], ['TipTooLow', 'BlockhashExpired']);
  assert.equal(frontierRefusal({ err: { InstructionError: [0, 'InvalidArgument'] } }).code, 'SimulationFailed');
  // Attributed by the first failing program (integ-W2 review of W2-D): a
  // System-program failure inside a CPI is the relay payer's, not BadData.
  const P = 'Frontier1111111111111111111111111111111111';
  const sys = frontierRefusal({ err: { InstructionError: [3, { Custom: 1 }] }, logs: [
    `Program ${P} invoke [1]`, 'Program 11111111111111111111111111111111 invoke [2]', 'Transfer: insufficient lamports 5, need 7',
    'Program 11111111111111111111111111111111 failed: custom program error: 0x1', `Program ${P} failed: custom program error: 0x1`] }, P);
  assert.deepEqual([sys.status, sys.code], [503, 'OperatorLowFunds']);
  const other = frontierRefusal({ err: { InstructionError: [3, { Custom: 1 }] }, logs: ['Program Other111111111111111111111111111111111 failed: custom program error: 0x1'] }, P);
  assert.equal(other.code, 'ProgramError');
  const ours = frontierRefusal({ err: { InstructionError: [3, { Custom: 1 }] }, logs: [`Program ${P} failed: custom program error: 0x1`] }, P);
  assert.equal(ours.code, 'BadData');
  const clock = Buffer.alloc(40);
  clock.writeBigUInt64LE(12n, 0);
  clock.writeBigInt64LE(1_800_000_123n, 32);
  assert.deepEqual(decodeClock(clock), { slot: 12n, unixTimestamp: 1_800_000_123 });
});

test('the server listens on 127.0.0.1 only and answers over a real socket (ports 0 in tests)', async () => {
  const dir = mkdtempSync(path.join(tmpdir(), 'frontier-relay-'));
  const program = Keypair.generate().publicKey.toBase58();
  const a = new FrontierAddresses({ programId: program, seasonId: 5n });
  const accounts = new Map([[a.season, { lamports: 1, data: Buffer.from(encodeAccount('Season', { SEASON_ID: 5n, GENESIS_TS: 100n, MIN_REVEAL_PRIORITY_MILLI: 433, REVEAL_CU_LIMIT: 26_000,
    REVEAL_LOADED_LIMIT: 1_048_576, MARCH_FEE: 10_000n, SEAL_BOND: 20_000n })) }], [CLOCK_SYSVAR, { lamports: 1, data: Buffer.alloc(40) }]]);
  const connection = {
    getAccountInfo: async k => accounts.get(new PublicKey(k).toBase58()) ?? null,
    getMultipleAccountsInfo: async ks => ks.map(() => ({ lamports: 5e9 })),
    getLatestBlockhash: async () => ({ blockhash: program, lastValidBlockHeight: 9 }),
  };
  // Port 0: an ephemeral operator port and no public listener (a test binds nothing else).
  const cfg = loadFrontierConfig(['--program', program, '--season', '5', '--port', '0', '--public-port', '0', '--dev', '--pool-size', '3',
    '--master-seed-file', path.join(dir, 'seed'), '--invite-secret-file', path.join(dir, 'inv'), '--state-file', path.join(dir, 'state.json')]);
  const relay = await startFrontierRelay(cfg, { connection, log: () => {} });
  try {
    assert.equal(relay.servers.operator.address().address, '127.0.0.1');
    const r = await fetch(`http://127.0.0.1:${relay.ports.operator}/f/season`);
    const j = await r.json();
    assert.equal(r.status, 200);
    assert.equal(j.season, a.season);
    assert.equal(j.relayPool, 3);
    const g = await (await fetch(`http://127.0.0.1:${relay.ports.operator}/f/relay`)).json();
    assert.ok(relay.ctx.pool.has(g.feePayer));
  } finally {
    await relay.close();
    rmSync(dir, { recursive: true, force: true });
  }
});

test('quota: pruning another key never changes what an idle key gets (integ-W2 review of W2-D)', () => {
  const q = new QuotaBook();
  q.charge('a', { day: 0 });
  assert.equal(q.view('a', 3).left, 60, 'idle days refill to the burst');
  q.charge('b', { day: 3 });
  assert.equal(q.view('a', 3).left, 60, 'still 60 after another key was charged (prune ran)');
  // An entry equal to a missing one's view is forgotten: a day spent to 0
  // refills to exactly the next day's 40.
  q.entries.c = { day: 0, left: 0, lamports: 0 };
  q.charge('d', { day: 1 });
  assert.equal(q.entries.c, undefined);
  assert.equal(q.view('c', 1).left, 40);
});
