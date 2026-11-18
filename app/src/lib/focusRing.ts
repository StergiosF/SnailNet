// When focus rings show. Two cases are quiet (app.css hides the ring until
// the element loses focus):
// - focus that lands after a mouse click, on anything but a text field;
// - focus a closing dialog hands back to the button that opened it (the
//   speed dialog, Add, Confirm), however the dialog was closed: Enter,
//   Escape or a click. The user asked for no ring there (2026-11-18); the
//   next Tab shows rings as usual.

const QUIET = "quietFocus";

/** Whether `el` is something you type into (its ring stays: it shows the caret's home). */
function typable(el: Element): boolean {
  return (
    el instanceof HTMLTextAreaElement ||
    (el instanceof HTMLElement && el.isContentEditable) ||
    (el instanceof HTMLInputElement && !["button", "checkbox", "radio", "range", "submit", "reset"].includes(el.type))
  );
}

/** Mark `el` quiet until it loses focus. */
export function quietFocus(el: HTMLElement): void {
  if (QUIET in el.dataset) return;
  el.dataset[QUIET] = "";
  el.addEventListener("blur", () => delete el.dataset[QUIET], { once: true });
}

export function installFocusRing(win: Window = window): void {
  let pointer = false;
  win.addEventListener("pointerdown", () => (pointer = true), true);
  win.addEventListener("keydown", () => (pointer = false), true);
  win.addEventListener(
    "focusin",
    (e) => {
      const el = e.target;
      if (!(el instanceof HTMLElement)) return;
      const from = (e as FocusEvent).relatedTarget;
      const fromDialog = from instanceof Element && from.closest("dialog") !== null && el.closest("dialog") === null;
      if (fromDialog || (pointer && !typable(el))) quietFocus(el);
    },
    true,
  );
}
