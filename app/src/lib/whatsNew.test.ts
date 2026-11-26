import { describe, expect, it } from "vitest";
import {
  checkJustUpdated,
  clearPendingUpdate,
  isNewerVersion,
  markUpdateSeen,
  notesForVersion,
  parseReleaseNotes,
  rememberPendingUpdate,
} from "./whatsNew";

function memStorage(): Storage {
  const map = new Map<string, string>();
  return {
    get length() {
      return map.size;
    },
    clear: () => map.clear(),
    getItem: (k) => map.get(k) ?? null,
    key: (i) => [...map.keys()][i] ?? null,
    removeItem: (k) => {
      map.delete(k);
    },
    setItem: (k, v) => {
      map.set(k, v);
    },
  };
}

describe("isNewerVersion", () => {
  it("compares dotted semantic versions cleanly", () => {
    expect(isNewerVersion("0.0.11", "0.0.10")).toBe(true);
    expect(isNewerVersion("0.1.0", "0.0.11")).toBe(true);
    expect(isNewerVersion("v1.0.0", "0.9.9")).toBe(true);
    expect(isNewerVersion("0.0.11", "0.0.11")).toBe(false);
    expect(isNewerVersion("0.0.10", "0.0.11")).toBe(false);
  });
});

describe("parseReleaseNotes", () => {
  it("parses markdown bullets with titles, descriptions, and inferred icons", () => {
    const raw = `
# Release 0.0.12
- **Smoother gaming**: Running games stay at the front of the queue.
- Faster CPU usage: Uses less memory while idle in the tray.
- [phone] Better voice calls — Calls stay crisp during downloads.
`;
    const items = parseReleaseNotes(raw);
    expect(items).toHaveLength(3);
    expect(items[0]).toMatchObject({ icon: "gaming", title: "Smoother gaming", body: "Running games stay at the front of the queue." });
    expect(items[1]).toMatchObject({ icon: "boost", title: "Faster CPU usage", body: "Uses less memory while idle in the tray." });
    expect(items[2]).toMatchObject({ icon: "phone", title: "Better voice calls", body: "Calls stay crisp during downloads." });
  });

  it("does not awkwardly split inline bold words without a separator", () => {
    const items = parseReleaseNotes("- **CPU** and **RAM** usage reduced while idle.");
    expect(items).toHaveLength(1);
    expect(items[0]?.title).toBe("CPU and RAM usage reduced while idle.");
    expect(items[0]?.body).toBeUndefined();
  });

  it("falls back gracefully when notes are empty", () => {
    const data = notesForVersion("0.0.99", "");
    expect(data.version).toBe("0.0.99");
    expect(data.items.length).toBe(1);
    expect(data.items[0]?.icon).toBe("check");
  });
});

describe("post-update modal lifecycle", () => {
  it("stays silent on a fresh install and records the version", () => {
    const s = memStorage();
    expect(checkJustUpdated("0.0.12", s)).toBeNull();
    expect(checkJustUpdated("0.0.12", s)).toBeNull();
  });

  it("greets existing pre-0.0.12 users upgrading to 0.0.12 once", () => {
    const s = memStorage();
    s.setItem("snailnet.firstRun.done", "1");
    const data = checkJustUpdated("0.0.12", s);
    expect(data).not.toBeNull();
    expect(data?.version).toBe("0.0.12");
    expect(data?.items.length).toBeGreaterThanOrEqual(3);
    markUpdateSeen("0.0.12", s);
    expect(checkJustUpdated("0.0.12", s)).toBeNull();
  });

  it("shows once after an in-app update and remembers notes when reopened from Settings", () => {
    const s = memStorage();
    expect(checkJustUpdated("0.0.12", s)).toBeNull();
    rememberPendingUpdate("0.0.13", "- Faster shaping: Lower CPU overhead on busy lines.", s);

    // Relaunched on 0.0.13
    const data = checkJustUpdated("0.0.13", s);
    expect(data).not.toBeNull();
    expect(data?.version).toBe("0.0.13");
    expect(data?.items[0]?.title).toBe("Faster shaping");
    expect(data?.items[0]?.icon).toBe("boost");

    // User clicks "Got it"
    markUpdateSeen("0.0.13", s);
    expect(checkJustUpdated("0.0.13", s)).toBeNull();

    // Reopening from Settings → About still remembers 0.0.13's updater notes.
    const reopened = notesForVersion("0.0.13", undefined, s);
    expect(reopened.version).toBe("0.0.13");
    expect(reopened.items[0]?.title).toBe("Faster shaping");
    expect(reopened.items[0]?.body).toBe("Lower CPU overhead on busy lines.");
  });

  it("does not trigger if installing the update failed", () => {
    const s = memStorage();
    checkJustUpdated("0.0.12", s);
    rememberPendingUpdate("0.0.13", "- Something new", s);
    clearPendingUpdate(s);
    expect(checkJustUpdated("0.0.12", s)).toBeNull();
  });

  it("shows curated notes once after upgrading via installer directly", () => {
    const s = memStorage();
    checkJustUpdated("0.0.11", s);
    const data = checkJustUpdated("0.0.12", s);
    expect(data).not.toBeNull();
    expect(data?.version).toBe("0.0.12");
    expect(data?.items.length).toBeGreaterThanOrEqual(3);
    markUpdateSeen("0.0.12", s);
    expect(checkJustUpdated("0.0.12", s)).toBeNull();
  });
});
