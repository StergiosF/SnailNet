import { describe, expect, it } from "vitest";
import { brandOf, contrast, inkOn } from "./index";
import credits from "./credits.json";

describe("brand logos", () => {
  it("knows the websites people boost", () => {
    for (const id of ["youtube", "twitch", "netflix", "spotify", "kick", "zoom", "google-meet"]) {
      expect(brandOf({ kind: "site", id })?.path.length).toBeGreaterThan(10);
    }
  });

  it("knows browsers and launchers by family", () => {
    expect(brandOf({ kind: "family", id: "brave" })).not.toBeNull();
    expect(brandOf({ kind: "family", id: "Chrome" })).not.toBeNull();
    expect(brandOf({ kind: "family", id: "steam" })).not.toBeNull();
  });

  it("has no look-alike for brands it doesn't carry", () => {
    expect(brandOf({ kind: "site", id: "disney-plus" })).toBeNull();
    expect(brandOf({ kind: "site", id: "teams" })).toBeNull();
    expect(brandOf({ kind: "family", id: "edge" })).toBeNull();
    expect(brandOf({ kind: "family", id: "system" })).toBeNull();
    expect(brandOf({ kind: "exe", id: "brave.exe" })).toBeNull();
  });

  it("keeps a brand's colour only where it reads on the dark tile", () => {
    expect(inkOn("FF0000")).toBe("#ff0000");
    // Steam's black would vanish: cream instead.
    expect(inkOn("000000")).toBe("#f3eee2");
    expect(brandOf({ kind: "family", id: "steam" })?.ink).toBe("#f3eee2");
    expect(contrast("#000000", "#ffffff")).toBeCloseTo(21, 0);
  });

  it("credits every logo it ships", () => {
    expect(credits.every((c) => c.licence === "CC0-1.0" && c.trademark)).toBe(true);
    expect(credits.length).toBe(20);
  });
});
