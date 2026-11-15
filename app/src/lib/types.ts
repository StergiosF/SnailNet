// The service's IPC shapes as the Rust side serialises them (serde JSON of
// snailnet-ipc / snailnet-core types). Keep in step with
// crates/snailnet-ipc/src/lib.rs and app/src-tauri/src/bridge.rs.

export type ServiceState =
  | "Starting"
  | "Shaping"
  | "Unshaped"
  | { Retrying: { reason: string } }
  | { SafeMode: { failures: number } };

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
  /** Basis points: 4500 = 45%. */
  share: number;
  priority: Priority;
  enabled: boolean;
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
}

export interface Config {
  version: number;
  active_profile: string;
  settings: { low_latency: boolean; lan_exempt: boolean };
  line: Line;
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

export interface Latest {
  link: Link;
  snapshot: Snapshot | null;
  stats: StatsTick | null;
}
