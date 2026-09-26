// The key each member was seated with (permutation-chain seat.rs): a later
// member that registered someone's session key again is seated with a
// substitute nobody can sign with, so its talk cannot be signed with that
// key; POST /talk (and scripts/verify-talk.mjs) verify against the seated key.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { Keypair } from '@solana/web3.js';
import { Writer } from '../client/src/borsh.mjs';
import { toHex } from '../client/src/bytes.mjs';
import { seatedKeyMap, seatKeys, seatsFromRecords, substituteKey } from '../src/seats.mjs';
import { signTalk, talkBytes } from '../src/talk.mjs';
import { call, gateway } from './gateway-fixtures.mjs';

const sha = (...parts) => new Uint8Array(createHash('sha256').update(Buffer.concat(parts.map(p => Buffer.from(p)))).digest());
const le64 = n => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const DOMAIN = Buffer.from('permutation-state/duplicate-session');
const b58 = k => k.publicKey.toBase58();

/** A PS_SEAT record as the state file keeps it: borsh Vec<(civ u16, key [32], stand u8, votes [u32; 4])>, hex. */
const seatRecord = seats => ({ members: toHex(new Writer().vec(seats, (w, s) => { w.u16(s.civ).fixed(s.key, 32).u8(0); for (let i = 0; i < 4; i++) w.u32(0xffffffff); }).toBytes()) });

test('seatKeys replays seat.rs: the first holder keeps a session key; a later one gets sha256(domain ‖ season ‖ wallet), hashed on while that is taken', () => {
  const k = Keypair.generate();
  const [wA, wB, wC, wD] = [0, 1, 2, 3].map(() => Keypair.generate());
  const sub = w => sha(DOMAIN, le64(42), w.publicKey.toBytes());
  assert.deepEqual(substituteKey(42n, wB.publicKey.toBase58()), sub(wB));
  const members = [
    { index: 0, wallet: b58(wA), session: b58(k) },
    { index: 1, wallet: b58(wB), session: b58(k) }, // a copy of member 0's key
    { index: 2, wallet: b58(wC), session: sub(wD) }, // registered exactly member 3's first substitute
    { index: 3, wallet: b58(wD), session: b58(k) }, // a copy again: its first substitute is taken
  ];
  const keys = seatKeys([...members].reverse(), 42n);
  assert.deepEqual(keys.get(0), k.publicKey.toBytes());
  assert.deepEqual(keys.get(1), sub(wB));
  assert.deepEqual(keys.get(2), sub(wD));
  assert.deepEqual(keys.get(3), sha(DOMAIN, le64(42), wD.publicKey.toBytes(), sub(wD)));
});

test('seatedKeyMap: the PS_SEAT records\' keys once they list every member; the replay before (or when they do not)', () => {
  const [a, b] = [Keypair.generate(), Keypair.generate()];
  const members = [{ index: 0, wallet: b58(Keypair.generate()), session: b58(a) }, { index: 1, wallet: b58(Keypair.generate()), session: b58(b) }];
  const recorded = Keypair.generate().publicKey.toBytes();
  const seating = [seatRecord([{ civ: 0, key: a.publicKey.toBytes() }]), seatRecord([{ civ: 1, key: recorded }])];
  assert.deepEqual(seatsFromRecords(seating).map(s => s.civ), [0, 1]);
  assert.deepEqual(seatedKeyMap({ members, seasonId: 42n, seating }).get(1), recorded, 'what the program logged');
  assert.deepEqual(seatedKeyMap({ members, seasonId: 42n, seating: seating.slice(0, 1) }).get(1), b.publicKey.toBytes(), 'seating not complete: replayed');
  assert.deepEqual(seatedKeyMap({ members, seasonId: 42n, seating: [{ members: 'zz' }] }).get(1), b.publicKey.toBytes(), 'an unreadable record: replayed');
});

test('POST /talk: a member that registered someone\'s session key cannot be spoken for with it; the first holder can', async () => {
  const k = Keypair.generate();
  const [wA, wB] = [Keypair.generate(), Keypair.generate()];
  const members = [{ index: 0, civ: 0, wallet: b58(wA), session: b58(k) }, { index: 1, civ: 1, wallet: b58(wB), session: b58(k) }];
  const g = gateway({ registryMembers: members });
  const say = (member, key, text) => {
    const signature = toHex(signTalk(talkBytes({ season: 42n, tick: 0, member, to: null, text }), key));
    return call(g.public, 'POST', '/talk', { body: { member, tick: 0, text, signature } });
  };
  const own = await say(0, k, 'hello');
  assert.equal(own.status, 200, JSON.stringify(own.json));
  const spoof = await say(1, k, 'I am member 1');
  assert.deepEqual([spoof.status, spoof.json.code], [400, 'TalkRefused'], 'K is member 0\'s key in the world, not member 1\'s');
  assert.deepEqual(g.store.state.talk.map(m => m.member), [0]);
});

test('POST /talk: with the world seated, the key the PS_SEAT records list is the one that signs', async () => {
  const [s0, s1, seated1] = [Keypair.generate(), Keypair.generate(), Keypair.generate()];
  const members = [{ index: 0, civ: 0, wallet: b58(Keypair.generate()), session: b58(s0) }, { index: 1, civ: 1, wallet: b58(Keypair.generate()), session: b58(s1) }];
  const seating = [seatRecord([{ civ: 0, key: s0.publicKey.toBytes() }, { civ: 1, key: seated1.publicKey.toBytes() }])];
  const g = gateway({ registryMembers: members, state: { seating } });
  const say = (key, text) => call(g.public, 'POST', '/talk', { body: { member: 1, tick: 0, text, signature: toHex(signTalk(talkBytes({ season: 42n, tick: 0, member: 1, to: null, text }), key)) } });
  assert.equal((await say(seated1, 'seated key')).status, 200);
  assert.equal((await say(s1, 'registry session')).json.code, 'TalkRefused');
});
