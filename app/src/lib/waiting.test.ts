import { describe, expect, it } from "vitest";
import type { FamilyTick } from "./types";
import { RULE, WaitWatch } from "./waiting";

// The same cases as crates/snailnet-sim/src/waiting.rs.
const f = (family: string, bps: number, queued_bytes: number, acted: number): FamilyTick => ({
  family,
  name: family,
  bps,
  unidentified_bps: 0,
  queued_bytes,
  acted,
});

describe("WaitWatch", () => {
  it("flags the pressed loser, never the pressed winner", () => {
    const w = new WaitWatch();
    let last: string[] = [];
    for (let i = 0; i < RULE.minPressed; i++) {
      last = w.feed([f("steam", 700_000, 50_000, 5), f("brave", 40_000, 0, 1), f("discord", 1_000, 0, 0)]);
    }
    expect(last).toEqual(["brave"]);
  });

  it("counts a queue without CoDel action as pressed", () => {
    const w = new WaitWatch();
    let last: string[] = [];
    for (let i = 0; i < RULE.minPressed; i++) {
      last = w.feed([f("steam", 700_000, 50_000, 5), f("brave", 40_000, 1500, 0)]);
    }
    expect(last).toEqual(["brave"]);
  });

  it("needs enough pressed ticks in the window", () => {
    const w = new WaitWatch();
    for (let i = 1; i < RULE.minPressed; i++) {
      expect(w.feed([f("steam", 700_000, 50_000, 5), f("brave", 40_000, 0, 1)])).toEqual([]);
    }
    // Quiet ticks push the pressed ones out of the window.
    for (let i = 0; i < RULE.window; i++) {
      expect(w.feed([f("steam", 700_000, 50_000, 5), f("brave", 40_000, 0, 0)])).toEqual([]);
    }
  });

  it("doesn't flag equal contenders", () => {
    const w = new WaitWatch();
    for (let i = 0; i < RULE.window; i++) {
      expect(w.feed([f("a", 300_000, 9_000, 2), f("b", 250_000, 9_000, 2), f("c", 220_000, 9_000, 1)])).toEqual([]);
    }
  });

  it("treats an app missing from a tick as not pressed", () => {
    const w = new WaitWatch();
    for (let i = 0; i < RULE.minPressed - 1; i++) w.feed([f("steam", 700_000, 50_000, 5), f("brave", 40_000, 0, 1)]);
    expect(w.feed([f("steam", 700_000, 50_000, 5)])).toEqual([]);
    w.reset();
    expect(w.feed([f("brave", 40_000, 0, 1)])).toEqual([]);
  });
});
