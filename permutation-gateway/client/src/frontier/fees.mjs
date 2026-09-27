// Fee and priority formulas of the Frontier (contract §10.1, pinned; kernel
// `permutation_rules::frontier::fees`, fclient `fees`). Same integer
// arithmetic as the kernel (bigints; ceil where the kernel uses div_ceil),
// checked against the shared vectors in test/frontier-vectors.json.
//
//   cost      = cu_limit + 720·n_sig + 300·n_write_locks + 8·⌈L / 32,768⌉
//   priority  = (priority_fee + 2,500) / cost
//   fee(p)    = max(0, ⌈p·cost⌉ − 2,500);  cu_price_µl = ⌈fee·10⁶ / cu_limit⌉
//   tip_min   = ⌈p_min × (reveal_cu_limit + 1,320 + 8·⌈L_reveal / 32,768⌉)⌉ + 2,500
//
// Priorities are in milli (433 = 0.433 lamports per cost unit), as the
// Season stores `min_reveal_priority_milli`.

export const SIG_COST = 720n;
export const WRITE_LOCK_COST = 300n;
export const LOADED_COST_PER_UNIT = 8n;
export const LOADED_UNIT = 32_768n;
export const LOADED_PER_ACCOUNT = 64n;
export const LOADED_PROGRAMDATA_EXTRA = 45n;
export const LOADED_MAX = 64n * 1024n * 1024n;
export const LOADED_DEFAULT = 1024 * 1024;
export const PRIORITY_BASE_LAMPORTS = 2_500n;
export const REVEAL_SIGS = 1;
export const REVEAL_WRITES = 2;
/** The base fee per signature (lamports); sponsored shapes pay no priority fee (CU price 0). */
export const LAMPORTS_PER_SIGNATURE = 5_000n;
/** Rent (§4.2): `(128 + size) × 5,080` lamports. */
export const RENT_PER_BYTE = 5_080n;
export const ACCOUNT_OVERHEAD = 128n;

const B = v => BigInt(v);
const ceilDiv = (a, b) => (a + b - 1n) / b;
const U64_MAX = (1n << 64n) - 1n;
const cap64 = v => (v > U64_MAX ? U64_MAX : v);

/** Rent-exempt minimum of an account of `size` data bytes. */
export const rent = size => (ACCOUNT_OVERHEAD + B(size)) * RENT_PER_BYTE;

export const loadedUnits = loaded => ceilDiv(B(loaded), LOADED_UNIT);

/** `cost = limit + 720·sigs + 300·writes + 8·⌈loaded / 32,768⌉`. */
export const cost = (limit, sigs, writes, loaded) => B(limit) + SIG_COST * B(sigs) + WRITE_LOCK_COST * B(writes) + LOADED_COST_PER_UNIT * loadedUnits(loaded);

/** `(fee + 2,500) · 1,000 / cost` (floor), the priority a fee buys. */
export const priorityMilli = (fee, c) => (B(c) === 0n ? 0n : cap64(((B(fee) + PRIORITY_BASE_LAMPORTS) * 1_000n) / B(c)));

/** The priority fee that reaches `pMilli` at `cost`: `max(0, ⌈p·cost⌉ − 2,500)`. */
export function feeForPriority(pMilli, c) {
  const need = ceilDiv(B(pMilli) * B(c), 1_000n);
  return cap64(need > PRIORITY_BASE_LAMPORTS ? need - PRIORITY_BASE_LAMPORTS : 0n);
}

/** µlamports per CU that pay `fee` at `cuLimit`: `⌈fee·10⁶ / cu_limit⌉`. */
export const cuPriceMicro = (fee, cuLimit) => (B(cuLimit) === 0n ? 0n : cap64(ceilDiv(B(fee) * 1_000_000n, B(cuLimit))));

/** The priority fee the runtime charges at `priceMicro` and `cuLimit`: `⌈price·limit / 10⁶⌉`. */
export const feeOfPrice = (priceMicro, cuLimit) => cap64(ceilDiv(B(priceMicro) * B(cuLimit), 1_000_000n));

