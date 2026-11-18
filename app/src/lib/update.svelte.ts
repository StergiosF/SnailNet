// Updates (PLAN.md §17.2), shared by the side rail's button and Settings.
// SnailNet looks at the release feed once when it starts and every six
// hours, quietly; installing always waits for a click, since it restarts
// SnailNet and the internet runs unshaped for a few seconds.

import { invoke } from "@tauri-apps/api/core";

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
  /** The newer version a check found, or null. */
  available = $derived(this.status.kind === "available" ? this.status.version : null);
  busy = $derived(this.status.kind === "checking" || this.status.kind === "installing");

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
    this.status = { kind: "installing", version };
    try {
      // Returns only on failure: on success the installer takes over.
      await invoke("install_update");
    } catch (e) {
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
