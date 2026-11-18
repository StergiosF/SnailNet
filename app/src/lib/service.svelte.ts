// The UI's view of the service, fed by the Rust bridge (app/src-tauri):
// `current` for whatever happened before the page loaded, then events.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { withSpeeds } from "./line";
import { withBar } from "./rules";
import { Steady } from "./steady";
import type { Families, Latest, Link, Rule, SaveError, ServiceState, Settings, Snapshot, StatsTick, Target } from "./types";

const notConnected: SaveError = { kind: "failed", message: "not connected" };

class Service {
  link = $state<Link>({ kind: "connecting" });
  snapshot = $state<Snapshot | null>(null);
  /** The live stats, steadied for the eye (steady.ts). */
  stats = $state<StatsTick | null>(null);
  #steady = new Steady();

  /** The freshest service state: the stats stream's, else the snapshot's. */
  state = $derived<ServiceState | null>(
    this.link.kind === "connected" ? (this.stats?.state ?? this.snapshot?.state ?? null) : null,
  );

  /** Gaming mode stands in for the picked mode while a game runs (§11.5);
   *  false when Gaming is the pick anyway. */
  playing = $derived(
    this.link.kind === "connected" &&
      this.stats?.playing === true &&
      this.snapshot?.config.active_profile !== "gaming",
  );

  activeProfile = $derived(
    this.snapshot?.config.profiles.find((p) => p.id === this.snapshot?.config.active_profile) ??
      null,
  );

  /** Display names of sites seen in the stats, kept after they go quiet. */
  #seenSiteNames = $state<Record<string, string>>({});

  /** Every site's display name: the service's known sites, plus any seen
   *  in the stats (custom sites included). */
  siteNames = $derived<Record<string, string>>({
    ...Object.fromEntries((this.snapshot?.sites ?? []).map((s) => [s.id, s.name])),
    ...this.#seenSiteNames,
  });

  /** Ask the service for a change (app/src-tauri/src/changes.rs) at the
   *  revision we hold. Resolves to the new revision; rejects with a
   *  SaveError (a stale `rev` is a conflict). */
  async #change(command: string, args: Record<string, unknown>): Promise<number> {
    const snap = this.snapshot;
    if (!snap || this.link.kind !== "connected") throw notConnected;
    return invoke<number>(command, { ...args, rev: snap.rev });
  }

  /** Save the active profile's rules. */
  async saveRules(rules: Rule[]): Promise<number> {
    const profile = this.activeProfile;
    if (!profile) throw notConnected;
    return this.saveRulesOf(profile.id, rules);
  }

  /** Save one profile's rules (Undo after a reset). */
  async saveRulesOf(profileId: string, rules: Rule[]): Promise<number> {
    return this.#change("set_rules", { profileId, rules });
  }

  /** Switch mode. */
  async switchMode(id: string): Promise<number> {
    return this.#change("switch_profile", { id });
  }

  /** Put a mode's split back to where it started. */
  async resetMode(id: string): Promise<number> {
    return this.#change("reset_profile", { id });
  }

  /** Copy mode `from` as `name`; the copy becomes the active mode. */
  async copyMode(from: string, name: string): Promise<number> {
    return this.#change("copy_profile", { from, name });
  }

  async renameMode(id: string, name: string): Promise<number> {
    return this.#change("rename_profile", { id, name });
  }

  async deleteMode(id: string): Promise<number> {
    return this.#change("delete_profile", { id });
  }

  /** Forget everything: rules, line speed, settings, boost. */
  async resetAll(): Promise<number> {
    return this.#change("reset_all", {});
  }

  /** Let `target` go first for `minutes` (null: until stopped). */
  async boost(target: Target, minutes: number | null): Promise<number> {
    return this.#change("boost", { target, minutes });
  }

  async stopBoost(): Promise<number> {
    return this.#change("stop_boost", {});
  }

  /** Step aside for `minutes` (null: until resumed). */
  async pause(minutes: number | null): Promise<number> {
    return this.#change("pause", { minutes });
  }

  async resume(): Promise<number> {
    return this.#change("resume", {});
  }

  /** Replace how apps are grouped (§11.6). */
  async setFamilies(families: Families): Promise<number> {
    return this.#change("set_families", { families });
  }

  /** Block or unblock an app's internet. */
  async setBlocked(familyId: string, blocked: boolean): Promise<number> {
    return this.#change("set_blocked", { familyId, blocked });
  }

  /** Save new slice sizes (the bar's full list; Everything else is
   *  dropped) onto the active profile's slices, in order. */
  async saveShares(shares: number[]): Promise<number> {
    return this.saveRules(withBar(this.activeProfile?.rules ?? [], shares));
  }

  /** Ask for a speed test (§10.1); the result arrives as a new line. */
  async runSpeedTest(): Promise<number> {
    return this.#change("run_speed_test", {});
  }

  /** Keep the typed speeds as they are (true), or let SnailNet follow the
   *  line (false). */
  async setPinned(pinned: boolean): Promise<number> {
    const line = this.snapshot?.config.line;
    if (!line) throw notConnected;
    const next = { ...line };
    if (pinned) next.pinned = true;
    else delete next.pinned;
    return this.#change("set_line", { line: next });
  }

  /** Change some general settings (§11.5: focus boost, Gaming mode while
   *  playing), keeping the rest. */
  async setSettings(change: Partial<Settings>): Promise<number> {
    const settings = this.snapshot?.config.settings;
    if (!settings) throw notConnected;
    return this.#change("set_settings", { settings: { ...settings, ...change } });
  }

  /** Save the line speed (PLAN.md §12.2 `SetLine`); null = don't shape
   *  that direction. Keeps the factor. Resolves to the new revision;
   *  rejects with a SaveError. */
  async saveLine(down: number | null, up: number | null): Promise<number> {
    const line = this.snapshot?.config.line;
    if (!line) throw notConnected;
    return this.#change("set_line", { line: withSpeeds(line, down, up) });
  }

  async start(): Promise<void> {
    // Listen first so nothing falls between `current` and the first event.
    await Promise.all([
      listen<Link>("link", (e) => {
        this.link = e.payload;
        if (e.payload.kind !== "connected") {
          this.stats = null;
          this.#steady.reset();
        }
      }),
      listen<Snapshot>("snapshot", (e) => (this.snapshot = e.payload)),
      listen<ServiceState>("service-state", (e) => {
        if (this.snapshot) this.snapshot.state = e.payload;
      }),
      listen<StatsTick>("stats", (e) => {
        this.stats = this.#steady.step(e.payload);
        for (const s of e.payload.down?.sites ?? []) {
          if (this.#seenSiteNames[s.site] !== s.name) this.#seenSiteNames[s.site] = s.name;
        }
      }),
    ]);
    const now = await invoke<Latest>("current");
    this.link = now.link;
    this.snapshot ??= now.snapshot;
    if (this.stats === null && now.stats) this.stats = this.#steady.step(now.stats);
  }
}

export const service = new Service();
