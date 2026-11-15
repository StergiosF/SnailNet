// Numbers as PLAN.md §14.7 writes them: Mbps with one decimal below 100 and
// none from 100 up; percentages as integers.

import type { DirStats, FamilyTick, ServiceState } from "./types";

/** "36.8", "120", "0.0" (no unit). */
export function mbps(bps: number): string {
  const v = Math.max(0, bps) / 1e6;
  return v < 99.95 ? v.toFixed(1) : Math.round(v).toString();
}

/** Basis points → "45%". */
export function percent(basisPoints: number): string {
  return `${Math.round(basisPoints / 100)}%`;
}

/** What a direction is moving right now, all classes together. */
export function totalBps(dir: DirStats | null | undefined): number {
  return dir ? dir.classes.reduce((sum, c) => sum + c.bps, 0) : 0;
}

export interface AppRate {
  family: string;
  name: string;
  down: number;
  up: number;
}

/** One row per app family with both directions, busiest first. */
export function appRates(
  down: DirStats | null | undefined,
  up: DirStats | null | undefined,
): AppRate[] {
  const rows = new Map<string, AppRate>();
  const add = (list: FamilyTick[] | undefined, key: "down" | "up") => {
    for (const f of list ?? []) {
      const row = rows.get(f.family) ?? { family: f.family, name: f.name, down: 0, up: 0 };
      row[key] += f.bps;
      rows.set(f.family, row);
    }
  };
  add(down?.families, "down");
  add(up?.families, "up");
  return [...rows.values()].sort((a, b) => b.down + b.up - (a.down + a.up));
}

export type StateKind = "starting" | "shaping" | "unshaped" | "retrying" | "safe_mode";

export function stateKind(s: ServiceState): StateKind {
  if (s === "Starting") return "starting";
  if (s === "Shaping") return "shaping";
  if (s === "Unshaped") return "unshaped";
  return "Retrying" in s ? "retrying" : "safe_mode";
}
