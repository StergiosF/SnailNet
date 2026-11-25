import { describe, expect, it } from "vitest";
import { ledger } from "./ledger";
import { availableModes, boostLeft, boostShare, copyName, firstOf, gameWhy, idle, inCall, inFront, liveBoost, modeName, users } from "./modes";
import type { Boost, Config, DirStats, Profile, Rule, Target } from "./types";

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

describe("what goes first", () => {
  const config = (active: string, own?: Profile) =>
    ({
      active_profile: active,
      profiles: [profile("balanced"), profile("calls"), profile("focus"), profile("gaming"), ...(own ? [own] : [])],
    }) as Config;
  it("is the active mode's behavior, one mode at a time", () => {
    expect(firstOf(config("balanced"))).toEqual({ calls: false, focus: false, gaming: false });
    expect(firstOf(config("calls"))).toEqual({ calls: true, focus: false, gaming: false });
    expect(firstOf(config("focus"))).toEqual({ calls: false, focus: true, gaming: false });
    expect(firstOf(config("gaming"))).toEqual({ calls: true, focus: true, gaming: true });
    expect(firstOf(null)).toEqual({ calls: false, focus: false, gaming: false });
  });
  it("a user's own mode keeps what it was copied with", () => {
    const own: Profile = { id: "mine", name: "Mine", rules: [], first: { calls: true, focus: true, gaming: true } };
    expect(firstOf(config("mine", own))).toEqual({ calls: true, focus: true, gaming: true });
    const plain: Profile = { id: "plain", name: "Plain", rules: [] };
    expect(firstOf(config("plain", plain))).toEqual({ calls: false, focus: false, gaming: false });
  });
  it("an older service's settings still speak for what goes first", () => {
    const old = {
      active_profile: "balanced",
      profiles: [profile("balanced"), profile("streaming")],
      settings: { focus_boost: true, calls_first: false, gaming_while_playing: true },
    } as unknown as Config;
    expect(firstOf(old)).toEqual({ calls: false, focus: true, gaming: true });
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

  it("gives a countdown share of the length the menu offered", () => {
    expect(boostShare(b, 10_000 - 30 * 60)).toBeCloseTo(0.5);
    expect(boostShare(b, 10_000 - 60 * 60)).toBe(1);
    // More than an hour left: a 4-hour boost.
    expect(boostShare(b, 10_000 - 120 * 60)).toBeCloseTo(0.5);
    expect(boostShare(b, 10_001)).toBe(0);
    expect(boostShare({ target: b.target }, 0)).toBeNull();
  });
});

describe("users", () => {
  it("lists who's moving, busiest first, without unrecognised traffic", () => {
    const u = users(ledger(rules, busy(), label), null, label);
    expect(u.map((x) => x.name)).toEqual(["Steam", "YouTube", "Discord", "Brave"]);
    // Steam moves 10.3 of the 23 Mbps SnailNet shares out.
    expect(u[0]?.status).toBe("a big part of your speed");
    expect(u[0]?.part).toBeCloseTo(10.3 / 23);
    expect(u[0]?.ink).toBe(-1);
    expect(u[1]?.status).toBe("getting all it needs");
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

  it("keeps an app in a call in view, however little it moves", () => {
    const l = ledger(rules, busy(), label);
    const u = users(l, null, label, 2, ["discord"]);
    expect(u.map((x) => x.name)).toEqual(["Steam", "Discord"]);
    // Already in view: nothing moves.
    expect(users(l, null, label, 5, ["discord"]).map((x) => x.name)).toEqual(["Steam", "YouTube", "Discord", "Brave"]);
    // A boost still wins the last place.
    const b = users(l, { target: { kind: "family", id: "brave" } }, label, 2, ["discord"]);
    expect(b.map((x) => x.name)).toEqual(["Steam", "Brave"]);
  });

  it("lists nobody when nothing is using the internet, and calls those apps idle", () => {
    // Discord and Steam just went quiet: steady.ts still holds them in
    // view at a trickle. The glance says nothing is using the internet,
    // so the list must not say they are (the calm-state contradiction).
    const calm: DirStats = {
      shaped_bps: 23 * M,
      classes: [{ name: "everything else", bps: 0.03 * M, drops: 0, queued_bytes: 0 }],
      families: [
        { family: "discord", name: "Discord", bps: 0.02 * M, unidentified_bps: 0, held: true },
        { family: "steam", name: "Steam", bps: 0.01 * M, unidentified_bps: 0, held: true },
      ],
      sites: [],
    };
    const l = ledger(rules, calm, label);
    expect(users(l, null, label)).toEqual([]);
    expect(idle(l).map((x) => x.name)).toEqual(["Discord", "Steam"]);
    // Still in a call, or boosted: those stay in view.
    expect(users(l, null, label, 5, ["discord"]).map((x) => x.name)).toEqual(["Discord"]);
    expect(users(l, { target: { kind: "family", id: "steam" } }, label).map((x) => x.name)).toEqual(["Steam"]);
    // Busy again: nobody is idle, and everyone moving is listed.
    expect(idle(ledger(rules, busy(), label))).toEqual([]);
  });

  it("keeps a busy boosted one that falls past the cut", () => {
    const u = users(ledger(rules, busy(), label), { target: { kind: "family", id: "brave" } }, label, 2);
    expect(u.map((x) => x.name)).toEqual(["Steam", "Brave"]);
  });
});

describe("in front", () => {
  it("is the app the service reports in front", () => {
    expect(inFront({ kind: "family", id: "firefox" }, "firefox", true)).toBe(true);
    expect(inFront({ kind: "family", id: "Firefox" }, "firefox", true)).toBe(true);
    expect(inFront({ kind: "family", id: "steam" }, "firefox", true)).toBe(false);
    expect(inFront({ kind: "site", id: "firefox" }, "firefox", true)).toBe(false);
    expect(inFront({ kind: "family", id: "firefox" }, undefined, true)).toBe(false);
  });
  it("says nothing while the switch is off", () => {
    expect(inFront({ kind: "family", id: "firefox" }, "firefox", false)).toBe(false);
  });
});

describe("in a call", () => {
  it("is an app the service reports in a call", () => {
    expect(inCall({ kind: "family", id: "discord" }, ["discord"], true)).toBe(true);
    expect(inCall({ kind: "family", id: "steam" }, ["discord"], true)).toBe(false);
    expect(inCall({ kind: "site", id: "discord" }, ["discord"], true)).toBe(false);
    expect(inCall({ kind: "family", id: "discord" }, undefined, true)).toBe(false);
    expect(inCall({ kind: "family", id: "Discord" }, ["discord"], true)).toBe(true);
  });
  it("says nothing while the switch is off", () => {
    expect(inCall({ kind: "family", id: "discord" }, ["discord"], false)).toBe(false);
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
