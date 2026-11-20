import { describe, expect, it } from "vitest";
import {
  APPLIED_MS,
  FixWatch,
  SUSTAIN_MS,
  isMuted,
  loadFix,
  mute,
  offerOf,
  pairKey,
  pairOf,
  saveFix,
  tomorrow,
  type Pair,
} from "./fix";
import { ledger } from "./ledger";
import { barShares } from "./rules";
import { isValid } from "./split";
import type { DirStats, Rule, Target } from "./types";

const M = 1_000_000;
const label = (t: Target) => ({ youtube: "YouTube", brave: "Brave", steam: "Steam" })[t.id] ?? t.id;
const fam = (id: string): Target => ({ kind: "family", id });
const rule = (id: string, share: number, extra: Partial<Rule> = {}): Rule => ({
  target: fam(id),
  share,
  priority: "normal",
  enabled: true,
  ...extra,
});

/** The motivating moment, no rules: Steam fills the line, Brave waits. */
function hogged(): DirStats {
  return {
    shaped_bps: 23 * M,
    classes: [{ name: "everything else", bps: 22 * M, drops: 9, queued_bytes: 80_000 }],
    families: [
      { family: "steam", name: "Steam", bps: 20.6 * M, unidentified_bps: 0 },
      { family: "brave", name: "Brave", bps: 1.4 * M, unidentified_bps: 0 },
    ],
    sites: [],
  };
}

describe("pairOf", () => {
  it("finds the hog and the app it leaves waiting", () => {
    const p = pairOf(ledger([], hogged(), label, ["brave"]), null);
    expect(p).toMatchObject({ dir: "down", hog: "family:steam", waiter: "family:brave", name: "Brave", kind: "give" });
  });

  it("needs someone waiting, a full line and a real hog", () => {
    expect(pairOf(ledger([], hogged(), label), null)).toBeNull();
    const calm = hogged();
    calm.classes = [{ name: "everything else", bps: 10 * M, drops: 0, queued_bytes: 0 }];
    expect(pairOf(ledger([], calm, label, ["brave"]), null)).toBeNull();
    const even = hogged();
    even.families = [
      { family: "steam", name: "Steam", bps: 10 * M, unidentified_bps: 0 },
      { family: "x", name: "X", bps: 10 * M, unidentified_bps: 0 },
      { family: "brave", name: "Brave", bps: 2 * M, unidentified_bps: 0 },
    ];
    expect(pairOf(ledger([], even, label, ["brave"]), null)).toBeNull();
    const unshaped = { ...hogged(), shaped_bps: 0 };
    expect(pairOf(ledger([], unshaped, label, ["brave"]), null)).toBeNull();
  });

  it("offers to grow a slice that's full, never an only-when-free one", () => {
    const d = hogged();
    d.classes = [
      { name: "family:brave", bps: 1.4 * M, drops: 3, queued_bytes: 40_000 },
      { name: "everything else", bps: 20.6 * M, drops: 9, queued_bytes: 80_000 },
    ];
    expect(pairOf(ledger([rule("brave", 1000)], d, label), null)).toMatchObject({ kind: "grow", waiter: "family:brave" });
    expect(pairOf(ledger([rule("brave", 0)], d, label), null)).toBeNull();
  });

  it("never offers unrecognised traffic", () => {
    const d = hogged();
    d.families.push({ family: "unknown", name: "Unknown", bps: 0.5 * M, unidentified_bps: 0 });
    expect(pairOf(ledger([], d, label, ["unknown"]), null)).toBeNull();
  });

  it("looks at upload when download has no pair", () => {
    expect(pairOf(null, ledger([], hogged(), label, ["brave"]))?.dir).toBe("up");
  });
});

const pair = (waiter = "family:brave"): Pair => ({
  dir: "down",
  hog: "family:steam",
  hogName: "Steam",
  waiter,
  name: "Brave",
  target: fam("brave"),
  kind: "give",
});

