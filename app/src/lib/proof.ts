// The proof (docs/HANDOFF-wow.md feature 1, PLAN.md §11.10): ping while
// the line is full, with SnailNet, next to the ping the last speed test on
// this network found without it. Every number is measured: the live ping
// comes from the service's probe (`StatsTick.ping`), the "without" from
// `config.line.measured`. When a number isn't there, this says less.
// Pure apart from ProofWatch's memory, so it's tested. The grade scale is
// snailnet-core::proof's twin.

import { top, type Ledger } from "./ledger";
import type { Measured, StatsTick } from "./types";

export type Grade = "A" | "B" | "C" | "D" | "F";

/** Upper edges of each grade, in ms of ping rise (Waveform's scale). */
export const GRADE_A_MS = 30;
export const GRADE_B_MS = 60;
export const GRADE_C_MS = 200;
export const GRADE_D_MS = 400;

/** How much ping rises under load → A to F. */
export function grade(riseMs: number): Grade {
  if (riseMs <= GRADE_A_MS) return "A";
  if (riseMs <= GRADE_B_MS) return "B";
  if (riseMs <= GRADE_C_MS) return "C";
  if (riseMs <= GRADE_D_MS) return "D";
  return "F";
}

/** Loaded − idle ping, never negative; null when either is missing. */
export function rise(idleMs: number | undefined, loadedMs: number | undefined): number | null {
  if (idleMs === undefined || loadedMs === undefined) return null;
  if (!Number.isFinite(idleMs) || !Number.isFinite(loadedMs)) return null;
  return Math.max(0, loadedMs - idleMs);
}

/** "Without" is shown only this far above "with": never claim credit
 *  SnailNet didn't earn. */
export const CLEAR_MS = 20;

export interface Proof {
  /** Live ping with SnailNet, ms. */
  now: number;
  /** The speed test's ping under load without SnailNet, ms; null when
   *  there's none for this direction or it isn't clearly worse. */
  without: number | null;
  /** When that speed test ran (Unix seconds); null without one. */
  at: number | null;
  /** No speed test number to compare with at all ("Measure to compare"). */
  missing: boolean;
  /** Who fills the line: the busiest app or slice. */
  name: string;
}

/** What to show for this tick, or null (line not full, no ping, not
 *  shaping). `down` and `up` are the two directions' ledgers. */
export function proofOf(
  tick: StatsTick | null | undefined,
  down: Ledger | null | undefined,
  up: Ledger | null | undefined,
  measured: Measured | undefined,
): Proof | null {
  if (!tick?.ping || tick.state !== "Shaping") return null;
  const downBusy = down?.busy === true;
  const upBusy = up?.busy === true;
  if (!downBusy && !upBusy) return null;
  const book = (downBusy ? down : up) as Ledger;
  const name = top(book);
  if (name === null) return null;
  const base = downBusy ? measured?.loaded_ms : measured?.loaded_up_ms;
  const now = tick.ping.now_ms;
  const clear = base !== undefined && base - now >= CLEAR_MS;
  return {
    now,
    without: clear ? base : null,
    at: clear && measured ? measured.at : null,
    missing: base === undefined,
    name,
  };
}

/** The line has to be full this long before the row shows (ms). */
export const SHOW_AFTER_MS = 3_000;
/** And it stays this long after the line calms down (ms). */
export const HOLD_MS = 10_000;
/** A gap in "full" shorter than this doesn't start the wait again (ms). */
const GAP_MS = 1_000;
/** The shown number follows a move of at least this much (ms)… */
export const MOVE_MS = 2;
/** …or, smaller moves, at most this often (ms). */
export const REFRESH_MS = 2_000;

/** Feed it every tick (`t_ms` is the tick's clock); it says what to show.
 *  Like steady.ts: the service reports four times a second, and a row that
 *  blinks in and out, or a number that jitters, reads as noise. `off`
 *  (paused, measuring, not shaping) hides the row at once, no hold. */
export class ProofWatch {
  #since: number | null = null;
  #lastSeen = -Infinity;
  #shown: Proof | null = null;
  #shownAt = -Infinity;

  step(proof: Proof | null, now: number, off = false): Proof | null {
    if (off) {
      this.reset();
      return null;
    }
    if (proof) {
      if (this.#since === null || now - this.#lastSeen > GAP_MS) {
        if (this.#shown === null) this.#since = now;
      }
      this.#lastSeen = now;
    }

    if (this.#shown === null) {
      if (!proof || this.#since === null || now - this.#since < SHOW_AFTER_MS) return null;
      this.#put(proof, now);
      return this.#shown;
    }

    if (!proof) {
      if (now - this.#lastSeen >= HOLD_MS) this.reset();
      return this.#shown;
    }
    const moved = Math.abs(proof.now - this.#shown.now) >= MOVE_MS;
    if (moved || now - this.#shownAt >= REFRESH_MS) this.#put(proof, now);
    return this.#shown;
  }

  #put(proof: Proof, now: number) {
    this.#shown = proof;
    this.#shownAt = now;
  }

  /** Forget everything (the link dropped, or the row went away). */
  reset(): void {
    this.#since = null;
    this.#lastSeen = -Infinity;
    this.#shown = null;
    this.#shownAt = -Infinity;
  }
}
