import { describe, expect, it } from "vitest";
import { className, mbps, percent, stateKind, targetLabel } from "./format";
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
    expect(stateKind({ Paused: { until: null } })).toBe("paused");
  });
});
