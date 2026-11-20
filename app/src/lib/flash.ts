// Change feedback (DESIGN.md, Motion): the user asked that "when I change
// something, I give a boost or I change priority etc., it should be shown
// the change with something subtle visually" (2026-11-20). One pattern for
// every change, where it happened: the thing that changed warms with the
// cream of "what's on" and fades back in about a second. Colour only, no
// movement, so reduced motion keeps it as it is.
//
// Use it as an action with what the user can change on that element:
//   <li use:flash={`${share}|${priority}|${enabled}`}>
// Live numbers (speeds) must stay out of the key, or it would never rest.

/** The warm start: ivory over the row, at about a tenth. */
export const FLASH_COLOR = "rgb(240 236 225 / 0.13)";
/** Long enough to notice, short enough to be gone before the next look. */
export const FLASH_MS = 1100;

type Animatable = Pick<HTMLElement, "animate">;

/** Svelte action: warm `node` each time `key` changes (never on mount). */
export function flash(node: Animatable, key: unknown) {
  let last = key;
  return {
    update(next: unknown) {
      if (Object.is(next, last)) return;
      last = next;
      // One keyframe: from the warm colour back to whatever the row's own
      // background is (hover, boosted), without a jump at the end.
      node.animate([{ backgroundColor: FLASH_COLOR }], {
        duration: FLASH_MS,
        easing: "cubic-bezier(0.16, 1, 0.3, 1)",
      });
    },
  };
}
