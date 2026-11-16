// The system look from the Rust side (app/src-tauri/src/window.rs): the
// Windows accent colour (live) and whether Mica is behind the page. Applied
// as CSS custom properties and a class on <html>; app.css keeps the Windows
// default blue as the fallback until (or unless) this arrives.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** `#rrggbb` shades, as `UISettings` reports them. */
export type Accent = {
  accent: string;
  dark1: string;
  dark2: string;
  dark3: string;
  light1: string;
  light2: string;
  light3: string;
};

export type Backdrop = "mica" | "solid";

export type SystemLook = { accent: Accent | null; backdrop: Backdrop };

/** Class on <html> when there is no Mica (Windows 10): the page paints
 *  its own solid layer colour instead of showing the desktop. */
export const SOLID_CLASS = "solid-backdrop";

const HEX = /^#[0-9a-f]{6}$/i;

/** The `--sys-accent-*` properties app.css reads, or none for an accent
 *  that is missing or malformed. */
export function accentVars(accent: Accent | null): Record<string, string> {
  if (!accent) return {};
  const vars: Record<string, string> = {};
  for (const [k, v] of Object.entries(accent)) {
    if (!HEX.test(v)) return {};
    vars[k === "accent" ? "--sys-accent" : `--sys-accent-${k}`] = v.toLowerCase();
  }
  return vars;
}

const ACCENT_KEYS = ["accent", "dark1", "dark2", "dark3", "light1", "light2", "light3"] as const;

export function applySystem(look: SystemLook, root: HTMLElement = document.documentElement): void {
  const vars = accentVars(look.accent);
  for (const k of ACCENT_KEYS) {
    const name = k === "accent" ? "--sys-accent" : `--sys-accent-${k}`;
    const v = vars[name];
    if (v) root.style.setProperty(name, v);
    else root.style.removeProperty(name);
  }
  root.classList.toggle(SOLID_CLASS, look.backdrop === "solid");
}

/** Apply the current look and follow changes. Failures leave the CSS
 *  defaults in place (blue accent, transparent page). */
export async function startSystem(): Promise<void> {
  try {
    await listen<SystemLook>("system", (e) => applySystem(e.payload));
    applySystem(await invoke<SystemLook>("system"));
  } catch {
    // Outside Tauri or the command failed: keep the defaults.
  }
}
