import { describe, expect, it } from "vitest";
import {
  addRule,
  barShares,
  canStep,
  shareOf,
  stepShare,
  clampLimit,
  formatLimit,
  parseLimit,
  pickable,
  removeRule,
  setCeiling,
  setEnabled,
  setRuleShare,
} from "./rules";
import { FLOOR, TOTAL, addSlice, isValid, removeSlice } from "./split";
import type { Rule, Target } from "./types";

const fam = (id: string): Target => ({ kind: "family", id });
const rule = (id: string, share: number, extra: Partial<Rule> = {}): Rule => ({
  target: fam(id),
  share,
  priority: "normal",
  enabled: true,
  ...extra,
});
const shares = (rules: Rule[]) => rules.map((r) => r.share);

describe("addSlice / removeSlice", () => {
  it("carves a new slice from Everything else first", () => {
    expect(addSlice([TOTAL], 2000)).toEqual([2000, 8000]);
    expect(addSlice([4000, 4500, 1500], 2000)).toEqual([3530, 3970, 2000, 500]);
  });
  it("then from the rules, in proportion to what they have above 1%", () => {
    const s = addSlice([4600, 4600, 800], 1000);
    expect(s).toEqual([4250, 4250, 1000, FLOOR]);
    expect(isValid(s)).toBe(true);
  });
  it("gives what there is when the bar is full", () => {
    expect(addSlice([9500, 500], 2000)).toEqual([7500, 2000, FLOOR]);
    expect(addSlice([9500, 500], 9999)).toEqual([100, 9400, FLOOR]);
  });
  it("hands a removed slice back in proportion to the others' size", () => {
    expect(removeSlice([4000, 4000, 2000], 0)).toEqual([6667, 3333]);
    expect(removeSlice([4000, 4000, 2000], 2)).toEqual([4000, 4000, 2000]);
  });
});

describe("rule edits keep the bar valid", () => {
  it("adds a rule with a 20% slice", () => {
    const r = addRule([], fam("steam"));
    expect(r).toEqual([rule("steam", 2000)]);
    const r2 = addRule([rule("steam", 4000), rule("brave", 4500)], { kind: "site", id: "youtube" });
    expect(shares(r2)).toEqual([3530, 3970, 2000]);
    expect(isValid(barShares(r2))).toBe(true);
  });
  it("removes a rule and gives its share back", () => {
    const r = removeRule([rule("steam", 4000), rule("brave", 4000)], 0);
    expect(r).toEqual([rule("brave", 6667)]);
  });
  it("sets a share, rebalancing the others", () => {
    const r = setRuleShare([rule("steam", 4000), rule("brave", 4500)], 0, 6000);
    expect(isValid(barShares(r))).toBe(true);
    expect(r[0]?.share).toBe(6000);
  });
  it("0 means only when free: no slice, the share goes back", () => {
    const r = setRuleShare([rule("steam", 4000), rule("brave", 4000)], 0, 0);
    expect(shares(r)).toEqual([0, 6667]);
    const back = setRuleShare(r, 0, 3000);
    expect(back[0]?.share).toBe(3000);
    expect(isValid(barShares(back))).toBe(true);
  });
  it("an off rule keeps its size and gets it back when turned on", () => {
    const off = setEnabled([rule("steam", 4000), rule("brave", 4000)], 0, false);
    expect(off[0]).toEqual(rule("steam", 4000, { enabled: false }));
    expect(off[1]?.share).toBe(6667);
    const on = setEnabled(off, 0, true);
    expect(on[0]).toEqual(rule("steam", 4000));
    expect(isValid(barShares(on))).toBe(true);
    // Changing an off rule's share doesn't touch the bar.
    expect(setRuleShare(off, 0, 1000)[1]).toEqual(off[1]);
  });
  it("sets and clears ceilings per direction", () => {
    let r = setCeiling([rule("steam", 4000)], 0, "down", { unit: "percent", value: 20 });
    expect(r[0]?.ceiling).toEqual({ down: "20%" });
    r = setCeiling(r, 0, "up", { unit: "mbps", value: 2.5 });
    expect(r[0]?.ceiling).toEqual({ down: "20%", up: "2.5 Mbps" });
    r = setCeiling(setCeiling(r, 0, "down", null), 0, "up", null);
    expect(r[0]?.ceiling).toBeUndefined();
  });
});

