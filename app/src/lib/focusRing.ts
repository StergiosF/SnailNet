// Focus rings for keyboard users only. Chromium shows a ring on a button
// that gets focus back from a dialog (the speed dialog, Add, Confirm) when
// the dialog's last focus was a text box, even after a mouse click on
// Save. So: remember whether the last input was the mouse or the keys,
// and when focus lands on anything but a text field after the mouse, mark
// it quiet (app.css hides the ring) until it loses focus.

const QUIET = "quietFocus";

/** Whether `el` is something you type into (its ring stays: it shows the caret's home). */
function typable(el: Element): boolean {
  return (
    el instanceof HTMLTextAreaElement ||
    (el instanceof HTMLElement && el.isContentEditable) ||
    (el instanceof HTMLInputElement && !["button", "checkbox", "radio", "range", "submit", "reset"].includes(el.type))
  );
}

export function installFocusRing(win: Window = window): void {
  let pointer = false;
  win.addEventListener("pointerdown", () => (pointer = true), true);
  win.addEventListener("keydown", () => (pointer = false), true);
  win.addEventListener(
    "focusin",
    (e) => {
      const el = e.target;
      if (!pointer || !(el instanceof HTMLElement) || typable(el)) return;
      el.dataset[QUIET] = "";
      el.addEventListener("blur", () => delete el.dataset[QUIET], { once: true });
    },
    true,
  );
}
