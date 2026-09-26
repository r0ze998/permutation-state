// What a wallet can claim, for pages that must not read the RPC themselves:
// its members of this gateway's season and of every season this one follows
// (the lineage in the state file), read from the base layer in one request.
//
//   GET /claims?wallet=<base58>
//     → {claims: [{seasonId, member, civ, name, amount, claimed, status}]}, newest season first:
//       seasonId as a decimal string; amount (base units, decimal string) = the prize plus the
//       share of the nation's treasury (codec.mjs claimParts, the program's claim_amount; 0
//       until the season is Finalized); status Registering | Genesis | Seating | Running |
//       Finalized. Seasons the wallet was not a member of are not listed. Cached 5 s per
//       wallet (a claim relayed through this gateway refreshes it).
//     400 InvalidOwner: `wallet` is not a public key
import { claimParts, decodeMember, decodeSeason } from '../../client/src/codec.mjs';
import { ownerKey } from './faucet.mjs';

/** The wallet's (PublicKey) members of `seasons` (ChainClients), as GET /claims lists them. */
export async function walletClaims(base, seasons, wallet) {
  const list = [...seasons].sort((a, b) => (a.seasonId > b.seasonId ? -1 : a.seasonId < b.seasonId ? 1 : 0));
  const infos = await base.getMultipleAccountsInfo(list.flatMap(c => [c.season, c.member(wallet)]), 'confirmed');
  const claims = [];
  list.forEach((c, i) => {
    const [s, m] = [infos[2 * i], infos[2 * i + 1]];
    if (!s || !m) return;
    let season, member;
    try {
      season = decodeSeason(s.data);
      member = decodeMember(m.data);
    } catch {
      return; // not an account of this program's layout: nothing to claim there
    }
    claims.push({ seasonId: c.seasonId.toString(), member: member.index, civ: member.civ, name: member.name,
      amount: claimParts(season, member).total.toString(), claimed: member.claimed, status: season.status });
  });
  return claims;
}

export const claimsRoutes = {
  'GET /claims': async (ctx, req) => {
    const wallet = ownerKey(req.url.searchParams.get('wallet'), 'wallet');
    const claims = await ctx.claims.get(wallet.toBase58(), () => walletClaims(ctx.base, ctx.claimSeasons().values(), wallet));
    return { body: { claims } };
  },
};
