// The one-tap fix (PLAN.md §11.11, docs/HANDOFF-wow.md feature 2): when
// one app fills the line and another is left waiting, the sentence at the
// top offers the cure, "Give Brave 60%". Pure, so it's tested: who the
// pair is this tick (`pairOf`), whether the same pair held long enough
// (`FixWatch`), what the button does (`offerOf`), and the mutes ("Not
// now" until tomorrow, 10 minutes after a fix so it can't ping-pong).
// fixer.svelte.ts runs it on the live stats.

import type { Ledger } from "./ledger";
import { QUIET_BPS } from "./ledger";
import { addRule, sameTarget, setEnabled, setRuleShare, shownShare } from "./rules";
import type { Rule, Target } from "./types";

/** The hog takes at least half of what SnailNet shares out. */
export const HOG_SHARE = 0.5;
/** The same pair for 12 of the last 15 seconds before the offer shows. */
export const SUSTAIN_MS = 12_000;
export const WINDOW_MS = 15_000;
/** A gap between ticks counts for at most this much (the stream stalled). */
const MAX_STEP_MS = 1_000;
/** A waiter without a slice is offered 60%; one whose slice is full, 15 points more. */
export const GIVE = 6_000;
export const GROW = 1_500;
/** After a fix, the same pair stays quiet this long. */
export const APPLIED_MS = 10 * 60_000;

export type Dir = "down" | "up";

export interface Pair {
  dir: Dir;
  /** Slice or member key of the app filling the line. */
  hog: string;
  hogName: string;
  /** The one left waiting. */
  waiter: string;
  name: string;
  target: Target;
  /** No slice of its own (give one), or its slice is full (grow it). */
  kind: "give" | "grow";
}

export function pairKey(p: Pair): string {
  return `${p.hog}>${p.waiter}`;
}

type Who = { key: string; name: string; bps: number; target: Target; waiting: "give" | "grow" | null };

function everyone(l: Ledger): Who[] {
  const out: Who[] = [];
  for (const s of l.slices) {
    if (s.target === null) {
      for (const m of s.members) {
        if (m.fixed) continue;
        out.push({ key: m.key, name: m.name, bps: m.bps, target: m.target, waiting: m.waiting ? "give" : null });
      }
      continue;
    }
    // "Only when free" and off rules are never offered more: the user chose that.
    const grows = s.enabled && s.share !== null && s.status.kind === "held";
    out.push({ key: s.key, name: s.name, bps: s.bps, target: s.target, waiting: grows ? "grow" : null });
  }
  return out;
}

/** This tick's pair in one direction: the line full, one app taking at
 *  least half of it, another left waiting. Null otherwise. */
function pairIn(dir: Dir, l: Ledger | null): Pair | null {
  if (!l || !l.busy || l.shaped === null) return null;
  const all = everyone(l).sort((a, b) => b.bps - a.bps);
  const hog = all[0];
  if (!hog || hog.bps < HOG_SHARE * l.shaped || hog.bps < QUIET_BPS) return null;
  const w = all.find((x) => x !== hog && x.waiting !== null);
  if (!w || w.waiting === null) return null;
  return { dir, hog: hog.key, hogName: hog.name, waiter: w.key, name: w.name, target: w.target, kind: w.waiting };
}

/** Download first: that's where waiting hurts (video, game updates). */
export function pairOf(down: Ledger | null, up: Ledger | null): Pair | null {
  return pairIn("down", down) ?? pairIn("up", up);
}

/** Fed every tick; says when the same pair has held for SUSTAIN_MS of the
 *  last WINDOW_MS. A different pair, or none, doesn't reset it at once:
 *  only the time the current pair held counts. */
export class FixWatch {
  #samples: { t: number; key: string | null; dt: number }[] = [];

