// The Frontier JS SDK (client/src/frontier) against the shared vectors:
// frontier-abi/vectors/*.json (the ABI's producer, W1-E) and
// test/frontier-vectors.json (fclient's, W1-F). Codec offsets, addresses,
// seals, fees and signed transaction shapes must agree byte for byte with
// the Rust side (contract §3.3, §3.5).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { Keypair } from '@solana/web3.js';
import { fromBase64, fromHex, toHex } from '../client/src/bytes.mjs';
import { compileMessage, parseTransaction, wireTransaction } from '../client/src/solana-tx.mjs';
import * as A from '../client/src/frontier/addresses.mjs';
import * as B from '../client/src/frontier/budgets.mjs';
import * as C from '../client/src/frontier/codec.mjs';
import * as F from '../client/src/frontier/fees.mjs';
import * as S from '../client/src/frontier/seal.mjs';
import * as SH from '../client/src/frontier/shapes.mjs';
import * as H from '../client/src/frontier/herald.mjs';

const abi = f => JSON.parse(readFileSync(new URL(`../../frontier-abi/vectors/${f}`, import.meta.url), 'utf8'));
const V = JSON.parse(readFileSync(new URL('./frontier-vectors.json', import.meta.url), 'utf8'));
const snake = s => s.replace(/([a-z])([A-Z])/g, '$1_$2').toLowerCase();

// ------------------------------------------------------------------ codec

test('codec: kinds, magics, sizes and rent agree with fclient and frontier-abi', () => {
  const layouts = abi('layouts.json');
  assert.equal(C.ACCOUNT_KINDS.length, 17, '17 account kinds (§0)');
  for (const kind of C.ACCOUNT_KINDS) {
    const l = C.layoutOf(kind);
    const k = snake(kind);
    assert.equal(l.magic, V.codec.magics[k], `${kind} magic`);
    assert.equal(l.size, V.codec.sizes[k], `${kind} size`);
    assert.equal(F.rent(l.size), BigInt(V.codec.rent[k]), `${kind} rent`);
    assert.equal(BigInt(l.rent), F.rent(l.size), `${kind} rent in the ABI`);
  }
  assert.equal(BigInt(layouts.rent_per_byte), F.RENT_PER_BYTE);
  // Per-player rent (I-31, I-47): Citizen + Holding = 9,753,600.
  assert.equal(F.rent(384) + F.rent(1280), 9_753_600n);
});

test('codec: every field offset fclient pins is the ABI\'s', () => {
  const records = { arrival: 'ArrivalRecord', entry: 'Entry', transit: 'Transit', site: 'SiteMirror', explore: 'ExploreRecord' };
  let checked = 0;
  for (const [k, offs] of Object.entries(V.codec.layouts)) {
    const kind = C.ACCOUNT_KINDS.find(x => snake(x) === k);
    const fields = kind ? C.layoutOf(kind).fields : records[k] ? C.LAYOUTS.records.find(r => r.name === records[k]).fields : null;
    if (!fields) continue;
    for (const [name, off] of Object.entries(offs)) {
      const f = fields.find(x => x.name === name.toUpperCase());
      if (!f) continue; // strides and lengths
      assert.equal(f.off, off, `${k}.${name}`);
      checked++;
    }
  }
  assert.ok(checked > 250, `${checked} offsets compared`);
});

test('codec: accounts encode and decode round trip; magic, size and season are checked', () => {
  const season = { SEASON_ID: 7n, STATUS: 2, MARCH_FEE: 10_000n, SEAL_BOND: 20_000n, MIN_REVEAL_PRIORITY_MILLI: 433, REVEAL_CU_LIMIT: 26_000,
    REVEAL_LOADED_LIMIT: 1_048_576, GENESIS_TS: -5n, JOIN_GATE: new Uint8Array(32).fill(9) };
  const bytes = C.encodeAccount('Season', season);
  assert.equal(bytes.length, 2048);
  const d = C.decodeAccount('Season', bytes, { seasonId: 7 });
  for (const [k, v] of Object.entries(season)) assert.deepEqual(d[k], v, k);
  assert.equal(C.accountKind(bytes), 'Season');
  assert.throws(() => C.decodeAccount('Season', bytes, { seasonId: 8 }), /season 7/);
  assert.throws(() => C.decodeAccount('Citizen', bytes), /magic/);
  assert.throws(() => C.decodeAccount('Season', bytes.slice(0, 2047)), /2047 bytes/);
  const cit = C.encodeAccount('Citizen', { SEASON_ID: 7n, TICKET_ESCROW: 123n, HOLDING: [{ P: -3, Q: 7, SITE: 11, GEN: 2 }, { P: 0, Q: 0, SITE: 0, GEN: 0 }, { P: 1, Q: -1, SITE: 1, GEN: 1 }] });
  const c = C.decodeAccount('Citizen', cit);
  assert.equal(c.TICKET_ESCROW, 123n);
  assert.deepEqual(c.HOLDING[0], { P: -3, Q: 7, SITE: 11, GEN: 2 });
  assert.throws(() => C.encodeAccount('Citizen', { FACTION: 256 }), /does not fit/);
});

