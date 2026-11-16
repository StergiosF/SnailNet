// The main window's own state: which direction the numbers show, which
// row is open, and rule changes on their way to the service. A change
// shows at once (`rules` is the draft until the service has it) and saves
// after a short pause, so a few quick − / + clicks become one save; a
// save waits for the previous one to come back, so they never race.

import { t } from "./i18n";
import { service } from "./service.svelte";
import type { Rule, SaveError } from "./types";

export type Dir = "down" | "up";

/** A state that needs attention (PLAN.md §14.8), shown in the header. */
export type Notice = {
  severity: "info" | "caution" | "critical";
  title: string;
  message: string;
  action?: { label: string; onclick: () => void };
};

const SETTLE_MS = 300;

class Ui {
  dir = $state<Dir>("down");
  /** The row whose editor is open (a slice key), or null. */
  open = $state<string | null>(null);
  /** The app picked inside Everything else, or null. */
  member = $state<string | null>(null);
  adding = $state(false);
  error = $state<string | null>(null);

  #draft = $state<Rule[] | null>(null);
  /** The revision the last save produced, until the snapshot has it. */
  #waiting: number | null = null;
  #timer: ReturnType<typeof setTimeout> | null = null;

  /** The active profile's rules, with any change not saved yet. */
  get rules(): Rule[] {
    return this.#draft ?? service.activeProfile?.rules ?? [];
  }

  get saving(): boolean {
    return this.#draft !== null;
  }

  change(next: Rule[]) {
    this.error = null;
    this.#draft = next;
    this.#schedule(SETTLE_MS);
  }

  #schedule(ms: number) {
    if (this.#timer) clearTimeout(this.#timer);
    this.#timer = setTimeout(() => void this.#flush(), ms);
  }

  async #flush() {
    this.#timer = null;
    const next = this.#draft;
    if (!next) return;
    const rev = service.snapshot?.rev ?? 0;
    if (this.#waiting !== null && rev < this.#waiting) {
      this.#schedule(50);
      return;
    }
    try {
      const saved = await service.saveRules(next);
      this.#waiting = saved;
      this.#settle();
    } catch (e) {
      const err = e as SaveError;
      this.error =
        err.kind === "conflict" ? t("editor.conflict") : t("editor.failed", { message: err.message });
      this.#draft = null;
      this.#waiting = null;
    }
  }

  /** Drop the draft once the service's snapshot holds it. Called after a
   *  save and on every new snapshot. */
  #settle() {
    const rev = service.snapshot?.rev ?? 0;
    if (this.#timer === null && this.#waiting !== null && rev >= this.#waiting) {
      this.#draft = null;
      this.#waiting = null;
    }
  }

  constructor() {
    $effect.root(() => {
      $effect(() => {
        void service.snapshot?.rev;
        this.#settle();
      });
    });
  }

  toggle(key: string) {
    this.open = this.open === key ? null : key;
  }
}

export const ui = new Ui();
