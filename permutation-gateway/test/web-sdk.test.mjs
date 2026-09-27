// The web client's copy of the SDK core (permutation-server/web/sdk,
// written by scripts/sync-web-sdk.mjs): exactly the browser-safe set, each
// file its client/src source under the generated-file header, free of
// node:*, Buffer and bare imports, importing only itself, and working in a
// runtime without Buffer. The same for the Frontier set (web/sdk/frontier,
// M1): its ABI modules generated from frontier-abi/vectors and fresh, its
// modules importing only the Frontier set and the flat set above. And the
// vendored noble tree (web/sdk/vendor/noble, scripts/vendor-noble.mjs):
// every file pinned by the manifest's sha256, nothing unlisted, relative
// imports only.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { expectedAbi, FRONTIER_GENERATED, HEADER, importsOf, problems, stripComments, WEB_SDK, WEB_SDK_DIRS, WEB_SDK_FRONTIER } from '../scripts/sync-web-sdk.mjs';

const src = new URL('../client/src/', import.meta.url);
const web = new URL('../../permutation-server/web/sdk/', import.meta.url);
const srcFrontier = new URL('frontier/', src);
const webFrontier = new URL('frontier/', web);
const noble = new URL('vendor/noble/', web);
const read = (dir, f) => readFileSync(new URL(f, dir), 'utf8');
const SYNC = 'node permutation-gateway/scripts/sync-web-sdk.mjs';
const isDir = u => existsSync(u) && statSync(u).isDirectory();

test('web/sdk holds exactly the browser-safe set, each file its source under the header', () => {
  const files = readdirSync(web).filter(f => !(WEB_SDK_DIRS.includes(f) && isDir(new URL(`${f}/`, web))));
  assert.deepEqual(files.sort(), [...WEB_SDK].sort(), `web/sdk has other files than the set (run ${SYNC})`);
  for (const d of readdirSync(web).filter(f => isDir(new URL(`${f}/`, web)))) assert.ok(WEB_SDK_DIRS.includes(d), `web/sdk/${d}/ is not a known subdirectory`);
  for (const f of WEB_SDK) {
    const [first, ...rest] = read(web, f).split('\n');
    assert.equal(first, HEADER, `${f}: header`);
    assert.equal(rest.join('\n'), read(src, f), `${f} differs from client/src/${f} (run ${SYNC})`);
  }
});