test('codec: every instruction sample of frontier-abi decodes and encodes back', () => {
  const { samples } = abi('ix.json');
  assert.equal(samples.length, 50, '50 instructions (§0)');
  for (const s of samples) {
    const data = fromHex(s.data);
    assert.equal(data.length, s.len, s.name);
    const d = C.decodeIxData(data);
    assert.equal(d.name, s.name);
    assert.equal(d.tag, s.tag);
    if (d.rest) continue; // CreateSeason, ArchiveAnchors: variable, carried raw
    const { tag, name, ...fields } = d;
    assert.equal(toHex(C.encodeIxData(name, fields)), s.data, s.name);
  }
  assert.throws(() => C.decodeIxData([0x50, 1, 2]), /Depart: data is 3 bytes/);
  assert.throws(() => C.decodeIxData([0x53]), /unknown instruction tag 83/, 'ProveBadSeal is gone (I-44)');
});

test('codec: error table (§5.4) and program errors from simulation', () => {
  assert.deepEqual(V.codec.errors.map(e => [e.code, e.name]), V.codec.errors.map(e => [e.code, C.errorName(e.code)]));
  for (const [code, name] of [[51, 'TipTooLow'], [52, 'AlreadyDone'], [53, 'LatchClosed'], [58, 'HostInTransit'], [59, 'JoinGate'], [61, 'TipNotPreset']]) {
    assert.equal(C.errorName(code), name);
    assert.equal(C.errorCode(name), code);
  }
  assert.deepEqual(C.programError({ InstructionError: [3, { Custom: 51 }] }), { index: 3, code: 51, name: 'TipTooLow' });
  assert.deepEqual(C.programError('{"InstructionError":[3,{"Custom":24}]}'), { index: 3, code: 24, name: 'NotFinal' });
  assert.equal(C.programError('BlockhashNotFound'), null);
});

// ------------------------------------------------------------------ addresses

test('addresses: every seed and address of frontier-abi\'s vector', () => {
  const v = abi('addresses.json');
  const season = v.season_pda.b58;
  const program = v.program_id.b58;
  const ctor = {
    Frontier: () => A.seeds.frontier(), DefencePool: () => A.seeds.defencePool(), RingSeed: k => A.seeds.ringSeed(k.d), ProvinceFund: k => A.seeds.provinceFund(k.w),
    JoinShard: k => A.seeds.joinShard(k.faction, k.shard), BeaconLog: k => A.seeds.beaconLog(k.region), Citizen: k => A.seeds.citizen(k.wallet),
    Province: k => A.seeds.province(k.p, k.q), Holding: k => A.seeds.holding(k.p, k.q, k.site), ArrivalSlot: k => A.seeds.arrivalSlot(k.p, k.q, k.bell, k.faction, k.i),
    ArrivalDay: k => A.seeds.arrivalDay(k.p, k.q, k.day), ClashInputs: k => A.seeds.clashInputs(k.p, k.q, k.bell), BellAnchor: k => A.seeds.bellAnchor(k.bell, k.region),
    SeedCache: k => A.seeds.seedCache(k.bell, k.region, k.nonce), AnchorArchive: k => A.seeds.anchorArchive(k.region, k.day),
    DefenceClaim: k => A.seeds.defenceClaim(k.beneficiary, k.day), 'PosturePDA (reserved, M3)': k => A.seeds.posture(k.p, k.q, k.bell, k.pos),
    'SealVerdict (reserved, removed v1.1)': k => A.seeds.sealVerdictReserved(k.host_id, k.arrive_bell),
  };
  for (const a of v.accounts) {
    const s = ctor[a.kind](a.key ?? {});
    assert.equal(s, a.seed, `${a.kind} ${JSON.stringify(a.key)}`);
    assert.ok(s.length <= 32);
    assert.equal(A.withSeed(season, s, program), a.address, `${a.kind} address`);
    if (a.kind === 'Citizen') assert.equal(toHex(A.citizenTag15(a.key.wallet)), a.key.tag15);
    if (a.kind === 'DefenceClaim') assert.equal(toHex(A.keeperTag8(a.key.beneficiary)), a.key.keeper_tag8);
  }
  assert.equal(v.tags.join_shard, A.joinShardOf(v.tags.wallet));
  assert.equal(A.citizenTag(v.tags.citizen_address), BigInt(v.tags.citizen_tag_u64));
});

