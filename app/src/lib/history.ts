// The Now page's "Last 10 minutes" (DESIGN.md, Now): what used your
// internet over time, in 5 s steps, with marks where something happened (a
// mode switch, a boost, a share given, an app that started). The app keeps
// it from the stats it already gets, so nothing new is asked of the
// service and nothing is invented: it starts empty when the app opens and
// forgets what's older than 10 minutes. Pure, so it's tested.

import { QUIET_BPS, type Ledger } from "./ledger";
import { sameTarget, shownShare } from "./rules";
import type { Config, Target } from "./types";

/** One step of the chart. */
export const STEP_MS = 5_000;
/** Ten minutes of steps. */
export const STEPS = 120;
export const SPAN_MS = STEP_MS * STEPS;
/** Everything that isn't one of the busiest few: one band, "Other apps". */
export const REST = "rest";
/** An app "started" when a step averages this much… */
const START_BPS = 1_000_000;
/** …after this many steps (30 s) under this. */
const START_QUIET_STEPS = 6;
const START_QUIET_BPS = 100_000;

/** Who moved how much in one tick, as the list names them. */
export interface Sample {
  key: string;
  name: string;
  /** Its share's colour slot; -1 inside Everything else. */
  ink: number;
  target: Target | null;
  bps: number;
}

/** One closed step: each key's average bits per second over it. */
export interface Step {
  /** When the step began, on the service's clock (StatsTick.t_ms). */
  t: number;
  down: Record<string, number>;
  up: Record<string, number>;
}

/** Something that happened, at a moment on the same clock. */
export interface Mark {
  t: number;
  text: string;
}

export interface Who {
  name: string;
  ink: number;
  target: Target | null;
}

/** One band of the chart: a key's averages, oldest first, one per step. */
export interface Band extends Who {
  key: string;
  values: number[];
  /** Its biggest step, bits per second. */
  peak: number;
}

/** Who the list would name in one direction, plus what's left unnamed
 *  (unknown traffic, apps too quiet to list) under REST. */
export function samplesOf(l: Ledger | null): Sample[] {
  if (!l) return [];
  const out: Sample[] = [];
  let named = 0;
  for (const s of l.slices) {
    if (s.target !== null) {
      out.push({ key: s.key, name: s.name, ink: s.ink, target: s.target, bps: s.bps });
      named += s.bps;
      continue;
    }
    for (const m of s.members) {
      if (m.fixed) continue;
      out.push({ key: m.key, name: m.name, ink: -1, target: m.target, bps: m.bps });
      named += m.bps;
    }
  }
  const rest = l.total - named;
  if (rest >= QUIET_BPS) out.push({ key: REST, name: "", ink: -1, target: null, bps: rest });
  return out;
}

interface Acc {
  start: number;
  n: number;
  down: Record<string, number>;
  up: Record<string, number>;
}

const fresh = (start: number): Acc => ({ start, n: 0, down: {}, up: {} });

function add(into: Record<string, number>, samples: Sample[]) {
  for (const s of samples) if (s.bps > 0) into[s.key] = (into[s.key] ?? 0) + s.bps;
}

function average(sum: Record<string, number>, n: number): Record<string, number> {
  const out: Record<string, number> = {};
  for (const [k, v] of Object.entries(sum)) out[k] = n > 0 ? v / n : 0;
  return out;
}

export class History {
  steps: Step[] = [];
  marks: Mark[] = [];
  /** Each key's latest name and colour. */
  who = new Map<string, Who>();
  #acc: Acc | null = null;
  #last = -Infinity;
  readonly #started: (name: string) => string;

  /** `started` words a mark for an app that started ("Steam started"). */
  constructor(started: (name: string) => string) {
    this.#started = started;
  }