describe("limits", () => {
  it("round-trips the core's text form", () => {
    expect(parseLimit("20%")).toEqual({ unit: "percent", value: 20 });
    expect(parseLimit("5 Mbps")).toEqual({ unit: "mbps", value: 5 });
    expect(parseLimit(" 2.5 mbps ")).toEqual({ unit: "mbps", value: 2.5 });
    expect(parseLimit("fast")).toBeNull();
    expect(parseLimit(undefined)).toBeNull();
    expect(formatLimit({ unit: "mbps", value: 2.5 })).toBe("2.5 Mbps");
  });
  it("clamps to what the service accepts", () => {
    expect(clampLimit({ unit: "percent", value: 140 })).toEqual({ unit: "percent", value: 100 });
    expect(clampLimit({ unit: "percent", value: 0 })).toEqual({ unit: "percent", value: 1 });
    expect(clampLimit({ unit: "mbps", value: 0.01 })).toEqual({ unit: "mbps", value: 0.1 });
  });
});

describe("pickable", () => {
  it("offers sites then apps, minus ruled ones, filtered", () => {
    const sites = [{ id: "youtube", name: "YouTube" }, { id: "twitch", name: "Twitch" }];
    const apps = [
      { family: "steam", name: "Steam" },
      { family: "brave", name: "Brave" },
      { family: "unknown", name: "Unknown" },
    ];
    const all = pickable(sites, apps, [rule("steam", 1000)], "");
    expect(all.map((p) => p.name)).toEqual(["YouTube", "Twitch", "Brave"]);
    expect(pickable(sites, apps, [], "tw").map((p) => p.target.id)).toEqual(["twitch"]);
  });
});

describe("stepShare", () => {
  const r: Rule[] = [
    { target: { kind: "site", id: "youtube" }, share: 4500, priority: "normal", enabled: true },
    { target: { kind: "family", id: "brave" }, share: 3000, priority: "normal", enabled: true },
  ];

  it("moves a slice 5% and the others make room", () => {
    const up = stepShare(r, 0, 1);
    expect(shareOf(up, 0)).toBe(5000);
    expect(shareOf(up, -1)).toBeGreaterThanOrEqual(500);
    expect(barShares(up).reduce((a, b) => a + b, 0)).toBe(10_000);
  });

  it("lands on a multiple of 5", () => {
    const odd: Rule[] = [{ ...r[0]!, share: 4300 }, r[1]!];
    expect(shareOf(stepShare(odd, 0, 1), 0)).toBe(4500);
    expect(shareOf(stepShare(odd, 0, -1), 0)).toBe(4000);
  });

  it("steps Everything else too, never below 5%", () => {
    expect(shareOf(stepShare(r, -1, 1), -1)).toBe(3000);
    const full: Rule[] = [{ ...r[0]!, share: 6000 }, { ...r[1]!, share: 3500 }];
    expect(canStep(full, -1, -1)).toBe(false);
  });

  it("gives an only-when-free rule a 5% slice, and never steps an off rule", () => {
    const free: Rule[] = [{ ...r[0]!, share: 0 }, r[1]!];
    expect(shareOf(stepShare(free, 0, 1), 0)).toBe(500);
    expect(canStep(free, 0, -1)).toBe(false);
    const off: Rule[] = [{ ...r[0]!, enabled: false }, r[1]!];
    expect(canStep(off, 0, 1)).toBe(false);
  });

  it("keeps a rule at 1% at least", () => {
    const small: Rule[] = [{ ...r[0]!, share: 300 }, r[1]!];
    expect(shareOf(stepShare(small, 0, -1), 0)).toBe(100);
    const tiny: Rule[] = [{ ...r[0]!, share: 100 }, r[1]!];
    expect(canStep(tiny, 0, -1)).toBe(false);
  });
});
