// Split-bar arithmetic in basis points, mirroring snailnet-core's
// scheduler::shares (PLAN.md §9.7, §14.3). A share list has one entry per
// rule, in order, then "Everything else" last; it always sums to TOTAL and
// Everything else never drops below FLOOR.

export const TOTAL = 10_000;
/** Everything else keeps 5%: it carries DNS, Windows and new apps. */
export const FLOOR = 500;
/** The bar never shrinks a rule below 1% (0 = "only when free" is set in
 *  the rule editor, not by dragging a slice out of existence). */
export const MIN_RULE = 100;

/** Integer parts of `amount` proportional to `weights` (largest remainder,
 *  ties to the lower index), summing to exactly `amount`. */
export function apportion(amount: number, weights: number[]): number[] {
  const total = weights.reduce((a, b) => a + b, 0);
  if (total === 0) return weights.map(() => 0);
  const parts = weights.map((w) => Math.floor((amount * w) / total));
  let left = amount - parts.reduce((a, b) => a + b, 0);
  const order = weights
    .map((w, i) => ({ i, rem: (amount * w) % total }))
    .sort((a, b) => b.rem - a.rem || a.i - b.i);
  for (const { i } of order) {
    if (left === 0) break;
    parts[i] = (parts[i] ?? 0) + 1;
    left--;
  }
  return parts;
}

/** Rule shares as stored → a full share list (the service's `normalize`). */
export function normalize(rules: number[]): number[] {
  const budget = TOTAL - FLOOR;
  const asked = rules.reduce((a, b) => a + b, 0);
  const out = asked <= budget ? [...rules] : apportion(budget, rules);
  const used = out.reduce((a, b) => a + b, 0);
  return [...out, TOTAL - used];
}

function minOf(shares: number[], i: number): number {
  return i === shares.length - 1 ? FLOOR : MIN_RULE;
}

/** Drag the divider between slice `i` and `i + 1` by `delta` basis points
 *  (positive grows `i`); both neighbours keep their minimum. */
export function moveDivider(shares: number[], i: number, delta: number): number[] {
  const a = shares[i];
  const b = shares[i + 1];
  if (a === undefined || b === undefined) return shares;
  const d = Math.max(minOf(shares, i) - a, Math.min(b - minOf(shares, i + 1), Math.round(delta)));
  const out = [...shares];
  out[i] = a + d;
  out[i + 1] = b - d;
  return out;
}

/** Set rule `i` to `value`; every other slice (Everything else included)
 *  gives or takes in proportion to what it has above its minimum. */
export function setShare(shares: number[], i: number, value: number): number[] {
  const last = shares.length - 1;
  if (i < 0 || i >= last) return shares;
  const others = shares.map((_, j) => j).filter((j) => j !== i);
  const mins = others.map((j) => minOf(shares, j));
  const minSum = mins.reduce((a, b) => a + b, 0);
  const v = Math.max(MIN_RULE, Math.min(TOTAL - minSum, Math.round(value)));
  const spare = TOTAL - v - minSum;
  let weights = others.map((j, k) => Math.max(0, (shares[j] ?? 0) - (mins[k] ?? 0)));
  // Nobody has anything above their minimum: Everything else takes it all.
  if (weights.every((w) => w === 0)) weights = others.map((j) => (j === last ? 1 : 0));
  const extra = apportion(spare, weights);
  const out = [...shares];
  out[i] = v;
  others.forEach((j, k) => (out[j] = (mins[k] ?? 0) + (extra[k] ?? 0)));
  return out;
}

export function isValid(shares: number[]): boolean {
  const sum = shares.reduce((a, b) => a + b, 0);
  return sum === TOTAL && (shares[shares.length - 1] ?? 0) >= FLOOR;
}
