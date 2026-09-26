// The talk Merkle tree (src/talk.mjs): every proof merkleProof gives
// verifies against merkleRoot, whatever the number of leaves (odd nodes are
// carried up without a sibling). And the stored message bytes, pinned: they
// are what the anchored roots hash (scripts/verify-talk.mjs).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { Keypair } from '@solana/web3.js';
import { merkleProof, merkleRoot, nextLevel, signTalk, TalkBook, talkBytes, verifyTalk } from '../src/talk.mjs';

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

// Captured from the Buffer-based talkBytes before the SDK core became
// isomorphic: bytes, signature and leaf (the root of a one-message tick).
const PINNED = [
  { season: 1790000000123n, tick: 0, member: 3, to: null, text: 'hello',
    bytes: '7065726d75746174696f6e2d72756c65732f74616c6b7b6c50c4a00100000000030000000068656c6c6f',
    signature: 'c50d2cfdf4a5dbbe856489006bd38a6a10e7402c0aeb7ad8c9d502b765e8083e63b2639cdc9117f796fa3fcbc87750428f10738f3aaaa5c3a678ff1c071f4700',
    leaf: '5b6e93b45b0718d8b51820659e2ad950a2b0aa25138e792703182042d76b01d4' },
  { season: 42n, tick: 17, member: 255, to: { civ: 2 }, text: '同盟を組もう 🤝',
    bytes: '7065726d75746174696f6e2d72756c65732f74616c6b2a000000000000001100ff000000010200e5908ce79b9fe38292e7b584e38282e3818620f09fa49d',
    signature: '10a5220f7ff5085d21cbdaff514f75c9e96698ba2b05ebf051322517a43b37e93647379eafd8fb413508037ff9257120ad30cc9bac37f667acd17a392bcf5f08',
    leaf: '0b69e92661b99b2693cc1b6bbc44f4661e95fd91baf5e674b4e69f9c545de143' },
  { season: 0xffffffffffffffffn, tick: 65535, member: 0xfffffffe, to: { member: 9 }, text: 'x'.repeat(280),
    bytes: `7065726d75746174696f6e2d72756c65732f74616c6bfffffffffffffffffffffeffffff0209000000${'78'.repeat(280)}`,
    signature: 'c42617affe4ed7c9a1fcfcf74236f6fb257c7fec4ca102b9e0fe2d3a792c21dc80f0bb761d09680bb27f677d930876fb46338d05ac6c4c6ad7d1c989ee13ab00',
    leaf: '82afdd2ca4e37b39cab211818b7d3aad29b10352a6a63bf568cde7ce660dbed1' },
];

test('stored talk: bytes, signature and leaf are unchanged (pinned)', () => {
  const key = Keypair.fromSeed(new Uint8Array(32).fill(7));
  for (const p of PINNED) {
    const bytes = talkBytes(p);
    assert.ok(bytes instanceof Uint8Array && !Buffer.isBuffer(bytes), 'talkBytes is a plain Uint8Array');
    assert.equal(Buffer.from(bytes).toString('hex'), p.bytes);
    const signature = signTalk(bytes, key);
    const m = new TalkBook({}).add({ ...p, signature, publicKey: key.publicKey.toBytes() });
    assert.equal(m.bytes, p.bytes, 'the hex TalkBook keeps');
    assert.equal(m.signature, p.signature);
    assert.equal(Buffer.from(TalkBook.root([m])).toString('hex'), p.leaf);
    assert.ok(verifyTalk(Buffer.from(m.bytes, 'hex'), Buffer.from(m.signature, 'hex'), key.publicKey.toBytes()));
  }
  // Out-of-range fields are refused, as Buffer's writers refused them.
  assert.throws(() => talkBytes({ ...PINNED[0], tick: 65536 }), RangeError);
  assert.throws(() => talkBytes({ ...PINNED[0], to: { civ: -1 } }), RangeError);
});
