// The UI's view of the service, fed by the Rust bridge (app/src-tauri):
// `current` for whatever happened before the page loaded, then events.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { withBar } from "./rules";
import type { Latest, Link, Rule, SaveError, ServiceState, Snapshot, StatsTick } from "./types";

class Service {
  link = $state<Link>({ kind: "connecting" });
  snapshot = $state<Snapshot | null>(null);
  stats = $state<StatsTick | null>(null);

  /** The freshest service state: the stats stream's, else the snapshot's. */
  state = $derived<ServiceState | null>(
    this.link.kind === "connected" ? (this.stats?.state ?? this.snapshot?.state ?? null) : null,
  );

  activeProfile = $derived(
    this.snapshot?.config.profiles.find((p) => p.id === this.snapshot?.config.active_profile) ??
      null,
  );

  /** Display names of sites seen in the stats, kept after they go quiet. */
  siteNames = $state<Record<string, string>>({});

  /** Save the active profile's rules. Resolves to the new revision;
   *  rejects with a SaveError (a stale `rev` is a conflict). */
  async saveRules(rules: Rule[]): Promise<number> {
    const snap = this.snapshot;
    const profile = this.activeProfile;
    if (!snap || !profile) throw { kind: "failed", message: "not connected" } satisfies SaveError;
    return invoke<number>("set_rules", { profileId: profile.id, rules, rev: snap.rev });
  }

  /** Save new slice sizes (the bar's full list; Everything else is
   *  dropped) onto the active profile's slices, in order. */
  async saveShares(shares: number[]): Promise<number> {
    return this.saveRules(withBar(this.activeProfile?.rules ?? [], shares));
  }

  async start(): Promise<void> {
    // Listen first so nothing falls between `current` and the first event.
    await Promise.all([
      listen<Link>("link", (e) => {
        this.link = e.payload;
        if (e.payload.kind !== "connected") this.stats = null;
      }),
      listen<Snapshot>("snapshot", (e) => (this.snapshot = e.payload)),
      listen<ServiceState>("service-state", (e) => {
        if (this.snapshot) this.snapshot.state = e.payload;
      }),
      listen<StatsTick>("stats", (e) => {
        this.stats = e.payload;
        for (const s of e.payload.down?.sites ?? []) {
          if (this.siteNames[s.site] !== s.name) this.siteNames[s.site] = s.name;
        }
      }),
    ]);
    const now = await invoke<Latest>("current");
    this.link = now.link;
    this.snapshot ??= now.snapshot;
    this.stats ??= now.stats;
  }
}

export const service = new Service();
