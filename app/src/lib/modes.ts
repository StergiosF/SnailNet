// The Now page's model (PLAN.md §14.2): the four modes, who is using the
// internet right now (with a Boost for each), what a boost has left, and
// the split in short. Pure, so it's tested.

import { sameTarget } from "./rules";
import { QUIET_BPS, statusText, top, type Ledger } from "./ledger";
import { t, type Key } from "./i18n";
import type { Boost, GameTick, GameWhy, Profile, Target } from "./types";

export type ModeId = "balanced" | "streaming" | "gaming" | "quiet-downloads";

export interface Mode {
  id: ModeId;
  name: Key;
  body: Key;
}

/** The built-in modes (snailnet-core `modes::MODES`), in order. */
export const MODES: Mode[] = [
  { id: "balanced", name: "mode.balanced", body: "mode.balanced.body" },
  { id: "streaming", name: "mode.streaming", body: "mode.streaming.body" },
  { id: "gaming", name: "mode.gaming", body: "mode.gaming.body" },
  { id: "quiet-downloads", name: "mode.quiet", body: "mode.quiet.body" },
];

/** A mode as the pickers show it: its words, and whether it's the user's
 *  own ("Duplicate & edit", §11.2) rather than built in. */
export interface ModeView {
  id: string;
  name: string;
  body: string;
  own: boolean;
}

export const isOwnMode = (id: string) => !MODES.some((m) => m.id === id);

/** The modes this config has, built-in ones first (an old service may
 *  lack some), then the user's own in config order. */
export function availableModes(profiles: Profile[]): ModeView[] {
  return [
    ...MODES.filter((m) => profiles.some((p) => p.id === m.id)).map((m) => ({
      id: m.id,
      name: t(m.name),
      body: t(m.body),
      own: false,
    })),
    ...profiles
      .filter((p) => isOwnMode(p.id))
      .map((p) => ({ id: p.id, name: p.name, body: t("mode.own.body"), own: true })),
  ];
}

/** A name for a copy of `name` no mode has yet: "Gaming copy", then
 *  "Gaming copy 2", … (the service refuses a taken name, any case). */
export function copyName(name: string, profiles: Profile[]): string {
  const taken = new Set(profiles.map((p) => p.name.trim().toLowerCase()));
  const base = t("mode.copy.name", { mode: name }).slice(0, 34);
  for (let n = 1; ; n++) {
    const candidate = n === 1 ? base : `${base} ${n}`;
    if (!taken.has(candidate.toLowerCase())) return candidate;
  }
}

/** A profile's name as the app writes it: a mode's own words, else the
 *  name the config gives it. */
export function modeName(profile: Profile | null): string {
  if (!profile) return "";
  const m = MODES.find((x) => x.id === profile.id);
  return m ? t(m.name) : profile.name;
}

/** The boost, if it's still on at `now` (Unix seconds). */
export function liveBoost(boost: Boost | undefined, now: number): Boost | null {
  if (!boost) return null;
  return boost.until === undefined || now < boost.until ? boost : null;
}

/** "52 min left", "1 h 5 min left", "until you stop it". */
export function boostLeft(boost: Boost, now: number): string {
  return timeLeft(boost.until, now);
}

/** The boost lengths the Boost menu offers, in minutes. */
export const BOOST_MINUTES = [60, 240];

/** How much of a boost is left, 0–1, for its countdown bar; null when it
 *  runs until stopped. The service keeps only the end, so the length is
 *  the shortest one the menu offers that still holds what's left. */
export function boostShare(boost: Boost, now: number): number | null {
  if (boost.until === undefined) return null;
  const left = Math.max(0, boost.until - now);
  const total = (BOOST_MINUTES.find((m) => m * 60 >= left) ?? Math.ceil(left / 60)) * 60;
  return total === 0 ? 0 : Math.min(1, left / total);
}

/** Time left until `until` (Unix seconds) in words; no end = "until you
 *  stop it". */
export function timeLeft(until: number | undefined, now: number): string {
  if (until === undefined) return t("boost.untilStopped");
  const min = Math.max(1, Math.ceil((until - now) / 60));
  if (min < 60) return t("boost.left.min", { min });
  const h = Math.floor(min / 60);
  const m = min % 60;
  return m === 0 ? t("boost.left.h", { h }) : t("boost.left.hmin", { h, min: m });
}

/** Someone using the internet, for the Now page. */
export interface User {
  key: string;
  target: Target;
  name: string;
  bps: number;
  /** What's happening to it, in words. */
  status: string;
  boosted: boolean;
  /** Its part of the line now, 0–1 (the row's bar). */
  part: number;
  /** Its slice's colour slot; -1 inside Everything else. */
  ink: number;
  /** Left waiting for room (waiting.ts, or a full slice wanting more). */
  waiting: boolean;
}

