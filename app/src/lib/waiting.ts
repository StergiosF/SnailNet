// "Waiting": which app keeps asking for more than it gets, the one-tap
// fix's signal (PLAN.md §11.11). The rule was settled in the simulator
// (crates/snailnet-sim/src/waiting.rs, `RULE`); this is the same rule, fed
// the service's raw stats ticks (4 a second) instead of the sim's.
//
// An app is *pressed* in a tick when the scheduler acted on it (CoDel
// dropped or marked one of its packets) or any of its bytes are queued.
// Every greedy app is pressed, the hog included; the waiting one is also
// *losing*: over the window it got less than half of an equal split of
// what the pressed apps' line delivered. Pure apart from its memory.

import type { FamilyTick } from "./types";

export interface WaitRule {
  /** Ticks looked back over. */
  window: number;
  /** Pressed in at least this many of them. */
  minPressed: number;
  /** Losing: delivered less than this fraction of an equal split. */
  losing: number;
}

/** Pressed in 14 of the last 20 ticks (70% of 5 s), under half an equal split. */
export const RULE: WaitRule = { window: 20, minPressed: 14, losing: 0.5 };

export function pressed(f: Pick<FamilyTick, "acted" | "queued_bytes">): boolean {
  return (f.acted ?? 0) > 0 || (f.queued_bytes ?? 0) > 0;
}

type Sample = { pressed: boolean; bps: number };

/** Feed it one direction's families each tick; it says who is waiting. */
export class WaitWatch {
  #ticks: Map<string, Sample>[] = [];

  constructor(readonly rule: WaitRule = RULE) {}

  /** Add a tick; returns the families waiting now. An app missing from a
   *  tick counts as neither pressed nor sending. */
  feed(families: FamilyTick[]): string[] {
    this.#ticks.push(new Map(families.map((f) => [f.family, { pressed: pressed(f), bps: f.bps }])));
    if (this.#ticks.length > this.rule.window) this.#ticks.shift();
    const hits = new Map<string, number>();
    const sent = new Map<string, number>();
    for (const tick of this.#ticks) {
      for (const [k, s] of tick) {
        hits.set(k, (hits.get(k) ?? 0) + (s.pressed ? 1 : 0));
        sent.set(k, (sent.get(k) ?? 0) + s.bps);
      }
    }
    const busy = [...hits].filter(([, h]) => h >= this.rule.minPressed).map(([k]) => k);
    const total = [...sent.values()].reduce((a, b) => a + b, 0);
    const fair = total / Math.max(1, busy.length);
    return busy.filter((k) => (sent.get(k) ?? 0) < this.rule.losing * fair);
  }

  reset(): void {
    this.#ticks = [];
  }
}
