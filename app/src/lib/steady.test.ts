import { describe, expect, it } from "vitest";
import { ledger } from "./ledger";
import { HOLD_MS, Steady } from "./steady";
import type { DirStats, StatsTick } from "./types";

function tick(t_ms: number, apps: Record<string, number>): StatsTick {
  const families = Object.entries(apps).map(([family, bps]) => ({ family, name: family, bps, unidentified_bps: 0 }));
  const down: DirStats = {
    shaped_bps: 20_000_000,
    classes: [{ name: "Everything else", bps: families.reduce((s, f) => s + f.bps, 0), drops: 0, queued_bytes: 0 }],
    families,
    sites: [],
  };
  return { t_ms, state: { kind: "running" } as unknown as StatsTick["state"], down, up: null, names: null };
}

const members = (t: StatsTick) => ledger([], t.down, (x) => x.id).slices.at(-1)?.members.map((m) => m.name) ?? [];

describe("steady", () => {
  it("takes the first tick as it is", () => {
    const s = new Steady();
    const out = s.step(tick(0, { brave: 8_000_000 }));
    expect(out.down?.families[0]?.bps).toBe(8_000_000);
  });

  it("keeps a bursty app listed between bursts", () => {
    const s = new Steady();
    s.step(tick(0, { brave: 8_000_000, bun: 100_000 }));
    // A quarter second of nothing: the service leaves both out.
    const gap = s.step(tick(250, {}));
    expect(members(gap)).toEqual(["brave", "bun"]);
    expect(gap.down?.families.find((f) => f.family === "brave")?.bps).toBeGreaterThan(6_000_000);
  });

  it("lets an app go after it stays quiet", () => {
    const s = new Steady();
    s.step(tick(0, { bun: 100_000 }));
    let out = s.step(tick(250, {}));
    for (let t = 500; t <= HOLD_MS + 500; t += 250) out = s.step(tick(t, {}));
    expect(members(out)).toEqual([]);
  });

  it("smooths a jumpy rate", () => {
    const s = new Steady();
    s.step(tick(0, { brave: 8_000_000 }));
    const out = s.step(tick(250, { brave: 100_000 }));
    const bps = out.down?.families[0]?.bps ?? 0;
    expect(bps).toBeGreaterThan(6_000_000);
    expect(bps).toBeLessThan(8_000_000);
  });

  it("keeps the busiest first", () => {
    const s = new Steady();
    s.step(tick(0, { a: 1_000_000, b: 5_000_000 }));
    const out = s.step(tick(250, { a: 2_000_000, b: 4_000_000 }));
    expect(out.down?.families.map((f) => f.family)).toEqual(["b", "a"]);
  });

  it("starts over after reset", () => {
    const s = new Steady();
    s.step(tick(0, { brave: 8_000_000 }));
    s.reset();
    const out = s.step(tick(250, {}));
    expect(out.down?.families).toEqual([]);
  });
});
