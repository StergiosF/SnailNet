import { describe, expect, it } from "vitest";
import { type Accent, SOLID_CLASS, accentVars, applySystem } from "./system";

const blue: Accent = {
  accent: "#0078D4",
  dark1: "#005fb8",
  dark2: "#004298",
  dark3: "#003d92",
  light1: "#0093f9",
  light2: "#60ccff",
  light3: "#98ecff",
};

function fakeRoot() {
  const props = new Map<string, string>();
  const classes = new Set<string>();
  const root = {
    style: {
      setProperty: (k: string, v: string) => props.set(k, v),
      removeProperty: (k: string) => props.delete(k),
    },
    classList: {
      toggle: (c: string, on: boolean) => (on ? classes.add(c) : classes.delete(c), on),
    },
  };
  return { root: root as unknown as HTMLElement, props, classes };
}

describe("accentVars", () => {
  it("maps every shade to its --sys-accent property, lowercased", () => {
    expect(accentVars(blue)).toEqual({
      "--sys-accent": "#0078d4",
      "--sys-accent-dark1": "#005fb8",
      "--sys-accent-dark2": "#004298",
      "--sys-accent-dark3": "#003d92",
      "--sys-accent-light1": "#0093f9",
      "--sys-accent-light2": "#60ccff",
      "--sys-accent-light3": "#98ecff",
    });
  });
  it("gives nothing for a missing or malformed accent", () => {
    expect(accentVars(null)).toEqual({});
    expect(accentVars({ ...blue, dark1: "red; background: url(x)" })).toEqual({});
  });
});

describe("applySystem", () => {
  it("sets the accent and the solid class on Windows 10", () => {
    const { root, props, classes } = fakeRoot();
    applySystem({ accent: blue, backdrop: "solid" }, root);
    expect(props.get("--sys-accent-light2")).toBe("#60ccff");
    expect(classes.has(SOLID_CLASS)).toBe(true);
  });
  it("falls back to the CSS defaults when the accent goes away", () => {
    const { root, props, classes } = fakeRoot();
    applySystem({ accent: blue, backdrop: "mica" }, root);
    applySystem({ accent: null, backdrop: "mica" }, root);
    expect(props.size).toBe(0);
    expect(classes.has(SOLID_CLASS)).toBe(false);
  });
});
