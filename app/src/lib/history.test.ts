import { describe, expect, it } from "vitest";
import { configMarks, History, REST, samplesOf, STEP_MS, STEPS, type Sample } from "./history";
import { ledger } from "./ledger";
import type { Config, DirStats, Rule, Target } from "./types";

const M = 1_000_000;
const label = (t: Target) => ({ youtube: "YouTube", steam: "Steam", discord: "Discord" })[t.id] ?? t.id;
const started = (name: string) => `${name} started`;

const steam = (bps: number): Sample => ({
  key: "family:steam",
  name: "Steam",
  ink: -1,
  target: { kind: "family", id: "steam" },
  bps,
});
const tube = (bps: number): Sample => ({
  key: "site:youtube",
  name: "YouTube",
  ink: 0,
  target: { kind: "site", id: "youtube" },
  bps,
});

/** Ticks every 100 ms (the app subscribes at 10 Hz) from `from` to `to`. */
function feed(h: History, from: number, to: number, down: (t: number) => Sample[], up: Sample[] = []) {
  let closed = 0;
  for (let t = from; t < to; t += 100) if (h.add(t, down(t), up)) closed++;
  return closed;
}

describe("samplesOf", () => {
  it("names each share and each app in Everything else, and keeps the rest unnamed", () => {
    const rules: Rule[] = [{ target: { kind: "site", id: "youtube" }, share: 4000, priority: "normal", enabled: true }];
    const dir: DirStats = {
      shaped_bps: 23 * M,
      classes: [
        { name: "site:youtube", bps: 8 * M, drops: 0, queued_bytes: 0 },
        { name: "everything else", bps: 12 * M, drops: 0, queued_bytes: 0 },
      ],
      families: [
        { family: "steam", name: "Steam", bps: 10 * M, unidentified_bps: 0 },
        { family: "unknown", name: "Unknown", bps: 1.5 * M, unidentified_bps: 0 },
      ],
      sites: [],
    };
    const s = samplesOf(ledger(rules, dir, label));
    expect(s.map((x) => [x.key, x.bps / M])).toEqual([
      ["site:youtube", 8],
      ["family:steam", 10],
      [REST, 2],
    ]);
    expect(s[0]?.ink).toBe(0);
    expect(s[1]?.ink).toBe(-1);
  });

  it("is empty without a ledger", () => {
    expect(samplesOf(null)).toEqual([]);
  });
});

describe("History", () => {
  it("closes a step every 5 s with each key's average", () => {
    const h = new History(started);
    // 2.5 s at 10 Mbps, 2.5 s at 20: 15 on average.
    const closed = feed(h, 0, STEP_MS + 100, (t) => [steam(t < 2500 ? 10 * M : 20 * M)]);
    expect(closed).toBe(1);
    expect(h.steps).toHaveLength(1);
    expect(h.steps[0]?.t).toBe(0);
    expect((h.steps[0]?.down["family:steam"] ?? 0) / M).toBeCloseTo(15, 5);
  });

  it("keeps ten minutes and forgets what's older", () => {
    const h = new History(started);
    feed(h, 0, STEP_MS * (STEPS + 10) + 100, () => [steam(5 * M)]);
    expect(h.steps).toHaveLength(STEPS);
    expect(h.steps[0]?.t).toBe(STEP_MS * 10);
  });

  it("fills a gap in the ticks with steps where nothing moved", () => {
    const h = new History(started);
    feed(h, 0, 3000, () => [steam(5 * M)]);
    h.add(STEP_MS * 4 + 50, [steam(5 * M)], []);
    expect(h.steps.map((s) => s.t)).toEqual([0, STEP_MS, STEP_MS * 2, STEP_MS * 3]);
    expect(h.steps[1]?.down).toEqual({});
  });

  it("starts again when the service's clock goes back (it restarted)", () => {
    const h = new History(started);
    feed(h, 50_000, 70_000, () => [steam(5 * M)]);
    h.note("Switched to Gaming");
    h.add(1_000, [steam(5 * M)], []);
    expect(h.steps).toEqual([]);
    expect(h.marks).toEqual([]);
  });

  it("marks an app that starts after being quiet, once", () => {
    const h = new History(started);
    feed(h, 0, 60_000, (t) => [steam(t >= 40_000 ? 12 * M : 0)]);
    expect(h.marks).toEqual([{ t: 40_000, text: "Steam started" }]);
  });

  it("doesn't mark what was already busy when the history began", () => {
    const h = new History(started);
    feed(h, 0, 60_000, () => [steam(12 * M)]);
    expect(h.marks).toEqual([]);
  });

  it("drops marks that slid out of the ten minutes", () => {
    const h = new History(started);
    h.add(0, [], []);
    h.note("Switched to Gaming");
    feed(h, 100, STEP_MS * (STEPS + 3), () => []);
    expect(h.marks).toEqual([]);
  });

  it("bands the busiest apps, biggest first, and the rest as one band", () => {
    const h = new History(started);
    const many = (n: number) =>
      Array.from({ length: n }, (_, i): Sample => ({
        key: `family:a${i}`,
        name: `App ${i}`,
        ink: -1,
        target: { kind: "family", id: `a${i}` },
        bps: (i + 1) * M,
      }));
    feed(h, 0, STEP_MS * 3 + 100, () => many(7));
    const bands = h.bands("down", 5);
    expect(bands.map((b) => b.key)).toEqual([
      "family:a6",
      "family:a5",
      "family:a4",
      "family:a3",
      "family:a2",
      REST,
    ]);
    // a0 and a1: 1 + 2 Mbps.
    expect((bands[5]?.values[0] ?? 0) / M).toBeCloseTo(3, 5);
    expect(bands[0]?.peak).toBeCloseTo(7 * M, 0);
  });

  it("keeps the directions apart", () => {
    const h = new History(started);
    feed(h, 0, STEP_MS + 100, () => [steam(8 * M)], [tube(0.5 * M)]);
    expect(h.bands("down").map((b) => b.key)).toEqual(["family:steam"]);
    expect(h.bands("up").map((b) => b.key)).toEqual(["site:youtube"]);
  });
});

