// Real logos for the apps and websites people know (AppIcon.svelte), from
// Simple Icons (CC0, pinned in package.json; credits.json says where each
// mark comes from). Only the icons named here end up in the bundle: Vite
// keeps the named imports and drops the rest of the package.
//
// A program's own .exe icon still wins (icon.rs); a logo is for websites,
// and for a known app until (or unless) its .exe icon arrives. Anything
// without a logo keeps its letter tile. Brands Simple Icons doesn't carry
// (Disney+, Prime Video, Teams, Edge) stay letters on purpose: no
// look-alikes.

import {
  siBattledotnet,
  siBrave,
  siDiscord,
  siDropbox,
  siEa,
  siEpicgames,
  siFirefoxbrowser,
  siGogdotcom,
  siGooglechrome,
  siGooglemeet,
  siKick,
  siNetflix,
  siOpera,
  siQbittorrent,
  siSpotify,
  siSteam,
  siTwitch,
  siUbisoft,
  siYoutube,
  siZoom,
} from "simple-icons";
import type { Target } from "../types";

export interface Brand {
  /** The mark, one path on a 24 × 24 box. */
  path: string;
  /** What to draw it in on the Garden's dark tiles (see `inkOn`). */
  ink: string;
  /** The brand's own colour, for a solid app-icon tile. */
  bg: string;
  /** The mark on that tile: white or near-black, whichever stands out. */
  mark: string;
}

type Icon = { path: string; hex: string };

/** Website ids (assets/sites.toml) → their logo. */
const SITES: Record<string, Icon> = {
  youtube: siYoutube,
  twitch: siTwitch,
  netflix: siNetflix,
  spotify: siSpotify,
  kick: siKick,
  zoom: siZoom,
  "google-meet": siGooglemeet,
};

/** App family ids (assets/families.toml) → their logo. */
const APPS: Record<string, Icon> = {
  brave: siBrave,
  chrome: siGooglechrome,
  firefox: siFirefoxbrowser,
  opera: siOpera,
  discord: siDiscord,
  spotify: siSpotify,
  zoom: siZoom,
  steam: siSteam,
  epic: siEpicgames,
  battlenet: siBattledotnet,
  ea: siEa,
  ubisoft: siUbisoft,
  gog: siGogdotcom,
  dropbox: siDropbox,
  qbittorrent: siQbittorrent,
};

/** The tile a logo sits on (--surface-2) and the cream it falls back to
 *  (--text-primary). */
const TILE = "#1b443c";
const CREAM = "#f3eee2";
/** The Garden's darkest green, for marks on light brand tiles. */
const DARK = "#0a211d";

function luminance(hex: string): number {
  const n = parseInt(hex.replace("#", ""), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * (r ?? 0) + 0.7152 * (g ?? 0) + 0.0722 * (b ?? 0);
}

/** WCAG contrast ratio of two #rrggbb colours. */
export function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return ((x ?? 0) + 0.05) / ((y ?? 0) + 0.05);
}

/** Below this a brand colour sinks into the tile. Logos are decorative
 *  here (the name is always written beside them, so WCAG 1.4.11 doesn't
 *  ask 3:1 of them); this only keeps each one plainly visible. */
const VISIBLE = 1.8;

/** The brand's own colour when it shows on the dark tile (lighter than
 *  the tile, and far enough from it), else cream: Steam's or EA's black
 *  would vanish. */
export function inkOn(hex: string, tile = TILE): string {
  const c = `#${hex.replace("#", "").toLowerCase()}`;
  return luminance(c) > luminance(tile) && contrast(c, tile) >= VISIBLE ? c : CREAM;
}

/** The logo for a website or app, or null (no logo: a letter tile). */
export function brandOf(target: Target): Brand | null {
  const id = target.id.toLowerCase();
  const icon = target.kind === "site" ? SITES[id] : target.kind === "family" ? APPS[id] : undefined;
  if (!icon) return null;
  const bg = `#${icon.hex.toLowerCase()}`;
  return { path: icon.path, ink: inkOn(icon.hex), bg, mark: markOn(bg) };
}

/** White or near-black on a brand's tile, whichever has more contrast. */
export function markOn(bg: string): string {
  return contrast("#ffffff", bg) >= contrast(DARK, bg) ? "#ffffff" : DARK;
}
