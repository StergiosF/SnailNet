import { describe, expect, it } from "vitest";
import { isPaused, pauseShown } from "./pause";
import type { Config, Rule } from "./types";

const steam: Rule = { target: { kind: "family", id: "steam" }, share: 2500, priority: "normal", enabled: true };

function config(change: Partial<Config> = {}, rules: Rule[] = []): Config {
  return {
    version: 1,
    active_profile: "balanced",
    settings: { low_latency: true, lan_exempt: true, focus_boost: true, gaming_while_playing: false },
    line: { down_mbps: 25, up_mbps: 5 },
    profiles: [
      { id: "balanced", name: "Balanced", rules },
      { id: "streaming", name: "Streaming", rules: [steam] },
    ],
    ...change,
  };
}

describe("pause shown", () => {
  it("hides Pause when nothing is set up", () => {
    expect(pauseShown(config(), 1000)).toBe(false);
    expect(pauseShown(undefined, 1000)).toBe(false);
  });

  it("shows it for a rule in the mode that's on, not another mode's", () => {
    expect(pauseShown(config({}, [steam]), 1000)).toBe(true);
    expect(pauseShown(config({ active_profile: "streaming" }), 1000)).toBe(true);
    // Only Streaming has a rule, and Balanced is on.
    expect(pauseShown(config(), 1000)).toBe(false);
    // A rule turned off does nothing.
    expect(pauseShown(config({}, [{ ...steam, enabled: false }]), 1000)).toBe(false);
    // "Only when the line is free" (no share) still does something.
    expect(pauseShown(config({}, [{ ...steam, share: 0 }]), 1000)).toBe(true);
  });

  it("shows it for a boost running or an app blocked", () => {
    const boost = { target: { kind: "family" as const, id: "steam" }, until: 2000 };
    expect(pauseShown(config({ boost }), 1000)).toBe(true);
    expect(pauseShown(config({ boost }), 2000)).toBe(false);
    expect(pauseShown(config({ boost: { target: boost.target } }), 1e12)).toBe(true);
    expect(pauseShown(config({ blocked: ["epic"] }), 1000)).toBe(true);
    expect(pauseShown(config({ blocked: [] }), 1000)).toBe(false);
  });

  it("keeps Resume in reach while paused", () => {
    expect(isPaused(config({ pause: {} }), 1000)).toBe(true);
    expect(pauseShown(config({ pause: { until: 2000 } }), 1000)).toBe(true);
    expect(isPaused(config({ pause: { until: 2000 } }), 2000)).toBe(false);
    expect(pauseShown(config({ pause: { until: 2000 } }), 2000)).toBe(false);
  });
});