  feed(now: number, pair: Pair | null): Pair | null {
    const last = this.#samples.at(-1);
    const dt = last ? Math.min(MAX_STEP_MS, Math.max(0, now - last.t)) : 0;
    const key = pair ? pairKey(pair) : null;
    this.#samples.push({ t: now, key, dt });
    while ((this.#samples[0]?.t ?? now) < now - WINDOW_MS) this.#samples.shift();
    if (!pair) return null;
    const held = this.#samples.filter((s) => s.key === key).reduce((sum, s) => sum + s.dt, 0);
    return held >= SUSTAIN_MS ? pair : null;
  }

  reset(): void {
    this.#samples = [];
  }
}

export interface Offer {
  pair: Pair;
  /** The share it will have, in whole percent (as much as fits). */
  pct: number;
  /** The active mode's rules with the fix. */
  rules: Rule[];
}

/** What the button does to `rules` (the active mode's, as saved): a new
 *  slice of 60% (as much as fits), or a full slice 15 points bigger. The
 *  rules.ts edits keep the split valid (§9.7). Null when nothing fits. */
export function offerOf(rules: Rule[], pair: Pair): Offer | null {
  const i = rules.findIndex((r) => sameTarget(r.target, pair.target));
  if (pair.kind === "give") {
    const r = rules[i];
    if (r?.enabled) return null;
    const next = r ? setRuleShare(setEnabled(rules, i, true), i, GIVE) : addRule(rules, pair.target, GIVE);
    const bp = shownShare(next, r ? i : rules.length) ?? 0;
    const pct = Math.round(bp / 100);
    return pct >= 1 ? { pair, pct, rules: next } : null;
  }
  const cur = i >= 0 ? shownShare(rules, i) : null;
  if (cur === null) return null;
  const next = setRuleShare(rules, i, cur + GROW);
  const bp = shownShare(next, i) ?? cur;
  const pct = Math.round(bp / 100);
  return pct > Math.round(cur / 100) ? { pair, pct, rules: next } : null;
}

/** Pair key → until when it's quiet (ms since the epoch). */
export type Mutes = Record<string, number>;

/** The next local midnight after `now`. */
export function tomorrow(now: Date): number {
  const d = new Date(now);
  d.setHours(24, 0, 0, 0);
  return d.getTime();
}

export function mute(mutes: Mutes, key: string, until: number, now: number): Mutes {
  const out: Mutes = { [key]: until };
  for (const [k, u] of Object.entries(mutes)) if (k !== key && u > now) out[k] = u;
  return out;
}

export function isMuted(mutes: Mutes, key: string, now: number): boolean {
  return (mutes[key] ?? 0) > now;
}

/** What's remembered: the Settings switch, and the mutes. */
export interface FixPrefs {
  on: boolean;
  mutes: Mutes;
}

const KEY = "snailnet.fix";

/** Storage can be missing or throw: then the fix is on and nothing is muted. */
export function loadFix(storage?: Pick<Storage, "getItem">): FixPrefs {
  try {
    const raw = (storage ?? globalThis.localStorage)?.getItem(KEY);
    if (!raw) return { on: true, mutes: {} };
    const v = JSON.parse(raw) as Partial<FixPrefs>;
    const mutes: Mutes = {};
    for (const [k, u] of Object.entries(v.mutes ?? {})) if (typeof u === "number") mutes[k] = u;
    return { on: v.on !== false, mutes };
  } catch {
    return { on: true, mutes: {} };
  }
}

export function saveFix(p: FixPrefs, storage?: Pick<Storage, "setItem">): void {
  try {
    (storage ?? globalThis.localStorage)?.setItem(KEY, JSON.stringify(p));
  } catch {
    // Not remembered: a muted offer may come back sooner.
  }
}

/** After "Reset SnailNet". */
export function forgetFix(storage?: Pick<Storage, "removeItem">): void {
  try {
    (storage ?? globalThis.localStorage)?.removeItem(KEY);
  } catch {
    // Nothing remembered to forget.
  }
}
