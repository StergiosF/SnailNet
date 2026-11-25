// A look at the UI without the service: `npm run dev`, then open
// /mock.html (?view=flyout for the tray panel). Tauri's own mocks answer
// the app's commands from a made-up snapshot and send live stats four
// times a second: the motivating moment, Steam's download filling a
// 25 Mbps line while a video in Brave is left waiting. Changes (Save, the
// fix, Undo) land in the made-up snapshot, so the pages react as they
// would. Dev only: nothing here is in the app's build.

import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { FamilyTick, Latest, Profile, Rule, Snapshot, StatsTick } from "./lib/types";

const M = 1_000_000;
const params = new URLSearchParams(location.search);
mockWindows(params.get("view") === "flyout" ? "flyout" : "main");

const fam = (id: string) => ({ kind: "family" as const, id });
const rule = (id: string, share: number, extra: Partial<Rule> = {}): Rule => ({
  target: fam(id),
  share,
  priority: "normal",
  enabled: true,
  ...extra,
});
const site = (id: string, share: number): Rule => ({ target: { kind: "site", id }, share, priority: "normal", enabled: true });

const profiles: Profile[] = [
  { id: "balanced", name: "Balanced", rules: [] },
  { id: "calls", name: "Calls", rules: [] },
  { id: "focus", name: "Focus", rules: [] },
  {
    id: "streaming",
    name: "Streaming",
    rules: [site("youtube", 4000), rule("discord", 1000, { priority: "high" }), rule("steam", 2500), rule("epic", 0)],
  },
  { id: "gaming", name: "Gaming", rules: [rule("discord", 1500, { priority: "high" })] },
  { id: "quiet-downloads", name: "Quiet downloads", rules: [rule("steam", 0), rule("epic", 0)] },
];

const snapshot: Snapshot = {
  rev: 7,
  state: "Shaping",
  config: {
    version: 1,
    active_profile: params.get("mode") ?? "balanced",
    settings: { low_latency: true, lan_exempt: true, focus_boost: true, gaming_while_playing: false, calls_first: true },
    line: {
      down_mbps: 25,
      up_mbps: 5,
      measured: { at: Math.floor(Date.now() / 1000) - 86_400, down_mbps: 25.4, up_mbps: 5.2, idle_ms: 18, loaded_ms: 212, with_ms: 31 },
    },
    profiles,
  },
  apps: [
    { family: "steam", name: "Steam", exe: "steam.exe" },
    { family: "brave", name: "Brave", exe: "brave.exe" },
    { family: "discord", name: "Discord", exe: "discord.exe" },
    { family: "windows-services", name: "Windows services", exe: "svchost.exe" },
    { family: "claude", name: "Claude Code", exe: "claude.exe" },
    { family: "epic", name: "Epic Games", exe: "EpicGamesLauncher.exe" },
  ],
  sites: [
    { id: "youtube", name: "YouTube" },
    { id: "twitch", name: "Twitch" },
  ],
};

function bump(): number {
  snapshot.rev += 1;
  void emit("snapshot", structuredClone(snapshot));
  return snapshot.rev;
}

mockIPC(
  (cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    switch (cmd) {
      case "current": {
        const latest: Latest = { link: { kind: "connected", service_version: "0.0.7" }, snapshot, stats: tick(0) };
        return structuredClone(latest);
      }
      case "set_rules": {
        const p = profiles.find((x) => x.id === a.profileId);
        if (p) p.rules = a.rules as Rule[];
        return bump();
      }
      case "switch_profile":
        snapshot.config.active_profile = a.id as string;
        return bump();
      case "set_settings":
        snapshot.config.settings = a.settings as Snapshot["config"]["settings"];
        return bump();
      case "check_update":
        return { kind: "up_to_date" };
      case "hotkey_info":
        return { keys: "Ctrl+Alt+S", registered: true };
      case "autostart_enabled":
      case "autostart_ask":
      case "network_metered":
        return false;
      case "autostart_answer":
        return a.on === true;
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

/** The active mode's rules decide which class a family's traffic is in. */
function tick(t: number): StatsTick {
  const rules = profiles.find((p) => p.id === snapshot.config.active_profile)?.rules ?? [];
  const has = (id: string) => rules.some((r) => r.enabled && r.target.id === id && r.share > 0);
  const wobble = (x: number) => x * (0.92 + 0.16 * Math.abs(Math.sin(t / 900 + x)));
  // With no slice for Brave, Steam's 16 connections take ~16x its one.
  const braveBps = has("brave") ? 7.6 * M : 1.4 * M;
  const steamBps = 22 * M - braveBps;
  const families: FamilyTick[] = [
    { family: "steam", name: "Steam", bps: wobble(steamBps), unidentified_bps: 0, queued_bytes: 60_000, acted: 4 },
    {
      family: "brave",
      name: "Brave",
      bps: wobble(braveBps),
      unidentified_bps: 0,
      queued_bytes: has("brave") ? 0 : 1_400,
      acted: has("brave") ? 0 : 1,
    },
    { family: "windows-services", name: "Windows services", bps: wobble(0.4 * M), unidentified_bps: 0 },
    { family: "claude", name: "Claude Code", bps: 0.03 * M, unidentified_bps: 0 },
  ];
  const classOf = (id: string) => (has(id) ? `family:${id}` : "everything else");
  const classes = new Map<string, { bps: number; queued: number }>();
  for (const f of families) {
    const c = classes.get(classOf(f.family)) ?? { bps: 0, queued: 0 };
    c.bps += f.bps;
    c.queued += f.queued_bytes ?? 0;
    classes.set(classOf(f.family), c);
  }
  return {
    t_ms: t,
    state: "Shaping",
    down: {
      shaped_bps: 23 * M,
      classes: [...classes].map(([name, c]) => ({ name, bps: c.bps, drops: 0, queued_bytes: c.queued })),
      families,
      sites: [],
    },
    up: { shaped_bps: 4.6 * M, classes: [{ name: "everything else", bps: 0.6 * M, drops: 0, queued_bytes: 0 }], families: [], sites: [] },
    names: null,
    ping: { now_ms: 29, low_ms: 17 },
  };
}

let t = 0;
setInterval(() => {
  t += 250;
  void emit("stats", tick(t));
}, 250);

await import("./main");

// ?page=split|settings opens that page, for screenshots.
const page = params.get("page");
if (page === "split" || page === "settings") {
  const { ui } = await import("./lib/ui.svelte");
  ui.go(page);
}
