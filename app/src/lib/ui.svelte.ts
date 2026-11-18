// The main window's own state: which direction the numbers show, which
// row is open, and rule changes on their way to the service. A change
// shows at once (`rules` is the draft until the service has it) and saves
// after a short pause, so a few quick − / + clicks become one save; a
// save waits for the previous one to come back, so they never race.

import { t } from "./i18n";
import { dismissFirstRun, firstRunDismissed, forgetFirstRun } from "./line";
import { service } from "./service.svelte";
import type { Rule, SaveError } from "./types";

export type Dir = "down" | "up";
export type Page = "now" | "split" | "settings";

/** A short message at the bottom of the window, with an optional action
 *  (Undo). It goes away by itself. */
export type Toast = { message: string; action?: { label: string; run: () => void } };

const TOAST_MS = 8000;

/** A state that needs attention (PLAN.md §14.8), shown in the header. */
export type Notice = {
  severity: "info" | "caution" | "critical";
  title: string;
  message: string;
  action?: { label: string; onclick: () => void };
};

const SETTLE_MS = 300;

class Ui {
  page = $state<Page>("now");
  /** First run (§14.5) was finished or skipped. */
  firstRunDone = $state(firstRunDismissed());
  dir = $state<Dir>("down");
  toast = $state<Toast | null>(null);
  /** Unix seconds, ticking every few seconds (boost countdowns). */
  now = $state(Math.floor(Date.now() / 1000));
  /** Whether Windows says this PC has a network (§14.8 Offline). */
  online = $state(typeof navigator === "undefined" ? true : navigator.onLine);
  #toastTimer: ReturnType<typeof setTimeout> | null = null;
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

  finishFirstRun() {
    dismissFirstRun();
    this.firstRunDone = true;
  }

  /** After "Reset SnailNet": back to the Now page, and first run again. */
  restartFirstRun() {
    forgetFirstRun();
    this.firstRunDone = false;
    this.page = "now";
    this.open = null;
    this.member = null;
  }

  say(toast: Toast) {
    if (this.#toastTimer) clearTimeout(this.#toastTimer);
    this.toast = toast;
    this.#toastTimer = setTimeout(() => (this.toast = null), TOAST_MS);
  }

  /** Keep `now` current; returns the stop function. */
  tick(): () => void {
    const id = setInterval(() => (this.now = Math.floor(Date.now() / 1000)), 5000);
    const net = () => (this.online = navigator.onLine);
    window.addEventListener("online", net);
    window.addEventListener("offline", net);
    return () => {
      clearInterval(id);
      window.removeEventListener("online", net);
      window.removeEventListener("offline", net);
    };
  }
}

export const ui = new Ui();
