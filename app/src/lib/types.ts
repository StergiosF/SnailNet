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
  profiles: Profile[];
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
}

export interface ClassTick {
  name: string;
  bps: number;
  drops: number;
  queued_bytes: number;
}

export interface FamilyTick {
  family: string;
  name: string;
  bps: number;
  unidentified_bps: number;
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
}

/** Why a change failed (app/src-tauri/src/changes.rs). */
export type SaveError = { kind: "conflict" } | { kind: "failed"; message: string };

export interface Latest {
  link: Link;
  snapshot: Snapshot | null;
  stats: StatsTick | null;
}
