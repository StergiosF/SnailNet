import { describe, expect, it } from "vitest";
import { menuSpot } from "./popover";

// A Boost button near the right edge of a 900 × 600 window.
const btn = (top: number) => ({ left: 780, right: 840, top, bottom: top + 32 });

describe("menuSpot", () => {
  it("opens below, right edges lined up, when it fits", () => {
    expect(menuSpot(btn(100), 260, 300, 900, 600)).toEqual({ left: 580, top: 138, maxHeight: 454, side: "below" });
  });

  it("opens above when only above has room", () => {
    const s = menuSpot(btn(450), 260, 300, 900, 600);
    expect(s.side).toBe("above");
    expect(s.top).toBe(450 - 6 - 300);
  });

  it("never runs past the window: too tall for both sides, it takes the bigger one and scrolls", () => {
    const s = menuSpot(btn(380), 260, 700, 900, 600);
    expect(s).toEqual({ left: 580, top: 8, maxHeight: 366, side: "above" });
    const low = menuSpot(btn(150), 260, 700, 900, 600);
    expect(low.side).toBe("below");
    expect(low.top + low.maxHeight).toBeLessThanOrEqual(600 - 8);
  });

  it("stays inside the left and right edges", () => {
    expect(menuSpot({ left: 10, right: 40, top: 100, bottom: 132 }, 260, 100, 900, 600).left).toBe(8);
    expect(menuSpot(btn(100), 260, 100, 900, 600, "below", "start").left).toBe(900 - 260 - 8);
  });
});
