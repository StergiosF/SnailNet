// Program icons for the list (src-tauri/src/icon.rs), by exe path. Each exe
// is asked for once; until the answer arrives (or when there's none) the
// page shows a letter tile instead.

import { invoke } from "@tauri-apps/api/core";
import type { AppInfo, Target } from "./types";

class Icons {
  #urls = $state<Record<string, string | null>>({});
  #asked = new Set<string>();

  /** The icon's data URL, or null (not known yet, or none). */
  url(exe: string): string | null {
    if (!this.#asked.has(exe)) {
      this.#asked.add(exe);
      invoke<string | null>("app_icon", { exe })
        .then((u) => (this.#urls[exe] = u))
        .catch(() => (this.#urls[exe] = null));
    }
    return this.#urls[exe] ?? null;
  }

  /** The icon for an app rule or list entry, if its exe is known. */
  forTarget(target: Target, apps: AppInfo[]): string | null {
    if (target.kind === "site") return null;
    const id = target.id.toLowerCase();
    const app =
      target.kind === "family"
        ? apps.find((a) => a.family.toLowerCase() === id)
        : apps.find((a) => a.exe.toLowerCase().endsWith(`\\${id}`) || a.exe.toLowerCase() === id);
    return app ? this.url(app.exe) : null;
  }
}

export const icons = new Icons();
