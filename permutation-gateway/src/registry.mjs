// The member registry: every Member PDA of the season on chain. Public: it
// never says which members this gateway hosts or which are the operator's
// AI members (V5 §18.2; the game server asks /operator/roster). Cached briefly.
import { PublicKey } from '@solana/web3.js';
import { MEMBER_KINDS } from '../client/src/codec.mjs';
import { seasonMembers } from './season.mjs';

const b58 = k => new PublicKey(k).toBase58();

export function createMemberRegistry({ base, chain, store, ttlMs = 2000, now = Date.now, fetchMembers = seasonMembers }) {
  let cache = { at: -Infinity, members: [] };
  return {
    async list() {
      if (now() - cache.at < ttlMs) return cache.members;
      const onChain = await fetchMembers(base, chain);
      cache = {
        at: now(),
        members: onChain.map(m => {
          return { index: m.index, civ: m.civ, name: m.name, kind: MEMBER_KINDS[m.kind] ?? 'undeclared',
            attested: m.attestation.some(b => b !== 0), wallet: b58(m.wallet), session: b58(m.session),
            stand: m.stand, shares: m.shares, claimed: m.claimed, tag: Buffer.from(m.tag).toString('hex') };
        }),
      };
      return cache.members;
    },
    invalidate() { cache.at = -Infinity; },
  };
}
