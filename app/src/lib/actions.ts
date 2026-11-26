// What the Now, Split and Settings pages do with one click: switch mode,
// boost, reset. Each says how it went in the toast at the bottom of the
// window; a reset of the split can be undone from there.

import { invoke } from "@tauri-apps/api/core";
import { firstChoiceOf, withFirstChoice, type FirstChoice } from "./firstChoice";
import { stateKind } from "./format";
import { t, type Key } from "./i18n";
import { copyName, modeName } from "./modes";
import { service } from "./service.svelte";
import { withMerge, withSplit, withoutMerge, withoutSplit } from "./families";
import type { Families, Measured, SaveError, Target } from "./types";
import { ui } from "./ui.svelte";

/** A failed change in words, under `key` ("Couldn't … {message}"). */
function failed(key: Key, e: unknown) {
  const err = e as SaveError;
  ui.say({
    kind: "error",
    message: err.kind === "conflict" ? t("editor.conflict") : t(key, { message: err.message ?? "" }),
  });
}

/** Switch mode; with unsaved changes on the Split page, ask first (the
 *  switch then happens once they're saved or dropped). */
export function switchMode(id: string): Promise<void> {
  if (service.snapshot?.config.active_profile === id) return Promise.resolve();
  if (!ui.dirty) return doSwitch(id);
  ui.guard(() => void doSwitch(id));
  return Promise.resolve();
}

async function doSwitch(id: string): Promise<void> {
  ui.open = null;
  ui.member = null;
  try {
    await service.switchMode(id);
  } catch (e) {
    failed("mode.failed", e);
  }
}

export async function boost(target: Target, minutes: number | null): Promise<void> {
  try {
    await service.boost(target, minutes);
  } catch (e) {
    failed("boost.failed", e);
  }
}

export async function stopBoost(): Promise<void> {
  try {
    await service.stopBoost();
  } catch (e) {
    failed("boost.failed", e);
  }
}

export async function pause(minutes: number | null): Promise<void> {
  try {
    await service.pause(minutes);
  } catch (e) {
    failed("pause.failed", e);
  }
}

export async function resume(): Promise<void> {
  try {
    await service.resume();
  } catch (e) {
    failed("pause.failed", e);
  }
}

/** Block an app's internet; the toast can undo it. */
export async function block(familyId: string, name: string): Promise<void> {
  try {
    await service.setBlocked(familyId, true);
    ui.say({
      message: t("block.done", { name }),
      action: { label: t("undo"), run: () => void unblock(familyId) },
    });
  } catch (e) {
    failed("block.failed", e);
  }
}

export async function unblock(familyId: string): Promise<void> {
  try {
    await service.setBlocked(familyId, false);
  } catch (e) {
    failed("block.failed", e);
  }
}

/** Save a new grouping (§11.6), saying `done` with an Undo back to how
 *  it was. */
async function regroup(next: (f: Families | undefined) => Families, done: string): Promise<void> {
  const before = service.snapshot?.config.families ?? {};
  try {
    await service.setFamilies(next(before));
    ui.say({
      message: done,
      action: { label: t("undo"), run: () => void service.setFamilies(before).catch((e) => failed("group.failed", e)) },
    });
  } catch (e) {
    failed("group.failed", e);
  }
}

/** `exe` becomes an app of its own, out of `app`. */
export function splitOut(exe: string, app: string): Promise<void> {
  return regroup((f) => withSplit(f, exe), t("group.split.done", { exe, app }));
}

/** A split-out `exe` goes back into its app. */
export function putBack(exe: string): Promise<void> {
  return regroup((f) => withoutSplit(f, exe), t("group.back.done", { exe }));
}

export function mergeInto(from: { id: string; name: string }, into: { id: string; name: string }): Promise<void> {
  return regroup((f) => withMerge(f, from, into), t("group.merge.done", { from: from.name, into: into.name }));
}

/** A merged app is on its own again. */
export function separate(from: string, name: string): Promise<void> {
  return regroup((f) => withoutMerge(f, from), t("group.separate.done", { name }));
}

/** Say whether an app goes first (§11.5 overrides); the toast can undo it. */
export async function setFirstChoice(familyId: string, name: string, choice: FirstChoice): Promise<void> {
  const settings = service.snapshot?.config.settings;
  if (!settings) return;
  const before = firstChoiceOf(settings, familyId);
  if (before === choice) return;
  try {
    await service.setSettings(withFirstChoice(settings, familyId, choice));
    ui.say({
      message: t(`first.done.${choice}`, { name }),
      action: {
        label: t("undo"),
        run: () => {
          const cur = service.snapshot?.config.settings ?? settings;
          void service
            .setSettings(withFirstChoice(cur, familyId, before))
            .then(() => ui.say({ message: t(`first.done.${before}`, { name }) }))
            .catch((e: unknown) => failed("first.failed", e));
        },
      },
    });
  } catch (e) {
    failed("first.failed", e);
  }
}

/** "Duplicate & edit" (§11.2): copy the active mode; the copy comes on,
 *  so the Split page edits it. Undo deletes the copy again. */
export async function copyMode(): Promise<void> {
  const profile = service.activeProfile;
  const profiles = service.snapshot?.config.profiles;
  if (!profile || !profiles) return;
  const name = copyName(modeName(profile), profiles);
  try {
    await caughtUp(await service.copyMode(profile.id, name));
    const copy = service.snapshot?.config.profiles.find((p) => p.name === name);
    ui.say({
      message: t("mode.copy.done", { mode: name }),
      action: copy ? { label: t("undo"), run: () => void deleteMode(copy.id, name, profile.id) } : undefined,
    });
  } catch (e) {
    failed("mode.change.failed", e);
  }
}

