// Motion settings for Svelte's own transitions and animations (CSS ones
// follow `prefers-reduced-motion` in app.css). With reduced motion,
// nothing travels: durations drop to zero.

import { cubicOut } from "svelte/easing";

const reduced = (): boolean =>
  typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

/** `ms`, or 0 when the person asked for less motion. */
export function ms(duration: number): number {
  return reduced() ? 0 : duration;
}

/** Rows gliding to their new place when the order changes. */
export const glide = (): { duration: number; easing: (t: number) => number } => ({
  duration: ms(320),
  easing: cubicOut,
});
