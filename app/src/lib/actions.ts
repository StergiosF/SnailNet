// What the Now, Split and Settings pages do with one click: switch mode,
// boost, reset. Each says how it went in the toast at the bottom of the
// window; a reset of the split can be undone from there.

import { invoke } from "@tauri-apps/api/core";
import { firstChoiceOf, withFirstChoice, type FirstChoice } from "./firstChoice";
import { stateKind } from "./format";
import { t, type Key } from "./i18n";
import { modeName } from "./modes";
import { service } from "./service.svelte";
import type { Measured, SaveError, Target } from "./types";
import { ui } from "./ui.svelte";

/** A failed change in words, under `key` ("Couldn't … {message}"). */
function failed(key: Key, e: unknown) {
  const err = e as SaveError;
  ui.say({ message: err.kind === "conflict" ? t("editor.conflict") : t(key, { message: err.message ?? "" }) });
}

export async function switchMode(id: string): Promise<void> {
  if (service.snapshot?.config.active_profile === id) return;
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
      action: { label: t("undo"), run: () => void setFirstChoice(familyId, name, before) },
    });
  } catch (e) {
    failed("first.failed", e);
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
  else if (outcome === "cancelled") ui.say({ message: t("start.cancelled") });
  else if (outcome === "not_installed") ui.say({ message: t("start.not_installed") });
  else ui.say({ message: t("start.failed", { message: outcome.failed.message }) });
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

/** Run the speed test (§10.1) and say how it went. The service turns
 *  shaping off, measures for ~20 s and saves the result as the line;
 *  if no new result arrives once it's done, the test failed. */
export async function measureLine(): Promise<Measured | null> {
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
  const m = service.snapshot?.config.line.measured;
  ui.say({
    message:
      got && m
        ? t("measure.done", { down: m.down_mbps, up: m.up_mbps })
        : t("measure.none"),
  });
  return got && m ? m : null;
}
