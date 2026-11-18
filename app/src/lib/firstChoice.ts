// The user's word on whether an app goes first (PLAN.md §11.5 overrides):
// SnailNet decides ("auto"), it's a game (first while it runs), or never
// first. Stored as two lists of family ids in the settings.

import type { Settings } from "./types";

export type FirstChoice = "auto" | "game" | "never";

const has = (list: string[] | undefined, id: string) =>
  (list ?? []).some((x) => x.toLowerCase() === id.toLowerCase());

export function firstChoiceOf(s: Settings | undefined, family: string): FirstChoice {
  if (has(s?.games, family)) return "game";
  if (has(s?.never_first, family)) return "never";
  return "auto";
}

/** The two lists with `family` moved to `choice` (out of both for auto). */
export function withFirstChoice(
  s: Settings,
  family: string,
  choice: FirstChoice,
): Pick<Settings, "games" | "never_first"> {
  const other = (list: string[] | undefined) => (list ?? []).filter((x) => x.toLowerCase() !== family.toLowerCase());
  const games = other(s.games);
  const never_first = other(s.never_first);
  if (choice === "game") games.push(family);
  if (choice === "never") never_first.push(family);
  return { games, never_first };
}
