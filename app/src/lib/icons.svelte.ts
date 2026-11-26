// Program icons for the list (src-tauri/src/icon.rs), by exe path. Each exe
// is asked for once; until the answer arrives (or when there's none) the
// page shows a letter tile instead.

import { invoke } from "@tauri-apps/api/core";
import type { AppInfo, Target } from "./types";

function hasPath(p: string | undefined): p is string {
  return !!p && (p.includes("\\") || p.includes("/"));
}

function exePath(app: AppInfo): string | null {
  if (hasPath(app.path)) return app.path;
  if (hasPath(app.exe)) return app.exe;
  return null;
}

class Icons {
  #urls = $state<Record<string, string | null>>({});
  #asked = new Set<string>();

  /** The icon's data URL, or null (not known yet, or none). */
  url(exe: string): string | null {
    if (!hasPath(exe)) return null;
    if (!this.#asked.has(exe)) {
      this.#asked.add(exe);
      invoke<string | null>("app_icon", { exe })
        .then((u) => (this.#urls[exe] = u))
        .catch(() => (this.#urls[exe] = null));
    }
    return this.#urls[exe] ?? null;
  }

  /** The icon for an app rule or list entry, if its full exe path is known. */
  forTarget(target: Target, apps: AppInfo[]): string | null {
    if (target.kind === "site") return null;
    const id = target.id.toLowerCase();
    const matches =
      target.kind === "family"
        ? apps.filter((a) => a.family.toLowerCase() === id)
        : apps.filter(
            (a) =>
              a.exe.toLowerCase().endsWith(`\\${id}`) ||
              a.exe.toLowerCase().endsWith(`/${id}`) ||
              a.exe.toLowerCase() === id ||
              a.path?.toLowerCase() === id,
          );
    // For a family with multiple exes (e.g. steamservice.exe + steam.exe),
    // try the main `<family>.exe` first and fall through if an exe has no icon.
    const ordered =
      target.kind === "family" && matches.length > 1
        ? [...matches].sort((a, b) => {
            const primary = `${id}.exe`;
            const aMain = a.exe.toLowerCase() === primary ? 0 : 1;
            const bMain = b.exe.toLowerCase() === primary ? 0 : 1;
            return aMain - bMain;
          })
        : matches;
    for (const app of ordered) {
      const path = exePath(app);
      if (!path) continue;
      const u = this.url(path);
      if (u) return u;
      // Wait for an in-flight lookup before probing fallback exes in the family.
      if (!(path in this.#urls)) return null;
    }
    return null;
  }
}

export const icons = new Icons();