test('the set uses no node:*, Buffer or bare imports, and imports only itself', () => {
  const code = s => s.replace(/\/\*[\s\S]*?\*\//g, '').split('\n').map(l => l.replace(/(^|\s)\/\/.*$/, '')).join('\n');
  for (const f of WEB_SDK) {
    const c = code(read(src, f));
    const specs = [...c.matchAll(/\bfrom\s*['"]([^'"]+)['"]|\bimport\s*\(?\s*['"]([^'"]+)['"]/g)].map(m => m[1] ?? m[2]);
    for (const s of specs) {
      assert.ok(!s.startsWith('node:'), `${f} imports ${s}`);
      assert.ok(s.startsWith('./') && !s.slice(2).includes('/'), `${f}: ${s} is not a sibling module`);
      assert.ok(WEB_SDK.includes(s.slice(2)), `${f} imports ${s}, outside the set`);
    }
    assert.doesNotMatch(c, /\bBuffer\b/, `${f} uses Buffer`);
    assert.doesNotMatch(c, /\brequire\s*\(|\bprocess\.|__dirname|import\.meta/, `${f} uses a Node-only global`);
  }
});

test('the web copies run without Buffer and compute what client/src computes', async () => {
  const probe = `
    delete globalThis.Buffer;
    const sdk = ${JSON.stringify(web.href)};
    const codec = await import(sdk + 'codec.mjs');
    const decision = await import(sdk + 'decision.mjs');
    const talk = await import(sdk + 'talk.mjs');
    const tx = await import(sdk + 'solana-tx.mjs');
    const player = await import(sdk + 'player.mjs');
    const batch = await import(sdk + 'batch.mjs');
    const offices = await import(sdk + 'offices.mjs');
    const bytes = await import(sdk + 'bytes.mjs');
    const input = JSON.parse(process.argv[1]);
    const b = { ...input.batch, decisionDigest: bytes.fromHex(input.batch.decisionDigest) };
    const programId = input.programId;
    const message = tx.compileMessage({ feePayer: input.feePayer, recentBlockhash: input.feePayer, instructions: [
      ...player.commitOrdersIxs({ programId, seasonId: 7n, signer: input.signer, civ: 3, role: 'Steward', tick: 9, commitment: codec.orderCommitment(b, bytes.fromHex(input.salt)) }),
    ] });
    console.log(JSON.stringify({
      commitment: bytes.toHex(codec.orderCommitment(b, bytes.fromHex(input.salt))),
      digest: decision.decisionDigest(input.decision),
      talk: bytes.toHex(talk.talkBytes({ ...input.talk, season: BigInt(input.talk.season) })),
      season: player.pda.season(programId, 7n),
      message: bytes.toHex(message),
      name: codec.memberName(bytes.fromHex('0102030405060708')),
      packed: batch.packBatch({ orders: [] }).used,
      office: offices.officeOf({ type: 'SetResearch', techs: [] }),
      seal: bytes.toHex(player.sealMessage({ seasonId: 7n, tick: 9, civ: 3, role: 'Steward', commitment: new Uint8Array(32) })),
      buffer: typeof Buffer,
    }));`;
  const vectors = JSON.parse(read(new URL('./', import.meta.url), 'vectors.json'));
  const [decisionVector] = JSON.parse(read(new URL('./', import.meta.url), 'decision-vectors.json'));
  const c = vectors.commitment;
  const input = {
    batch: { civ: c.civ, tick: c.tick, role: c.role, member: c.member, decisionDigest: c.decisionDigest, orders: c.orders, adopt: c.adopt },
    salt: c.salt, decision: decisionVector, talk: { season: '42', tick: 17, member: 255, to: { civ: 2 }, text: '同盟を組もう 🤝' },
    programId: 'J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n', feePayer: '11111111111111111111111111111112', signer: 'SysvarRent111111111111111111111111111111111',
  };
  const out = JSON.parse(execFileSync(process.execPath, ['--input-type=module', '-e', probe, JSON.stringify(input)], { encoding: 'utf8' }));
  assert.equal(out.buffer, 'undefined');
  assert.equal(out.commitment, c.commitment, 'orderCommitment matches the Rust vector');
  assert.equal(out.digest, decisionVector.digest, 'decisionDigest matches the engine');
  assert.equal(out.talk, '7065726d75746174696f6e2d72756c65732f74616c6b2a000000000000001100ff000000010200e5908ce79b9fe38292e7b584e38282e3818620f09fa49d');
  const { pda, commitOrdersIxs, sealMessage } = await import('../client/src/player.mjs');
  const { compileMessage } = await import('../client/src/solana-tx.mjs');
  const { memberName, orderCommitment } = await import('../client/src/codec.mjs');
  const { fromHex, toHex } = await import('../client/src/bytes.mjs');
  assert.equal(out.season, pda.season(input.programId, 7n));
  const b = { ...input.batch, decisionDigest: fromHex(c.decisionDigest) };
  assert.equal(out.message, toHex(compileMessage({ feePayer: input.feePayer, recentBlockhash: input.feePayer, instructions: commitOrdersIxs({ programId: input.programId,
    seasonId: 7n, signer: input.signer, civ: 3, role: 'Steward', tick: 9, commitment: orderCommitment(b, fromHex(c.salt)) }) })));
  assert.equal(out.name, memberName(fromHex('0102030405060708')));
  assert.equal(out.packed, 0);
  assert.equal(out.office, 'Science');
  assert.equal(out.seal, toHex(sealMessage({ seasonId: 7n, tick: 9, civ: 3, role: 'Steward', commitment: new Uint8Array(32) })));
});

// ------------------------------------------------------------------ Frontier (M1)

test('client/src/frontier: the ABI modules are generated from frontier-abi/vectors and fresh', () => {
  const want = expectedAbi();
  assert.deepEqual([...want.keys()].sort(), Object.keys(FRONTIER_GENERATED).sort());
  for (const [f, content] of want) assert.equal(read(srcFrontier, f), content, `client/src/frontier/${f} is stale against frontier-abi/vectors (run ${SYNC})`);
});

test('web/sdk/frontier holds exactly the Frontier set, each file its source under the header', () => {
  assert.deepEqual(readdirSync(webFrontier).sort(), [...WEB_SDK_FRONTIER].sort(), `web/sdk/frontier has other files than the set (run ${SYNC})`);
  assert.deepEqual(readdirSync(srcFrontier).sort(), [...WEB_SDK_FRONTIER].sort(), 'client/src/frontier holds only the Frontier set');
  for (const f of WEB_SDK_FRONTIER) {
    const [first, ...rest] = read(webFrontier, f).split('\n');
    assert.equal(first, HEADER, `frontier/${f}: header`);
    assert.equal(rest.join('\n'), read(srcFrontier, f), `frontier/${f} differs from client/src/frontier/${f} (run ${SYNC})`);
  }
});

test('the Frontier set uses no node:*, Buffer or bare imports, and imports only itself and the flat set', () => {
  for (const f of WEB_SDK_FRONTIER) {
    const source = read(srcFrontier, f);
    assert.deepEqual(problems(f, source, { frontier: true }), [], f);
    const c = stripComments(source);
    for (const s of importsOf(c)) {
      assert.ok(/^\.\/[\w-]+\.mjs$/.test(s) || /^\.\.\/[\w-]+\.mjs$/.test(s), `${f}: ${s} is not a sibling or a flat-set module`);
      if (s.startsWith('../')) assert.ok(WEB_SDK.includes(s.slice(3)), `${f} imports ${s}, outside the flat set`);
      else assert.ok(WEB_SDK_FRONTIER.includes(s.slice(2)), `${f} imports ${s}, outside the Frontier set`);
    }
    assert.doesNotMatch(c, /\bBuffer\b/, `${f} uses Buffer`);
    assert.doesNotMatch(c, /\brequire\s*\(|\bprocess\.|__dirname|import\.meta/, `${f} uses a Node-only global`);
  }
  // The checker itself refuses what it must.
  assert.equal(problems('x.mjs', "import { a } from '../node-only.mjs';", { frontier: true }).length, 1);
  assert.equal(problems('x.mjs', "import { a } from './unknown.mjs';", { frontier: true }).length, 1);
  assert.equal(problems('x.mjs', "import { a } from '../bytes.mjs';", { frontier: true }).length, 0);
});

test('the web copies of the Frontier set run without Buffer and compute what client/src/frontier computes', async () => {
  const probe = `
    delete globalThis.Buffer;
    const sdk = ${JSON.stringify(webFrontier.href)};
    const addresses = await import(sdk + 'addresses.mjs');
    const seal = await import(sdk + 'seal.mjs');
    const fees = await import(sdk + 'fees.mjs');
    const shapes = await import(sdk + 'shapes.mjs');
    const codec = await import(sdk + 'codec.mjs');
    const herald = await import(sdk + 'herald.mjs');
    const bytes = await import(${JSON.stringify(new URL('bytes.mjs', web).href)});
    const [program, wallet] = JSON.parse(process.argv[1]);
    const a = new addresses.FrontierAddresses({ programId: program, seasonId: 3n });
    const k = new Uint8Array(16).fill(7);
    const pt = { hostId: addresses.hostId(-3, 7, 11, 1, 9), arriveBell: 147, destP: -2, destQ: 7, destTile: 30, stance: 1, retreatBps: 0, ...(() => { const p = seal.encodePath([0, 1, 5]); return { pathLen: p.pathLen, path: p.path }; })() };
    const parts = seal.sealParts(pt, k);
    const message = shapes.shapeMessage({ programId: program, name: 'Harvest', accounts: { actor: wallet, season: a.season, citizen: a.citizen(wallet), holding: a.holding(-3, 7, 11) },
      fields: {}, feePayer: program, recentBlockhash: wallet });
    console.log(JSON.stringify({
      season: a.season, citizen: a.citizen(wallet), commit: bytes.toHex(parts.commit), body: bytes.toHex(parts.body),
      tip: fees.minTipLamports(433, 26000, 1048576).toString(), message: bytes.toHex(message),
      err: codec.errorName(58), overview: herald.decodeOverview(herald.encodeOverview({ season: 1n, ring: 0, bell: 0, slot: 0n, provinces: [] })).ring,
      buffer: typeof Buffer,
    }));`;
  const input = ['J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n', 'SysvarRent111111111111111111111111111111111'];
  const out = JSON.parse(execFileSync(process.execPath, ['--input-type=module', '-e', probe, JSON.stringify(input)], { encoding: 'utf8' }));
  assert.equal(out.buffer, 'undefined');
  const addresses = await import('../client/src/frontier/addresses.mjs');
  const seal = await import('../client/src/frontier/seal.mjs');
  const shapes = await import('../client/src/frontier/shapes.mjs');
  const { toHex } = await import('../client/src/bytes.mjs');
  const a = new addresses.FrontierAddresses({ programId: input[0], seasonId: 3n });
  assert.equal(out.season, a.season);
  assert.equal(out.citizen, a.citizen(input[1]));
  const p = seal.encodePath([0, 1, 5]);
  const parts = seal.sealParts({ hostId: addresses.hostId(-3, 7, 11, 1, 9), arriveBell: 147, destP: -2, destQ: 7, destTile: 30, stance: 1, retreatBps: 0, pathLen: p.pathLen, path: p.path }, new Uint8Array(16).fill(7));
  assert.equal(out.commit, toHex(parts.commit));
  assert.equal(out.body, toHex(parts.body));
  assert.equal(out.tip, '14441');
  assert.equal(out.message, toHex(shapes.shapeMessage({ programId: input[0], name: 'Harvest', accounts: { actor: input[1], season: a.season, citizen: a.citizen(input[1]),
    holding: a.holding(-3, 7, 11) }, fields: {}, feePayer: input[0], recentBlockhash: input[1] })));
  assert.equal(out.err, 'HostInTransit');
  assert.equal(out.overview, 0);
});

// ------------------------------------------------------------------ vendored noble (W2-E's tree)

/**
 * `src` with its comments blanked, strings and template literals kept (the
 * vendored files carry `import … from '@noble/…'` examples in doc comments).
 */
function maskComments(src) {
  let out = '';
  for (let i = 0; i < src.length; i++) {
    const c = src[i];
    if (c === '"' || c === "'" || c === '`') {
      const start = i;
      for (i++; i < src.length && src[i] !== c; i++) if (src[i] === '\\') i++;
      out += src.slice(start, i + 1);
    } else if (c === '/' && src[i + 1] === '/') {
      while (i < src.length && src[i] !== '\n') i++;
      out += '\n';
    } else if (c === '/' && src[i + 1] === '*') {
      const end = src.indexOf('*/', i + 2);
      i = end < 0 ? src.length : end + 1;
      out += ' ';
    } else out += c;
  }
  return out;
}

/** Every file under `dir` (URL), relative, '/'-separated. */
function tree(dir, rel = '') {
  return readdirSync(new URL(rel, dir)).flatMap(f => (isDir(new URL(`${rel}${f}/`, dir)) ? tree(dir, `${rel}${f}/`) : [`${rel}${f}`]));
}

test('web/sdk/vendor/noble: the manifest pins every vendored file, and the modules import only each other', t => {
  if (!isDir(noble)) {
    t.skip('web/sdk/vendor/noble is not in this tree yet (scripts/vendor-noble.mjs, W2-E)');
    return;
  }
  assert.ok(existsSync(new URL('manifest.json', noble)), 'web/sdk/vendor/noble has no manifest.json (run node scripts/vendor-noble.mjs)');
  const manifest = JSON.parse(read(noble, 'manifest.json'));
  const listed = Array.isArray(manifest.files) ? Object.fromEntries(manifest.files.map(x => [x.path, x.sha256])) : manifest.files;
  assert.ok(listed && typeof listed === 'object', 'manifest.files');
  const onDisk = tree(noble).filter(f => f !== 'manifest.json').sort();
  assert.deepEqual(Object.keys(listed).sort(), onDisk, 'the manifest lists exactly the files on disk');
  for (const f of onDisk) {
    const hash = createHash('sha256').update(readFileSync(new URL(f, noble))).digest('hex');
    assert.equal(hash, listed[f], `${f}: sha256 differs from the manifest (run node scripts/vendor-noble.mjs)`);
  }
  for (const e of manifest.entries ?? []) assert.ok(onDisk.includes(e), `entry ${e} is vendored`);
  const versions = manifest.packages ?? {};
  if (versions['@noble/curves']) assert.match(versions['@noble/curves'].version, /^1\.9\.\d+$/, '@noble/curves 1.9.x (contract §3.3)');
  if (versions['@noble/hashes']) assert.equal(versions['@noble/hashes'].version, '1.8.0', '@noble/hashes 1.8.0 (contract §3.3)');
  for (const f of onDisk.filter(x => x.endsWith('.mjs'))) {
    const code = maskComments(read(noble, f));
    for (const spec of importsOf(code)) {
      assert.ok(spec.startsWith('./') || spec.startsWith('../'), `vendor/noble/${f} imports ${spec} (bare)`);
      const target = new URL(spec, new URL(f, noble));
      assert.ok(target.href.startsWith(noble.href) && existsSync(target), `vendor/noble/${f} imports ${spec}, which is not in the tree`);
    }
  }
});
