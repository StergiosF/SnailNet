// How the user groups apps (PLAN.md §7.2, §11.6): a program split out of
// its app ("Split out"), or a whole app counted as part of another
// ("Merge into…"). The service applies them; these build the new lists.

import type { AppInfo, Families, Merge } from "./types";

/** A split-out program's family id. */
export const SPLIT = "split:";

const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

/** The programs (exe names) seen under `family`, sorted. */
export function members(apps: AppInfo[], family: string): string[] {
  const exes = apps.filter((a) => same(a.family, family) && a.exe !== "").map((a) => a.exe);
  return [...new Set(exes)].sort();
}

/** The program a split-out family holds (`null`: not one). */
export function splitExe(family: string): string | null {
  return family.toLowerCase().startsWith(SPLIT) ? family.slice(SPLIT.length) : null;
}

const merges = (f: Families | undefined): Merge[] => f?.merge ?? [];
const splits = (f: Families | undefined): string[] => f?.split ?? [];

export function withSplit(f: Families | undefined, exe: string): Families {
  const e = exe.toLowerCase();
  return { split: [...splits(f).filter((x) => x !== e), e], merge: merges(f) };
}

/** `exe` back in its app; a merge of it goes too. */
export function withoutSplit(f: Families | undefined, exe: string): Families {
  const e = exe.toLowerCase();
  return {
    split: splits(f).filter((x) => x !== e),
    merge: merges(f).filter((m) => m.from !== SPLIT + e),
  };
}

/** `from` counts as part of `into`. What was merged into `from` follows
 *  it (merges don't chain). */
export function withMerge(
  f: Families | undefined,
  from: { id: string; name: string },
  into: { id: string; name: string },
): Families {
  const [a, b] = [from.id.toLowerCase(), into.id.toLowerCase()];
  const rest = merges(f)
    .filter((m) => m.from !== a)
    .map((m) => (m.into === a ? { ...m, into: b, name: into.name } : m));
  return {
    split: splits(f),
    merge: [...rest, { from: a, from_name: from.name, into: b, name: into.name }],
  };
}

export function withoutMerge(f: Families | undefined, from: string): Families {
  const a = from.toLowerCase();
  return { split: splits(f), merge: merges(f).filter((m) => m.from !== a) };
}

/** One line of "How apps are grouped" in the settings. */
export type Grouping =
  | { kind: "split"; key: string; exe: string; name: string }
  | { kind: "merge"; key: string; from: string; name: string; into: string };

export function groupings(f: Families | undefined, apps: AppInfo[]): Grouping[] {
  const named = (id: string) => apps.find((a) => same(a.family, id))?.name;
  return [
    ...splits(f).map((exe): Grouping => ({
      kind: "split",
      key: `s ${exe}`,
      exe,
      name: named(SPLIT + exe) ?? exe,
    })),
    ...merges(f).map((m): Grouping => ({
      kind: "merge",
      key: `m ${m.from}`,
      from: m.from,
      name: m.from_name || splitExe(m.from) || m.from,
      into: m.name,
    })),
  ];
}
