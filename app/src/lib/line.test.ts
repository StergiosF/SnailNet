import { describe, expect, it } from "vitest";
import {
  coachSeen,
  forgetCoach,
  markCoachSeen,
  dismissFirstRun,
  firstRunDismissed,
  forgetFirstRun,
  needsFirstRun,
  newNetworkName,
  noSpeed,
  parseSpeed,
  speedText,
  withSpeeds,
} from "./line";
import type { Config, Line, Link, Snapshot } from "./types";

describe("parseSpeed", () => {
  it("reads plain, decimal and comma-decimal numbers", () => {
    expect(parseSpeed("25")).toEqual({ ok: true, mbps: 25 });
    expect(parseSpeed(" 40.5 ")).toEqual({ ok: true, mbps: 40.5 });
    expect(parseSpeed("40,5")).toEqual({ ok: true, mbps: 40.5 });
    expect(parseSpeed(".5")).toEqual({ ok: true, mbps: 0.5 });
    expect(parseSpeed("100 Mbps")).toEqual({ ok: true, mbps: 100 });
  });
  it("rounds to a tenth", () => {
    expect(parseSpeed("25.37")).toEqual({ ok: true, mbps: 25.4 });
  });
  it("treats an empty box as not set", () => {
    expect(parseSpeed("")).toEqual({ ok: true, mbps: null });
    expect(parseSpeed("   ")).toEqual({ ok: true, mbps: null });
  });
  it("keeps to what the service accepts (0.1–10000)", () => {
    expect(parseSpeed("0.1")).toEqual({ ok: true, mbps: 0.1 });
    expect(parseSpeed("10000")).toEqual({ ok: true, mbps: 10000 });
    expect(parseSpeed("0")).toEqual({ ok: false });
    expect(parseSpeed("0.04")).toEqual({ ok: false });
    expect(parseSpeed("10001")).toEqual({ ok: false });
  });
  it("refuses text that isn't a number", () => {
    for (const bad of ["fast", "-5", "1e3", "25.5.5", "Infinity", "0x10", "1,000.5"]) {
      expect(parseSpeed(bad)).toEqual({ ok: false });
    }
  });
});

describe("speedText", () => {
  it("shows the saved speed, or nothing", () => {
    expect(speedText(25)).toBe("25");
    expect(speedText(40.5)).toBe("40.5");
    expect(speedText(undefined)).toBe("");
  });
});

describe("withSpeeds", () => {
  it("keeps the factor and drops a cleared direction", () => {
    const line: Line = { down_mbps: 25, up_mbps: 40, factor: 0.85 };
    expect(withSpeeds(line, 50, null)).toEqual({ down_mbps: 50, factor: 0.85 });
  });
  it("keeps the pin", () => {
    expect(withSpeeds({ down_mbps: 25, pinned: true }, 30, null)).toEqual({ down_mbps: 30, pinned: true });
  });
  it("adds no factor that wasn't there", () => {
    expect(withSpeeds({}, 25, 40)).toEqual({ down_mbps: 25, up_mbps: 40 });
  });
});

describe("first run", () => {
  const connected: Link = { kind: "connected", service_version: "0.1.0" };
  const snap = (line: Line) => ({ config: { line } }) as Snapshot;

  it("shows when connected and no speed is set", () => {
    expect(noSpeed({})).toBe(true);
    expect(needsFirstRun(connected, snap({}), false)).toBe(true);
    expect(needsFirstRun(connected, snap({ factor: 0.9 }), false)).toBe(true);
  });
  it("stays away once a speed is set, or once dismissed", () => {
    expect(needsFirstRun(connected, snap({ up_mbps: 40 }), false)).toBe(false);
    expect(needsFirstRun(connected, snap({}), true)).toBe(false);
  });
  it("waits for the service", () => {
    expect(needsFirstRun({ kind: "connecting" }, null, false)).toBe(false);
    expect(needsFirstRun({ kind: "not_running" }, snap({}), false)).toBe(false);
    expect(needsFirstRun(connected, null, false)).toBe(false);
  });

  it("remembers the dismissal", () => {
    const store = new Map<string, string>();
    const storage = {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
    };
    expect(firstRunDismissed(storage)).toBe(false);
    dismissFirstRun(storage);
    expect(firstRunDismissed(storage)).toBe(true);
  });
  it("survives storage that throws", () => {
    const broken = {
      getItem: (): string | null => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
    };
    expect(firstRunDismissed(broken)).toBe(false);
    expect(() => dismissFirstRun(broken)).not.toThrow();
    expect(() =>
      forgetFirstRun({
        removeItem: () => {
          throw new Error("denied");
        },
      }),
    ).not.toThrow();
  });
  it("forgets the dismissal after a reset", () => {
    const store = new Map<string, string>();
    const storage = {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
      removeItem: (k: string) => void store.delete(k),
    };
    dismissFirstRun(storage);
    forgetFirstRun(storage);
    expect(firstRunDismissed(storage)).toBe(false);
  });
});

describe("split hint", () => {
  it("is remembered once put away, and forgotten on reset", () => {
    const m = new Map<string, string>();
    const store = {
      getItem: (k: string) => m.get(k) ?? null,
      setItem: (k: string, v: string) => void m.set(k, v),
      removeItem: (k: string) => void m.delete(k),
    };
    expect(coachSeen(store)).toBe(false);
    markCoachSeen(store);
    expect(coachSeen(store)).toBe(true);
    forgetCoach(store);
    expect(coachSeen(store)).toBe(false);
  });

  it("shows again when storage throws", () => {
    const broken = { getItem: () => { throw new Error("no storage"); } };
    expect(coachSeen(broken)).toBe(false);
  });
});

describe("newNetworkName", () => {
  const base = { version: 1, active_profile: "balanced", settings: { low_latency: true, lan_exempt: true }, profiles: [] };
  const home = { id: "gw-a", name: "Home", left: 1, line: { down_mbps: 25 } };
  const phone = { id: "gw-b", name: "Phone" };

  it("names a network with no speed yet while another has one", () => {
    const c: Config = { ...base, line: {}, network: phone, networks: [home] };
    expect(newNetworkName(c)).toBe("Phone");
  });

  it("stays quiet once the network has a speed, or when none has", () => {
    expect(newNetworkName({ ...base, line: { down_mbps: 8 }, network: phone, networks: [home] })).toBeNull();
    expect(newNetworkName({ ...base, line: {}, network: phone })).toBeNull();
    expect(newNetworkName({ ...base, line: {} })).toBeNull();
    expect(newNetworkName(undefined)).toBeNull();
  });
});
