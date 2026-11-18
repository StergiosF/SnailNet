// Steadies the live stats for the eye. The service reports four times a
// second, and real traffic comes in bursts: an app moves 9 Mbps, then
// nothing for a quarter second, then 8 again. Shown raw, rows appear and
// vanish, numbers jump and the order reshuffles on every tick. So each
// app, site and slice gets a smoothed rate (an exponential average over
// about two seconds), and one that went quiet stays listed, marked
// `held`, until it has been quiet for HOLD_MS. Pure apart from its memory,
// so it's tested.

import type { ClassTick, DirStats, FamilyTick, SiteTick, StatsTick } from "./types";

/** How quickly the smoothed rate follows the real one (ms). */
export const TAU_MS = 1_500;
/** How long something quiet stays listed (ms). */
export const HOLD_MS = 8_000;
/** Below this a rate counts as quiet (ledger.ts QUIET_BPS). */
const QUIET = 50_000;

interface Memory<T> {
  last: T;
  ema: number;
  /** Last time its raw rate was above quiet. */
  active: number;
  /** Second smoothed field (a family's unidentified rate). */
  ema2: number;
}

class Track<T> {
  #m = new Map<string, Memory<T>>();

  /** Smooth `items` (this tick's) against memory; returns what to show. */
  step(
    items: T[],
    key: (x: T) => string,
    bps: (x: T) => number,
    put: (x: T, bps: number, held: boolean, second: number) => T,
    now: number,
    a: number,
    second: (x: T) => number = () => 0,
  ): T[] {
    const seen = new Set<string>();
    for (const x of items) {
      const k = key(x);
      seen.add(k);
      const raw = bps(x);
      const m = this.#m.get(k);
      if (!m) {
        this.#m.set(k, { last: x, ema: raw, ema2: second(x), active: raw >= QUIET ? now : -Infinity });
        continue;
      }
      m.last = x;
      m.ema += a * (raw - m.ema);
      m.ema2 += a * (second(x) - m.ema2);
      if (raw >= QUIET) m.active = now;
    }
    for (const [k, m] of this.#m) {
      if (seen.has(k)) continue;
      m.ema += a * (0 - m.ema);
      m.ema2 += a * (0 - m.ema2);
    }
    const out: T[] = [];
    for (const [k, m] of this.#m) {
      const held = now - m.active < HOLD_MS;
      if (!seen.has(k) && !held) {
        this.#m.delete(k);
        continue;
      }
      out.push(put(m.last, Math.max(0, m.ema), held && m.ema < QUIET, Math.max(0, m.ema2)));
    }
    return out;
  }
}

class SteadyDir {
  #families = new Track<FamilyTick>();
  #sites = new Track<SiteTick>();
  #classes = new Track<ClassTick>();

  step(dir: DirStats, now: number, a: number): DirStats {
    return {
      shaped_bps: dir.shaped_bps,
      families: this.#families
        .step(
          dir.families,
          (f) => f.family,
          (f) => f.bps,
          (f, bps, held, unidentified) => ({ ...f, bps, unidentified_bps: unidentified, held }),
          now,
          a,
          (f) => f.unidentified_bps,
        )
        .sort((x, y) => y.bps - x.bps),
      sites: this.#sites.step(
        dir.sites,
        (s) => `${s.family}\u0000${s.site}`,
        (s) => s.bps,
        (s, bps) => ({ ...s, bps }),
        now,
        a,
      ),
      classes: this.#classes.step(
        dir.classes,
        (c) => c.name.toLowerCase(),
        (c) => c.bps,
        (c, bps, held) => ({ ...c, bps, held }),
        now,
        a,
      ),
    };
  }
}

/** Feed it every tick; it hands back the same tick, steadied. */
export class Steady {
  #down = new SteadyDir();
  #up = new SteadyDir();
  #last: number | null = null;

  step(tick: StatsTick): StatsTick {
    const now = tick.t_ms;
    const dt = this.#last === null ? 0 : Math.max(0, now - this.#last);
    this.#last = now;
    // The first tick, or one after a long gap, is taken as it is.
    const a = dt === 0 || dt > HOLD_MS ? 1 : 1 - Math.exp(-dt / TAU_MS);
    return {
      ...tick,
      down: tick.down ? this.#down.step(tick.down, now, a) : null,
      up: tick.up ? this.#up.step(tick.up, now, a) : null,
    };
  }

  /** Forget everything (the link to the service dropped). */
  reset(): void {
    this.#down = new SteadyDir();
    this.#up = new SteadyDir();
    this.#last = null;
  }
}