test('addresses: fclient\'s season PDA, program data, citizen and seeds', () => {
  const v = V.addresses;
  const a = new A.FrontierAddresses({ programId: v.program, seasonId: v.season_id });
  assert.equal(a.season, v.season);
  assert.equal(a.bump, v.bump);
  assert.equal(a.programData, v.programdata);
  assert.equal(a.citizen(v.citizen.wallet), v.citizen.address);
  assert.equal(toHex(A.citizenTag15(v.citizen.wallet)), v.citizen.tag15);
  assert.equal(A.citizenTag(v.citizen.address), BigInt(v.citizen.citizen_tag));
  assert.equal(A.joinShardOf(v.citizen.wallet), v.citizen.join_shard);
  assert.equal(toHex(A.keeperTag8(v.citizen.wallet)), v.citizen.keeper_tag8);
  for (const s of v.seeds) assert.equal(a.of(s.seed), s.address, s.seed);
});

test('addresses: host ids and province indices round trip (kernel geometry)', () => {
  const season = abi('addresses.json').season_pda.b58;
  const program = abi('addresses.json').program_id.b58;
  for (const h of [...abi('addresses.json').host_ids, ...V.addresses.host_ids.map(x => ({ ...x, host_id: x.id, holding_address: x.holding }))]) {
    const id = A.hostId(h.p, h.q, h.site, h.gen, h.seq);
    assert.equal(id, BigInt(h.host_id), JSON.stringify(h));
    const parts = A.hostParts(id);
    assert.deepEqual([parts.p, parts.q, parts.site, parts.gen, parts.seq], [h.p, h.q, h.site, h.gen, h.seq]);
    if (h.holding_address && h.id === undefined) assert.equal(A.withSeed(season, A.seeds.holding(h.p, h.q, h.site), program), h.holding_address);
  }
  const fc = new A.FrontierAddresses({ programId: V.addresses.program, seasonId: V.addresses.season_id });
  for (const h of V.addresses.host_ids) assert.equal(fc.holdingOfHost(BigInt(h.id)), h.holding);
  for (let i = 0; i < A.provincesWithin(12); i++) {
    const c = A.provinceFromIndex(i);
    assert.equal(A.provinceIndex(c.p, c.q), i);
  }
  assert.equal(A.provinceFromIndex(A.provincesWithin(128)), null);
  assert.equal(A.hostId(129, 0, 0, 0, 0), A.HOST_ID_INVALID);
  assert.equal(A.hostId(0, 0, 12, 0, 0), A.HOST_ID_INVALID);
  assert.equal(A.hostParts(A.HOST_ID_INVALID), null);
});

// ------------------------------------------------------------------ seal

