// The line speed as people type it (PLAN.md §10, §14.5, §14.6 Connection),
// and when the first-run guide shows. Pure functions; vitest in line.test.ts.

import type { Line, Link, Snapshot } from "./types";

/** What snailnet-core accepts for a direction (rules.rs `LINE_MBPS`). */
export const MIN_MBPS = 0.1;
export const MAX_MBPS = 10000;

/** A typed speed: empty means "don't shape this direction". */
export type Speed = { ok: true; mbps: number | null } | { ok: false };

/** Parse one speed box. Accepts "25", "25.5", "25,5" (a comma decimal, as
 *  many keyboards type it) and a trailing "Mbps"; rounds to 0.1 Mbps. */
export function parseSpeed(text: string): Speed {
  const s = text.trim().replace(/\s*mbps$/i, "").replace(",", ".");
  if (s === "") return { ok: true, mbps: null };
  if (!/^\d*\.?\d+$|^\d+\.$/.test(s)) return { ok: false };
  const v = Math.round(Number(s) * 10) / 10;
  return v >= MIN_MBPS && v <= MAX_MBPS ? { ok: true, mbps: v } : { ok: false };
}

/** A speed as the box shows it: "25", "40.5", "" when not set. */
export function speedText(mbps: number | undefined): string {
  return mbps === undefined ? "" : String(Math.round(mbps * 10) / 10);
}

/** The line to save: new speeds, the factor kept as it was. */
export function withSpeeds(line: Line, down: number | null, up: number | null): Line {
  const next: Line = {};
  if (down !== null) next.down_mbps = down;
  if (up !== null) next.up_mbps = up;
  if (line.factor !== undefined) next.factor = line.factor;
  return next;
}

/** True when neither direction has a speed, so nothing is shaped. */
export function noSpeed(line: Line | undefined): boolean {
  return line?.down_mbps === undefined && line?.up_mbps === undefined;
}

/** First run (§14.5) shows once the service is reachable, while no speed
 *  is set, unless the person already finished or skipped it. */
export function needsFirstRun(link: Link, snapshot: Snapshot | null, dismissed: boolean): boolean {
  return !dismissed && link.kind === "connected" && snapshot !== null && noSpeed(snapshot.config.line);
}

const DISMISSED_KEY = "snailnet.firstRun.done";

/** Whether first run was finished before. Storage can be missing or throw
 *  (private profile, cleared data): then it shows again, which is harmless. */
export function firstRunDismissed(storage?: Pick<Storage, "getItem">): boolean {
  try {
    return (storage ?? globalThis.localStorage)?.getItem(DISMISSED_KEY) === "1";
  } catch {
    return false;
  }
}

export function dismissFirstRun(storage?: Pick<Storage, "setItem">): void {
  try {
    (storage ?? globalThis.localStorage)?.setItem(DISMISSED_KEY, "1");
  } catch {
    // Not remembered; first run may show again next time.
  }
}

/** After "Reset SnailNet": show first run again. */
export function forgetFirstRun(storage?: Pick<Storage, "removeItem">): void {
  try {
    (storage ?? globalThis.localStorage)?.removeItem(DISMISSED_KEY);
  } catch {
    // Nothing remembered to forget.
  }
}

const COACH_KEY = "snailnet.coach.split";

/** Whether the one-time hint on "The split" (§14.5 step 4) was put away. */
export function coachSeen(storage?: Pick<Storage, "getItem">): boolean {
  try {
    return (storage ?? globalThis.localStorage)?.getItem(COACH_KEY) === "1";
  } catch {
    return false;
  }
}

export function markCoachSeen(storage?: Pick<Storage, "setItem">): void {
  try {
    (storage ?? globalThis.localStorage)?.setItem(COACH_KEY, "1");
  } catch {
    // Not remembered; the hint may show once more.
  }
}

/** After "Reset SnailNet": the hint shows again with first run. */
export function forgetCoach(storage?: Pick<Storage, "removeItem">): void {
  try {
    (storage ?? globalThis.localStorage)?.removeItem(COACH_KEY);
  } catch {
    // Nothing remembered to forget.
  }
}
