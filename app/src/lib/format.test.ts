import { describe, expect, it } from "vitest";
import { appRates, className, mbps, percent, stateKind, targetLabel, totalBps } from "./format";
import type { DirStats } from "./types";

const dir = (families: [string, number][], classes: number[] = []): DirStats => ({
  shaped_bps: 23_000_000,
  classes: classes.map((bps, i) => ({ name: `c${i}`, bps, drops: 0, queued_bytes: 0 })),
  families: families.map(([family, bps]) => ({ family, name: family, bps, unidentified_bps: 0 })),
  sites: [],
});

describe("mbps", () => {
  it("keeps one decimal below 100 and none above", () => {
    expect(mbps(36_800_000)).toBe("36.8");
    expect(mbps(0)).toBe("0.0");
    expect(mbps(99_940_000)).toBe("99.9");
    expect(mbps(99_960_000)).toBe("100");
    expect(mbps(1_234_000_000)).toBe("1234");
  });
  it("never shows a negative rate", () => {
    expect(mbps(-5)).toBe("0.0");
  });
});

describe("percent", () => {
  it("turns basis points into a whole percentage", () => {
    expect(percent(4500)).toBe("45%");
    expect(percent(3333)).toBe("33%");
  });
});

describe("totals and rows", () => {
  it("sums every class of a direction", () => {
    expect(totalBps(dir([], [1_000_000, 500_000]))).toBe(1_500_000);
    expect(totalBps(null)).toBe(0);
  });
  it("merges directions per family, busiest first", () => {
    const rows = appRates(
      dir([
        ["steam", 30_000_000],
        ["discord", 1_000_000],
      ]),
      dir([
        ["discord", 2_000_000],
        ["brave", 40_000_000],
      ]),
    );
    expect(rows.map((r) => r.family)).toEqual(["brave", "steam", "discord"]);
    expect(rows[2]).toEqual({ family: "discord", name: "discord", down: 1_000_000, up: 2_000_000 });
  });
});

describe("targetLabel", () => {
  const apps = [{ family: "brave", name: "Brave", exe: "brave.exe" }];
  it("uses the app's or site's own name, else a capitalised id", () => {
    expect(targetLabel({ kind: "family", id: "brave" }, apps, {})).toBe("Brave");
    expect(targetLabel({ kind: "site", id: "youtube" }, apps, { youtube: "YouTube" })).toBe("YouTube");
    expect(targetLabel({ kind: "site", id: "twitch" }, apps, {})).toBe("Twitch");
    expect(targetLabel({ kind: "exe", id: "a.exe" }, apps, {})).toBe("A.exe");
  });
  it("names the class the scheduler reports", () => {
    expect(className({ kind: "site", id: "YouTube" })).toBe("site:youtube");
  });
});

describe("stateKind", () => {
  it("names each service state", () => {
    expect(stateKind("Shaping")).toBe("shaping");
    expect(stateKind({ Retrying: { reason: "x" } })).toBe("retrying");
    expect(stateKind({ SafeMode: { failures: 3 } })).toBe("safe_mode");
  });
});
