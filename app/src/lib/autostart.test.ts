import { describe, expect, it } from "vitest";
import { askAutostartNow } from "./autostart";
import type { Link, Snapshot } from "./types";

const connected: Link = { kind: "connected", service_version: "0.0.7" };
const snap = {} as Snapshot;

describe("askAutostartNow", () => {
  it("asks once the service has answered", () => {
    expect(askAutostartNow(true, false, connected, snap)).toBe(true);
  });
  it("asks even when SnailNet's service isn't running", () => {
    expect(askAutostartNow(true, false, { kind: "not_running" }, null)).toBe(true);
  });
  it("never when already answered", () => {
    expect(askAutostartNow(false, false, connected, snap)).toBe(false);
  });
  it("waits for first run to finish", () => {
    expect(askAutostartNow(true, true, connected, snap)).toBe(false);
  });
  it("waits while connecting or before the config arrives", () => {
    expect(askAutostartNow(true, false, { kind: "connecting" }, null)).toBe(false);
    expect(askAutostartNow(true, false, connected, null)).toBe(false);
  });
});
