// Whether the connection-speed dialog is open, and which box it focuses.
// Opened from the header's line button and the "Watching only" notice.

export type Direction = "down" | "up";

class LineDialogState {
  open = $state(false);
  focus = $state<Direction>("down");
  /** What had focus when the dialog opened; it gets focus back on close. */
  private opener: HTMLElement | null = null;

  show(focus: Direction = "down") {
    if (!this.open) {
      const el = document.activeElement;
      this.opener = el instanceof HTMLElement ? el : null;
    }
    this.focus = focus;
    this.open = true;
  }

  close() {
    this.open = false;
  }

  /** Called by the dialog once it has closed (a modal dialog keeps focus
   *  inside it while open, so this can't happen in `close`). */
  returnFocus() {
    const el = this.opener;
    this.opener = null;
    if (el?.isConnected) el.focus();
  }
}

export const lineDialog = new LineDialogState();