describe("FixWatch", () => {
  it("offers once the same pair held 12 seconds", () => {
    const w = new FixWatch();
    let out: Pair | null = null;
    let t = 0;
    for (; t <= SUSTAIN_MS - 250; t += 250) out = w.feed(t, pair());
    expect(out).toBeNull();
    expect(w.feed(t, pair())).toEqual(pair());
  });

  it("counts only the time this pair held", () => {
    const w = new FixWatch();
    let t = 0;
    for (; t < 6_000; t += 250) w.feed(t, pair("family:zoom"));
    for (; t < 15_000; t += 250) expect(w.feed(t, pair())).toBeNull();
    // 12 s of Brave in the last 15 once the Zoom ticks fall out.
    for (; t < 18_250; t += 250) w.feed(t, pair());
    expect(w.feed(t, pair())).toEqual(pair());
  });

  it("tolerates a short gap but not a long one", () => {
    const w = new FixWatch();
    let t = 0;
    for (; t < 6_000; t += 250) w.feed(t, pair());
    for (; t < 8_000; t += 250) w.feed(t, null);
    for (; t < 14_000; t += 250) expect(w.feed(t, pair())).toBeNull();
    for (; t < 15_000; t += 250) w.feed(t, pair());
    expect(w.feed(t, pair())).toEqual(pair());
  });

  it("doesn't count a stalled stream as time held", () => {
    const w = new FixWatch();
    w.feed(0, pair());
    expect(w.feed(13_000, pair())).toBeNull();
  });
});

describe("offerOf", () => {
  it("gives a waiter without a slice 60%", () => {
    const o = offerOf([], pair());
    expect(o?.pct).toBe(60);
    expect(o?.rules).toEqual([{ target: fam("brave"), share: 6000, priority: "normal", enabled: true }]);
  });

  it("says the real number when the slices are crowded", () => {
    const crowded = [rule("steam", 4500), rule("discord", 4500)];
    const o = offerOf(crowded, pair());
    expect(o?.pct).toBe(60);
    expect(isValid(barShares(o?.rules ?? []))).toBe(true);
    const full = [rule("a", 100), rule("b", 100)];
    // Everything else gives down to its 5%: 93% fits, so 60% does.
    expect(offerOf(full, pair())?.pct).toBe(60);
  });

  it("turns a switched-off rule back on with 60%", () => {
    const o = offerOf([rule("brave", 1000, { enabled: false })], pair());
    expect(o?.rules[0]).toMatchObject({ enabled: true, share: 6000 });
  });

  it("makes a full slice 15 points bigger, as far as it goes", () => {
    const grow: Pair = { ...pair(), kind: "grow" };
    expect(offerOf([rule("brave", 2000)], grow)?.pct).toBe(35);
    expect(offerOf([rule("brave", 9500)], grow)).toBeNull();
    expect(offerOf([rule("brave", 0)], grow)).toBeNull();
  });
});

describe("mutes", () => {
  it("Not now lasts until local midnight", () => {
    const now = new Date(2026, 10, 20, 21, 30);
    expect(tomorrow(now)).toBe(new Date(2026, 10, 21, 0, 0).getTime());
  });

  it("expire, and drop the expired ones when another is added", () => {
    const key = pairKey(pair());
    let m = mute({}, key, 1000 + APPLIED_MS, 1000);
    expect(isMuted(m, key, 1000 + APPLIED_MS - 1)).toBe(true);
    expect(isMuted(m, key, 1000 + APPLIED_MS)).toBe(false);
    m = mute(m, "x>y", 5_000_000_000, 1000 + APPLIED_MS);
    expect(Object.keys(m)).toEqual(["x>y"]);
  });

  it("are remembered with the switch, and survive bad storage", () => {
    const store = new Map<string, string>();
    const s = { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) };
    expect(loadFix(s)).toEqual({ on: true, mutes: {} });
    saveFix({ on: false, mutes: { a: 5 } }, s);
    expect(loadFix(s)).toEqual({ on: false, mutes: { a: 5 } });
    store.set("snailnet.fix", "{not json");
    expect(loadFix(s)).toEqual({ on: true, mutes: {} });
  });
});
