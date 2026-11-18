import { describe, expect, it, vi } from "vitest";
import { noticeOf } from "./notice";

const actions = { setSpeed: vi.fn(), resume: vi.fn(), startService: vi.fn() };
const connected = { kind: "connected", service_version: "0.1.0" } as const;

describe("noticeOf", () => {
  it("offers to start the service when it isn't running", () => {
    const n = noticeOf({ kind: "not_running" }, null, actions, 0);
    expect(n?.title).toBe("SnailNet isn't running");
    n?.action?.onclick();
    expect(actions.startService).toHaveBeenCalledOnce();
  });

  it("says when the PC is offline", () => {
    const n = noticeOf(connected, "Shaping", actions, 0, false);
    expect(n?.title).toBe("No internet connection");
    expect(n?.action).toBeUndefined();
  });

  it("puts a missing service before being offline", () => {
    expect(noticeOf({ kind: "not_running" }, null, actions, 0, false)?.title).toBe("SnailNet isn't running");
  });

  it("is quiet while shaping and online", () => {
    expect(noticeOf(connected, "Shaping", actions, 0, true)).toBeNull();
  });
});
