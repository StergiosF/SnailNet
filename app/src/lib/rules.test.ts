import { describe, expect, it } from "vitest";
import {
  addRule,
  barShares,
  clampLimit,
  formatLimit,
  listRows,
  parseLimit,
  pickable,
  removeRule,
  setCeiling,
  setEnabled,
  setRuleShare,
} from "./rules";
import { FLOOR, TOTAL, addSlice, isValid, removeSlice } from "./split";
import type { DirStats, Rule, Target } from "./types";

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

describe("listRows", () => {
  const dir = (fams: [string, number][], sites: [string, number][] = []): DirStats => ({
    shaped_bps: 0,
    classes: [],
    families: fams.map(([family, bps]) => ({ family, name: family.toUpperCase(), bps, unidentified_bps: 0 })),
    sites: sites.map(([site, bps]) => ({ site, name: `S-${site}`, family: "brave", bps })),
  });
  const label = (t: Target) => `L-${t.id}`;

  it("lists apps and sites by speed, then quiet rules by name", () => {
    const rules = [rule("zed", 1000), rule("steam", 2000), { ...rule("x", 0), target: { kind: "site" as const, id: "youtube" } }];
    const rows = listRows(dir([["steam", 5], ["brave", 1]], [["youtube", 3]]), null, rules, label);
    expect(rows.map((r) => [r.key, r.rule, r.idle])).toEqual([
      ["family:steam", 1, false],
      ["site:youtube", 2, false],
      ["family:brave", -1, false],
      ["family:zed", 0, true],
    ]);
    expect(rows[3]?.name).toBe("L-zed");
  });
  it("marks unknown traffic as not ruleable", () => {
    const rows = listRows(dir([["unknown", 5]]), null, [], label);
    expect(rows[0]?.fixed).toBe(true);
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
