import { describe, expect, it } from "vitest";
import { FLOOR, MIN_RULE, TOTAL, apportion, isValid, moveDivider, normalize, setShare } from "./split";

describe("normalize (same results as snailnet-core shares::normalize)", () => {
  it("keeps rules that fit and gives the rest to Everything else", () => {
    expect(normalize([4000, 4500])).toEqual([4000, 4500, 1500]);
    expect(normalize([])).toEqual([TOTAL]);
  });
  it("scales rules down when they ask for more than 95%", () => {
    const s = normalize([6000, 6000]);
    expect(s).toEqual([4750, 4750, 500]);
    expect(isValid(s)).toBe(true);
  });
});

describe("apportion", () => {
  it("sums exactly and breaks ties to the lower index", () => {
    expect(apportion(10, [1, 1, 1])).toEqual([4, 3, 3]);
    expect(apportion(0, [0, 0])).toEqual([0, 0]);
  });
});

describe("moveDivider", () => {
  const s = [4000, 4500, 1500];
  it("trades between the two neighbours", () => {
    expect(moveDivider(s, 0, 500)).toEqual([4500, 4000, 1500]);
    expect(moveDivider(s, 1, -1000)).toEqual([4000, 3500, 2500]);
  });
  it("never takes Everything else below 5% or a rule below 1%", () => {
    expect(moveDivider(s, 1, 5000)).toEqual([4000, 5500, FLOOR]);
    expect(moveDivider(s, 0, -9000)).toEqual([MIN_RULE, 8400, 1500]);
  });
  it("ignores a divider that doesn't exist", () => {
    expect(moveDivider(s, 2, 100)).toBe(s);
  });
});

describe("setShare", () => {
  it("rebalances the others in proportion to what they have", () => {
    // Steam 40 → 60: YouTube and Everything else give 20 between them,
    // in proportion to what each has above its minimum (4400 : 1000).
    const out = setShare([4000, 4500, 1500], 0, 6000);
    expect(out[0]).toBe(6000);
    expect(isValid(out)).toBe(true);
    expect(out).toEqual([6000, 2870, 1130]);
  });
  it("gives freed share back the same way", () => {
    // 6900 above the minimums, split 3900 : 1000.
    expect(setShare([4000, 4500, 1500], 1, 2500)).toEqual([5592, 2500, 1908]);
  });
  it("clamps to what the minimums leave", () => {
    expect(setShare([4000, 4500, 1500], 0, 10_000)).toEqual([9400, MIN_RULE, FLOOR]);
    expect(setShare([4000, 4500, 1500], 0, 0)[0]).toBe(MIN_RULE);
  });
  it("lets Everything else take everything when nobody else can", () => {
    expect(setShare([9400, MIN_RULE, FLOOR], 0, 5000)).toEqual([5000, MIN_RULE, 4900]);
  });
  it("can't set Everything else directly", () => {
    const s = [4000, 6000];
    expect(setShare(s, 1, 100)).toBe(s);
  });
  it("always returns a valid list", () => {
    for (let v = 0; v <= TOTAL; v += 137) {
      expect(isValid(setShare([1000, 2000, 3000, 4000], 1, v))).toBe(true);
    }
  });
});
