import { describe, expect, it } from "vitest";
import { glance, ledger, membersOf, parts, statusText, top, unnamed } from "./ledger";
import { setOther } from "./split";
import type { DirStats, Rule, Target } from "./types";

const M = 1_000_000;
const label = (t: Target) => ({ youtube: "YouTube", brave: "Brave", steam: "Steam" })[t.id] ?? t.id;

const rules: Rule[] = [
  { target: { kind: "site", id: "youtube" }, share: 4500, priority: "normal", enabled: true },
  { target: { kind: "family", id: "brave" }, share: 3000, priority: "normal", enabled: true },
];

/** The user's typical moment: YouTube 4K in Brave, Discord, a Steam
 *  download taking whatever it can (docs/HANDOFF-ui.md). */
function busy(): DirStats {
  return {
    shaped_bps: 23 * M,
    classes: [
      { name: "site:youtube", bps: 9.8 * M, drops: 0, queued_bytes: 0 },
      { name: "family:brave", bps: 0.4 * M, drops: 0, queued_bytes: 0 },
      { name: "everything else", bps: 12.8 * M, drops: 3, queued_bytes: 60_000 },
    ],
    families: [
      { family: "steam", name: "Steam", bps: 10.3 * M, unidentified_bps: 0 },
      { family: "brave", name: "Brave", bps: 10.2 * M, unidentified_bps: 0 },
      { family: "discord", name: "Discord", bps: 2.4 * M, unidentified_bps: 0 },
      { family: "claude", name: "Claude Code", bps: 0.1 * M, unidentified_bps: 0 },
    ],
    sites: [{ site: "youtube", name: "YouTube", family: "brave", bps: 9.8 * M }],
  };
}

describe("ledger", () => {
  it("lists the rules in order, then Everything else with its apps", () => {
    const l = ledger(rules, busy(), label);
    expect(l.slices.map((s) => s.name)).toEqual(["YouTube", "Brave", "Everything else"]);
    expect(l.slices.map((s) => s.share)).toEqual([4500, 3000, 2500]);
    expect(l.slices[2]?.members.map((m) => m.name)).toEqual(["Steam", "Discord", "Claude Code"]);
    expect(l.busy).toBe(true);
  });

  it("says what each slice is doing", () => {
    const [yt, brave, other] = ledger(rules, busy(), label).slices;
    expect(yt?.status).toEqual({ kind: "all" });
    expect(brave?.status.kind).toBe("lending");
    expect(statusText(brave!.status)).toBe("6.5 Mbps of its slice spare for others");
    expect(other?.status.kind).toBe("borrowing");
    expect(other?.kept).toBeCloseTo(5.75 * M);
  });

  it("keeps a site's traffic out of its browser's row in Everything else", () => {
    const off: Rule[] = [rules[0]!, { ...rules[1]!, enabled: false }];
    const members = membersOf(busy(), off);
    expect(members.find((m) => m.name === "Brave")?.bps).toBeCloseTo(0.4 * M);
  });

  it("drops quiet apps and marks unrecognised traffic", () => {
    const d = busy();
    d.families.push({ family: "unknown", name: "Unknown", bps: 0.2 * M, unidentified_bps: 0 });
    d.families.push({ family: "idle", name: "Idle", bps: 10_000, unidentified_bps: 0 });
    const members = membersOf(d, rules);
    expect(members.map((m) => m.name)).not.toContain("Idle");
    expect(members.find((m) => m.name === "Unknown")?.fixed).toBe(true);
  });

  it("shows off and only-when-free rules without a slice", () => {
    const r: Rule[] = [
      { ...rules[0]!, share: 0 },
      { ...rules[1]!, enabled: false },
    ];
    const [yt, brave, other] = ledger(r, busy(), label).slices;
    expect(yt?.status.kind).toBe("free");
    expect(yt?.share).toBeNull();
    expect(brave?.status.kind).toBe("off");
    expect(other?.share).toBe(10_000);
  });

  it("has no kept rate while not shaping", () => {
    const d = busy();
    d.shaped_bps = 0;
    const l = ledger(rules, d, label);
    expect(l.shaped).toBeNull();
    expect(l.slices[0]?.kept).toBeNull();
    expect(l.slices[0]?.status.kind).toBe("all");
  });
});