  /** The latest moment seen, or null before the first tick. */
  get now(): number | null {
    return Number.isFinite(this.#last) ? this.#last : null;
  }

  /** Feeds one tick. True when a step closed, i.e. the chart changed. */
  add(t: number, down: Sample[], up: Sample[]): boolean {
    // The service restarted: its clock began again, so does the history.
    if (t < this.#last) this.clear();
    this.#last = t;
    let closed = false;
    if (this.#acc === null) {
      this.#acc = fresh(t);
    } else if (t - this.#acc.start >= STEP_MS) {
      this.#push({ t: this.#acc.start, down: average(this.#acc.down, this.#acc.n), up: average(this.#acc.up, this.#acc.n) });
      let start = this.#acc.start + STEP_MS;
      // No ticks for a while (the service was busy or gone): nothing moved.
      if (t - start >= SPAN_MS) {
        this.steps = [];
        start = t;
      }
      while (t - start >= STEP_MS) {
        this.#push({ t: start, down: {}, up: {} });
        start += STEP_MS;
      }
      this.#acc = fresh(start);
      closed = true;
    }
    add(this.#acc.down, down);
    add(this.#acc.up, up);
    this.#acc.n += 1;
    for (const s of [...down, ...up]) {
      if (s.key !== REST) this.who.set(s.key, { name: s.name, ink: s.ink, target: s.target });
    }
    return closed;
  }

  /** Marks something that happened now (or at `t`). */
  note(text: string, t: number | null = this.now) {
    if (t === null) return;
    this.marks.push({ t, text });
    this.#trim();
  }

  clear() {
    this.steps = [];
    this.marks = [];
    this.#acc = null;
    this.#last = -Infinity;
  }

  #push(step: Step) {
    this.steps.push(step);
    const i = this.steps.length - 1;
    if (i >= START_QUIET_STEPS) {
      for (const [key, bps] of Object.entries(step.down)) {
        if (key === REST || bps < START_BPS) continue;
        const before = this.steps.slice(i - START_QUIET_STEPS, i);
        if (before.every((s) => (s.down[key] ?? 0) < START_QUIET_BPS)) {
          const name = this.who.get(key)?.name;
          if (name) this.marks.push({ t: step.t, text: this.#started(name) });
        }
      }
    }
    this.#trim();
  }

  #trim() {
    if (this.steps.length > STEPS) this.steps.splice(0, this.steps.length - STEPS);
    const from = this.steps[0]?.t ?? this.#last;
    this.marks = this.marks.filter((m) => m.t >= from).slice(-12);
  }

  /** The chart's bands in one direction: the `max` busiest keys over the
   *  ten minutes, biggest first (the chart stacks them from the floor), and
   *  everything else as one REST band (empty name: the caller names it). */
  bands(dir: "down" | "up", max = 5): Band[] {
    const sums = new Map<string, number>();
    for (const s of this.steps) {
      for (const [k, v] of Object.entries(s[dir])) sums.set(k, (sums.get(k) ?? 0) + v);
    }
    const keys = [...sums.entries()]
      .filter(([k, v]) => k !== REST && v > 0 && this.who.has(k))
      .sort((a, b) => b[1] - a[1])
      .map(([k]) => k);
    const shown = keys.slice(0, max);
    const out: Band[] = shown.map((key) => {
      const values = this.steps.map((s) => s[dir][key] ?? 0);
      const w = this.who.get(key) as Who;
      return { key, ...w, values, peak: Math.max(0, ...values) };
    });
    const others = new Set([REST, ...keys.slice(max)]);
    const rest = this.steps.map((s) =>
      Object.entries(s[dir]).reduce((sum, [k, v]) => (others.has(k) ? sum + v : sum), 0),
    );
    if (rest.some((v) => v >= QUIET_BPS)) {
      out.push({ key: REST, name: "", ink: -1, target: null, values: rest, peak: Math.max(...rest) });
    }
    return out;
  }
}

/** Stone steps for apps without a share, lightest for the busiest, so
 *  neighbouring bands part; Other apps takes the darkest. */
const STONES = ["var(--stone-0)", "var(--stone-2)", "var(--stone-3)", "var(--stone-1)"];

/** Each band's colour, and the same for its row in the list: a share's own
 *  ink, else a stone step. */
export function bandColors(bands: Band[]): Map<string, string> {
  const out = new Map<string, string>();
  let k = 0;
  for (const b of bands) {
    if (b.key === REST) out.set(b.key, "var(--stone-4)");
    else if (b.ink >= 0) out.set(b.key, `var(--ink-${b.ink})`);
    else out.set(b.key, STONES[k++ % STONES.length] as string);
  }
  return out;
}

/** What changed between two configs, in words, for the chart's marks: a
 *  mode switch, a boost that began, a share given or changed. `mode`
 *  names a profile, `label` a target. Several share changes at once (the
 *  Split page's Save) are one "split changed" mark. */
export function configMarks(
  prev: Config | null,
  next: Config,
  words: {
    mode: (id: string) => string;
    label: (t: Target) => string;
    switched: (mode: string) => string;
    boosted: (name: string) => string;
    gets: (name: string, pct: number) => string;
    spare: (name: string) => string;
    changed: (mode: string) => string;
  },
): string[] {
  if (!prev) return [];
  const out: string[] = [];
  if (prev.active_profile !== next.active_profile) {
    out.push(words.switched(words.mode(next.active_profile)));
  } else {
    const before = prev.profiles.find((p) => p.id === prev.active_profile)?.rules ?? [];
    const rules = next.profiles.find((p) => p.id === next.active_profile)?.rules ?? [];
    const changed = rules
      .map((r, i) => ({ r, i, was: before.find((b) => sameTarget(b.target, r.target)) }))
      .filter(({ r, was }) => r.enabled && (!was || !was.enabled || was.share !== r.share));
    const removed = before.some((b) => !rules.some((r) => sameTarget(r.target, b.target)));
    if (changed.length === 1 && !removed) {
      const { r, i } = changed[0] as (typeof changed)[number];
      const pct = shownShare(rules, i);
      out.push(
        pct === null || r.share === 0
          ? words.spare(words.label(r.target))
          : words.gets(words.label(r.target), Math.round(pct / 100)),
      );
    } else if (changed.length > 0 || removed) {
      out.push(words.changed(words.mode(next.active_profile)));
    }
  }
  const b = next.boost;
  const was = prev.boost;
  if (b && (!was || !sameTarget(was.target, b.target) || was.until !== b.until)) {
    out.push(words.boosted(words.label(b.target)));
  }
  return out;
}