test('seal: plaintext pack/unpack, salt, commitment, body and root match the Rust vectors', () => {
  const s = V.seal;
  const pt = { version: 1, hostId: BigInt(s.plain.host_id), arriveBell: s.plain.arrive_bell, destP: s.plain.dest_p, destQ: s.plain.dest_q, destTile: s.plain.dest_tile,
    stance: s.plain.stance, retreatBps: s.plain.retreat_bps, pathLen: s.plain.path_len, path: fromHex(s.plain.path) };
  const plain = S.pack(pt);
  assert.equal(toHex(plain), s.plain.packed);
  assert.deepEqual({ ...S.unpack(plain), reserved: undefined }, { ...pt, reserved: undefined });
  const k = fromHex(s.k);
  assert.equal(toHex(S.saltOf(k)), s.salt);
  assert.equal(toHex(S.commit(plain, S.saltOf(k))), s.commit);
  assert.equal(toHex(S.bodyXor(k, plain)), s.body);
  assert.equal(S.validate(S.unpack(plain), { hostId: pt.hostId, arriveBell: pt.arriveBell }), null);
  assert.deepEqual(s.domains, { keystream: S.DOMAIN_KS, march: S.DOMAIN_MARCH, posture: S.DOMAIN_POSTURE, salt: S.DOMAIN_SALT });
  assert.equal(s.retreat_max_bps, S.RETREAT_MAX_BPS);
  // Rust → JS: the seal sealed by the tlock crate; its body opens with k, its root is Depart's.
  const r = s.rust_to_js;
  const seal = fromHex(r.seal);
  assert.equal(seal.length, S.SEAL_LEN);
  assert.equal(toHex(S.ctHash(seal)), r.ct_hash);
  assert.equal(toHex(S.sealRoot(fromHex(r.commit), fromHex(r.ct_hash))), r.seal_root);
  assert.equal(toHex(S.openBody(fromHex(r.k), seal, fromHex(r.commit))), r.plain);
  assert.equal(S.openBody(new Uint8Array(16), seal, fromHex(r.commit)), null, 'the wrong k does not open it');
  assert.equal(toHex(S.assembleSeal(S.splitSeal(seal))), r.seal);
  assert.equal(S.auditSeal({ k: fromHex(r.k), seal, plain: fromHex(r.plain), commitment: fromHex(r.commit), hostId: pt.hostId, arriveBell: pt.arriveBell }), null);
  assert.equal(S.auditSeal({ k: fromHex(r.k), seal, plain: fromHex(r.plain), commitment: fromHex(r.commit), hostId: pt.hostId + 1n, arriveBell: pt.arriveBell }), 'BadPlaintext:HostMismatch');
  // Reveal material (I-24): the salt, not k.
  const m = S.revealMaterial({ holding: 'x', transitSlot: 1, plain, salt: S.saltOf(k), seal });
  assert.equal(toHex(fromBase64(m.ct_hash_b64)), r.ct_hash);
  assert.equal(toHex(fromBase64(m.salt_b64)), s.salt);
});

test('seal: every invalid plaintext of the vector is refused for its reason (I-28)', () => {
  const why = { version: 'Version', reserved: 'Reserved', path_len: 'PathTooLong', path_bits: 'PathBits', direction: 'Direction', tile: 'Tile', stance: 'Stance', retreat: 'Retreat',
    host: 'HostMismatch', arrive: 'ArriveMismatch' };
  assert.ok(V.seal.invalid_plaintexts.length >= 8);
  for (const c of V.seal.invalid_plaintexts) {
    const got = S.validate(S.unpack(fromHex(c.packed)), { hostId: BigInt(c.host_id), arriveBell: c.arrive_bell });
    assert.equal(got, why[c.why] ?? c.why, c.why);
  }
  const p = S.encodePath([0, 1, 2, 3, 4, 5, 0, 5]);
  assert.deepEqual(S.decodePath({ ...p }), [0, 1, 2, 3, 4, 5, 0, 5]);
  assert.equal(S.encodePath([6]), null);
  assert.equal(S.encodePath(Array(33).fill(0)), null);
});

// ------------------------------------------------------------------ fees and budgets

test('fees: cost, priority, tips, loaded limits and defence refunds match the kernel vectors', () => {
  const f = V.fees;
  for (const c of f.cost) assert.equal(F.cost(c.limit, c.sigs, c.writes, c.loaded), BigInt(c.cost));
  for (const c of f.min_tip) assert.equal(F.minTipLamports(c.p_milli, c.limit, c.loaded), BigInt(c.tip_min));
  for (const c of f.p_tip_milli) assert.equal(F.priorityMilli(BigInt(c.tip) - 2_500n - 0n, c.cost) >= BigInt(c.p_milli), true);
  for (const c of f.priority) {
    assert.equal(F.feeForPriority(c.p_milli, c.cost), BigInt(c.fee), `fee at ${c.p_milli}`);
    assert.equal(F.cuPriceMicro(c.fee, c.cu_limit), BigInt(c.cu_price_micro));
    assert.equal(F.priorityMilli(c.fee, c.cost), BigInt(c.priority_milli));
  }
  for (const c of f.loaded_limit) assert.equal(F.loadedLimit(c.programdata_len, c.account_bytes, c.n_accounts), c.loaded_limit);
  for (const c of f.deploy_max_len) assert.equal(F.deployMaxLen(c.so), c.max_len);
  for (const c of f.defence_refund) {
    assert.equal(F.defenceRefund({ priceMicro: c.ev_price, limit: c.ev_limit, loaded: c.ev_loaded, createdDay: c.created_day }, { defenceCapMilli: c.defence_cap_milli, tipMin: c.tip_min }),
      BigInt(c.refund), JSON.stringify(c));
  }
  // §10.1: 14,441 at 26k with L = 1 MiB; 10,111 at 16k; the three presets (I-51).
  assert.equal(F.minTipLamports(433, 26_000, 1 << 20), 14_441n);
  assert.deepEqual(F.tipPresets(14_441n), [14_441n, 21_662n, 28_882n]);
  assert.equal(F.tipPriorityMilli(14_441n, 26_000, 1 << 20), 433n);
});