describe("glance", () => {
  it("names the biggest user and who still gets what they ask for", () => {
    const g = glance(ledger(rules, busy(), label), 25 * M);
    expect(g.title).toBe("Steam is using the most of your line.");
    expect(g.detail).toBe(
      "It runs on room Brave isn't using. YouTube and Discord still get all they ask for.",
    );
  });

  it("says there's room when the line isn't busy", () => {
    const d = busy();
    d.classes = [
      { name: "site:youtube", bps: 2 * M, drops: 0, queued_bytes: 0 },
      { name: "everything else", bps: 1 * M, drops: 0, queued_bytes: 0 },
    ];
    d.families = [{ family: "brave", name: "Brave", bps: 2 * M, unidentified_bps: 0 }];
    d.sites = [{ site: "youtube", name: "YouTube", family: "brave", bps: 2 * M }];
    const g = glance(ledger(rules, d, label), 25 * M);
    expect(g.title).toBe("YouTube is using the most right now.");
    expect(g.detail).toBe("3.0 of 25.0 Mbps in use. There's room for everyone.");
  });

  it("says when nothing is using the internet", () => {
    expect(glance(ledger(rules, null, label), 25 * M).title).toBe(
      "Nothing is using your internet right now.",
    );
  });

  it("says who else wants more when a slice is held", () => {
    const d = busy();
    d.classes[0] = { name: "site:youtube", bps: 10 * M, drops: 2, queued_bytes: 40_000 };
    const g = glance(ledger(rules, d, label), 25 * M);
    expect(g.detail).toContain("YouTube wants more room too.");
  });

  it("says an app in Everything else is waiting once the rule caught it", () => {
    // No rules: Steam's 16 connections take the line, Brave's one video waits.
    const d = busy();
    d.classes = [{ name: "everything else", bps: 22 * M, drops: 9, queued_bytes: 80_000 }];
    d.families = [
      { family: "steam", name: "Steam", bps: 20.6 * M, unidentified_bps: 0 },
      { family: "brave", name: "Brave", bps: 1.4 * M, unidentified_bps: 0 },
    ];
    d.sites = [];
    const l = ledger([], d, label, ["brave"]);
    expect(l.slices[0]?.members.map((m) => [m.name, m.waiting])).toEqual([
      ["Steam", false],
      ["Brave", true],
    ]);
    const g = glance(l, 25 * M);
    expect(g.title).toBe("Steam is using the most of your line.");
    expect(g.detail).toBe("Brave wants more room too.");
    // Without the rule's word, Brave is just a smaller app.
    expect(glance(ledger([], d, label), 25 * M).detail).not.toContain("wants more room");
  });

  it("never calls a slice the lender while one of its own apps waits", () => {
    // Streaming: Steam has a slice and runs over it; Brave, inside
    // Everything else, is left waiting. Everything else's numbers say it
    // isn't using its part, but it isn't lending room Brave wants.
    const d = busy();
    d.classes = [
      { name: "family:steam", bps: 20 * M, drops: 0, queued_bytes: 0 },
      { name: "everything else", bps: 1.4 * M, drops: 0, queued_bytes: 0 },
    ];
    d.families = [
      { family: "steam", name: "Steam", bps: 20 * M, unidentified_bps: 0 },
      { family: "brave", name: "Brave", bps: 1.4 * M, unidentified_bps: 0 },
    ];
    d.sites = [];
    const steam: Rule = { target: { kind: "family", id: "steam" }, share: 2500, priority: "normal", enabled: true };
    const g = glance(ledger([steam], d, label, ["brave"]), 25 * M);
    expect(g.detail).not.toContain("Everything else isn't using");
    expect(g.detail).toBe("It runs on spare room. Brave wants more room too.");
  });

  it("keeps a waiting app listed even when it moves almost nothing", () => {
    const d = busy();
    d.families.push({ family: "zoom", name: "Zoom", bps: 0.01 * M, unidentified_bps: 0 });
    const m = membersOf(d, rules, ["zoom"]).find((x) => x.name === "Zoom");
    expect(m?.waiting).toBe(true);
    expect(membersOf(d, rules).some((x) => x.name === "Zoom")).toBe(false);
  });
});

describe("parts", () => {
  it("splits Everything else by app in the live strip", () => {
    const p = parts(ledger(rules, busy(), label));
    expect(p.map((x) => x.key)).toEqual([
      "site:youtube",
      "family:brave",
      "family:steam",
      "family:discord",
      "family:claude",
    ]);
  });
});

describe("setOther", () => {
  it("grows Everything else from the rules in proportion", () => {
    expect(setOther([4500, 3000, 2500], 3500)).toEqual([3897, 2603, 3500]);
  });

  it("keeps the 5% floor and 1% per rule", () => {
    expect(setOther([4500, 3000, 2500], 0)).toEqual([5705, 3795, 500]);
    expect(setOther([4500, 3000, 2500], 10_000)).toEqual([100, 100, 9800]);
  });

  it("is everything with no rules", () => {
    expect(setOther([10_000], 500)).toEqual([10_000]);
  });
});

describe("unnamed", () => {
  const fam = (name: string, unidentified_bps: number) => ({ family: name, name, bps: unidentified_bps * 2, unidentified_bps });
  const dir = (families: ReturnType<typeof fam>[]) => ({ shaped_bps: 0, classes: [], families, sites: [] });

  it("is quiet below the quiet line", () => {
    expect(unnamed(dir([fam("Brave", 10_000)]))).toBeNull();
    expect(unnamed(null)).toBeNull();
  });

  it("sums hidden website traffic, busiest app first", () => {
    expect(unnamed(dir([fam("Brave", 40_000), fam("Chrome", 900_000), fam("Steam", 0)]))).toEqual({
      bps: 940_000,
      apps: ["Chrome", "Brave"],
    });
  });
});

describe("top", () => {
  it("names the busiest app or slice, the glance's own", () => {
    const l = ledger(rules, busy(), label);
    expect(top(l)).toBe("Steam");
    expect(glance(l, null).title).toContain("Steam");
  });

  it("is null when nobody moves data", () => {
    expect(top(ledger(rules, null, label))).toBeNull();
  });
});