export async function renameMode(id: string, name: string): Promise<boolean> {
  try {
    await service.renameMode(id, name);
    return true;
  } catch (e) {
    failed("mode.change.failed", e);
    return false;
  }
}

/** Delete one of the user's own modes; `back` is switched to afterwards
 *  (Undo of a copy goes back to the mode it was copied from). */
export async function deleteMode(id: string, name: string, back?: string): Promise<void> {
  try {
    await service.deleteMode(id);
    if (back) await service.switchMode(back);
    else ui.say({ message: t("mode.delete.done", { mode: name }) });
  } catch (e) {
    failed("mode.change.failed", e);
  }
}

/** Put the active mode's split back to where it started; Undo puts the
 *  rules it had back. */
export async function resetSplit(): Promise<void> {
  const profile = service.activeProfile;
  if (!profile) return;
  const before = profile.rules;
  const mode = modeName(profile);
  ui.open = null;
  try {
    await service.resetMode(profile.id);
    ui.say({
      message: t("split.reset.done", { mode }),
      action: {
        label: t("undo"),
        run: () => {
          service.saveRulesOf(profile.id, before).catch((e: unknown) => failed("split.reset.failed", e));
        },
      },
    });
  } catch (e) {
    failed("split.reset.failed", e);
  }
}

/** Forget everything and show setup again. */
export async function resetAll(): Promise<boolean> {
  try {
    await service.resetAll();
    return true;
  } catch (e) {
    failed("settings.resetAll.failed", e);
    return false;
  }
}

/** What `start_service` (app/src-tauri/src/service_start.rs) reports. */
type StartOutcome = "asked" | "cancelled" | "not_installed" | { failed: { message: string } };

/** "Start SnailNet" on the not-running notice: Windows asks for admin
 *  rights, then the app reconnects by itself. */
export async function startService(): Promise<void> {
  let outcome: StartOutcome;
  try {
    outcome = await invoke<StartOutcome>("start_service");
  } catch (e) {
    outcome = { failed: { message: String(e) } };
  }
  if (outcome === "asked") ui.say({ message: t("start.asked") });
  else if (outcome === "cancelled") ui.say({ kind: "error", message: t("start.cancelled") });
  else if (outcome === "not_installed") ui.say({ kind: "error", message: t("start.not_installed") });
  else ui.say({ kind: "error", message: t("start.failed", { message: outcome.failed.message }) });
}

/** Wait (up to 2 s) until the snapshot shows revision `rev`, so the next
 *  change is made against it. */
function caughtUp(rev: number): Promise<boolean> {
  return until(() => (service.snapshot?.rev ?? 0) >= rev, 2_000);
}

/** Resolves once `ok()` holds (checked every 250 ms), or after `ms`. */
function until(ok: () => boolean, ms: number): Promise<boolean> {
  return new Promise((resolve) => {
    const end = Date.now() + ms;
    const check = () => {
      if (ok()) resolve(true);
      else if (Date.now() >= end) resolve(false);
      else setTimeout(check, 250);
    };
    check();
  });
}

/** Whether a speed test can start now: connected, and not measuring,
 *  paused or in safe mode (a paused line can't run the check with
 *  SnailNet, §11.10). */
export function canMeasure(): boolean {
  const s = service.state;
  if (ui.measuring || s === null || service.link.kind !== "connected") return false;
  const k = stateKind(s);
  return k !== "measuring" && k !== "paused" && k !== "safe_mode";
}

/** How long the check with SnailNet may take after the test (§11.10). */
const CHECK_WAIT_MS = 20_000;

/** Run the speed test (§10.1) and say how it went. The service turns
 *  shaping off, measures for ~20 s and saves the result as the line;
 *  if no new result arrives once it's done, the test failed. Then it
 *  shapes at the new speed and downloads once more (~10 s) for the ping
 *  with SnailNet (`with_ms`); the toast gives the before and after when
 *  that arrives, the plain result if it doesn't. */
export async function measureLine(): Promise<Measured | null> {
  if (ui.measuring) return null;
  ui.measuring = true;
  try {
    return await measure();
  } finally {
    ui.measuring = false;
  }
}

async function measure(): Promise<Measured | null> {
  const before = service.snapshot?.config.line.measured?.at;
  try {
    await service.runSpeedTest();
  } catch (e) {
    failed("measure.failed", e);
    return null;
  }
  const measuring = () => service.state !== null && stateKind(service.state) === "measuring";
  await until(measuring, 10_000);
  await until(() => !measuring(), 90_000);
  // The new line comes as a config change, maybe just after the state.
  const got = await until(() => (service.snapshot?.config.line.measured?.at ?? before) !== before, 3_000);
  const first = service.snapshot?.config.line.measured;
  if (!got || !first) {
    ui.say({ kind: "error", message: t("measure.none") });
    return null;
  }
  // No ping during the test (ICMP blocked): there's nothing to compare.
  if (first.loaded_ms !== undefined) {
    ui.say({ message: t("measure.checking") });
    await until(() => {
      const m = service.snapshot?.config.line.measured;
      return m?.at !== first.at || m.with_ms !== undefined;
    }, CHECK_WAIT_MS);
  }
  const now = service.snapshot?.config.line.measured;
  const m = now?.at === first.at ? now : first;
  ui.say({
    message:
      m.with_ms !== undefined && m.loaded_ms !== undefined
        ? t("measure.done.proof", {
            down: m.down_mbps,
            up: m.up_mbps,
            without: Math.round(m.loaded_ms),
            with: Math.round(m.with_ms),
          })
        : t("measure.done", { down: m.down_mbps, up: m.up_mbps }),
  });
  return m;
}
