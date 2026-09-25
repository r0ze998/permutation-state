// Axial hex coordinates, as the game server reports them: `{q, r}` objects or `[q, r]` pairs.

const qr = h => (Array.isArray(h) ? h : [h.q, h.r]);

/** Steps between two hexes. */
export function hexDist(a, b) {
  const [aq, ar] = qr(a);
  const [bq, br] = qr(b);
  return (Math.abs(aq - bq) + Math.abs(ar - br) + Math.abs(aq + ar - bq - br)) / 2;
}
