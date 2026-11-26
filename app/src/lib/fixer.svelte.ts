// The one-tap fix, live (fix.ts on the stats stream). Each window (the
// main one and the tray flyout) runs its own watch on the same stream, so
// both show the same offer; the switch and the mutes are shared through
// storage, and a change in one window reaches the other by the storage
// event. No toasts and no pop-ups (PLAN.md §11.11): the offer sits on the
// sentence the person is already reading, and what a click did, with
// Undo, shows in the same place.

import { FixWatch, isMuted, loadFix, mute, offerOf, pairKey, pairOf, saveFix, tomorrow, APPLIED_MS, type Offer } from "./fix";
import { stateKind } from "./format";
import { t } from "./i18n";
import type { Ledger } from "./ledger";
import { liveBoost } from "./modes";
import { service } from "./service.svelte";
import type { Rule, SaveError } from "./types";
import { ui } from "./ui.svelte";

const DONE_MS = 10_000;

/** What the last click did, for "Brave gets 60% now · Undo". */
type Done = { message: string; undo: { profile: string; rev: number; before: Rule[] } | null };

class Fixer {
  prefs = $state(loadFix());
  offer = $state<Offer | null>(null);
  done = $state<Done | null>(null);
  busy = $state(false);
  #watch = new FixWatch();
  #doneTimer: ReturnType<typeof setTimeout> | null = null;

  /** Why there's no button right now, though there's an offer: the Split
   *  page has unsaved changes the fix would clobber. */
  get blockedByDraft(): boolean {
    return ui.dirty;
  }

  constructor() {
    if (typeof window !== "undefined") {
      window.addEventListener("storage", (e) => {
        if (e.key === null || e.key === "snailnet.fix") {
          this.prefs = loadFix();
          if (!this.prefs.on) this.offer = null;
        }
      });
    }
  }

  /** Feed one stats tick's down and up ledgers (computed once by App / Flyout). */
  sync(down: Ledger | null, up: Ledger | null): void {
    const stats = service.stats;
    const profile = service.activeProfile;
    if (!stats || !profile || service.link.kind !== "connected" || (!down && !up)) {
      this.#watch.reset();
      this.offer = null;
      return;
    }
    const rules = profile.rules;
    const pair = pairOf(down, up);
    const held = this.#watch.feed(stats.t_ms, pair);
    const st = service.state;
    const quiet =
      !this.prefs.on ||
      held === null ||
      service.playing ||
      liveBoost(service.snapshot?.config.boost, Math.floor(Date.now() / 1000)) !== null ||
      st === null ||
      stateKind(st) !== "shaping" ||
      isMuted(this.prefs.mutes, pairKey(held), Date.now());
    const next = quiet || held === null ? null : offerOf(rules, held);
    const same =
      next === null
        ? this.offer === null
        : this.offer !== null && pairKey(this.offer.pair) === pairKey(next.pair) && this.offer.pct === next.pct;
    if (!same) this.offer = next;
  }

  #say(done: Done) {
    if (this.#doneTimer) clearTimeout(this.#doneTimer);
    this.done = done;
    this.#doneTimer = setTimeout(() => (this.done = null), DONE_MS);
  }

  #mute(key: string, until: number) {
    const now = Date.now();
    this.prefs = { ...this.prefs, mutes: mute(this.prefs.mutes, key, until, now) };
    saveFix(this.prefs);
  }

  /** The button: save the fix to the active mode (rev-checked). */
  async apply(): Promise<void> {
    const o = this.offer;
    const profile = service.activeProfile;
    if (!o || !profile || this.busy || ui.dirty) return;
    this.busy = true;
    const before = profile.rules;
    try {
      const rev = await service.saveRules(o.rules);
      this.#mute(pairKey(o.pair), Date.now() + APPLIED_MS);
      this.#watch.reset();
      this.offer = null;
      this.#say({
        message: t("fix.done", { name: o.pair.name, pct: o.pct }),
        undo: { profile: profile.id, rev, before },
      });
    } catch (e) {
      const err = e as SaveError;
      this.#say({
        message: err.kind === "conflict" ? t("editor.conflict") : t("fix.failed", { message: err.message }),
        undo: null,
      });
    } finally {
      this.busy = false;
    }
  }

  /** Put back the split from before the fix, if nothing changed it since. */
  async undo(): Promise<void> {
    const u = this.done?.undo;
    if (!u || this.busy) return;
    if (service.rev !== u.rev || service.activeProfile?.id !== u.profile) {
      this.#say({ message: t("fix.stale"), undo: null });
      return;
    }
    this.busy = true;
    try {
      await service.saveRulesOf(u.profile, u.before);
      this.#say({ message: t("fix.undone"), undo: null });
    } catch {
      this.#say({ message: t("fix.stale"), undo: null });
    } finally {
      this.busy = false;
    }
  }

  /** "Not now": this pair stays quiet until tomorrow. */
  later(): void {
    const o = this.offer;
    if (!o) return;
    this.#mute(pairKey(o.pair), tomorrow(new Date()));
    this.offer = null;
  }

  /** The Settings switch. */
  setOn(on: boolean): void {
    this.prefs = { ...this.prefs, on };
    saveFix(this.prefs);
    if (!on) this.offer = null;
  }
}

export const fixer = new Fixer();
