// Rule edits for the rule editor (PLAN.md §11.1, §14.2), as pure functions
// over a profile's rule list. A rule is a *slice* of the split bar when it
// is enabled and its share is above 0; a 0 share means "only when the line
// is free". Every edit keeps the bar's arithmetic (split.ts): giving a rule
// a slice carves it out of Everything else first, taking one away hands it
// back to the other slices in proportion.

import { appRates, percent } from "./format";
import { t } from "./i18n";
import { MIN_RULE, addSlice, normalize, removeSlice, setShare } from "./split";
import type { DirStats, Limit, Priority, Rule, SiteInfo, Target } from "./types";

/** A new slice's starting size: 20%. */
export const NEW_SLICE = 2000;

export function isSlice(r: Rule): boolean {
  return r.enabled && r.share > 0;
}

export function sameTarget(a: Target, b: Target): boolean {
  return a.kind === b.kind && a.id.toLowerCase() === b.id.toLowerCase();
}

/** The split bar's share list for these rules (slices, then Everything else). */
export function barShares(rules: Rule[]): number[] {
  return normalize(rules.filter(isSlice).map((r) => r.share));
}

/** Put a bar share list back onto the rules' slices, in order. */
export function withBar(rules: Rule[], shares: number[]): Rule[] {
  let k = 0;
  return rules.map((r) => (isSlice(r) ? { ...r, share: shares[k++] ?? r.share } : r));
}

/** Where rule `i` sits in the bar, or -1 if it isn't a slice. */
function sliceIndex(rules: Rule[], i: number): number {
  const r = rules[i];
  return r && isSlice(r) ? rules.slice(0, i).filter(isSlice).length : -1;
}

/** Rule `i`'s slice as the bar shows it (after normalising), or null. */
export function shownShare(rules: Rule[], i: number): number | null {
  const k = sliceIndex(rules, i);
  return k < 0 ? null : (barShares(rules)[k] ?? null);
}

function patch(rules: Rule[], i: number, p: Partial<Rule>): Rule[] {
  return rules.map((r, j) => (j === i ? { ...r, ...p } : r));
}

/** Rule `i` becomes a slice of up to `want`, carved from the others. */
function giveSlice(rules: Rule[], i: number, want: number): Rule[] {
  const without = patch(rules, i, { enabled: false });
  const s = addSlice(barShares(without), want);
  const out = withBar(without, [...s.slice(0, -2), s[s.length - 1] ?? 0]);
  return patch(out, i, { enabled: true, share: s[s.length - 2] ?? 0 });
}

/** Rule `i` stops being a slice (`p` says how: off, or share 0); its share
 *  goes back to the other slices. */
function takeSlice(rules: Rule[], i: number, p: Partial<Rule>): Rule[] {
  const k = sliceIndex(rules, i);
  if (k < 0) return patch(rules, i, p);
  return withBar(patch(rules, i, p), removeSlice(barShares(rules), k));
}

/** A new rule for `target`, with a starting slice. */
export function addRule(rules: Rule[], target: Target, want = NEW_SLICE): Rule[] {
  const rule: Rule = { target, share: 0, priority: "normal", enabled: true };
  return giveSlice([...rules, rule], rules.length, want);
}

export function removeRule(rules: Rule[], i: number): Rule[] {
  return takeSlice(rules, i, { enabled: false }).filter((_, j) => j !== i);
}

/** Set rule `i`'s share in basis points; 0 = only when the line is free. */
export function setRuleShare(rules: Rule[], i: number, bp: number): Rule[] {
  const r = rules[i];
  if (!r) return rules;
  // An off rule just remembers the size for when it's turned on.
  if (!r.enabled) return patch(rules, i, { share: Math.max(0, Math.round(bp)) });
  if (bp <= 0) return takeSlice(rules, i, { share: 0 });
  const k = sliceIndex(rules, i);
  if (k < 0) {
    const given = giveSlice(rules, i, Math.max(MIN_RULE, bp));
    return isSlice(given[i] as Rule) ? setRuleShare(given, i, bp) : given;
  }
  return withBar(rules, setShare(barShares(rules), k, bp));
}

/** Turn rule `i` on (it gets its old slice back, as far as there's room)
 *  or off (its slice goes back to the others; its size is remembered). */
export function setEnabled(rules: Rule[], i: number, on: boolean): Rule[] {
  const r = rules[i];
  if (!r || r.enabled === on) return rules;
  if (!on) return takeSlice(rules, i, { enabled: false });
  return r.share > 0 ? giveSlice(rules, i, r.share) : patch(rules, i, { enabled: true });
}

export function setPriority(rules: Rule[], i: number, priority: Priority): Rule[] {
  return patch(rules, i, { priority });
}

/** Set or clear one direction's ceiling of rule `i`. */
export function setCeiling(
  rules: Rule[],
  i: number,
  dir: "down" | "up",
  limit: Limit | null,
): Rule[] {
  const r = rules[i];
  if (!r) return rules;
  const ceiling = { ...r.ceiling };
  if (limit === null) delete ceiling[dir];
  else ceiling[dir] = formatLimit(limit);
  return patch(rules, i, {
    ceiling: ceiling.down === undefined && ceiling.up === undefined ? undefined : ceiling,
  });
}

