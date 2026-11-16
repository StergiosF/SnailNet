// The main window's model (PLAN.md §14.2, the Ledger): the active profile's
// rules as rows, each with what it moves now, what its slice keeps for it
// on a busy line, and a plain-words status; Everything else last, with the
// apps that land in it. Also the sentence at the top that answers "who is
// using my internet, and is it a problem?". Pure, so it's tested.

import { className, mbps } from "./format";
import { t } from "./i18n";
import { barShares, isSlice, sameTarget } from "./rules";
import type { ClassTick, DirStats, Priority, Rule, Target } from "./types";

/** The scheduler's name for Everything else (snailnet-core OTHER_CLASS). */
export const OTHER_CLASS = "everything else";
/** The family of traffic SnailNet couldn't tie to an app. */
export const UNKNOWN_FAMILY = "unknown";

/** Below this an app counts as quiet (50 kbps: background chatter). */
export const QUIET_BPS = 50_000;
/** The line counts as busy from this share of the shaped rate. */
export const BUSY = 0.9;
/** A slice is lending only when it leaves at least this much unused. */
const LEND_BPS = 1_000_000;
/** And using spare room only above its slice by this much. */
const BORROW_BPS = 300_000;
/** Queued bytes that mean "wants more than it gets" (a few packets). */
const HELD_BYTES = 6_000;

export type Status =
  | { kind: "quiet" }
  | { kind: "all" }
  | { kind: "lending"; spare: number }
  | { kind: "borrowing"; extra: number }
  | { kind: "held" }
  | { kind: "free" }
  | { kind: "off" };

/** An app inside Everything else. */
export interface Member {
  key: string;
  target: Target;
  name: string;
  bps: number;
  /** Unrecognised traffic: can't get a slice. */
  fixed: boolean;
}

export interface Slice {
  key: string;
  /** Index in the profile's rules; -1 for Everything else. */
  rule: number;
  target: Target | null;
  name: string;
  /** Basis points as the bar has it; null for "only when free" and off. */
  share: number | null;
  priority: Priority | null;
  enabled: boolean;
  bps: number;
  /** What the slice keeps for it on a busy line, or null (not shaping). */
  kept: number | null;
  status: Status;
  /** Everything else only: its apps, busiest first. */
  members: Member[];
  /** Colour slot (0–5) for rules; -1 for Everything else. */
  ink: number;
}

export interface Ledger {
  slices: Slice[];
  /** Everything SnailNet moved this tick in this direction. */
  total: number;
  /** The rate SnailNet shares out, or null when not shaping. */
  shaped: number | null;
  busy: boolean;
}

function classTick(dir: DirStats | null | undefined, name: string): ClassTick | undefined {
  return dir?.classes.find((c) => c.name.toLowerCase() === name);
}

function status(
  bps: number,
  kept: number | null,
  busy: boolean,
  tick: ClassTick | undefined,
): Status {
  if (kept !== null) {
    if (busy && kept - bps >= LEND_BPS) return { kind: "lending", spare: kept - bps };
    if (bps > kept + BORROW_BPS) return { kind: "borrowing", extra: bps - kept };
  }
  if (bps < QUIET_BPS) return { kind: "quiet" };
  if (kept !== null && (tick?.queued_bytes ?? 0) > HELD_BYTES) return { kind: "held" };
  return { kind: "all" };
}

/** Which apps land in Everything else, and how much each moves: an app's
 *  traffic minus what site slices take, unless the app has a slice. */
export function membersOf(
  dir: DirStats | null | undefined,
  rules: Rule[],
): Member[] {
  const live = rules.filter((r) => r.enabled);
  const siteRule = (site: string) =>
    live.some((r) => sameTarget(r.target, { kind: "site", id: site }));
  const out: Member[] = [];
  for (const f of dir?.families ?? []) {
    const target: Target = { kind: "family", id: f.family };
    if (live.some((r) => sameTarget(r.target, target))) continue;
    const toSites = (dir?.sites ?? [])
      .filter((s) => s.family === f.family && siteRule(s.site))
      .reduce((sum, s) => sum + s.bps, 0);
    const bps = Math.max(0, f.bps - toSites);
    if (bps < QUIET_BPS) continue;
    out.push({
      key: `family:${f.family}`,
      target,
      name: f.name,
      bps,
      fixed: f.family === UNKNOWN_FAMILY,
    });
  }
  return out.sort((a, b) => b.bps - a.bps || a.name.localeCompare(b.name));
}

/** The rows for one direction. `label` names a rule's target. */
export function ledger(
  rules: Rule[],
  dir: DirStats | null | undefined,
  label: (t: Target) => string,
): Ledger {
  const total = dir ? dir.classes.reduce((sum, c) => sum + c.bps, 0) : 0;
  const shaped = dir && dir.shaped_bps > 0 ? dir.shaped_bps : null;
  const busy = shaped !== null && total >= BUSY * shaped;
  const shares = barShares(rules);
  const keptOf = (bp: number) => (shaped === null ? null : (shaped * bp) / 10_000);

  let k = 0;
  let ink = 0;
  const slices: Slice[] = rules.map((r, i) => {
    const tick = r.enabled ? classTick(dir, className(r.target)) : undefined;
    const bps = tick?.bps ?? 0;
    const share = isSlice(r) ? (shares[k++] ?? null) : null;
    const kept = share === null ? null : keptOf(share);
    const st: Status = !r.enabled
      ? { kind: "off" }
      : share === null
        ? { kind: "free" }
        : status(bps, kept, busy, tick);
    return {
      key: `${r.target.kind}:${r.target.id.toLowerCase()}`,
      rule: i,
      target: r.target,
      name: label(r.target),
      share,
      priority: r.priority,
      enabled: r.enabled,
      bps,
      kept,
      status: st,
      members: [],
      ink: ink++ % 6,
    };
  });

  const otherShare = shares[shares.length - 1] ?? 10_000;
  const otherTick = classTick(dir, OTHER_CLASS);
  const otherBps = otherTick?.bps ?? 0;
  const otherKept = keptOf(otherShare);
  slices.push({
    key: "other",
    rule: -1,
    target: null,
    name: t("split.everything"),
    share: otherShare,
    priority: null,
    enabled: true,
    bps: otherBps,
    kept: otherKept,
    status: status(otherBps, otherKept, busy, otherTick),
    members: membersOf(dir, rules),
    ink: -1,
  });
  return { slices, total, shaped, busy };
}