/** How much of the line `part` (0–1) is, in words. */
export function amount(part: number): string {
  if (part >= 0.5) return t("who.amount.most");
  if (part >= 0.2) return t("who.amount.big");
  if (part >= 0.05) return t("who.amount.some");
  return t("who.amount.little");
}

const keyOf = (t: Target) => `${t.kind}:${t.id.toLowerCase()}`;

/** Who is using the internet, busiest first: apps and sites with their own
 *  slice, and the apps inside Everything else. The boosted one is always
 *  listed (so it can be stopped), even when quiet. At most `max`. */
export function users(
  l: Ledger,
  boost: Boost | null,
  label: (t: Target) => string,
  max = 5,
  calls: string[] = [],
): User[] {
  const out: User[] = [];
  // The line the bars measure against: what SnailNet shares out, else
  // what moves now.
  const line = l.shaped ?? l.total;
  const part = (bps: number) => (line > 0 ? Math.min(1, bps / line) : 0);
  for (const s of l.slices) {
    if (s.target !== null) {
      if (s.active) {
        out.push({
          key: s.key,
          target: s.target,
          name: s.name,
          bps: s.bps,
          status: statusText(s.status),
          boosted: false,
          part: part(s.bps),
          ink: s.ink,
          waiting: s.status.kind === "held",
        });
      }
      continue;
    }
    for (const m of s.members) {
      if (m.fixed) continue;
      out.push({
        key: m.key,
        target: m.target,
        name: m.name,
        bps: m.bps,
        status: m.waiting ? t("who.waiting") : amount(part(m.bps)),
        boosted: false,
        part: part(m.bps),
        ink: -1,
        waiting: m.waiting,
      });
    }
  }
  out.sort((a, b) => b.bps - a.bps || a.name.localeCompare(b.name));
  const calling = (u: User) => inCall(u.target, calls, true);
  // Nothing is using the internet (the glance says so): what steady.ts
  // still holds in view at a trickle isn't "using your internet", or the
  // page would contradict its own heading (seen 2026-11-20). Those go to
  // idle() instead; an app left waiting or in a call stays.
  const pool = top(l) === null ? out.filter((u) => u.waiting || calling(u)) : out;
  const shown = pool.slice(0, max);
  // An app in a call stays in view however little it moves (a voice call
  // is ~0.05 Mbps), so "in a call" is seen while a download is busy: it
  // takes the place of the quietest app that isn't in a call.
  for (const u of pool) {
    if (shown.includes(u) || !calling(u)) continue;
    let i = shown.length - 1;
    while (i >= 0 && calling(shown[i]!)) i--;
    if (i < 0) break;
    shown.splice(i, 1, u);
  }
  shown.sort((a, b) => b.bps - a.bps || a.name.localeCompare(b.name));
  if (boost) {
    const hit = out.find((u) => sameTarget(u.target, boost.target));
    if (hit) {
      hit.boosted = true;
      if (!shown.includes(hit)) shown.splice(max - 1, 1, hit);
    } else {
      shown.unshift({
        key: keyOf(boost.target),
        target: boost.target,
        name: label(boost.target),
        bps: 0,
        status: t("status.quiet"),
        boosted: true,
        part: 0,
        ink: -1,
        waiting: false,
      });
      if (shown.length > max) shown.pop();
    }
  }
  return shown;
}

/** The apps that are open but idle while nothing is using the internet:
 *  held in view by steady.ts at a trickle, so users() leaves them out.
 *  Busiest first; empty as soon as anything moves for real. */
export function idle(l: Ledger): { key: string; target: Target; name: string }[] {
  if (top(l) !== null) return [];
  const out: { key: string; target: Target; name: string; bps: number }[] = [];
  for (const s of l.slices) {
    if (s.target !== null) {
      if (s.active && s.bps < QUIET_BPS) out.push({ key: s.key, target: s.target, name: s.name, bps: s.bps });
      continue;
    }
    for (const m of s.members) if (!m.fixed && !m.waiting && m.bps < QUIET_BPS) out.push(m);
  }
  return out
    .sort((a, b) => b.bps - a.bps || a.name.localeCompare(b.name))
    .map(({ key, target, name }) => ({ key, target, name }));
}

/** Why `target` goes first as a game right now (§11.5), or null: not a
 *  running game, or "Put what you're using first" is off. */
export function gameWhy(target: Target, games: GameTick[] | undefined, focusBoost: boolean): GameWhy | null {
  if (!focusBoost || target.kind !== "family") return null;
  return games?.find((g) => g.family === target.id)?.why ?? null;
}

/** Whether `target` is an app in a live call that goes first (§11.9). */
export function inCall(target: Target, calls: string[] | undefined, on: boolean): boolean {
  const id = target.id.toLowerCase();
  return on && target.kind === "family" && (calls?.some((c) => c.toLowerCase() === id) ?? false);
}