/** `L = round_up(programdata_len + 45 + Σ(data) + 64·n, 32 KiB)`, capped at 64 MiB (I-45). */
export function loadedLimit(programdataLen, accountBytes, nAccounts) {
  const sum = B(programdataLen) + LOADED_PROGRAMDATA_EXTRA + B(accountBytes) + LOADED_PER_ACCOUNT * B(nAccounts);
  const r = ceilDiv(sum, LOADED_UNIT) * LOADED_UNIT;
  return Number(r > LOADED_MAX ? LOADED_MAX : r);
}

/** The deploy `--max-len`: `round_up(1.25 × .so, 4,096)`. */
export const deployMaxLen = soLen => Number(ceilDiv(ceilDiv(B(soLen) * 5n, 4n), 4_096n) * 4_096n);

/** A Reveal's cost (1 signature, 2 write locks). */
export const revealCost = (limit, loaded) => cost(limit, REVEAL_SIGS, REVEAL_WRITES, loaded);

/** `tip_min = ⌈p × reveal cost⌉ + 2,500` (I-08). */
export const minTipLamports = (pMilli, limit, loaded) => cap64(ceilDiv(B(pMilli) * revealCost(limit, loaded), 1_000n) + PRIORITY_BASE_LAMPORTS);

/** The priority (milli) a tip buys when a keeper spends all of it: `(tip − 2,500) · 1,000 / cost` (floor). */
export function tipPriorityMilli(tip, limit, loaded) {
  const c = revealCost(limit, loaded);
  if (c === 0n) return 0n;
  const t = B(tip) > PRIORITY_BASE_LAMPORTS ? B(tip) - PRIORITY_BASE_LAMPORTS : 0n;
  return (t * 1_000n) / c;
}

/**
 * ClaimDefence's refund for one Reveal (§5.12, v1.2):
 * `min(⌈price·limit / 10⁶⌉, ⌊cap·cost⌋ − 2,500) − (tip_min − 2,500)` if positive,
 * `cost = limit + 720 + 300·(2 + created_day) + 8·⌈loaded / 32,768⌉`.
 * `ev = {priceMicro, limit, loaded, createdDay}`, `s = {defenceCapMilli, tipMin}`.
 */
export function defenceRefund(ev, s) {
  const c = cost(ev.limit, 1, 2 + (ev.createdDay ? 1 : 0), ev.loaded);
  const paid = feeOfPrice(ev.priceMicro, ev.limit);
  const raw = (B(s.defenceCapMilli) * c) / 1_000n;
  const capped = cap64(raw > PRIORITY_BASE_LAMPORTS ? raw - PRIORITY_BASE_LAMPORTS : 0n);
  const spent = paid < capped ? paid : capped;
  const base = B(s.tipMin) > PRIORITY_BASE_LAMPORTS ? B(s.tipMin) - PRIORITY_BASE_LAMPORTS : 0n;
  return spent > base ? spent - base : 0n;
}

/**
 * The three sponsored Depart tips (§8.3, I-51): `tip_min`, `⌈1.5 × tip_min⌉`,
 * `2 × tip_min`. The relay refuses any other tip (`TipNotPreset`); the
 * march composer offers exactly these (§9.4).
 */
export const tipPresets = tipMin => [B(tipMin), ceilDiv(B(tipMin) * 3n, 2n), B(tipMin) * 2n];

/** `tip_min` of a decoded Season (codec.mjs `decodeAccount('Season', …)`). */
export const seasonTipMin = s => minTipLamports(s.MIN_REVEAL_PRIORITY_MILLI, s.REVEAL_CU_LIMIT, s.REVEAL_LOADED_LIMIT);

/** A Depart's escrow at `tip` (moved from `payer` into the Holding, §5.11): `tip + march_fee + seal_bond`. */
export const departEscrow = (s, tip) => B(tip) + B(s.MARCH_FEE) + B(s.SEAL_BOND);

/** The base fee of a transaction with `sigs` signatures at CU price 0. */
export const baseFee = sigs => LAMPORTS_PER_SIGNATURE * B(sigs);