/** A row's status in words: "gets all it asks for", "lending 6.5 it
 *  isn't using", "7.0 of it is spare room"… */
export function statusText(s: Status): string {
  switch (s.kind) {
    case "quiet":
      return t("status.quiet");
    case "all":
      return t("status.all");
    case "lending":
      return t("status.lending", { mbps: mbps(s.spare) });
    case "borrowing":
      return t("status.borrowing", { mbps: mbps(s.extra) });
    case "held":
      return t("status.held");
    case "free":
      return t("status.free");
    case "off":
      return t("status.off");
  }
}

/** "A", "A and B", "A, B and C". */
export function names(list: string[]): string {
  if (list.length <= 1) return list[0] ?? "";
  return t("glance.and", { a: list.slice(0, -1).join(", "), b: list[list.length - 1] ?? "" });
}

export interface Glance {
  title: string;
  detail: string;
}

/** The sentence at the top: who uses the most, and whether that's a
 *  problem for anyone else. `lineBps` is the line as the user set it. */
export function glance(l: Ledger, lineBps: number | null): Glance {
  type Who = { name: string; bps: number; slice: Slice; member: boolean };
  const other = l.slices[l.slices.length - 1];
  const everyone: Who[] = [
    ...l.slices.filter((s) => s.target !== null).map((s) => ({ name: s.name, bps: s.bps, slice: s, member: false })),
    ...(other?.members ?? [])
      .filter((m) => !m.fixed)
      .map((m) => ({ name: m.name, bps: m.bps, slice: other as Slice, member: true })),
  ].filter((w) => w.bps >= QUIET_BPS);
  everyone.sort((a, b) => b.bps - a.bps);
  const top = everyone[0];
  if (!top) return { title: t("glance.quiet.title"), detail: t("glance.quiet.body") };

  if (!l.busy) {
    return {
      title: t("glance.top.calm", { name: top.name }),
      detail:
        lineBps !== null
          ? t("glance.room", { used: mbps(l.total), line: mbps(lineBps) })
          : t("glance.used", { used: mbps(l.total) }),
    };
  }

  const title = t("glance.top.busy", { name: top.name });
  // Who else is moving and isn't left waiting. Inside Everything else
  // SnailNet shares fairly, so an app below its fair part there isn't
  // the one waiting.
  const otherMembers = other?.members.length ?? 1;
  const fine = everyone
    .filter((w) => w !== top)
    .filter((w) =>
      w.member
        ? w.bps < w.slice.bps / Math.max(1, otherMembers)
        : w.slice.status.kind !== "held",
    )
    .slice(0, 2)
    .map((w) => w.name);
  const waiting = everyone
    .filter((w) => w !== top && !w.member && w.slice.status.kind === "held")
    .map((w) => w.name);
  const lender = l.slices
    .filter((s) => s.status.kind === "lending")
    .sort((a, b) => b.bps - a.bps)[0];

  let detail: string;
  if (top.slice.status.kind === "borrowing") {
    detail = lender
      ? t("glance.borrow.from", { lender: lender.name })
      : t("glance.borrow.spare");
  } else if (top.slice.status.kind === "held") {
    detail = t("glance.held");
  } else {
    detail = "";
  }
  if (waiting.length > 0) {
    detail = [detail, t("glance.waiting", { names: names(waiting) })].filter(Boolean).join(" ");
  } else if (fine.length > 0) {
    detail = [detail, t(fine.length > 1 ? "glance.fine.many" : "glance.fine.one", { names: names(fine) })]
      .filter(Boolean)
      .join(" ");
  } else if (!detail) {
    detail = t("glance.alone");
  }
  return { title, detail };
}

export interface Part {
  key: string;
  name: string;
  /** What it is, for its icon; null for the rest of Everything else. */
  target: Target | null;
  bps: number;
  /** Colour slot: a rule's ink, or -1 (Everything else) with a shade. */
  ink: number;
  shade: number;
}

/** The live strip: what each slice moves, Everything else split by app
 *  (four at most, the rest together). */
export function parts(l: Ledger): Part[] {
  const out: Part[] = [];
  for (const s of l.slices) {
    if (s.target !== null) {
      if (s.bps > 0) out.push({ key: s.key, name: s.name, target: s.target, bps: s.bps, ink: s.ink, shade: 0 });
      continue;
    }
    const shown = s.members.slice(0, 4);
    shown.forEach((m, i) =>
      out.push({ key: m.key, name: m.name, target: m.target, bps: m.bps, ink: -1, shade: i }),
    );
    const rest = s.bps - shown.reduce((sum, m) => sum + m.bps, 0);
    if (rest > QUIET_BPS) {
      out.push({ key: "other:rest", name: t("strip.others"), target: null, bps: rest, ink: -1, shade: 4 });
    }
  }
  return out;
}
