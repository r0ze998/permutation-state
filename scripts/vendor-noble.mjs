#!/usr/bin/env node
// Vendor the pinned @noble/curves and @noble/hashes ESM files the browser
// seal needs (M1 contract §3.3, §11 W2-E; web design §8.3) into
// permutation-server/web/sdk/vendor/noble/, with a manifest of file hashes.
//
// Source: permutation-gateway/node_modules (installed by `npm ci` from the
// gateway's lockfile, so the tarballs are integrity-checked by npm). Only
// the import closure of the entry points below is copied; every file is
// renamed to .mjs (the page has no bundler and node treats .mjs as ESM
// wherever it sits) and every bare `@noble/...` specifier is rewritten to
// a relative path. Nothing is fetched from the network here.
//
//   node scripts/vendor-noble.mjs           write the tree and manifest.json
//   node scripts/vendor-noble.mjs --check   exit 1 unless the tree on disk
//                                           equals what would be written
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, posix, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const MODULES = join(ROOT, 'permutation-gateway', 'node_modules', '@noble');
const OUT = join(ROOT, 'permutation-server', 'web', 'sdk', 'vendor', 'noble');
const LOCK = join(ROOT, 'permutation-gateway', 'package-lock.json');

/** Pinned versions (contract §3.3 expected set: curves 1.9.x, hashes 1.8.0). */
export const PINS = { curves: '1.9.7', hashes: '1.8.0' };
/** What the seal worker imports (package-relative esm paths). */
export const ENTRIES = [
  ['curves', 'bls12-381.js'],
  ['hashes', 'sha2.js'],
  ['hashes', 'utils.js'],
];

const sha256 = b => createHash('sha256').update(b).digest('hex');
const outName = (pkg, file) => posix.join(pkg, file.replace(/\.js$/, '.mjs'));

/** A bare or relative specifier inside `pkg/file` → [pkg, esm-relative file]. */
function resolve(pkg, file, spec) {
  if (spec.startsWith('./') || spec.startsWith('../')) {
    let f = posix.normalize(posix.join(posix.dirname(file), spec));
    if (!f.endsWith('.js')) f += '.js';
    return [pkg, f];
  }
  const m = /^@noble\/(curves|hashes)\/(.+)$/.exec(spec);
  if (!m) throw new Error(`${pkg}/${file}: unexpected import ${spec}`);
  let f = m[2];
  if (!f.endsWith('.js')) f += '.js';
  return [m[1], f];
}

const IMPORT = /(\bfrom\s*|\bimport\s*)(['"])([^'"]+)\2/g;

/** `src` with comments blanked (same length), so examples in doc comments are not imports. */
function maskComments(src) {
  const out = src.split('');
  for (let i = 0; i < src.length; i++) {
    const c = src[i];
    if (c === '"' || c === "'" || c === '`') {
      for (i++; i < src.length && src[i] !== c; i++) if (src[i] === '\\') i++;
      continue;
    }
    if (c === '/' && src[i + 1] === '/') { for (; i < src.length && src[i] !== '\n'; i++) out[i] = ' '; continue; }
    if (c === '/' && src[i + 1] === '*') {
      const e = src.indexOf('*/', i + 2);
      const end = e < 0 ? src.length : e + 2;
      for (; i < end; i++) if (out[i] !== '\n') out[i] = ' ';
      i--;
    }
  }
  return out.join('');
}

/** Rewrite the import specifiers of `src` (code only, never comments). */
function rewriteImports(src, fn) {
  const masked = maskComments(src);
  let out = '', last = 0;
  for (const m of masked.matchAll(IMPORT)) {
    const at = m.index + m[1].length + 1;
    out += src.slice(last, at) + fn(src.slice(at, at + m[3].length));
    last = at + m[3].length;
  }
  return out + src.slice(last);
}

/** The files to write: Map(outPath → text). */
export function build() {
  for (const [pkg, want] of Object.entries(PINS)) {
    const pj = JSON.parse(readFileSync(join(MODULES, pkg, 'package.json'), 'utf8'));
    if (pj.version !== want) throw new Error(`@noble/${pkg} is ${pj.version}, pinned ${want}: run npm ci in permutation-gateway`);
  }
  const files = new Map();
  const todo = [...ENTRIES];
  const seen = new Set();
  while (todo.length) {
    const [pkg, file] = todo.pop();
    const id = `${pkg}/${file}`;
    if (seen.has(id)) continue;
    seen.add(id);
    const src = readFileSync(join(MODULES, pkg, 'esm', file), 'utf8');
    const text = rewriteImports(src.replace(/\n\/\/# sourceMappingURL=.*$/m, ''), spec => {
      const [p, f] = resolve(pkg, file, spec);
      todo.push([p, f]);
      let rel = posix.relative(posix.dirname(outName(pkg, file)), outName(p, f));
      if (!rel.startsWith('.')) rel = `./${rel}`;
      return rel;
    });
    files.set(outName(pkg, file), `// vendored from @noble/${pkg}@${PINS[pkg]} esm/${file} by scripts/vendor-noble.mjs — do not edit\n${text}`);
  }
  for (const pkg of Object.keys(PINS)) files.set(`${pkg}/LICENSE`, readFileSync(join(MODULES, pkg, 'LICENSE'), 'utf8'));
  const lock = JSON.parse(readFileSync(LOCK, 'utf8'));
  const packages = {};
  for (const pkg of Object.keys(PINS)) {
    const l = lock.packages?.[`node_modules/@noble/${pkg}`] ?? {};
    packages[`@noble/${pkg}`] = { version: PINS[pkg], resolved: l.resolved ?? null, integrity: l.integrity ?? null };
  }
  const manifest = {
    generator: 'scripts/vendor-noble.mjs',
    note: 'The seal worker imports curves/bls12-381.mjs and hashes/sha2.mjs; every file below is pinned by its sha256.',
    packages,
    entries: ENTRIES.map(([p, f]) => outName(p, f)),
    files: Object.fromEntries([...files.keys()].sort().map(k => [k, sha256(files.get(k))])),
  };
  files.set('manifest.json', `${JSON.stringify(manifest, null, 2)}\n`);
  return files;
}

function onDisk() {
  const out = new Map();
  if (!existsSync(OUT)) return out;
  const walk = dir => {
    for (const name of readdirSync(dir)) {
      const p = join(dir, name);
      if (statSync(p).isDirectory()) walk(p);
      else out.set(relative(OUT, p).split('\\').join('/'), readFileSync(p, 'utf8'));
    }
  };
  walk(OUT);
  return out;
}

function main() {
  const check = process.argv.includes('--check');
  const want = build();
  if (check) {
    const have = onDisk();
    const bad = [];
    for (const [k, v] of want) if (have.get(k) !== v) bad.push(have.has(k) ? `changed: ${k}` : `missing: ${k}`);
    for (const k of have.keys()) if (!want.has(k)) bad.push(`extra: ${k}`);
    if (bad.length) {
      console.error(`vendor/noble is stale (run node scripts/vendor-noble.mjs):\n  ${bad.join('\n  ')}`);
      process.exit(1);
    }
    console.log(`vendor/noble fresh: ${want.size} files`);
    return;
  }
  rmSync(OUT, { recursive: true, force: true });
  let bytes = 0;
  for (const [k, v] of want) {
    const p = join(OUT, k);
    mkdirSync(dirname(p), { recursive: true });
    writeFileSync(p, v);
    bytes += Buffer.byteLength(v);
  }
  console.log(`vendor/noble: ${want.size} files, ${bytes} bytes`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main();
