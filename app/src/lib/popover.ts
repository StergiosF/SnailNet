// Place a native popover menu next to the button that opened it, kept
// inside the window: under the button with right edges lined up (or above
// it when there's more room there), then focus its first item. A menu
// taller than the room on its side gets that room as its height and
// scrolls, so it never runs past the window's edge.

export type Side = "below" | "above";

/** Gap between the button and the menu, and between the menu and the window's edge. */
const GAP = 6;
const EDGE = 8;

export interface Spot {
  left: number;
  top: number;
  /** The most the menu may be tall here (it scrolls beyond). */
  maxHeight: number;
  side: Side;
}

/** Where a `w` × `h` menu goes for a button at `r` in a `vw` × `vh` window. */
export function menuSpot(
  r: { left: number; right: number; top: number; bottom: number },
  w: number,
  h: number,
  vw: number,
  vh: number,
  prefer: Side = "below",
  align: "start" | "end" = "end",
): Spot {
  const x = align === "end" ? r.right - w : r.left;
  const left = Math.max(EDGE, Math.min(x, vw - w - EDGE));
  const room = { below: vh - r.bottom - GAP - EDGE, above: r.top - GAP - EDGE };
  const other: Side = prefer === "below" ? "above" : "below";
  // The preferred side if it fits, else the other if that fits, else
  // whichever has more room.
  const side: Side =
    h <= room[prefer] ? prefer : h <= room[other] ? other : room[prefer] >= room[other] ? prefer : other;
  const maxHeight = Math.max(0, Math.floor(room[side]));
  const shown = Math.min(h, maxHeight);
  const top = side === "below" ? r.bottom + GAP : r.top - GAP - shown;
  return { left, top: Math.max(EDGE, top), maxHeight, side };
}

export function placeMenu(button: HTMLElement, menu: HTMLElement, prefer: Side = "below", align: "start" | "end" = "end") {
  // Measure at its natural height, not the last placement's.
  menu.style.maxHeight = "";
  const s = menuSpot(
    button.getBoundingClientRect(),
    menu.offsetWidth,
    menu.offsetHeight,
    window.innerWidth,
    window.innerHeight,
    prefer,
    align,
  );
  menu.style.left = `${s.left}px`;
  menu.style.top = `${s.top}px`;
  menu.style.maxHeight = `${s.maxHeight}px`;
  menu.dataset.side = s.side;
  menu.scrollTop = 0;
  menu.querySelector<HTMLButtonElement>("button")?.focus({ preventScroll: true });
}