describe("configMarks", () => {
  const words = {
    mode: (id: string) => ({ balanced: "Balanced", streaming: "Streaming" })[id] ?? id,
    label,
    switched: (m: string) => `Switched to ${m}`,
    boosted: (n: string) => `Boosted ${n}`,
    gets: (n: string, pct: number) => `${n} gets ${pct}%`,
    spare: (n: string) => `${n} only uses spare speed`,
    changed: (m: string) => `${m}'s split changed`,
  };
  const yt: Target = { kind: "site", id: "youtube" };
  const st: Target = { kind: "family", id: "steam" };
  const config = (active: string, rules: Rule[], boost?: Config["boost"]): Config =>
    ({
      version: 1,
      active_profile: active,
      settings: {},
      line: {},
      boost,
      profiles: [
        { id: "balanced", name: "Balanced", rules },
        { id: "streaming", name: "Streaming", rules: [] },
      ],
    }) as unknown as Config;
  const rule = (target: Target, share: number): Rule => ({ target, share, priority: "normal", enabled: true });

  it("says nothing on the first config", () => {
    expect(configMarks(null, config("balanced", []), words)).toEqual([]);
  });

  it("names a mode switch", () => {
    expect(configMarks(config("balanced", []), config("streaming", []), words)).toEqual(["Switched to Streaming"]);
  });

  it("names a share given, as the bar shows it", () => {
    expect(configMarks(config("balanced", []), config("balanced", [rule(yt, 4000)]), words)).toEqual([
      "YouTube gets 40%",
    ]);
  });

  it("names an app put on spare speed", () => {
    expect(configMarks(config("balanced", []), config("balanced", [rule(st, 0)]), words)).toEqual([
      "Steam only uses spare speed",
    ]);
  });

  it("says the split changed when several shares did", () => {
    const before = config("balanced", [rule(yt, 4000), rule(st, 0)]);
    const after = config("balanced", [rule(yt, 3000), rule(st, 1000)]);
    expect(configMarks(before, after, words)).toEqual(["Balanced's split changed"]);
  });

  it("says nothing when nothing it shows changed", () => {
    const c = config("balanced", [rule(yt, 4000)]);
    expect(configMarks(c, structuredClone(c), words)).toEqual([]);
  });

  it("names a boost that began, not one that carries on", () => {
    const before = config("balanced", []);
    const after = config("balanced", [], { target: st, until: 100 });
    expect(configMarks(before, after, words)).toEqual(["Boosted Steam"]);
    expect(configMarks(after, structuredClone(after), words)).toEqual([]);
  });
});
