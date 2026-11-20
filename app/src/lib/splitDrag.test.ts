import { describe, expect, it } from "vitest";
import { barShares } from "./rules";
import { FLOOR, MIN_RULE, isValid } from "./split";
import {
  FULL_PX,
  PCT_PX,
  dragBetween,
  edgeBetween,
  fitLabel,
  handlePositions,
  nudgeBetween,
  pickHandle,
  savedTicks,
} from "./splitDrag";
import type { Rule } from "./types";

const rule = (id: string, share: number, extra: Partial<Rule> = {}): Rule => ({
  target: { kind: "family", id },
  share,
  priority: "normal",
  enabled: true,
  ...extra,
});
// Steam 40, YouTube 45, Everything else 15.
const rules = [rule("steam", 4000), rule("youtube", 4500)];

describe("handlePositions", () => {
  it("puts a handle at each boundary", () => {
    expect(handlePositions([4000, 4500, 1500])).toEqual([40, 85]);
    expect(handlePositions([10_000])).toEqual([]);
  });
});

describe("dragBetween", () => {
  it("trades only between the two neighbours, sum kept", () => {
    const out = dragBetween(rules, 0, 37.3);
    expect(barShares(out)).toEqual([3730, 4770, 1500]);
    expect(isValid(barShares(out))).toBe(true);
  });
  it("snaps the left one to a step of 5% on release", () => {
    expect(barShares(dragBetween(rules, 0, 37.3, true))).toEqual([3500, 5000, 1500]);
    expect(barShares(dragBetween(rules, 1, 88.9, true))).toEqual([4000, 5000, 1000]);
  });
  it("holds Everything else's 5% and a rule's 1%", () => {
    expect(barShares(dragBetween(rules, 1, 100))).toEqual([4000, 5500, FLOOR]);
    expect(barShares(dragBetween(rules, 0, -20))).toEqual([MIN_RULE, 8400, 1500]);
  });
  it("leaves rules that aren't slices alone", () => {
    const mixed = [rule("steam", 4000), rule("epic", 0), rule("youtube", 4500), rule("x", 2000, { enabled: false })];
    const out = dragBetween(mixed, 0, 50);
    expect(out.map((r) => r.share)).toEqual([5000, 0, 3500, 2000]);
  });
  it("ignores a handle that doesn't exist", () => {
    expect(dragBetween(rules, 2, 50)).toBe(rules);
    expect(dragBetween(rules, -1, 50)).toBe(rules);
  });
});

describe("nudgeBetween / edgeBetween", () => {
  it("steps to the next multiple of 5", () => {
    const odd = [rule("steam", 3730), rule("youtube", 4770)];
    expect(barShares(nudgeBetween(odd, 0, 1))).toEqual([4000, 4500, 1500]);
    expect(barShares(nudgeBetween(odd, 0, -1))).toEqual([3500, 5000, 1500]);
    expect(barShares(nudgeBetween(rules, 0, 2))).toEqual([5000, 3500, 1500]);
  });
  it("Home and End go as far as the floors allow", () => {
    expect(barShares(edgeBetween(rules, 0, "min"))).toEqual([MIN_RULE, 8400, 1500]);
    expect(barShares(edgeBetween(rules, 1, "max"))).toEqual([4000, 5500, FLOOR]);
  });
});

describe("pickHandle", () => {
  it("catches the nearest handle within half the hit area", () => {
    expect(pickHandle([100, 300], 110)).toBe(0);
    expect(pickHandle([100, 300], 200)).toBeNull();
  });
  it("lets the first move choose between overlapping handles", () => {
    expect(pickHandle([100, 106], 103)).toBeNull();
    expect(pickHandle([100, 106], 103, -2)).toBe(0);
    expect(pickHandle([100, 106], 103, 2)).toBe(1);
  });
});

describe("fitLabel", () => {
  it("full, then just the %, then nothing", () => {
    expect(fitLabel(FULL_PX)).toBe("full");
    expect(fitLabel(FULL_PX - 1)).toBe("pct");
    expect(fitLabel(PCT_PX)).toBe("pct");
    expect(fitLabel(PCT_PX - 1)).toBe("none");
  });
});

describe("savedTicks", () => {
  it("marks where the handles were once something moved", () => {
    expect(savedTicks(rules, dragBetween(rules, 0, 30))).toEqual([40, 85]);
    expect(savedTicks(rules, rules)).toBeNull();
  });
  it("draws none when the slices themselves changed", () => {
    expect(savedTicks(rules, [rules[0] as Rule])).toBeNull();
    expect(savedTicks(rules, [...rules].reverse())).toBeNull();
  });
});
