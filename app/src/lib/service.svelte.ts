// The UI's view of the service, fed by the Rust bridge (app/src-tauri):
// `current` for whatever happened before the page loaded, then events.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { withSpeeds } from "./line";
import { withBar } from "./rules";
import { Steady } from "./steady";
import { WaitWatch } from "./waiting";
import type { Families, Latest, Link, Rule, SaveError, ServiceState, Settings, Snapshot, StatsTick, Target } from "./types";

const notConnected: SaveError = { kind: "failed", message: "not connected" };
/** Cap buffered catchup ticks for windows that never register `onCatchup` (e.g. flyout). */
const MAX_PENDING_CATCHUP = 2_400;

class Service {
  link = $state<Link>({ kind: "connecting" });
  snapshot = $state<Snapshot | null>(null);
  /** Highest config revision we know the service holds: updated when a
   *  snapshot arrives and immediately when a write returns a new rev, so
   *  back-to-back writes or immediate Undo don't race the bridge push. */
  #knownRev = $state(0);
  /** The live stats, steadied for the eye (steady.ts). */
  stats = $state<StatsTick | null>(null);
  #steady = new Steady();
  /** Apps left waiting (waiting.ts), per direction, from the raw ticks:
   *  the rule needs each tick's pressure, not the steadied rates. */
  waiting = $state<{ down: string[]; up: string[] }>({ down: [], up: [] });
  #waitDown = new WaitWatch();
  #waitUp = new WaitWatch();
  #catchupListeners = new Set<(ticks: StatsTick[]) => void>();
  #pendingCatchup: StatsTick[] = [];

  get rev(): number {
    return Math.max(this.snapshot?.rev ?? 0, this.#knownRev);
  }

  #applySnapshot(snap: Snapshot) {
    this.snapshot = snap;
    if (snap.rev > this.#knownRev) this.#knownRev = snap.rev;
    this.#flushCatchup();
  }

  #rememberSites(tick: StatsTick) {
    for (const s of tick.down?.sites ?? []) {
      if (this.#seenSiteNames[s.site] !== s.name) this.#seenSiteNames[s.site] = s.name;
    }
    for (const s of tick.up?.sites ?? []) {
      if (this.#seenSiteNames[s.site] !== s.name) this.#seenSiteNames[s.site] = s.name;
    }
  }

  #flushCatchup() {
    if (!this.snapshot || this.#catchupListeners.size === 0 || this.#pendingCatchup.length === 0) return;
    const pending = this.#pendingCatchup;
    this.#pendingCatchup = [];
    for (const fn of this.#catchupListeners) fn(pending);
  }

  #deliverCatchup(ticks: StatsTick[]) {
    if (ticks.length === 0) return;
    for (const tick of ticks) this.#rememberSites(tick);
    if (!this.snapshot || this.#catchupListeners.size === 0) {
      this.#pendingCatchup.push(...ticks);
      if (this.#pendingCatchup.length > MAX_PENDING_CATCHUP) {
        this.#pendingCatchup.splice(0, this.#pendingCatchup.length - MAX_PENDING_CATCHUP);
      }
      return;
    }
    for (const fn of this.#catchupListeners) fn(ticks);
  }

  /** Receive ticks missed while the window was hidden, in chronological
   *  order. Flushes any buffered catchup ticks if registered after `start()`. */
  onCatchup(fn: (ticks: StatsTick[]) => void): () => void {
    this.#catchupListeners.add(fn);
    if (this.snapshot && this.#pendingCatchup.length > 0) {
      const pending = this.#pendingCatchup;
      this.#pendingCatchup = [];
      fn(pending);
    }
    return () => {
      this.#catchupListeners.delete(fn);
    };
  }

  #take(tick: StatsTick) {
    this.#rememberSites(tick);
    this.stats = this.#steady.step(tick);
    const down = this.#waitDown.feed(tick.down?.families ?? []);
    const up = this.#waitUp.feed(tick.up?.families ?? []);
    const same = (a: string[], b: string[]) => a.length === b.length && a.every((x, i) => x === b[i]);
    if (!same(down, this.waiting.down) || !same(up, this.waiting.up)) this.waiting = { down, up };
  }

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
    const rev = await invoke<number>(command, { ...args, rev: this.rev });
    if (rev > this.#knownRev) this.#knownRev = rev;
    return rev;
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
    // Track live events that arrive while `current` is in flight so stale
    // `current` fields never overwrite fresher events and catchup ticks
    // always reach the timeline strictly before newer live ticks.
    let ready = false;
    let sawLink = false;
    let sawSnapshot = false;
    let sawState: ServiceState | null = null;
    let sawCatchup = false;
    const earlyCatchup: StatsTick[][] = [];
    const earlyStats: StatsTick[] = [];
    // Listen first so nothing falls between `current` and the first event.
    await Promise.all([
      listen<Link>("link", (e) => {
        sawLink = true;
        this.link = e.payload;
        if (e.payload.kind !== "connected") {
          this.stats = null;
          this.#knownRev = 0;
          this.#pendingCatchup = [];
          earlyCatchup.length = 0;
          earlyStats.length = 0;
          this.#steady.reset();
          this.#waitDown.reset();
          this.#waitUp.reset();
          this.waiting = { down: [], up: [] };
        }
      }),
      listen<Snapshot>("snapshot", (e) => {
        sawSnapshot = true;
        sawState = null;
        this.#applySnapshot(e.payload);
      }),
      listen<ServiceState>("service-state", (e) => {
        if (this.snapshot) this.snapshot.state = e.payload;
        else sawState = e.payload;
      }),
      listen<StatsTick>("stats", (e) => {
        if (!ready) {
          earlyStats.push(e.payload);
          return;
        }
        this.#take(e.payload);
      }),
      listen<StatsTick[]>("stats-catchup", (e) => {
        sawCatchup = true;
        if (!ready) {
          earlyCatchup.push(e.payload);
          return;
        }
        this.#deliverCatchup(e.payload);
      }),
    ]);
    try {
      const now = await invoke<Latest>("current");
      if (!sawLink) this.link = now.link;
      if (!sawSnapshot && now.snapshot) {
        this.#applySnapshot(sawState !== null ? { ...now.snapshot, state: sawState } : now.snapshot);
      }
      if (this.link.kind === "connected") {
        if (!sawCatchup && now.missed_stats && now.missed_stats.length > 0) {
          this.#deliverCatchup(now.missed_stats);
        }
        for (const batch of earlyCatchup) {
          this.#deliverCatchup(batch);
        }
        if (earlyStats.length === 0 && now.stats) {
          this.#take(now.stats);
        } else {
          for (const tick of earlyStats) {
            this.#take(tick);
          }
        }
      }
    } finally {
      ready = true;
    }
  }
}

export const service = new Service();
