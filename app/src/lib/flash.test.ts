import { describe, expect, it, vi } from "vitest";
import { FLASH_COLOR, flash } from "./flash";

function node() {
  const animate = vi.fn();
  return { el: { animate } as unknown as Pick<HTMLElement, "animate">, animate };
}

describe("flash", () => {
  it("warms only when what it watches changes, never on mount", () => {
    const { el, animate } = node();
    const a = flash(el, "20|normal");
    expect(animate).not.toHaveBeenCalled();
    a.update("20|normal");
    expect(animate).not.toHaveBeenCalled();
    a.update("25|normal");
    expect(animate).toHaveBeenCalledTimes(1);
    expect(animate.mock.calls[0]?.[0]).toEqual([{ backgroundColor: FLASH_COLOR }]);
    a.update("25|high");
    a.update("25|high");
    expect(animate).toHaveBeenCalledTimes(2);
  });

  it("treats a boost starting and stopping as changes", () => {
    const { el, animate } = node();
    const a = flash(el, false);
    a.update(true);
    a.update(false);
    expect(animate).toHaveBeenCalledTimes(2);
  });
});
