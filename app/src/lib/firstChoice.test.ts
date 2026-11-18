import { describe, expect, it } from "vitest";
import { firstChoiceOf, withFirstChoice } from "./firstChoice";
import type { Settings } from "./types";

const base: Settings = { low_latency: true, lan_exempt: true, focus_boost: true, gaming_while_playing: false };

describe("first choice", () => {
  it("is automatic when the lists are missing", () => {
    expect(firstChoiceOf(base, "hades")).toBe("auto");
    expect(firstChoiceOf(undefined, "hades")).toBe("auto");
  });

  it("reads either list, ignoring case", () => {
    expect(firstChoiceOf({ ...base, games: ["Hades"] }, "hades")).toBe("game");
    expect(firstChoiceOf({ ...base, never_first: ["obs"] }, "OBS")).toBe("never");
  });

  it("moves an app between the lists and keeps the others", () => {
    const s = { ...base, games: ["Hades", "cs2"], never_first: ["obs"] };
    expect(withFirstChoice(s, "hades", "never")).toEqual({ games: ["cs2"], never_first: ["obs", "hades"] });
    expect(withFirstChoice(s, "obs", "game")).toEqual({ games: ["Hades", "cs2", "obs"], never_first: [] });
    expect(withFirstChoice(s, "cs2", "auto")).toEqual({ games: ["Hades"], never_first: ["obs"] });
    expect(withFirstChoice(base, "x", "game")).toEqual({ games: ["x"], never_first: [] });
  });
});
