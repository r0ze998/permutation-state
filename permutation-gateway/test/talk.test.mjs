// The talk Merkle tree (src/talk.mjs): every proof merkleProof gives
// verifies against merkleRoot, whatever the number of leaves (odd nodes are
// carried up without a sibling).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { merkleProof, merkleRoot, nextLevel } from '../src/talk.mjs';

const sha256 = (...parts) => { const h = createHash('sha256'); for (const p of parts) h.update(p); return new Uint8Array(h.digest()); };

/** Fold a proof from `leaf` up: a left sibling goes first, as merkleRoot pairs them. */
function verifyProof(leaf, proof, root) {
  let node = leaf;
  for (const { hash, left } of proof) {
    const sib = Buffer.from(hash, 'hex');
    node = left ? sha256(Buffer.from([1]), sib, node) : sha256(Buffer.from([1]), node, sib);
  }
  return Buffer.from(node).equals(Buffer.from(root));
}

test('every leaf\'s proof verifies against the root, for 1..9 leaves', () => {
  for (let n = 1; n <= 9; n++) {
    const leaves = Array.from({ length: n }, (_, i) => sha256(Buffer.from([0, i])));
    const root = merkleRoot(leaves);
    for (let i = 0; i < n; i++) assert.ok(verifyProof(leaves[i], merkleProof(leaves, i), root), `${n} leaves, leaf ${i}`);
    // A proof of one leaf does not verify another.
    if (n > 1) assert.ok(!verifyProof(leaves[1], merkleProof(leaves, 0), root), `${n} leaves: proof of 0 for leaf 1`);
  }
});

test('merkleRoot: none is 32 zero bytes, one is the leaf; nextLevel carries an odd node up', () => {
  assert.deepEqual(merkleRoot([]), new Uint8Array(32));
  const a = sha256(Buffer.from('a')), b = sha256(Buffer.from('b')), c = sha256(Buffer.from('c'));
  assert.deepEqual(merkleRoot([a]), a);
  const up = nextLevel([a, b, c]);
  assert.deepEqual(up, [sha256(Buffer.from([1]), a, b), c]);
});
