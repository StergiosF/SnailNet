// What the Now, Split and Settings pages do with one click: switch mode,
// boost, reset. Each says how it went in the toast at the bottom of the
// window; a reset of the split can be undone from there.

import { invoke } from "@tauri-apps/api/core";
import { t, type Key } from "./i18n";
import { modeName } from "./modes";
import { service } from "./service.svelte";
import type { SaveError, Target } from "./types";
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
