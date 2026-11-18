import { describe, expect, it } from "vitest";
import { groupings, members, splitExe, withMerge, withSplit, withoutMerge, withoutSplit } from "./families";
import type { AppInfo } from "./types";

const app = (family: string, name: string, exe: string): AppInfo => ({
  family,
  name,
  exe,
});

describe("families", () => {
  it("lists an app's programs once each", () => {
    const apps = [
      app("steam", "Steam", "steamwebhelper.exe"),
      app("Steam", "Steam", "steam.exe"),
      app("steam", "Steam", "steam.exe"),
      app("brave", "Brave", "brave.exe"),
      app("system", "System", ""),
    ];
    expect(members(apps, "steam")).toEqual(["steam.exe", "steamwebhelper.exe"]);
    expect(members(apps, "system")).toEqual([]);
  });

  it("splits a program out and puts it back with its merge", () => {
    const f = withSplit(undefined, "Helper.exe");
    expect(f).toEqual({ split: ["helper.exe"], merge: [] });
    expect(splitExe("split:helper.exe")).toBe("helper.exe");
    expect(splitExe("steam")).toBeNull();
    const moved = withMerge(f, { id: "split:helper.exe", name: "Teams (helper.exe)" }, { id: "zoom", name: "Zoom" });
    expect(withoutSplit(moved, "helper.exe")).toEqual({ split: [], merge: [] });
  });

  it("merges without chains", () => {
    let f = withMerge(undefined, { id: "a", name: "A" }, { id: "b", name: "B" });
    // b goes into c: a follows it.
    f = withMerge(f, { id: "b", name: "B" }, { id: "C", name: "Cee" });
    expect(f.merge).toEqual([
      { from: "a", from_name: "A", into: "c", name: "Cee" },
      { from: "b", from_name: "B", into: "c", name: "Cee" },
    ]);
    // Merging a again replaces its merge.
    f = withMerge(f, { id: "a", name: "A" }, { id: "d", name: "D" });
    expect(f.merge?.filter((m) => m.from === "a")).toEqual([{ from: "a", from_name: "A", into: "d", name: "D" }]);
    expect(withoutMerge(f, "A").merge?.map((m) => m.from)).toEqual(["b"]);
  });

  it("says what is grouped, by name", () => {
    const f = withMerge(
      withSplit(undefined, "x.exe"),
      { id: "sync", name: "Sync" },
      { id: "dropbox", name: "Dropbox" },
    );
    expect(groupings(f, [app("split:x.exe", "Steam (x.exe)", "x.exe")])).toEqual([
      { kind: "split", key: "s x.exe", exe: "x.exe", name: "Steam (x.exe)" },
      {
        kind: "merge",
        key: "m sync",
        from: "sync",
        name: "Sync",
        into: "Dropbox",
      },
    ]);
    expect(groupings(undefined, [])).toEqual([]);
  });
});
