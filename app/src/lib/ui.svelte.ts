// The main window's own state: which direction the numbers show, which
// row is open, and rule changes on their way to the service.
//
// The Split page edits a draft (draft.ts): nothing reaches the line until
// Save (`commit`), and leaving the page or switching mode with unsaved
// changes asks first (`guard`).

import { forgetFix } from "./fix";
import { edit, isDirty, isSavable, isStale, startDraft, type Draft } from "./draft";
import { t } from "./i18n";
import { modeName } from "./modes";
import { dismissFirstRun, firstRunDismissed, forgetFirstRun } from "./line";
import { service } from "./service.svelte";
import type { Rule, SaveError } from "./types";

export type Dir = "down" | "up";
export type Page = "now" | "split" | "settings";

/** A short message at the bottom of the window, with an optional action
 *  (Undo). It goes away by itself. */
export type Toast = { message: string; kind?: "ok" | "error"; action?: { label: string; run: () => void } };

const TOAST_MS = 8000;

/** A state that needs attention (PLAN.md §14.8), shown in the header. */
export type Notice = {
  severity: "info" | "caution" | "critical";
  title: string;
  message: string;
  action?: { label: string; onclick: () => void };
};

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
  /** Rows the Split page's bar points at (a hovered segment, a handle's
   *  two neighbours): slice keys. */
  hot = $state<string[]>([]);
  /** A speed test this window started is running, its check with
   *  SnailNet included (actions.ts measureLine). */
  measuring = $state(false);

  #draft = $state<Rule[] | null>(null);
  /** The revision the last save produced, until the snapshot has it. */
  #waiting: number | null = null;

  /** The Split page's unsaved copy, or null (nothing changed). */
  #edit = $state<Draft | null>(null);
  /** Save is on its way. */
  committing = $state(false);
  /** Why the last Save didn't go through, or null. */
  draftError = $state<string | null>(null);
  /** What the person asked for (another page, another mode) while there
   *  were unsaved changes: done once they've said Save or Discard. */
  pending = $state<{ run: () => void } | null>(null);

  /** The active profile's rules, with any change not saved yet. */
  get rules(): Rule[] {
    return this.#edit?.rules ?? this.#draft ?? service.activeProfile?.rules ?? [];
  }

  get saving(): boolean {
    return this.#draft !== null || this.committing;
  }

  /** The draft, while it differs from what's saved. */
  get draft(): Draft | null {
    return this.#edit;
  }

  /** Unsaved changes on the Split page. */
  get dirty(): boolean {
    return isDirty(this.#edit);
  }

  /** The saved rules changed somewhere else since editing started. */
  get stale(): boolean {
    const p = service.activeProfile;
    return this.dirty && isStale(this.#edit, p?.id ?? null, p?.rules ?? []);
  }

  /** A Split-page edit: into the draft, not the service. Back to exactly
   *  what's saved, the draft goes away. */
  edit(next: Rule[]) {
    const p = service.activeProfile;
    if (!p) return;
    this.draftError = null;
    const d = edit(this.#edit ?? startDraft(p.id, p.rules), next);
    this.#edit = isDirty(d) ? d : null;
  }

  /** Drop the unsaved changes. */
  discard() {
    this.#edit = null;
    this.draftError = null;
  }

  /** Save the draft (rev-checked, like every change). On a conflict, or
   *  any failure, the draft stays and says why. */
  async commit(): Promise<boolean> {
    const d = this.#edit;
    if (!d) return true;
    if (!isSavable(d)) {
      this.draftError = t("draft.invalid");
      return false;
    }
    this.committing = true;
    this.draftError = null;
    try {
      const rev = await service.saveRulesOf(d.profile, d.rules);
      const p = service.activeProfile;
      // Keep showing the saved rules until the snapshot has them.
      if (p?.id === d.profile) {
        this.#draft = d.rules;
        this.#waiting = rev;
      }
      this.#edit = null;
      this.#settle();
      const mode = modeName(service.snapshot?.config.profiles.find((x) => x.id === d.profile) ?? null);
      this.say({ message: t("draft.saved", { mode }) });
      return true;
    } catch (e) {
      const err = e as SaveError;
      this.draftError = err.kind === "conflict" ? t("draft.conflict") : t("draft.failed", { message: err.message });
      return false;
    } finally {
      this.committing = false;
    }
  }

  /** Run `go` now, or, with unsaved changes, once the person has said
   *  what to do with them (LeaveDialog). */
  guard(go: () => void) {
    if (this.dirty) this.pending = { run: go };
    else go();
  }

  /** The answer to "Save your changes?". */
  async answer(choice: "save" | "discard" | "stay") {
    const p = this.pending;
    if (choice === "stay" || !p) {
      this.pending = null;
      return;
    }
    if (choice === "save" && !(await this.commit())) {
      // The bar on the page says why; stay there.
      this.pending = null;
      return;
    }
    if (choice === "discard") this.discard();
    this.pending = null;
    p.run();
  }

  /** Open a page; leaving the Split page with unsaved changes asks first. */
  go(page: Page) {
    if (page !== this.page) this.guard(() => (this.page = page));
  }

  /** Drop the draft once the service's snapshot holds it. Called after a
   *  save and on every new snapshot. */
  #settle() {
    const rev = service.snapshot?.rev ?? 0;
    if (this.#waiting !== null && rev >= this.#waiting) {
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
    this.discard();
    forgetFirstRun();
    forgetFix();
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