test('budgets: the prefix is limit, price 0, L(kind), as fclient builds it', () => {
  const rows = abi('budgets.json').instructions;
  assert.equal(B.allBudgets().length, rows.length);
  for (const r of rows) {
    const b = B.budgetOf(r.tag);
    assert.equal(b.cuLimit, r.cu_limit);
    assert.equal(b.loadedLimit, r.loaded_limit);
    assert.equal(B.budgetOf(r.name).tag, r.tag);
  }
  const [lim, price, loaded] = B.budgetPrefix('Depart').map(B.parseComputeBudgetIx);
  assert.deepEqual([lim, price, loaded], [{ kind: 'limit', value: 15_000 }, { kind: 'price', value: 0n }, { kind: 'loaded', value: 1_048_576 }]);
  assert.equal(B.parseComputeBudgetIx({ programId: B.COMPUTE_BUDGET_PROGRAM, keys: [], data: new Uint8Array([2, 0, 0]) }), null);
  assert.equal(B.budgetPrefix('Reveal', { heap: 262_144 }).length, 4);
});

// ------------------------------------------------------------------ shapes

const PROGRAM = V.addresses.program;
const vectorTx = name => {
  const c = V.shapes.cases.find(x => x.name === name);
  const wire = fromBase64(c.wire);
  return { c, wire, tx: parseTransaction(wire) };
};

test('shapes: fclient\'s signed player and settle shapes classify; Reveal is UseRevealRoute; keeper shapes are refused', () => {
  for (const name of ['join', 'file_ticket', 'harvest', 'muster', 'depart', 'settle_transit']) {
    const { c, wire, tx } = vectorTx(name);
    const r = SH.classify(tx, { programId: PROGRAM, wireBytes: wire.length });
    assert.ok(r.ok, `${name}: ${r.problem}`);
    assert.equal(r.tag, c.tag);
    assert.equal(r.feePayer, c.fee_payer);
    assert.equal(r.kind, name === 'settle_transit' ? 'settle' : 'player');
    assert.equal(r.accounts.payer, c.fee_payer);
    assert.equal(r.budget.cuLimit, c.cu_limit);
    assert.equal(r.cu.loaded, c.loaded_limit);
    assert.equal(wire.length, c.bytes);
    if (name === 'join') assert.equal(r.authority, V.shapes.keys.wallet);
    else if (r.kind === 'player') assert.equal(r.authority, V.shapes.keys.session);
    else assert.equal(r.authority, null);
  }
  const rv = vectorTx('reveal');
  assert.deepEqual(SH.classify(rv.tx, { programId: PROGRAM }).code, 'UseRevealRoute');
  const pa = vectorTx('post_anchor');
  assert.equal(SH.classify(pa.tx, { programId: PROGRAM }).code, 'RelayRejected');
});

test('shapes: the builder reproduces fclient\'s instructions (accounts, flags, data)', () => {
  for (const name of ['join', 'file_ticket', 'harvest', 'muster', 'depart', 'settle_transit']) {
    const { tx, wire } = vectorTx(name);
    const r = SH.classify(tx, { programId: PROGRAM, wireBytes: wire.length });
    const { tag, name: ixName, ...fields } = r.data;
    const ixs = SH.shapeIxs(PROGRAM, ixName, r.accounts, fields);
    assert.equal(ixs.length, 4);
    const want = tx.instructions;
    for (let i = 0; i < 4; i++) {
      assert.equal(ixs[i].programId, want[i].programId);
      assert.equal(toHex(ixs[i].data), toHex(want[i].data), `${name} ix ${i} data`);
    }
    assert.deepEqual(ixs[3].keys.map(k => k.pubkey), want[3].keys.map(k => k.pubkey), `${name} accounts`);
    for (const [j, k] of ixs[3].keys.entries()) {
      if (k.isSigner) assert.ok(want[3].keys[j].isSigner, `${name} account ${j} signs`);
      if (k.isWritable) assert.ok(want[3].keys[j].isWritable, `${name} account ${j} writable`);
    }
  }
});

