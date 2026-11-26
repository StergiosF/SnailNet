// Updates (PLAN.md §17.2), shared by the side rail's button and Settings.
// SnailNet looks at the release feed once when it starts and every six
// hours, quietly; installing always waits for a click, since it restarts
// SnailNet and the internet runs unshaped for a few seconds.

import { invoke } from "@tauri-apps/api/core";
import {
  checkJustUpdated,
  clearPendingUpdate,
  markUpdateSeen,
  notesForVersion,
  rememberPendingUpdate,
  type UpdateModalData,
} from "./whatsNew";

type Check =
  | { kind: "up_to_date" }
  | { kind: "available"; version: string; notes: string | null }
  | { kind: "failed"; message: string };

export type UpdateStatus =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "installing"; version: string }
  | Check;

const EVERY_MS = 6 * 60 * 60 * 1000;

class Updates {
  status = $state<UpdateStatus>({ kind: "idle" });
  /** Shown once right after installing an update (or reopened from Settings). */
  justUpdated = $state<UpdateModalData | null>(null);
  #currentVersion = "";
  /** The newer version a check found, or null. */
  available = $derived(this.status.kind === "available" ? this.status.version : null);
  busy = $derived(this.status.kind === "checking" || this.status.kind === "installing");

  /** Called on app startup with the running version; opens the "What's new"
   *  modal once if SnailNet was just updated. */
  initVersion(version: string): void {
    this.#currentVersion = version;
    const modal = checkJustUpdated(version);
    if (modal) this.justUpdated = modal;
  }

  /** Close the post-update modal and remember that this version was seen. */
  dismissWhatsNew(): void {
    const version = this.justUpdated?.version ?? this.#currentVersion;
    if (version) markUpdateSeen(version);
    this.justUpdated = null;
  }

  /** Reopen the current version's update notes (from Settings → About). */
  openWhatsNew(version?: string): void {
    const v = version || this.#currentVersion;
    if (!v) return;
    this.justUpdated = notesForVersion(v);
  }

  async check(): Promise<void> {
    if (this.busy) return;
    this.status = { kind: "checking" };
    try {
      this.status = await invoke<Check>("check_update");
    } catch (e) {
      this.status = { kind: "failed", message: String(e) };
    }
  }

  async install(): Promise<void> {
    const version = this.available;
    if (!version) return;
    const notes = this.status.kind === "available" ? this.status.notes : null;
    rememberPendingUpdate(version, notes);
    this.status = { kind: "installing", version };
    try {
      // Returns only on failure: on success the installer takes over.
      await invoke("install_update");
    } catch (e) {
      clearPendingUpdate();
      this.status = { kind: "failed", message: String(e) };
    }
  }

  /** Check now and every few hours; a failed quiet check says nothing. */
  watch(): () => void {
    const quiet = async () => {
      if (this.status.kind === "available" || this.busy) return;
      await this.check();
      if (this.status.kind === "failed") this.status = { kind: "idle" };
    };
    void quiet();
    const id = setInterval(() => void quiet(), EVERY_MS);
    return () => clearInterval(id);
  }
}

export const updates = new Updates();
