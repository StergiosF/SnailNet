// Post-update "What's new" notes (shown once after installing an update).
// Combines curated plain-English highlights for known releases with a
// smart fallback parser for release notes fetched from `latest.json`, so
// every future update automatically gets clean, icon-led notes.

import type { GlyphName } from "./components/Glyph.svelte";
import { firstRunDismissed } from "./line";

export type NoteTone = "jade" | "warm" | "sky" | "neutral";

export interface ReleaseNote {
  icon: GlyphName;
  title: string;
  body?: string;
  tone?: NoteTone;
}

export interface UpdateModalData {
  version: string;
  items: ReleaseNote[];
}

interface StoredUpdate {
  version: string;
  notes: string | null;
}

const SEEN_KEY = "snailnet.seenVersion";
const PENDING_KEY = "snailnet.pendingUpdate";
const LAST_NOTES_KEY = "snailnet.lastUpdateNotes";

/** Curated, plain-English highlights per version (shown in the post-update modal). */
export const RELEASE_NOTES: Record<string, ReleaseNote[]> = {
  "0.0.12": [
    {
      icon: "boost",
      tone: "jade",
      title: "Faster, lighter traffic shaping",
      body: "Less CPU work per packet and zero background wakeups while SnailNet sits quietly in the tray.",
    },
    {
      icon: "gaming",
      tone: "warm",
      title: "Smarter game priority",
      body: "Running games get priority right away, borderless apps won't trigger Gaming mode by mistake, and Ctrl+Alt+S won't pull you out of fullscreen.",
    },
    {
      icon: "apps",
      tone: "sky",
      title: "Real Windows app icons",
      body: "Installed games and programs now display their own icons instead of a generic window tile.",
    },
    {
      icon: "check",
      tone: "jade",
      title: "Smoother controls and reliability",
      body: "Clearer feedback in the tray flyout, faster speed test cancellation, and more reliable speed limits and Undo.",
    },
  ],
};

const VALID_ICONS = new Set<GlyphName>([
  "now",
  "split",
  "settings",
  "pause",
  "play",
  "boost",
  "balanced",
  "streaming",
  "gaming",
  "quiet-downloads",
  "own-mode",
  "plus",
  "minus",
  "chevron",
  "down",
  "free",
  "ping",
  "copy",
  "reset",
  "line",
  "apps",
  "power",
  "info",
  "clock",
  "infinity",
  "ban",
  "unlink",
  "undo",
  "merge",
  "gauge",
  "phone",
  "window",
  "bulb",
  "flag",
  "keyboard",
  "download",
  "upload",
  "list",
  "trash",
  "check",
  "pen",
  "auto",
  "more",
  "search",
  "close",
]);

