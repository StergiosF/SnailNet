// "Start SnailNet with Windows?" (autostart.rs, AutostartDialog): when the
// main window may ask it. Pure, so it's tested.

import type { Link, Snapshot } from "./types";

/** Ask once the service has answered (or is known not to be there), never
 *  over first run: a fresh install sets its speed first, then is asked.
 *  `pending`: autostart.rs says the question is still open. */
export function askAutostartNow(
  pending: boolean,
  firstRun: boolean,
  link: Link,
  snapshot: Snapshot | null,
): boolean {
  if (!pending || firstRun) return false;
  if (link.kind === "connecting") return false;
  // Connected but no config yet: first run may still be about to start.
  return !(link.kind === "connected" && snapshot === null);
}
