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

/** Raw exe file names people shouldn't have to read: the common name of
 *  the app they belong to. The family pack names these on newer services;
 *  this covers older ones that still report the exe (2026-11-25). */
const EXE_NAMES: Record<string, string> = {
  "com.docker.build.exe": "Docker",
  "com.docker.backend.exe": "Docker",
  "com.docker.service.exe": "Docker",
  "docker desktop.exe": "Docker",
  "docker.exe": "Docker",
  "dockerd.exe": "Docker",
  "opencode.exe": "OpenCode",
};

/** A family id that is really an exe path (the service's fallback for
 *  unlisted apps) → its lowercased file name; plain ids pass through. */
export function exeBase(id: string): string {
  const i = Math.max(id.lastIndexOf("\\"), id.lastIndexOf("/"));
  return (i >= 0 ? id.slice(i + 1) : id).toLowerCase();
}

/** The name to show for an app: the common name for a raw exe, else the
 *  service's name without a ".exe" tail ("MyGame", not "MyGame.exe"). */
export function appLabel(name: string, family: string): string {
  const known = EXE_NAMES[exeBase(family)];
  if (known) return known;
  return name.toLowerCase().endsWith(".exe") ? name.slice(0, -4) : name;
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
  if (known !== undefined) return appLabel(known, target.id);
  return target.id.charAt(0).toUpperCase() + target.id.slice(1);
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
