// The Split page's unsaved copy (PLAN.md §14.3): − / +, the settings in a
// row, Add a slice and Remove change a draft of the mode's rules, not the
// service. Nothing reaches the line until Save; Discard drops it. A draft
// remembers the rules it started from, so it can tell when they changed
// somewhere else meanwhile (the tray, a hotkey, another mode). Pure, so it's
// tested; ui.svelte.ts holds the one draft the window has.

import { barShares, isSlice, sameTarget, shownShare } from "./rules";
import { isValid } from "./split";
import type { Rule } from "./types";

export interface Draft {
  /** The mode (profile id) being edited. */
  profile: string;
  /** Its rules as saved when editing started. */
  base: Rule[];
  /** The rules as edited. */
  rules: Rule[];
}

export function startDraft(profile: string, saved: Rule[]): Draft {
  return { profile, base: saved, rules: saved };
}

/** The draft with `rules` as its new edit. */
export function edit(d: Draft, rules: Rule[]): Draft {
  return { ...d, rules };
}

/** A rule's settings, in a fixed order, for comparing. */
function shape(r: Rule): string {
  return JSON.stringify([
    r.target.kind,
    r.target.id.toLowerCase(),
    r.share,
    r.priority,
    r.enabled,
    r.ceiling?.down ?? null,
    r.ceiling?.up ?? null,
  ]);
}

/** Whether two rule lists are the same split (key order and a missing vs
 *  empty ceiling don't count). */
export function sameRules(a: Rule[], b: Rule[]): boolean {
  return a.length === b.length && a.every((r, i) => shape(r) === shape(b[i] as Rule));
}

/** Whether the draft differs from what it started from. */
export function isDirty(d: Draft | null): boolean {
  return d !== null && !sameRules(d.base, d.rules);
}

/** Whether the saved rules moved on since the draft started: the mode
 *  switched, or its rules changed somewhere else. */
export function isStale(d: Draft | null, profile: string | null, saved: Rule[]): boolean {
  return d !== null && (d.profile !== profile || !sameRules(d.base, saved));
}

/** Whether the draft can be saved: shares that add up, Everything else
 *  keeping its floor (§9.7). The rules.ts edits keep this; this is the
 *  last check before it goes out. */
export function isSavable(d: Draft): boolean {
  return isValid(barShares(d.rules));
}

/** What the slice for rule `i` of the draft had when editing started, in
 *  basis points as the bar showed it; null when it had no slice then
 *  (new, off, or only when free) or it's unchanged. `i` -1 is Everything
 *  else. For the "was 20%" beside a changed share. */
export function wasShare(d: Draft, i: number): number | null {
  const now = i < 0 ? (barShares(d.rules).at(-1) ?? null) : shownShare(d.rules, i);
  let before: number | null;
  if (i < 0) {
    before = barShares(d.base).at(-1) ?? null;
  } else {
    const r = d.rules[i];
    const j = r ? d.base.findIndex((b) => sameTarget(b.target, r.target)) : -1;
    const b = j >= 0 ? d.base[j] : undefined;
    before = b && isSlice(b) ? shownShare(d.base, j) : null;
  }
  return before !== null && before !== now ? before : null;
}
