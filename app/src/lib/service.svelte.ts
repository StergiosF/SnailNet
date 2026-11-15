// The UI's view of the service, fed by the Rust bridge (app/src-tauri):
// `current` for whatever happened before the page loaded, then events.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Latest, Link, ServiceState, Snapshot, StatsTick } from "./types";

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
      listen<StatsTick>("stats", (e) => (this.stats = e.payload)),
    ]);
    const now = await invoke<Latest>("current");
    this.link = now.link;
    this.snapshot ??= now.snapshot;
    this.stats ??= now.stats;
  }
}

export const service = new Service();
