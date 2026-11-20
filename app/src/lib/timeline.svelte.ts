// The live side of "Last 10 minutes" (history.ts): one History for the
// window, fed by App.svelte from every stats tick (both directions, whatever
// page is open, also while the window sits in the tray) and from every new
// config (the marks). `rev` moves when the chart has something new to draw.

import { configMarks, History, samplesOf } from "./history";
import { t } from "./i18n";
import type { Ledger } from "./ledger";
import { modeName } from "./modes";
import type { Config, Target } from "./types";

class Timeline {
  readonly history = new History((name) => t("history.started", { name }));
  rev = $state(0);
  #config: Config | null = null;

  /** One stats tick, as the two directions' ledgers see it. */
  feed(tMs: number, down: Ledger | null, up: Ledger | null) {
    if (this.history.add(tMs, samplesOf(down), samplesOf(up))) this.rev++;
  }

  /** A config from the service: marks what changed since the last one. */
  config(next: Config, label: (t: Target) => string) {
    const marks = configMarks(this.#config, next, {
      mode: (id) => modeName(next.profiles.find((p) => p.id === id) ?? null),
      label,
      switched: (mode) => t("history.switched", { mode }),
      boosted: (name) => t("history.boosted", { name }),
      gets: (name, pct) => t("history.gets", { name, pct }),
      spare: (name) => t("history.spare", { name }),
      changed: (mode) => t("history.changed", { mode }),
    });
    this.#config = next;
    for (const m of marks) this.history.note(m);
    if (marks.length > 0) this.rev++;
  }
}

export const timeline = new Timeline();
