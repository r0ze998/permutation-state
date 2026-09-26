// The member registry: every Member PDA of the season on chain. Public: it
// never says which members are the operator's AI members (V5 §18.2; the
// game server asks /operator/roster), and /season leaves out everyone's
// self-declared kind in a season with AI members. Cached briefly.
import { PublicKey } from '@solana/web3.js';
import { toHex } from '../client/src/bytes.mjs';
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
          // `kind` is the self-declared kind's name (MEMBER_KINDS: 'human', 'agent',
          // 'undeclared'), not the account's u8. In a season with AI members
          // everyone registers 'undeclared' (x402 refuses anything else) and
          // /season does not show it.
          return { index: m.index, civ: m.civ, name: m.name, kind: MEMBER_KINDS[m.kind] ?? 'undeclared',
            attested: m.attestation.some(b => b !== 0), wallet: b58(m.wallet), session: b58(m.session),
            stand: m.stand, shares: m.shares, claimed: m.claimed, tag: toHex(m.tag) };
        }),
      };
      return cache.members;
    },
    invalidate() { cache.at = -Infinity; },
  };
}
