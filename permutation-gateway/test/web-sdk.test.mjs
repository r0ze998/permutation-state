// The web client's copy of the SDK core (permutation-server/web/sdk,
// written by scripts/sync-web-sdk.mjs): exactly the browser-safe set, each
// file its client/src source under the generated-file header, free of
// node:*, Buffer and bare imports, importing only itself, and working in a
// runtime without Buffer.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import { HEADER, WEB_SDK } from '../scripts/sync-web-sdk.mjs';

const src = new URL('../client/src/', import.meta.url);
const web = new URL('../../permutation-server/web/sdk/', import.meta.url);
const read = (dir, f) => readFileSync(new URL(f, dir), 'utf8');
const SYNC = 'node permutation-gateway/scripts/sync-web-sdk.mjs';

test('web/sdk holds exactly the browser-safe set, each file its source under the header', () => {
  assert.deepEqual(readdirSync(web).sort(), [...WEB_SDK].sort(), `web/sdk has other files than the set (run ${SYNC})`);
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
