// The Split page's drag bar (PLAN.md §14.3): one coloured segment per
// slice, Everything else last, with a handle between each pair of
// neighbours. Dragging the handle between A and B trades only between A and
// B (their sum stays), so nothing else jumps; a rule keeps 1%, Everything
// else 5% (split.ts). The bar follows the pointer, then settles on a step of
// 5% for A on release. Pure, so it's tested; SplitBar.svelte draws it.

import { STEP, barShares, isSlice, sameTarget, withBar } from "./rules";
import { TOTAL, moveDivider } from "./split";
import type { Rule } from "./types";

/** The handle's hit area: wider than the cream grip, so it's easy to catch. */
export const HIT_PX = 32;

/** A share settled on the nearest step of 5%. */
function snapped(bp: number): number {
  return Math.round(bp / STEP) * STEP;
}

/** Where each handle sits, as % of the bar from the left (handle `i` is
 *  between segment `i` and `i + 1`). */
export function handlePositions(shares: number[]): number[] {
  const out: number[] = [];
  let at = 0;
  for (const s of shares.slice(0, -1)) {
    at += s;
    out.push((at / TOTAL) * 100);
  }
  return out;
}

/** Move handle `i` to `pct` % of the bar (from the left); only its two
 *  neighbours change, each keeping its floor. `snap` settles the left one
 *  on a step of 5%, as on release. */
export function dragBetween(rules: Rule[], i: number, pct: number, snap = false): Rule[] {
  const shares = barShares(rules);
  if (i < 0 || i >= shares.length - 1) return rules;
  const before = shares.slice(0, i).reduce((a, b) => a + b, 0);
  const a = shares[i] ?? 0;
  let want = pct * 100 - before;
  if (snap) want = snapped(want);
  return withBar(rules, moveDivider(shares, i, want - a));
}

/** Handle `i` by keyboard: `steps` steps of 5% (negative shrinks the left
 *  neighbour), landing on a multiple of 5 like − / +. */
export function nudgeBetween(rules: Rule[], i: number, steps: number): Rule[] {
  const shares = barShares(rules);
  const a = shares[i];
  if (a === undefined || i >= shares.length - 1 || steps === 0) return rules;
  const base = steps > 0 ? Math.floor(a / STEP) * STEP : Math.ceil(a / STEP) * STEP;
  return withBar(rules, moveDivider(shares, i, base + steps * STEP - a));
}

/** Home / End: the left neighbour as small, or as big, as it can be. */
export function edgeBetween(rules: Rule[], i: number, end: "min" | "max"): Rule[] {
  const shares = barShares(rules);
  if (i < 0 || i >= shares.length - 1) return rules;
  return withBar(rules, moveDivider(shares, i, end === "min" ? -TOTAL : TOTAL));
}

/** Which handle a press at `x` px catches: the nearest within half the hit
 *  area. Where two overlap (a slice of a few %), the first move decides:
 *  leftwards takes the left one, rightwards the right one, so a thin slice
 *  can always grow. With `dx` 0 and an overlap, null: wait for a move. */
export function pickHandle(xs: number[], x: number, dx = 0): number | null {
  const near = xs.map((h, i) => ({ i, d: Math.abs(h - x) })).filter((h) => h.d <= HIT_PX / 2);
  if (near.length === 0) return null;
  if (near.length === 1) return near[0]?.i ?? null;
  if (dx < 0) return near[0]?.i ?? null;
  if (dx > 0) return near[near.length - 1]?.i ?? null;
  return null;
}

/** How much of its label a segment `px` wide can show: logo, name and %;
 *  just the %; or nothing (its row below says it). */
export type Fit = "full" | "pct" | "none";

/** Room for logo + a few letters of the name + "40%", and for "40%" alone. */
export const FULL_PX = 128;
export const PCT_PX = 44;

export function fitLabel(px: number): Fit {
  return px >= FULL_PX ? "full" : px >= PCT_PX ? "pct" : "none";
}

/** Where the handles stood when editing started, for the faint "was"
 *  ticks; null when the slices themselves changed (added, removed, turned
 *  off), since then the old handles don't line up with the new ones, or
 *  when nothing moved. */
export function savedTicks(base: Rule[], rules: Rule[]): number[] | null {
  const a = base.filter(isSlice);
  const b = rules.filter(isSlice);
  if (a.length !== b.length || a.some((r, k) => !sameTarget(r.target, (b[k] as Rule).target))) return null;
  const was = handlePositions(barShares(base));
  const now = handlePositions(barShares(rules));
  return was.some((p, k) => Math.abs(p - (now[k] ?? p)) >= 0.5) ? was : null;
}
