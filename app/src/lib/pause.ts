// When the window offers Pause (the user, 2026-11-20: "the pause button
// should appear only when snailnet is being used; when I have not enabled
// any boost or any split or anything at all it should not appear"). The
// tray menu always has Pause, so one click still pauses everything.
// Pure, so it's tested.

import { liveBoost } from "./modes";
import type { Config } from "./types";

/** Paused now (until a time still ahead, or until resumed). */
export function isPaused(config: Config | undefined, now: number): boolean {
  const p = config?.pause;
  return p !== undefined && (p.until === undefined || now < p.until);
}

/** Whether SnailNet is doing something the user set up, so Pause has
 *  something to pause: a rule on in the mode that's on (a slice, "only
 *  when the line is free", a priority or a limit), a boost running, an
 *  app blocked; or it's paused already, so Resume stays in reach. */
export function pauseShown(config: Config | undefined, now: number): boolean {
  if (!config) return false;
  if (isPaused(config, now)) return true;
  if (liveBoost(config.boost, now)) return true;
  if ((config.blocked ?? []).length > 0) return true;
  const mode = config.profiles.find((p) => p.id === config.active_profile);
  return (mode?.rules ?? []).some((r) => r.enabled);
}
