// The client package: its subpath exports are exactly the modules index.mjs re-exports.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const pkg = JSON.parse(readFileSync(new URL('../client/package.json', import.meta.url), 'utf8'));
const index = readFileSync(new URL('../client/src/index.mjs', import.meta.url), 'utf8');

test('package.json exports match index.mjs', async () => {
  const reexported = [...index.matchAll(/from '\.\/([\w-]+)\.mjs'/g)].map(m => m[1]).sort();
  const subpaths = Object.entries(pkg.exports).filter(([k]) => k !== '.');
  for (const [k, v] of subpaths) assert.equal(v, `./src/${k.slice(2)}.mjs`, k);
  assert.deepEqual(subpaths.map(([k]) => k.slice(2)).sort(), [...new Set(reexported)]);
  assert.equal(pkg.exports['.'], './src/index.mjs');
  const mod = await import('../client/src/index.mjs');
  for (const name of ['GameClient', 'HttpError', 'officeOf', 'loadOrCreateKeypair', 'ChainClient', 'IX', 'IX_TAG', 'claimAmount', 'chainError', 'TOOLS']) assert.ok(name in mod, name);
});
