// Place a native popover menu next to the button that opened it, kept
// inside the window: under the button with right edges lined up (or above
// it when there's no room below), then focus its first item.

export type Side = "below" | "above";

export function placeMenu(button: HTMLElement, menu: HTMLElement, prefer: Side = "below", align: "start" | "end" = "end") {
  const r = button.getBoundingClientRect();
  const w = menu.offsetWidth;
  const h = menu.offsetHeight;
  const x = align === "end" ? r.right - w : r.left;
  const left = Math.max(8, Math.min(x, window.innerWidth - w - 8));
  const below = r.bottom + 6;
  const above = r.top - h - 6;
  const fitsBelow = below + h <= window.innerHeight - 8;
  const fitsAbove = above >= 8;
  const top = prefer === "below" ? (fitsBelow || !fitsAbove ? below : above) : fitsAbove || !fitsBelow ? above : below;
  menu.style.left = `${left}px`;
  menu.style.top = `${Math.max(8, top)}px`;
  menu.querySelector<HTMLButtonElement>("button")?.focus();
}