test('shapes: the allowlist refuses what the relay must not sponsor', () => {
  const programId = PROGRAM;
  const relay = Keypair.generate().publicKey.toBase58();
  const session = Keypair.generate().publicKey.toBase58();
  const a = new A.FrontierAddresses({ programId, seasonId: 1 });
  const accounts = { actor: session, payer: relay, season: a.season, citizen: a.citizen(session), holding: a.holding(-3, 7, 11) };
  const txOf = (ixs, feePayer = relay) => parseTransaction(wireTransaction(compileMessage({ feePayer, recentBlockhash: relay, instructions: ixs })));
  const good = SH.shapeIxs(programId, 'Harvest', accounts, {});
  const ok = SH.classify(txOf(good), { programId });
  assert.ok(ok.ok, ok.problem);
  assert.equal(ok.name, 'Harvest');
  assert.equal(ok.authority, session);
  const cases = [
    ['wrong program', [...good.slice(0, 3), { ...good[3], programId: Keypair.generate().publicKey.toBase58() }]],
    ['extra instruction', [...good, good[3]]],
    ['no prefix', [good[3]]],
    ['CU price above 0', [good[0], B.setComputeUnitPrice(1n), good[2], good[3]]],
    ['wrong CU limit', [B.setComputeUnitLimit(1_400_000), good[1], good[2], good[3]]],
    ['wrong loaded limit', [good[0], good[1], B.setLoadedAccountsDataSizeLimit(65_536), good[3]]],
    ['prefix out of order', [good[1], good[0], good[2], good[3]]],
    ['heap frame', [good[0], good[1], good[2], B.requestHeapFrame(262_144), good[3]]],
    ['payer is not the fee payer', SH.shapeIxs(programId, 'Harvest', { ...accounts, payer: session }, {})],
    ['a keeper instruction', [...good.slice(0, 3), { ...good[3], data: Uint8Array.from([0x10]) }]],
    ['Depart data too short', [...good.slice(0, 3), { ...good[3], data: Uint8Array.from([0x50, 1]) }]],
    ['holding account missing', [...good.slice(0, 3), { ...good[3], keys: good[3].keys.slice(0, 4) }]],
    ['actor does not sign', [...good.slice(0, 3), { ...good[3], keys: good[3].keys.map((k, i) => (i === 0 ? { ...k, isSigner: false } : k)) }]],
    ['an extra signer', [...good.slice(0, 3), { ...good[3], keys: good[3].keys.map((k, i) => (i === 4 ? { ...k, isSigner: true } : k)) }]],
    ['the authority is the fee payer', SH.shapeIxs(programId, 'Harvest', { ...accounts, actor: relay }, {})],
  ];
  for (const [what, ixs] of cases) {
    const r = SH.classify(txOf(ixs), { programId });
    assert.equal(r.ok, false, what);
    assert.equal(r.code, 'RelayRejected', what);
  }
  // A Reveal anywhere, even after a valid prefix: UseRevealRoute.
  assert.equal(SH.classify(txOf([...good.slice(0, 3), { ...good[3], data: Uint8Array.from([0x51]) }]), { programId }).code, 'UseRevealRoute');
  assert.equal(SH.classify(txOf([...good, { ...good[3], data: Uint8Array.from([0x51]) }]), { programId }).code, 'UseRevealRoute');
  // Oversize for the kind (Harvest's ceiling).
  assert.equal(SH.classify(txOf(good), { programId, wireBytes: B.budgetOf('Harvest').txCeiling + 1 }).code, 'RelayRejected');
  // A settle shape signed by anyone besides the fee payer.
  const st = vectorTx('settle_transit');
  const c = SH.classify(st.tx, { programId });
  const { tag, name, ...fields } = c.data;
  const ixs = SH.shapeIxs(programId, 'SettleTransit', c.accounts, fields);
  ixs[3].keys[2] = { ...ixs[3].keys[2], isSigner: true };
  assert.equal(SH.classify(txOf(ixs, st.tx.signers[0]), { programId }).ok, false);
});

