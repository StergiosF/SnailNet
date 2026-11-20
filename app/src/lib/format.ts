// Numbers as PLAN.md §14.7 writes them: Mbps with one decimal below 100 and
// none from 100 up; percentages as integers.

import type { AppInfo, ServiceState, Target } from "./types";

/** "36.8", "120", "0.0" (no unit). */
export function mbps(bps: number): string {
  const v = Math.max(0, bps) / 1e6;
  return v < 99.95 ? v.toFixed(1) : Math.round(v).toString();
}

/** An app's speed on its row: like [`mbps`], but traffic too small to
 *  show as 0.1 reads "<0.1", never "0.0" (a voice call is ~0.04 Mbps). */
export function rowMbps(bps: number): string {
  return bps > 0 && bps < 50_000 ? "<0.1" : mbps(bps);
}

/** Basis points → "45%". */
export function percent(basisPoints: number): string {
  return `${Math.round(basisPoints / 100)}%`;
}

/** A rule target as people read it: the app's or site's own name when
 *  known ("Brave", "YouTube"), else its id with a capital first letter. */
export function targetLabel(
  target: Target,
  apps: AppInfo[],
  siteNames: Record<string, string>,
): string {
  const known =
    target.kind === "site"
      ? siteNames[target.id]
      : target.kind === "family"
        ? apps.find((a) => a.family === target.id)?.name
        : undefined;
  return known ?? target.id.charAt(0).toUpperCase() + target.id.slice(1);
}

/** The scheduler class a rule becomes (`family:brave`, `site:youtube`). */
export function className(target: Target): string {
  return `${target.kind}:${target.id.toLowerCase()}`;
}

export type StateKind = "starting" | "shaping" | "unshaped" | "retrying" | "safe_mode" | "paused" | "measuring";

export function stateKind(s: ServiceState): StateKind {
  if (s === "Starting") return "starting";
  if (s === "Shaping") return "shaping";
  if (s === "Unshaped") return "unshaped";
  if (s === "Measuring") return "measuring";
  if ("Paused" in s) return "paused";
  return "Retrying" in s ? "retrying" : "safe_mode";
}

/** "12 Nov": the day of a speed test (Unix seconds), as the locale writes
 *  it. */
export function shortDate(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleDateString(undefined, { day: "numeric", month: "short" });
}
