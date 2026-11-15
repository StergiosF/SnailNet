// The UI's view of the service, fed by the Rust bridge (app/src-tauri):
// `current` for whatever happened before the page loaded, then events.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Latest, Link, SaveError, ServiceState, Snapshot, StatsTick } from "./types";

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

  /** Save new slice sizes for the active profile's enabled rules, in order
   *  (`shares` is the bar's full list; Everything else is dropped). Resolves
   *  to the new revision; rejects with a SaveError. */
  async saveShares(shares: number[]): Promise<number> {
    const snap = this.snapshot;
    const profile = this.activeProfile;
    if (!snap || !profile) throw { kind: "failed", message: "not connected" } satisfies SaveError;
    let k = 0;
    const rules = profile.rules.map((r) => (r.enabled ? { ...r, share: shares[k++] ?? r.share } : r));
    return invoke<number>("set_rules", { profileId: profile.id, rules, rev: snap.rev });
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