// ------------------------------------------------------------------ herald

test('herald: overview files round trip and are checked; envelopes decode; paths are the contract\'s', () => {
  const provinces = [
    { p: 2, q: -1, owners: [0, 1, 2, 3, 4, 5, 6, 7, 7, 7, 7, 7], sites: ['free', 'holding', 'camp', 'reserved', 'free', 'free', 'free', 'free', 'free', 'free', 'free', 'holding'],
      hostsByFaction: [1, 0, 0, 3, 0, 0, 255], clash: true, dormant: false, opened: true, resolvedNext: 4031 },
    { p: -3, q: 7, owners: Array(12).fill(7), sites: Array(12).fill('free'), hostsByFaction: [0, 0, 0, 0, 0, 0, 0], clash: false, dormant: true, opened: false, resolvedNext: 0 },
  ];
  const bytes = H.encodeOverview({ season: 7n, ring: 3, bell: 144, slot: 99n, provinces });
  assert.equal(bytes.length, 32 + 2 * 24);
  const d = H.decodeOverview(bytes);
  assert.equal(d.season, 7n);
  assert.equal(d.ring, 3);
  assert.equal(d.bell, 144);
  assert.deepEqual(d.provinces.map(x => [x.p, x.q]), [[-3, 7], [2, -1]], 'sorted by (P, Q)');
  const x = d.provinces[1];
  assert.deepEqual(x.owners, provinces[0].owners);
  assert.deepEqual(x.sites, provinces[0].sites);
  assert.deepEqual(x.hostsByFaction, provinces[0].hostsByFaction);
  assert.equal(x.flags, 5);
  assert.equal(x.resolvedNext, 4031);
  assert.throws(() => H.decodeOverview(bytes.slice(0, 40)), /bytes for 2 records/);
  const bad = Uint8Array.from(bytes); bad[0] = 0;
  assert.throws(() => H.decodeOverview(bad), /not an overview/);
  const env = H.decodeEnvelope({ v: 1, key: 'pv:-3,7', bell: 5, slot: 9, seq: '18446744073709551615', head: '00'.repeat(32), bytes: 'AAEC',
    slots: [{ key: 'ar:-3,7,5,2,3', slot: 8, bytes: 'AQ==' }], day: { key: 'ad:-3,7,0', bytes: '' }, inputs: null });
  assert.deepEqual(env.key, { kind: 'Province', p: -3, q: 7 });
  assert.equal(env.seq, 2n ** 64n - 1n);
  assert.deepEqual([...env.bytes], [0, 1, 2]);
  assert.deepEqual(env.slots[0].key, { kind: 'ArrivalSlot', p: -3, q: 7, bell: 5, faction: 2, i: 3 });
  assert.throws(() => H.decodeEnvelope({ v: 2 }), /version/);
  assert.throws(() => H.parseKey('pv:1'), /bad herald key/);
  assert.equal(H.heraldPaths.overview(3), '/h/overview/3/latest.bin');
  assert.equal(H.heraldPaths.province(-3, 7, 12), '/h/province/-3,7/12');
  assert.equal(H.heraldPaths.bell(12, 4), '/h/bell/12/region/4');
  assert.equal(H.heraldPaths.events(500), '/h/events?after=500');
});

test('herald: the client fetches, decodes and reports HTTP failures', async () => {
  const seen = [];
  const bytes = H.encodeOverview({ season: 1n, ring: 0, bell: 0, slot: 0n, provinces: [] });
  const fetch = async url => {
    seen.push(url);
    if (url.endsWith('/latest.bin')) return { ok: true, status: 200, arrayBuffer: async () => bytes.buffer };
    if (url.includes('/h/season')) return { ok: true, status: 200, json: async () => ({ v: 1, programId: 'x' }) };
    return { ok: false, status: 404 };
  };
  const h = new H.HeraldClient({ base: 'http://herald.test/', fetch });
  assert.deepEqual(await h.season(), { v: 1, programId: 'x' });
  assert.equal((await h.overview(0)).provinces.length, 0);
  await assert.rejects(h.clash(0, 0, 1), e => e.status === 404 && /HTTP 404/.test(e.message));
  assert.deepEqual(seen, ['http://herald.test/h/season', 'http://herald.test/h/overview/0/latest.bin', 'http://herald.test/h/clash/0,0/1']);
});
