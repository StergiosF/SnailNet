// Which rule the side panel shows (PLAN.md §14.2 "Row expand / side
// panel"): nothing, the picker for a new slice, or one target's rule.

import type { Target } from "./types";

export type Panel = { mode: "pick" } | { mode: "edit"; target: Target };

class Editor {
  panel = $state<Panel | null>(null);
  /** What had focus when the panel opened; it gets focus back on close. */
  private opener: HTMLElement | null = null;

  open(panel: Panel) {
    if (this.panel === null) {
      const el = document.activeElement;
      this.opener = el instanceof HTMLElement ? el : null;
    }
    this.panel = panel;
  }

  close() {
    this.panel = null;
    const el = this.opener;
    this.opener = null;
    if (el?.isConnected) el.focus();
  }
}

export const editor = new Editor();
