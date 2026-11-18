import { describe, expect, it } from "vitest";
import { ledger } from "./ledger";
import { availableModes, boostLeft, copyName, gameWhy, liveBoost, modeName, summary, users } from "./modes";
import type { Boost, DirStats, Profile, Rule, Target } from "./types";

const M = 1_000_000;
const label = (t: Target) =>
  ({ youtube: "YouTube", brave: "Brave", steam: "Steam", discord: "Discord", twitch: "Twitch" })[t.id] ?? t.id;

const rules: Rule[] = [
  { target: { kind: "site", id: "youtube" }, share: 4500, priority: "normal", enabled: true },
  { target: { kind: "family", id: "brave" }, share: 3000, priority: "normal", enabled: true },
  { target: { kind: "family", id: "epic" }, share: 0, priority: "low", enabled: true },
];

/** YouTube 4K in Brave, Discord, a Steam download (docs/HANDOFF-ui.md). */
function busy(): DirStats {
  return {
    shaped_bps: 23 * M,
    classes: [
      { name: "site:youtube", bps: 9.8 * M, drops: 0, queued_bytes: 0 },
      { name: "family:brave", bps: 0.4 * M, drops: 0, queued_bytes: 0 },
      { name: "family:epic", bps: 0, drops: 0, queued_bytes: 0 },
      { name: "everything else", bps: 12.8 * M, drops: 3, queued_bytes: 60_000 },
    ],
    families: [
      { family: "steam", name: "Steam", bps: 10.3 * M, unidentified_bps: 0 },
      { family: "brave", name: "Brave", bps: 10.2 * M, unidentified_bps: 0 },
      { family: "discord", name: "Discord", bps: 2.4 * M, unidentified_bps: 0 },
      { family: "unknown", name: "Unknown", bps: 0.5 * M, unidentified_bps: 0 },
    ],
    sites: [{ site: "youtube", name: "YouTube", family: "brave", bps: 9.8 * M }],
  };
}

const profile = (id: string, name = id): Profile => ({ id, name, rules: [] });

describe("modes", () => {
  it("shows the modes the config has, in order", () => {
    const ps = [profile("gaming"), profile("balanced"), profile("mine", "Mine")];
    const shown = availableModes(ps);
    expect(shown.map((m) => m.id)).toEqual(["balanced", "gaming", "mine"]);
    expect(shown.map((m) => m.own)).toEqual([false, false, true]);
    expect(shown[1]?.name).toBe("Gaming");
    expect(shown[2]?.name).toBe("Mine");
    expect(modeName(profile("quiet-downloads", "x"))).toBe("Quiet downloads");
    expect(modeName(profile("mine", "Mine"))).toBe("Mine");
    expect(modeName(null)).toBe("");
  });
});

describe("copy name", () => {
  it("finds a name no mode has, ignoring case", () => {
    expect(copyName("Gaming", [profile("gaming", "Gaming")])).toBe("Gaming copy");
    const ps = [profile("a", "gaming COPY"), profile("b", "Gaming copy 2")];
    expect(copyName("Gaming", ps)).toBe("Gaming copy 3");
  });
});

describe("boost", () => {
  const b: Boost = { target: { kind: "family", id: "steam" }, until: 10_000 };

  it("is live until its time", () => {
    expect(liveBoost(b, 9_999)).toBe(b);
    expect(liveBoost(b, 10_000)).toBeNull();
    expect(liveBoost({ target: b.target }, 1e12)).not.toBeNull();
    expect(liveBoost(undefined, 0)).toBeNull();
  });

  it("says what's left in words", () => {
    expect(boostLeft(b, 10_000 - 52 * 60)).toBe("52 min left");
    expect(boostLeft(b, 10_000 - 10)).toBe("1 min left");
    expect(boostLeft(b, 10_000 - 65 * 60)).toBe("1 h 5 min left");
    expect(boostLeft(b, 10_000 - 4 * 3600)).toBe("4 h left");
    expect(boostLeft({ target: b.target }, 0)).toBe("until you stop it");
  });
});

describe("users", () => {
  it("lists who's moving, busiest first, without unrecognised traffic", () => {
    const u = users(ledger(rules, busy(), label), null, label);
    expect(u.map((x) => x.name)).toEqual(["Steam", "YouTube", "Discord", "Brave"]);
    expect(u[0]?.status).toBe("in Everything else");
    expect(u[1]?.status).toBe("gets all it asks for");
    expect(u.some((x) => x.boosted)).toBe(false);
  });

  it("marks the boosted one, and lists it even when it's quiet", () => {
    const l = ledger(rules, busy(), label);
    const steam = users(l, { target: { kind: "family", id: "Steam" } }, label);
    expect(steam.find((x) => x.boosted)?.name).toBe("Steam");

    const twitch = users(l, { target: { kind: "site", id: "twitch" } }, label, 3);
    expect(twitch.map((x) => x.name)).toEqual(["Twitch", "Steam", "YouTube"]);
    expect(twitch[0]).toMatchObject({ boosted: true, bps: 0 });
  });

  it("keeps a busy boosted one that falls past the cut", () => {
    const u = users(ledger(rules, busy(), label), { target: { kind: "family", id: "brave" } }, label, 2);
    expect(u.map((x) => x.name)).toEqual(["Steam", "Brave"]);
  });
});

describe("summary", () => {
  it("has the slices with a share and counts the free-only ones", () => {
    const s = summary(ledger(rules, busy(), label));
    expect(s.slices.map((x) => x.name)).toEqual(["YouTube", "Brave", "Everything else"]);
    expect(s.free).toBe(1);
  });
});

describe("game why", () => {
  const games = [{ family: "hades", why: "library" as const }];
  it("says why a running game goes first", () => {
    expect(gameWhy({ kind: "family", id: "hades" }, games, true)).toBe("library");
    expect(gameWhy({ kind: "family", id: "obs" }, games, true)).toBeNull();
    expect(gameWhy({ kind: "site", id: "hades" }, games, true)).toBeNull();
    expect(gameWhy({ kind: "family", id: "hades" }, undefined, true)).toBeNull();
  });
  it("says nothing while focus boost is off", () => {
    expect(gameWhy({ kind: "family", id: "hades" }, games, false)).toBeNull();
  });
});