/** `"20%"` / `"5 Mbps"` as snailnet-core writes them → a Limit. */
export function parseLimit(text: string | undefined): Limit | null {
  if (text === undefined) return null;
  const s = text.trim().toLowerCase();
  if (s.endsWith("%")) {
    const v = Number(s.slice(0, -1).trim());
    return Number.isInteger(v) ? { unit: "percent", value: v } : null;
  }
  if (s.endsWith("mbps")) {
    const v = Number(s.slice(0, -4).trim());
    return Number.isFinite(v) ? { unit: "mbps", value: v } : null;
  }
  return null;
}

export function formatLimit(l: Limit): string {
  return l.unit === "percent" ? `${l.value}%` : `${l.value} Mbps`;
}

/** The ranges snailnet-core accepts: 1–100%, 0.1–10000 Mbps. */
export function clampLimit(l: Limit): Limit {
  return l.unit === "percent"
    ? { unit: "percent", value: Math.min(100, Math.max(1, Math.round(l.value))) }
    : { unit: "mbps", value: Math.min(10_000, Math.max(0.1, Math.round(l.value * 10) / 10)) };
}

/** One row of the app list: an app or a site with traffic now, or a rule
 *  whose target is quiet. */
export interface Row {
  key: string;
  target: Target;
  name: string;
  down: number;
  up: number;
  /** Index of its rule in the profile, or -1. */
  rule: number;
  /** Has no traffic now (listed because it has a rule). */
  idle: boolean;
  /** Can't have a rule (traffic SnailNet couldn't tie to an app). */
  fixed: boolean;
}

const UNKNOWN_FAMILY = "unknown";

export function listRows(
  down: DirStats | null | undefined,
  up: DirStats | null | undefined,
  rules: Rule[],
  label: (t: Target) => string,
): Row[] {
  const ruleOf = (t: Target) => rules.findIndex((r) => sameTarget(r.target, t));
  const rows: Row[] = appRates(down, up).map((a) => {
    const target: Target = { kind: "family", id: a.family };
    return {
      key: `family:${a.family}`,
      target,
      name: a.name,
      down: a.down,
      up: a.up,
      rule: ruleOf(target),
      idle: false,
      fixed: a.family === UNKNOWN_FAMILY,
    };
  });
  const sites = new Map<string, Row>();
  const addSites = (dir: DirStats | null | undefined, key: "down" | "up") => {
    for (const s of dir?.sites ?? []) {
      const target: Target = { kind: "site", id: s.site };
      const row = sites.get(s.site) ?? {
        key: `site:${s.site}`,
        target,
        name: s.name,
        down: 0,
        up: 0,
        rule: ruleOf(target),
        idle: false,
        fixed: false,
      };
      row[key] += s.bps;
      sites.set(s.site, row);
    }
  };
  addSites(down, "down");
  addSites(up, "up");
  const active = [...rows, ...sites.values()].sort((a, b) => b.down + b.up - (a.down + a.up));
  const quiet: Row[] = rules
    .map((r, i) => ({ r, i }))
    .filter(({ r }) => !active.some((row) => sameTarget(row.target, r.target)))
    .map(({ r, i }) => ({
      key: `${r.target.kind}:${r.target.id.toLowerCase()}`,
      target: r.target,
      name: label(r.target),
      down: 0,
      up: 0,
      rule: i,
      idle: true,
      fixed: false,
    }))
    .sort((a, b) => a.name.localeCompare(b.name));
  return [...active, ...quiet];
}

/** Things a new rule can name: known sites, then apps seen, without the
 *  ones that already have a rule; filtered by `query` (name or id). */
export function pickable(
  sites: SiteInfo[],
  apps: { family: string; name: string }[],
  rules: Rule[],
  query: string,
): { target: Target; name: string }[] {
  const q = query.trim().toLowerCase();
  const seen = new Set<string>();
  const out: { target: Target; name: string }[] = [];
  const push = (target: Target, name: string) => {
    const key = `${target.kind}:${target.id}`;
    if (seen.has(key) || rules.some((r) => sameTarget(r.target, target))) return;
    if (q && !name.toLowerCase().includes(q) && !target.id.includes(q)) return;
    seen.add(key);
    out.push({ target, name });
  };
  for (const s of sites) push({ kind: "site", id: s.id }, s.name);
  const appList = [...apps]
    .filter((a) => a.family !== UNKNOWN_FAMILY)
    .sort((a, b) => a.name.localeCompare(b.name));
  for (const a of appList) push({ kind: "family", id: a.family }, a.name);
  return out;
}

/** A row's slice chip: "40% · Low", "When free · High", "Rule off", or
 *  "Everything else" when `i` is -1 (no rule). */
export function ruleChip(rules: Rule[], i: number): string {
  const rule = rules[i];
  if (!rule) return t("list.chip.everything");
  if (!rule.enabled) return t("list.chip.off");
  const share = shownShare(rules, i);
  return t("list.chip", {
    share: share === null ? t("list.chip.free") : percent(share),
    priority: t(`priority.${rule.priority}`),
  });
}
