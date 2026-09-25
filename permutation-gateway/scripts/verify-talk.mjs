// Check the members' messages of a season against the chain (V5 §18.7):
// every message is signed by its member's session key (read from the Member
// account on the base layer, not from the gateway), and every tick's
// messages hash to the Merkle root the program logged (`PS_TALK`, from the
// `AnchorTalk` transaction on the ER). Words bind nothing, but who said what
// by when is provable.
//
//   node scripts/verify-talk.mjs [--gateway http://127.0.0.1:4191] [--base URL] [--er URL]
import { Connection, PublicKey } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { decodeMember } from '../client/src/codec.mjs';
import { verifyTalk } from '../client/src/talk.mjs';
import { DEFAULTS, parseArgs } from '../src/config.mjs';
import { records } from '../src/send.mjs';
import { TalkBook } from '../src/talk.mjs';

const args = parseArgs(process.argv.slice(2));
const gateway = (args.gateway ?? DEFAULTS.gatewayUrl).replace(/\/$/, '');
const get = async p => (await fetch(`${gateway}${p}`)).json();
const info = await get('/season');
const base = new Connection(args.base ?? info.endpoints.base, 'confirmed');
const er = new Connection(args.er ?? info.endpoints.er, 'confirmed');
const chain = new ChainClient(info.programId, BigInt(info.season.seasonId));
const { messages } = await get('/talk');

let bad = 0;
const check = (ok, what) => { console.log(`${ok ? '✓' : '✗'} ${what}`); if (!ok) bad++; };

// Session keys from the Member accounts on chain.
const session = new Map();
for (const m of info.members) {
  const acc = await base.getAccountInfo(chain.member(new PublicKey(m.wallet)), 'confirmed');
  if (acc) session.set(m.index, decodeMember(acc.data).session);
}
const signed = messages.filter(m => verifyTalk(Buffer.from(m.bytes, 'hex'), Buffer.from(m.signature, 'hex'), session.get(m.member) ?? new Uint8Array(32)));
check(signed.length === messages.length, `${signed.length}/${messages.length} messages signed by their member's session key`);

// Per tick: the root over the tick's messages equals the anchored one.
const byTick = new Map();
for (const m of messages) byTick.set(m.tick, [...(byTick.get(m.tick) ?? []), m]);
let anchored = 0;
for (const [tick, ms] of [...byTick].sort((a, b) => a[0] - b[0])) {
  const sig = ms[0].anchored?.signature;
  if (!sig) { console.log(`· tick ${tick}: ${ms.length} messages not anchored yet`); continue; }
  const t = await er.getTransaction(sig, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 });
  const rec = records(t?.meta?.logMessages ?? []).find(r => r.tag === 'PS_TALK' && r.tick === tick);
  const root = Buffer.from(TalkBook.root(ms)).toString('hex');
  const ok = !!rec && rec.count === ms.length && Buffer.from(rec.root).toString('hex') === root && rec.seasonId === BigInt(info.season.seasonId);
  check(ok, `tick ${tick}: ${ms.length} messages hash to the anchored root ${root.slice(0, 16)}… (${sig.slice(0, 12)}…)`);
  anchored += ok;
}
console.log(bad ? `FAILED: ${bad} checks` : `VERIFIED: ${messages.length} messages, ${anchored} anchored ticks`);
process.exit(bad ? 1 : 0);
