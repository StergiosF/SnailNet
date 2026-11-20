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

/** A bar growing out from its start, once, as it appears (the proof's
 *  ping bars). Reduced motion: it's simply there. */
export function grow(_node: Element, { delay = 0 }: { delay?: number } = {}) {
  return {
    delay: ms(delay),
    duration: ms(560),
    easing: cubicOut,
    css: (t: number) => `transform: scaleX(${t})`,
  };
}