function stripMarkdown(s: string): string {
  return s
    .replace(/\*\*(.+?)\*\*/g, "$1")
    .replace(/__(.+?)__/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .trim();
}

function inferIconAndTone(text: string): { icon: GlyphName; tone: NoteTone } {
  const lower = text.toLowerCase();
  if (/\b(game|games|gaming|fullscreen|play)\b/.test(lower)) return { icon: "gaming", tone: "warm" };
  if (/\b(call|calls|voice|video|discord|zoom|teams|mic)\b/.test(lower)) return { icon: "phone", tone: "sky" };
  if (/\b(ping|latency|lag|ms|bufferbloat|jitter)\b/.test(lower)) return { icon: "ping", tone: "jade" };
  if (/\b(speed|fast|faster|cpu|memory|battery|lighter|optimiz|performance|boost)\b/.test(lower)) {
    return { icon: "boost", tone: "jade" };
  }
  if (/\b(icon|icons|app|apps|program)\b/.test(lower)) return { icon: "apps", tone: "sky" };
  if (/\b(shortcut|hotkey|keyboard|ctrl|key)\b/.test(lower)) return { icon: "keyboard", tone: "sky" };
  if (/\b(split|share|shares|mode|modes|limit|ceiling)\b/.test(lower)) return { icon: "split", tone: "warm" };
  if (/\b(tray|flyout|window|monitor|dpi|display)\b/.test(lower)) return { icon: "window", tone: "sky" };
  if (/\b(security|secure|safe|protect|trust|hardening)\b/.test(lower)) return { icon: "power", tone: "warm" };
  if (/\b(speed test|measure|download|upload)\b/.test(lower)) return { icon: "gauge", tone: "sky" };
  if (/\b(fix|fixed|fixes|bug|crash|reliab|smooth|undo|stab)\b/.test(lower)) return { icon: "check", tone: "jade" };
  return { icon: "bulb", tone: "jade" };
}

function toneForIcon(icon: GlyphName): NoteTone {
  switch (icon) {
    case "gaming":
    case "split":
    case "power":
    case "flag":
      return "warm";
    case "apps":
    case "phone":
    case "window":
    case "keyboard":
    case "gauge":
    case "line":
      return "sky";
    default:
      return "jade";
  }
}

/** Parse plain text or markdown bullet release notes (from `latest.json`) into
 *  icon-led highlights. Supports optional `[glyph]` prefixes and `Title: body`. */
export function parseReleaseNotes(raw: string | null | undefined): ReleaseNote[] {
  if (!raw) return [];
  const items: ReleaseNote[] = [];
  for (const rawLine of raw.split(/\r?\n/)) {
    const trimmed = rawLine.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    let line = trimmed.replace(/^(?:[-*•]|\d+\.)\s+/, "").trim();
    if (!line) continue;

    let explicitIcon: GlyphName | null = null;
    const tagMatch = /^\[([a-z-]+)\]\s+(.+)$/i.exec(line);
    if (tagMatch) {
      const candidate = tagMatch[1]?.toLowerCase() as GlyphName;
      if (VALID_ICONS.has(candidate)) {
        explicitIcon = candidate;
        line = (tagMatch[2] ?? "").trim();
      }
    }

    // Split `**Title**: body`, `Title: body`, or `Title — body` when concise.
    let title: string;
    let body: string | undefined;
    const boldSplit = /^\*\*(.+?)\*\*\s*(?::\s*|[—–-]\s+)(.+)$/.exec(line);
    if (boldSplit) {
      title = stripMarkdown(boldSplit[1] ?? "");
      body = stripMarkdown(boldSplit[2] ?? "");
    } else {
      const sepMatch = /^([^:—–]{3,64})(?::\s+|\s+[—–]\s+)(.+)$/.exec(line);
      if (sepMatch) {
        title = stripMarkdown(sepMatch[1] ?? "");
        body = stripMarkdown(sepMatch[2] ?? "");
      } else {
        title = stripMarkdown(line);
      }
    }

    if (!title) continue;
    const inferred = inferIconAndTone(`${title} ${body ?? ""}`);
    const icon = explicitIcon ?? inferred.icon;
    const tone = explicitIcon ? toneForIcon(explicitIcon) : inferred.tone;
    items.push(body ? { icon, tone, title, body } : { icon, tone, title });
  }
  return items.slice(0, 6);
}

/** Compare simple dotted versions (`0.0.11` > `0.0.10`). */
export function isNewerVersion(next: string, prev: string): boolean {
  const parse = (v: string) =>
    v
      .trim()
      .replace(/^v/i, "")
      .split("-")[0]!
      .split(".")
      .map((n) => Number.parseInt(n, 10) || 0);
  const a = parse(next);
  const b = parse(prev);
  const len = Math.max(a.length, b.length);
  for (let i = 0; i < len; i++) {
    const da = a[i] ?? 0;
    const db = b[i] ?? 0;
    if (da !== db) return da > db;
  }
  return false;
}

function readStoredUpdate(key: string, s: Storage): StoredUpdate | null {
  try {
    const raw = s.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<StoredUpdate>;
    if (typeof parsed.version !== "string") return null;
    return { version: parsed.version.trim().replace(/^v/i, ""), notes: parsed.notes ?? null };
  } catch {
    try {
      s.removeItem(key);
    } catch {
      // Ignore.
    }
    return null;
  }
}

function saveLastNotes(version: string, notes: string | null, s: Storage) {
  if (!notes || !notes.trim()) return;
  try {
    const payload: StoredUpdate = { version: version.trim().replace(/^v/i, ""), notes };
    s.setItem(LAST_NOTES_KEY, JSON.stringify(payload));
  } catch {
    // Storage full or unavailable.
  }
}

/** Resolve icon-led highlights for `version`, preferring curated built-in notes
 *  or updater notes (passed directly or saved from the last update), with a
 *  friendly fallback if none exist. */
export function notesForVersion(version: string, rawNotes?: string | null, storage?: Storage): UpdateModalData {
  const clean = version.trim().replace(/^v/i, "");
  let notes = rawNotes;
  if (notes === undefined) {
    const s = getStorage(storage);
    const saved = s ? readStoredUpdate(LAST_NOTES_KEY, s) : null;
    if (saved && saved.version === clean) notes = saved.notes;
  }
  const parsed = parseReleaseNotes(notes);
  const curated = RELEASE_NOTES[clean];
  // Prefer curated notes for known releases; if a future release ships notes
  // in `latest.json` without a built-in entry, use the parsed notes.
  const items: ReleaseNote[] =
    curated && curated.length > 0
      ? curated
      : parsed.length > 0
        ? parsed
        : [
            {
              icon: "check",
              tone: "jade",
              title: "SnailNet has been updated",
              body: "You're running the newest version with the latest improvements and fixes.",
            },
          ];
  return { version: clean || version, items };
}

function getStorage(storage?: Storage): Storage | null {
  if (storage) return storage;
  return typeof localStorage === "undefined" ? null : localStorage;
}

/** Remember an update right before running the installer so the new version
 *  greets the user once with its update notes. */
export function rememberPendingUpdate(version: string, notes: string | null, storage?: Storage) {
  const s = getStorage(storage);
  if (!s) return;
  try {
    const payload: StoredUpdate = { version: version.trim().replace(/^v/i, ""), notes };
    s.setItem(PENDING_KEY, JSON.stringify(payload));
  } catch {
    // Storage full or unavailable.
  }
}

/** Clear any pending update record (called if installing fails or is cancelled). */
export function clearPendingUpdate(storage?: Storage) {
  const s = getStorage(storage);
  if (!s) return;
  try {
    s.removeItem(PENDING_KEY);
  } catch {
    // Ignore.
  }
}

/** Check on startup whether SnailNet was just updated to `currentVersion`.
 *  - Brand-new installs silently record `currentVersion` and return `null`.
 *  - Existing pre-`seenVersion` installs (first run already completed) and
 *    in-app or installer upgrades return the modal data once until
 *    `markUpdateSeen(currentVersion)` is called. */
export function checkJustUpdated(currentVersion: string, storage?: Storage): UpdateModalData | null {
  const clean = currentVersion.trim().replace(/^v/i, "");
  if (!clean) return null;
  const s = getStorage(storage);
  if (!s) return null;

  const pending = readStoredUpdate(PENDING_KEY, s);
  if (pending && pending.version === clean) {
    saveLastNotes(clean, pending.notes, s);
    return notesForVersion(clean, pending.notes, s);
  }

  const seen = s.getItem(SEEN_KEY);
  if (!seen) {
    if (firstRunDismissed(s)) {
      // Existing user upgrading from a version prior to `seenVersion` tracking.
      return notesForVersion(clean, null, s);
    }
    // Fresh install: remember this version quietly without showing an update modal.
    try {
      s.setItem(SEEN_KEY, clean);
    } catch {
      // Ignore.
    }
    return null;
  }

  if (seen !== clean && isNewerVersion(clean, seen)) {
    return notesForVersion(clean, null, s);
  }

  return null;
}

/** Mark `currentVersion`'s update notes as seen so the modal won't reappear. */
export function markUpdateSeen(currentVersion: string, storage?: Storage) {
  const clean = currentVersion.trim().replace(/^v/i, "");
  const s = getStorage(storage);
  if (!s) return;
  try {
    if (clean) {
      const pending = readStoredUpdate(PENDING_KEY, s);
      if (pending && pending.version === clean) {
        saveLastNotes(clean, pending.notes, s);
      }
      s.setItem(SEEN_KEY, clean);
      if (!pending || pending.version === clean || !isNewerVersion(pending.version, clean)) {
        s.removeItem(PENDING_KEY);
      }
    } else {
      s.removeItem(PENDING_KEY);
    }
  } catch {
    // Ignore.
  }
}
