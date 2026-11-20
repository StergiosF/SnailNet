import { describe, expect, it } from "vitest";
import { edit, isDirty, isSavable, isStale, sameRules, startDraft, wasShare } from "./draft";
import { addRule, removeRule, setCeiling, setPriority, stepShare } from "./rules";
import type { Rule, Target } from "./types";

const fam = (id: string): Target => ({ kind: "family", id });
const rule = (id: string, share: number, extra: Partial<Rule> = {}): Rule => ({
  target: fam(id),
  share,
  priority: "normal",
  enabled: true,
  ...extra,
});
const saved = [rule("steam", 4000), rule("brave", 3000), rule("updates", 0, { priority: "low" })];

describe("the Split page's draft", () => {
  it("starts clean", () => {
    const d = startDraft("balanced", saved);
    expect(isDirty(d)).toBe(false);
    expect(isDirty(null)).toBe(false);
  });

  it("is dirty after a step, and clean again when the rules are as saved", () => {
    const d = startDraft("balanced", saved);
    const up = edit(d, stepShare(d.rules, 0, 1));
    expect(isDirty(up)).toBe(true);
    expect(isDirty(edit(up, saved.map((r) => ({ ...r }))))).toBe(false);
  });

  it("counts every kind of edit", () => {
    const d = startDraft("balanced", saved);
    expect(isDirty(edit(d, setPriority(d.rules, 1, "high")))).toBe(true);
    expect(isDirty(edit(d, setCeiling(d.rules, 1, "down", { unit: "mbps", value: 5 })))).toBe(true);
    expect(isDirty(edit(d, removeRule(d.rules, 2)))).toBe(true);
    expect(isDirty(edit(d, addRule(d.rules, fam("discord"))))).toBe(true);
  });

  it("doesn't count key order or an empty ceiling as a change", () => {
    const a = [rule("steam", 4000)];
    const b = [{ enabled: true, priority: "normal", share: 4000, target: { id: "Steam", kind: "family" } } as Rule];
    expect(sameRules(a, b)).toBe(true);
    expect(sameRules(a, [rule("steam", 4000, { ceiling: undefined })])).toBe(true);
    expect(sameRules(a, [rule("steam", 4500)])).toBe(false);
    expect(sameRules(a, [])).toBe(false);
  });

  it("goes stale when the rules change somewhere else, or the mode switches", () => {
    const d = edit(startDraft("balanced", saved), stepShare(saved, 0, 1));
    expect(isStale(d, "balanced", saved)).toBe(false);
    expect(isStale(d, "balanced", setPriority(saved, 0, "high"))).toBe(true);
    expect(isStale(d, "gaming", saved)).toBe(true);
    expect(isStale(null, "gaming", saved)).toBe(false);
  });

  it("only saves a split that adds up", () => {
    const d = startDraft("balanced", saved);
    expect(isSavable(edit(d, stepShare(d.rules, 0, 1)))).toBe(true);
    expect(isSavable(edit(d, addRule(d.rules, fam("discord"))))).toBe(true);
  });

  it("says what a changed slice was", () => {
    const d = startDraft("balanced", saved);
    const up = edit(d, stepShare(d.rules, 0, 1));
    expect(wasShare(up, 0)).toBe(4000);
    // Brave made room, so it changed too; Everything else as well.
    expect(wasShare(up, 1)).toBe(3000);
    expect(wasShare(up, -1)).toBe(3000);
    // Unchanged, only when free, or new: nothing to say.
    expect(wasShare(d, 0)).toBeNull();
    expect(wasShare(up, 2)).toBeNull();
    const added = edit(d, addRule(d.rules, fam("discord")));
    expect(wasShare(added, 3)).toBeNull();
  });
});
