// The service's IPC shapes as the Rust side serialises them (serde JSON of
// snailnet-ipc / snailnet-core types). Keep in step with
// crates/snailnet-ipc/src/lib.rs and app/src-tauri/src/bridge.rs.

export type ServiceState =
  | "Starting"
  | "Shaping"
  | "Unshaped"
  | { Retrying: { reason: string } }
  | { SafeMode: { failures: number } }
  /** Paused by the user until `until` (Unix seconds), or until resumed. */
  | { Paused: { until: number | null } }
  /** Running a speed test: shaping is off for its ~20 s. */
  | "Measuring";

export type Link =
  | { kind: "connecting" }
  | { kind: "connected"; service_version: string }
  | { kind: "not_running" }
  | { kind: "denied"; detail: string }
  | { kind: "rejected" }
  | { kind: "lost"; detail: string };

export type Priority = "high" | "normal" | "low";

export interface Target {
  kind: "family" | "exe" | "site";
  id: string;
}

export interface Rule {
  target: Target;
  /** Basis points: 4500 = 45%. 0 = only when the line is free. */
  share: number;
  priority: Priority;
  /** Hard caps per direction, as snailnet-core writes them: "20%" or
   *  "5 Mbps". Absent = no cap. */
  ceiling?: { down?: string; up?: string };
  enabled: boolean;
}

/** A ceiling, parsed (rules.ts parseLimit / formatLimit). */
export type Limit = { unit: "percent"; value: number } | { unit: "mbps"; value: number };

export interface SiteInfo {
  id: string;
  name: string;
}

export interface Profile {
  id: string;
  name: string;
  rules: Rule[];
}

export interface Line {
  down_mbps?: number;
  up_mbps?: number;
  factor?: number;
  /** Keep the typed speeds; the estimator doesn't adapt them. */
  pinned?: boolean;
  /** The last speed test. */
  measured?: Measured;
}

/** A network as the service tells them apart: `id` is the router's
 *  hardware address; `name` the Wi-Fi name or the adapter's. */
export interface Network {
  id: string;
  name: string;
}

/** Another network's line, kept while away from it. */
export interface SavedNetwork extends Network {
  /** When it was left, Unix seconds. */
  left: number;
  line: Line;
}

/** What a speed test found (snailnet-core `Measured`). */
export interface Measured {
  /** Unix seconds. */
  at: number;
  down_mbps: number;
  up_mbps: number;
  /** Ping before the test, ms. */
  idle_ms?: number;
  /** Ping while downloading, without SnailNet, ms. */
  loaded_ms?: number;
}

/** One app or site going first for a while (snailnet-core `Boost`). */
export interface Boost {
  target: Target;
  /** Unix seconds; absent = until stopped. */
  until?: number;
}

/** General settings (snailnet-core `Settings`). */
export interface Settings {
  low_latency: boolean;
  lan_exempt: boolean;
  /** The app in front, and games while they run, go first. */
  focus_boost: boolean;
  /** Use the Gaming mode while a game runs. */
  gaming_while_playing: boolean;
  /** Families the user says are games (missing when none). */
  games?: string[];
  /** Families the user says never go first (missing when none). */
  never_first?: string[];
  /** The first time an app goes online: a notification, or nothing
   *  (missing from older services: notify). */
  new_apps?: "notify" | "strict" | "off";
  /** Fetch newer signed site, app and game lists once a day (missing
   *  from older services: on). */
  pack_updates?: boolean;
}

export interface Config {
  version: number;
  active_profile: string;
  settings: Settings;
  line: Line;
  /** The network `line` belongs to (PLAN.md §10.3). */
  network?: Network;
  /** The other networks seen, with their own speeds. */
  networks?: SavedNetwork[];
  boost?: Boost;
  /** Paused until `until` (Unix seconds), or until resumed. */
  pause?: { until?: number };
  /** Apps (family ids) with no internet, in every mode. */
  blocked?: string[];
  /** How the user groups apps (§11.6; missing: as SnailNet found them). */
  families?: Families;
  profiles: Profile[];
}

/** A family counted as part of another (§11.6). */
export interface Merge {
  from: string;
  from_name?: string;
  into: string;
  /** `into`'s name. */
  name: string;
}

export interface Families {
  /** Programs (exe names) that are apps of their own. */
  split?: string[];
  merge?: Merge[];
}

export interface AppInfo {
  family: string;
  name: string;
  exe: string;
}

export interface Snapshot {
  rev: number;
  state: ServiceState;
  config: Config;
  apps: AppInfo[];
  /** Every site a rule may name (missing from older services). */
  sites?: SiteInfo[];
  /** New apps not answered yet (§11.4; missing from older services). */
  prompts?: NewApp[];
}

/** An app went online for the first time (§11.4). */
export interface NewApp {
  prompt_id: number;
  family: string;
  name: string;
  exe: string;
  /** Strict mode: it waits for the answer (missing: it's online). */
  held?: boolean;
}

export interface ClassTick {
  name: string;
  bps: number;
  drops: number;
  queued_bytes: number;
  /** UI only (steady.ts): quiet just now, still listed for a few seconds. */
  held?: boolean;
}

export interface FamilyTick {
  family: string;
  name: string;
  bps: number;
  unidentified_bps: number;
  /** UI only (steady.ts): quiet just now, still listed for a few seconds. */
  held?: boolean;
}

export interface SiteTick {
  site: string;
  name: string;
  family: string;
  bps: number;
}

export interface DirStats {
  shaped_bps: number;
  classes: ClassTick[];
  families: FamilyTick[];
  sites: SiteTick[];
}

export interface NameTick {
  sni: number;
  dns: number;
  ech: number;
  none: number;
  naming: number;
}

export interface StatsTick {
  t_ms: number;
  state: ServiceState;
  down: DirStats | null;
  up: DirStats | null;
  names: NameTick | null;
  /** A game runs and "Gaming mode while you play" is on: the service plans
   *  with Gaming; the picked mode stays saved (§11.5). Missing from older
   *  services. */
  playing?: boolean;
  /** The games running now and why each counts as one (§11.5). Missing
   *  when there are none, or from older services. */
  games?: GameTick[];
}

/** Why SnailNet counts a program as a game (snailnet-ipc `GameWhy`). */
export type GameWhy = "fullscreen" | "library" | "marked";

export interface GameTick {
  family: string;
  why: GameWhy;
}

/** Why a change failed (app/src-tauri/src/changes.rs). */
export type SaveError = { kind: "conflict" } | { kind: "failed"; message: string };

export interface Latest {
  link: Link;
  snapshot: Snapshot | null;
  stats: StatsTick | null;
}
