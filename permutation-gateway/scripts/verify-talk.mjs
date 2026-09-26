// Check the members' messages of a season against the chain (V5 §18.7):
// every message is signed by the key its member was seated with in the world
// (its session key, read from the Member account on the base layer, not from
// the gateway; for a member that registered someone else's session key again,
// the substitute the program seated it with, which nobody can sign with:
// src/seats.mjs, cross-checked with the program's PS_SEAT records when the
// gateway lists them), and every tick's messages hash to the Merkle root the
// program logged (`PS_TALK`, from the `AnchorTalk` transaction on the ER).
// Words bind nothing, but who said what by when is provable.
//
//   node scripts/verify-talk.mjs [--gateway http://127.0.0.1:4191] [--base URL] [--er URL]
// (--base / --er are needed when the gateway does not publish its RPC URLs,
// e.g. its public listener without --public-base-rpc / --public-er-rpc)
import { Connection, PublicKey } from '@solana/web3.js';
import { fromHex, toHex } from '../client/src/bytes.mjs';
import { ChainClient } from '../client/src/chain.mjs';
import { decodeMember } from '../client/src/codec.mjs';
import { verifyTalk } from '../client/src/talk-node.mjs';
import { DEFAULTS, parseArgs } from '../src/config.mjs';
import { seatedKeyMap } from '../src/seats.mjs';
import { records } from '../src/send.mjs';
import { TalkBook } from '../src/talk.mjs';

const args = parseArgs(process.argv.slice(2));
const gateway = (args.gateway ?? DEFAULTS.gatewayUrl).replace(/\/$/, '');
const get = async p => (await fetch(`${gateway}${p}`)).json();
const info = await get('/season');
const rpc = (flag, which) => {
  const url = args[flag] ?? info.endpoints?.[which];
  if (!url) { console.error(`error: pass --${flag} (the gateway does not publish its ${which} RPC URL)`); process.exit(2); }
  return url;
};
const base = new Connection(rpc('base', 'base'), 'confirmed');
const er = new Connection(rpc('er', 'er'), 'confirmed');
const seasonId = BigInt(info.season.seasonId);
const chain = new ChainClient(info.programId, seasonId);
const { messages } = await get('/talk');

let bad = 0;
const check = (ok, what) => { console.log(`${ok ? '✓' : '✗'} ${what}`); if (!ok) bad++; };

// Every member's session key and wallet from its Member account on chain,
// then the key each one was seated with (PS_SEAT records re-read from the
// chain when the gateway indexed them; else replayed as the program seats).
const onChain = [];
for (const m of info.members) {
  const acc = await base.getAccountInfo(chain.member(new PublicKey(m.wallet)), 'confirmed');
  if (acc) { const d = decodeMember(acc.data); onChain.push({ index: d.index, wallet: d.wallet, session: d.session }); }
}
const seating = [];
for (const rec of info.seating ?? []) {
  const t = await base.getTransaction(rec.signature, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 }).catch(() => null);
  const seat = records(t?.meta?.logMessages ?? []).find(r => r.tag === 'PS_SEAT' && toHex(r.root) === rec.root);
  if (seat) seating.push({ members: toHex(seat.members) });
}
const seated = seatedKeyMap({ members: onChain, seasonId, seating: seating.length === (info.seating ?? []).length ? seating : [] });
const substituted = onChain.filter(m => toHex(seated.get(m.index)) !== toHex(m.session)).map(m => m.index);
if (substituted.length) console.log(`· members ${substituted.join(', ')} registered a session key someone had already: seated with a substitute key`);
const signed = messages.filter(m => verifyTalk(fromHex(m.bytes), fromHex(m.signature), seated.get(m.member) ?? new Uint8Array(32)));
check(signed.length === messages.length, `${signed.length}/${messages.length} messages signed by the key their member was seated with`);

// Per tick: the root over the tick's messages equals the anchored one.
const byTick = new Map();
for (const m of messages) byTick.set(m.tick, [...(byTick.get(m.tick) ?? []), m]);
let anchored = 0;
for (const [tick, ms] of [...byTick].sort((a, b) => a[0] - b[0])) {
  const sig = ms[0].anchored?.signature;
  if (!sig) { console.log(`· tick ${tick}: ${ms.length} messages not anchored yet`); continue; }
  const t = await er.getTransaction(sig, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 });
  const rec = records(t?.meta?.logMessages ?? []).find(r => r.tag === 'PS_TALK' && r.tick === tick);
  const root = toHex(TalkBook.root(ms));
  const ok = !!rec && rec.count === ms.length && toHex(rec.root) === root && rec.seasonId === BigInt(info.season.seasonId);
  check(ok, `tick ${tick}: ${ms.length} messages hash to the anchored root ${root.slice(0, 16)}… (${sig.slice(0, 12)}…)`);
  anchored += ok;
}
console.log(bad ? `FAILED: ${bad} checks` : `VERIFIED: ${messages.length} messages, ${anchored} anchored ticks`);
process.exit(bad ? 1 : 0);
