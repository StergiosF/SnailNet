// Every UI string comes from en.json (PLAN.md §15); keys are typed so a
// missing one fails the type check.

import en from "./en.json";

export type Key = keyof typeof en;

/** The string for `key`, with `{name}` placeholders filled from `vars`. */
export function t(key: Key, vars: Record<string, string | number> = {}): string {
  return en[key].replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in vars ? String(vars[name]) : whole,
  );
}
