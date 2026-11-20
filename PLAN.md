# SnailNet — Product & Engineering Plan

> Put the bandwidth hogs in the slow lane.
> A Windows app that splits your internet between apps and websites by **percentage**, with **priorities** for low latency, wrapped in a calm Windows 11 interface.

- **Status:** Planning (greenfield)
- **Plan date:** 2026-11-14
- **Target:** Windows 10 / 11, x64
- **Stack:** Rust (engine + service) · Tauri 2 + Svelte 5 + TypeScript (UI) · WinDivert 2.2 (packet interception)

---

## Table of contents

1. [The problem and the promise](#1-the-problem-and-the-promise)
2. [Decision log](#2-decision-log)
3. [How SnailNet works (the user's mental model)](#3-how-snailnet-works-the-users-mental-model)
4. [System architecture](#4-system-architecture)
5. [Repository layout](#5-repository-layout)
6. [Packet engine (WinDivert)](#6-packet-engine-windivert)
7. [Identifying apps](#7-identifying-apps)
8. [Identifying websites (per-site rules)](#8-identifying-websites-per-site-rules)
9. [The scheduler: percentages + priorities](#9-the-scheduler-percentages--priorities)
10. [Line speed: auto-detect, adapt, low-latency mode](#10-line-speed-auto-detect-adapt-low-latency-mode)
11. [v1 features in detail](#11-v1-features-in-detail)
12. [Service ↔ app communication (IPC)](#12-service--app-communication-ipc)
13. [Configuration and storage](#13-configuration-and-storage)
14. [UI / UX specification](#14-ui--ux-specification)
15. [Frontend implementation](#15-frontend-implementation)
16. [Dependencies](#16-dependencies)
17. [Installer, driver, service, updates](#17-installer-driver-service-updates)
18. [Safety, security, and compatibility](#18-safety-security-and-compatibility)
19. [Testing strategy](#19-testing-strategy)
20. [Performance budget](#20-performance-budget)
21. [Roadmap and milestones](#21-roadmap-and-milestones)
22. [Risks and mitigations](#22-risks-and-mitigations)
23. [Idea backlog](#23-idea-backlog)
24. [Glossary](#24-glossary)

---

## 1. The problem and the promise

**The moment that motivates SnailNet:** a game download takes the entire connection and YouTube won't even buffer. Windows offers no fix. NetLimiter and NetBalancer can fix it, but they speak in priorities and absolute KB/s limits, look like 2008 tools, and make you work out the numbers yourself.

**The promise:** drag a bar so the game gets 40% and YouTube 60%. Both keep working, the download just takes a little longer, and the moment you close YouTube the game gets everything back.

### Product principles

1. **Percentages first, numbers second.** People think in "most of it for this, a bit for that", not in Mbps. Mbps stays visible, but as secondary information.
2. **Never waste bandwidth.** A share is a guarantee under contention, not a cage. Idle shares flow to whoever needs them. Hard ceilings exist, but only when the user asks for one.
3. **Never break the internet.** If anything goes wrong, traffic flows normally (fail-open). One click pauses everything.
4. **Zero-config useful.** Ten seconds after install, with no rules at all, Low-latency mode already improves gaming and calls.
5. **Quiet and premium.** Native Windows 11 look, no nagging, no fake urgency, no dashboards nobody asked for.

### Non-goals (v1)

- Router- or LAN-wide shaping of other devices (SnailNet shapes only *this* PC).
- ARM64 Windows, macOS, Linux.
- Usage history, schedules, data caps (planned for v1.1).
- Deep packet inspection beyond reading the website name (no content inspection, ever).

---

## 2. Decision log

Settled during the planning interview. Change these only deliberately, and note the change here.

| # | Topic | Decision | Why |
|---|---|---|---|
| D1 | Audience | Personal first, built clean enough to publish later | No paid certificates or licensing yet; keep the code and LGPL boundary clean |
| D2 | Meaning of % | **Guaranteed share**, idle bandwidth redistributed (work-conserving), plus an optional **hard ceiling** per rule | Matches the intuition behind "40/60" without wasting bandwidth |
| D3 | Direction | Upload **and** download; one shared split by default, "separate up/down" per rule | Download hogging is the main pain point |
| D4 | Engine | **WinDivert 2.2** behind a `PacketEngine` trait | Signed, proven, per-process info; can be swapped for a custom WFP driver later. **Phase 0: Go (2026-11-15)**, see `docs/phase0-report.md`; anti-cheat still untested |
| D5 | Line speed | Auto speed test, adaptive afterwards, manual override, **Low-latency mode** | % needs a 100%; low latency is the standout feature |
| D6 | Primary use | Gaming ping + taming big downloads (game download 40% / YouTube 60%) | Drives defaults and presets |
| D7 | Per-site rules | **Yes**, YouTube, Twitch, etc. inside browsers; **a site rule applies in every app and beats the app's rule** (2026-11-16, §8.3) | YouTube is a tab, not an app |
| D8 | Priority | **% = how much, priority = who goes first** | Priority lowers latency without changing bandwidth shares |
| D9 | v1 extras | Smart (Focus boost, Low-latency mode) + Control (profiles, block, new-app prompt, app families) | Differentiators plus essentials |
| D10 | Unlisted apps | One **"Everything else"** slice | New apps can never starve your rules |
| D11 | % input | **− % + per slice** (5% steps, always 100%: the other slices make room); the split bar with draggable dividers was built, then replaced (2026-11-16) | Readable at a glance, one click to give something more, the same in the flyout |
| D12 | Main screen | **The Garden** (2026-11-17): a side rail (Now · Split · Settings); **Now** = four mode tiles, who is using the internet with a Boost each, the split in short; **Split** = the slices as rows with their settings in place (the former Ledger); **tray flyout** with modes, Pause and the slices | Most visits are "pick a mode, boost one thing"; the exact split is one step back |
| D13 | Style | **SnailNet's own dark look, from the snail icon** (§14.1): its greens, its shell. Chosen by the user from three real-size mockups ("C with B's side rail", 2026-11-17) after the Ledger read as too plain | Distinctive, calm and premium, and recognisably SnailNet |
| D14 | Platform | Windows 10/11 x64 | WinDivert has no official ARM64 build |
| D15 | UI stack | **Tauri 2 + Svelte 5 + TypeScript** | Premium UI with a small installer; Rust core |
| D16 | Runtime | **Windows service** (LocalSystem) + per-user tray app | No admin prompt on launch, shaping from boot |
| D17 | Name | **SnailNet** | Playful, memorable, snail mascot |
| D18 | Updates | Tauri updater from **GitHub Releases** | Free, signed update manifests |
| D19 | Language | English, all strings in translation files | Adding a language later is just a file |
| D20 | Plan home | This file | Build phase by phase from it |

### Defaults adopted (confirmed in the interview)

- Live per-app speeds in the main list are **core** in v1. Only history and big graphs move to v1.1.
- **Phase 0 is a CLI spike** that proves download shaping before any UI work.
- Websites are identified from **DNS answers + TLS SNI + QUIC Initial SNI**, with a fallback for when the site name is hidden.
- Focus boost detects games by fullscreen/D3D state + known launcher libraries; the user can override it.
- Config is TOML in `C:\ProgramData\SnailNet\`; SQLite arrives in v1.1.
- The installer ships the WinDivert driver and registers the service.
- IPC uses a named pipe restricted to SYSTEM, Administrators and the interactive user.

---

## 3. How SnailNet works (the user's mental model)

The UI teaches exactly this, and nothing more:

```
Your connection (auto-measured)     ▼ 100 Mbps  ▲ 20 Mbps
┌────────────────────────────────────────────────────────────┐
│ Steam  40%          │ YouTube  45%             │ Other 15% │   ← split bar
└────────────────────────────────────────────────────────────┘
```

1. **Slices.** Each rule gets a slice of the line. The slices always add up to 100%. "Everything else" is always one slice.
2. **Sharing.** When an app isn't using its slice, the others use it. Close YouTube and Steam gets 100%.
3. **Ceilings (optional).** "Never more than X", even when the line is free, e.g. Windows Update never above 30%.
4. **Priority.** High / Normal / Low. Higher priority **goes first in line**, so its packets wait less (lower ping). It does *not* take more than its slice over time.
5. **Low-latency mode.** SnailNet keeps the line a little below full so your router never clogs up. Ping stays low even during downloads.

### Worked example (D2 + D8)

Line: 100 Mbps down. Low-latency mode on → SnailNet shapes at ~92 Mbps.

| Situation | Steam (40%, Low) | YouTube (45%, Normal) | Everything else (15%) | Game (no %, High via Focus boost) |
|---|---|---|---|---|
| Only Steam downloading | **92** | 0 | 0 | — |
| Steam + YouTube 4K (needs ~25) | **67** (gets YouTube's unused part) | **25** (all it needs) | 0 | — |
| Steam + 2 heavy YouTube tabs | **36.8** (its 40%) | **41.4** (45%) + leftover from Other | 13.8 | — |
| Steam + YouTube + game online | ~same as above | ~same | game takes its ~0.5 Mbps from *Other's* slice; with **High** its packets jump the queue | ping ≈ unloaded ping |

> Apps without their own rule live in **Everything else**. A High-priority app in that slice still gets queue-jumping, so a game needs no % at all to get low ping.

---

## 4. System architecture

```
┌───────────────────────── user session ─────────────────────────┐
│  SnailNet.exe  (Tauri 2)                                       │
│  ┌──────────────┐  ┌────────────────┐  ┌─────────────────────┐ │
│  │ Svelte UI    │  │ Tray + flyout  │  │ Session helpers     │ │
│  │ (WebView2)   │  │ Global hotkeys │  │ • foreground watch  │ │
│  └──────┬───────┘  └───────┬────────┘  │ • fullscreen/game   │ │
│         │  tauri commands  │           │ • icon extraction   │ │
│  ┌──────▼──────────────────▼────────┐  │ • toast notifications│ │
│  │ snailnet-client (Rust)           │◄─┴─────────────────────┘ │
│  └──────────────┬───────────────────┘                          │
└─────────────────┼──────────────────────────────────────────────┘
                  │ named pipe \\.\pipe\snailnet.v1 (ACL'd)
┌─────────────────▼──────────── session 0 ───────────────────────┐
│  snailnet-service.exe  (Windows service, LocalSystem)          │
│  ┌────────────┐ ┌────────────┐ ┌──────────────┐ ┌────────────┐ │
│  │ IPC server │ │ Rule store │ │ Line-speed   │ │ Stats      │ │
│  │            │ │ (TOML)     │ │ estimator    │ │ aggregator │ │
│  └─────┬──────┘ └─────┬──────┘ └──────┬───────┘ └─────▲──────┘ │
│        └──────────────▼───────────────▼───────────────┤        │
│        ┌────────────────────────────────────────────┐ │        │
│        │ Classifier: flow → app family / site / rule│─┘        │
│        └───────────────┬────────────────────────────┘          │
│        ┌───────────────▼────────────────────────────┐          │
│        │ Scheduler (per direction): HTB-style shares │          │
│        │ + priority + CoDel/ECN queues               │          │
│        └───────────────┬────────────────────────────┘          │
│        ┌───────────────▼────────────────────────────┐          │
│        │ PacketEngine trait  ──► WinDivertEngine     │          │
│        └───────────────┬────────────────────────────┘          │
└────────────────────────┼───────────────────────────────────────┘
                 WinDivert.dll / WinDivert64.sys (kernel, signed)
```

### Why the split matters

- **The service** owns everything that needs admin rights and must keep running: the packet engine, scheduler, rules and the line-speed estimator. It has **no UI** and runs in session 0, where it can't see the desktop.
- **The tray app** owns everything that needs the user's desktop: windows, tray, toasts, hotkeys, **which window is in front** and **fullscreen detection** (needed for Focus boost), and exe icons. It pushes those facts to the service over IPC.
- If the UI isn't running, shaping continues and the service queues pending events (e.g. new-app prompts) until the UI reconnects. The tray app autostarts at login, minimised to the tray.

### Threading model in the service

| Thread | Job | Notes |
|---|---|---|
| `flow-sniffer` | WinDivert **FLOW** layer (sniff) → PID ↔ 5-tuple | Cheap; events only |
| `socket-guard` | WinDivert **SOCKET** layer → blocking, plus early PID mapping on `connect`/`bind` | Used for Block and "ask first" |
| `rx-N` (1–2) | WinDivert **NETWORK** layer `RecvEx` batches → classify → enqueue | Batch 16–64 packets per call |
| `scheduler` | Dequeue packets per direction when tokens allow → `SendEx` | High-resolution waitable timer |
| `tokio runtime` | IPC, config, estimator, stats, speed test | Never touches the packet fast path |

Fast path to slow path communication uses lock-free channels (`crossbeam`) and atomics. The fast path **never** allocates per packet: buffers come from a pre-allocated slab pool.

---

## 5. Repository layout

Cargo workspace plus a Tauri app:

```
SnailNet/
├─ PLAN.md
├─ Cargo.toml                    # [workspace]
├─ crates/
│  ├─ snailnet-core/             # pure logic, no Windows APIs → fully unit-testable
│  │  ├─ scheduler/              # HTB-style classes, DRR, priority, CoDel, token buckets
│  │  ├─ rules/                  # rule model, profiles, validation, % normalisation
│  │  ├─ classify/               # flow table, app family resolution, site matching
│  │  ├─ parse/                  # DNS, TLS ClientHello SNI, QUIC Initial SNI (fuzzed)
│  │  ├─ estimator/              # line-speed + latency estimator
│  │  └─ clock.rs                # Clock trait (real + simulated)
│  ├─ snailnet-engine/           # PacketEngine trait + WinDivertEngine + MockEngine
│  ├─ snailnet-win/              # Windows helpers: process info, svchost services, UWP, ETW-free lookups
│  ├─ snailnet-ipc/              # protocol types (serde), versioning, pipe framing
│  ├─ snailnet-service/          # the Windows service binary (+ `install`/`uninstall`/`run --console`)
│  ├─ snailnet-cli/              # dev/diagnostic CLI: status, rules, dump flows, bench
│  └─ snailnet-sim/              # offline simulator: replay traffic models against the scheduler
├─ app/                          # Tauri 2 app
│  ├─ src-tauri/                 # Rust side: tray, commands, session helpers, ipc client
│  └─ src/                       # Svelte 5 + TS UI
│     ├─ lib/components/         # SplitBar, AppRow, RuleEditor, ProfileSwitcher, Toggle…
│     ├─ lib/stores/             # live stats, rules, profiles (runes-based)
│     ├─ lib/i18n/en.json
│     └─ routes/                 # main, settings, first-run, flyout
├─ assets/sitepacks/             # youtube.toml, twitch.toml, netflix.toml… (domain lists)
├─ assets/gamepacks/             # launcher paths & known game exes
├─ installer/                    # NSIS hooks for driver + service
├─ third_party/windivert/        # WinDivert.dll, WinDivert64.sys, LICENSE (pinned 2.2.x)
└─ docs/                         # ADRs, design notes, test reports
```

**Rule:** `snailnet-core` has zero Windows dependencies, so the scheduler and parsers run and test on any machine, deterministically, with a simulated clock.

---

## 6. Packet engine (WinDivert)

### 6.1 The `PacketEngine` trait

```rust
pub trait PacketEngine: Send + Sync {
    fn recv_batch(&self, out: &mut PacketBatch) -> Result<usize>;   // NETWORK layer
    fn send_batch(&self, pkts: &PacketBatch) -> Result<usize>;
    fn flow_events(&self) -> Receiver<FlowEvent>;                   // FLOW layer (PID)
    fn socket_events(&self) -> Receiver<SocketEvent>;               // SOCKET layer (PID, block)
    fn shutdown(&self);                                             // closes handles → fail-open
}
```

`WinDivertEngine` implements it now. A future `WfpEngine` (own kernel driver) or `MockEngine` (tests) plugs in without touching the scheduler.

### 6.2 Layers and handles

From the WinDivert docs: only the **NETWORK** layer can delay, drop and reinject packets, but it carries **no process ID**. The **FLOW** and **SOCKET** layers carry the PID but can't inject. So SnailNet joins them:

| Handle | Layer | Flags | Filter (sketch) | Purpose |
|---|---|---|---|---|
| H1 | FLOW | `SNIFF \| RECV_ONLY` | `true` | flow established/deleted → PID ↔ 5-tuple |
| H2 | SOCKET | none (can block) | `event == CONNECT or event == BIND` (+ LISTEN/ACCEPT) | PID before the first packet, Block, "ask first" |
| H3 | NETWORK | divert | `!loopback and (tcp or udp) and not <exclusions>` | shaping: queue → reinject later |
| H4 | NETWORK | `SNIFF` | `udp.SrcPort == 53 or tcp.SrcPort == 53` | read DNS answers for site recognition (no delay) |

**Exclusions (never shaped, never delayed):** loopback; LAN/private ranges (option, default on: printers, NAS and casting stay fast); DHCP; ICMP; the SnailNet speed-test flow; optionally VPN outer flows (see §18.4).

**Handle priorities:** H2 > H3 so blocking decisions happen before shaping; H4 sniffs at a priority that doesn't interfere with other WinDivert users.

### 6.3 Packet path

1. `rx` thread `RecvEx` returns up to N packets plus `WINDIVERT_ADDRESS` (direction, interface, IPv6, timestamp).
2. Parse the 5-tuple (`etherparse`-style zero-copy view).
3. Flow table lookup → `FlowEntry { pid, app_family, site, class_id, first_seen, bytes }`.
   - **Miss** (packet beat the FLOW event): consult the SOCKET-layer cache, then `GetExtendedTcpTable` / `GetExtendedUdpTable` (PID by local port), else classify as *pending*. Pending packets go to Everything else for ≤ 50 ms and are re-classified when the event arrives.
4. On the first packets of a flow, run site sniffers (TLS ClientHello / QUIC Initial) → set `site`.
5. Enqueue into `class_id`'s queue for that direction (or **express lane** for tiny control packets, §9.5).
6. `scheduler` thread dequeues when tokens allow and calls `SendEx` with the original address (checksums untouched unless ECN-marked).

### 6.4 Fail-open guarantees

- If the service exits or crashes, its WinDivert handles close and **traffic flows normally again**. This is WinDivert's built-in behaviour and the cornerstone of safety.
- The service watchdog: if the scheduler thread stalls for > 500 ms, the engine closes H3 (pass-through) and logs a diagnostic.
- Crash loop (3 crashes in 10 min): start in **Safe mode** (monitor only, no shaping), and the UI explains what happened.

### 6.5 Timing

- Scheduler wake-ups use `CreateWaitableTimerExW(CREATE_WAITABLE_TIMER_HIGH_RESOLUTION)` (Win10 1803+) with ~0.5–1 ms granularity. No global `timeBeginPeriod`.
  - **Changed 2026-11-18 (measured, §20):** no spin-waits, and naps are at least 1 ms. Spinning bought no accuracy that matters (the root bucket holds 2 ms of credit, so a slightly late wake-up just releases what came due; 10 ms since 2026-11-20, below), and short naps meant ~2 packets per send call; most of the shaper's CPU is kernel time per WinDivert call. With a 1 ms floor a send carries ~4 packets and the scheduler's kernel time per packet halved.
- Packets are released in micro-batches to amortise syscalls.
- **Changed 2026-11-20 (never slower, the user: "the service does not make the internet slower, it should only help"):** the root bucket holds **10 ms** of credit (`scheduler::ROOT_BURST_MS`, one helper for the service, `spike` and the simulator), and the scheduler and receive threads run at `THREAD_PRIORITY_HIGHEST` (`snailnet_win::thread::raise_priority`, via `FastPathConfig::on_thread_start`). The thread naps through its whole wait without taking new packets, so a wake-up later than the burst loses credit for good: in the simulator (`SimConfig::wake_late`, test `a_lone_download_keeps_its_speed_when_the_timer_wakes_late`) a thread 3 ms late gave a lone download 59% of the rate with 2 ms of burst, ≥ 97% with 10 ms. A busy PC (a build on every core) wakes normal-priority threads that late. Cost: at most 10 ms of queue at the modem after an idle moment.

---

## 7. Identifying apps

### 7.1 From PID to "app"

| Step | API | Notes |
|---|---|---|
| exe path | `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `QueryFullProcessImageNameW` | Works for most processes from LocalSystem |
| identity key | normalised exe path + (optional) Authenticode publisher | Rule matches survive updates that keep the path |
| PID reuse | key by `(pid, process creation time)` via `GetProcessTimes` | Avoids attributing a new process to an old rule |
| display name | `FileDescription` from version info, else exe name | e.g. "Steam Client WebHelper" → grouped (§7.2) |
| icon | extracted **in the UI process** (`SHGetFileInfoW` / `ExtractIconExW`), cached as PNG | Session 0 can't render shell icons reliably |
| UWP / Store apps | `GetPackageFamilyName` | Group by package (e.g. Xbox app, Netflix app) |

### 7.2 App families (helper processes count as one app)

Users think "Chrome", "Steam", "Discord", not 14 processes. Resolution order:

1. **Family packs** (shipped TOML): known patterns, e.g.
   - `steam` → `steam.exe`, `steamwebhelper.exe`, `steamservice.exe`, games under `…\steamapps\common\**` are **not** Steam (they're games, §11.4).
   - `chrome` → `chrome.exe` (all), `edge` → `msedge.exe`, `msedgewebview2.exe` (but WebView2 inside *other* apps belongs to the host app, see 3).
   - `discord` → `Discord.exe` + `Update.exe` under Discord's folder.
2. **Same install folder + same publisher** → same family (generic fallback).
3. **Parent chain** for helpers: if exe is a generic host (`msedgewebview2.exe`, `node.exe`, `java.exe`, `python.exe`), attribute it to the nearest non-generic ancestor.
4. **Windows services in `svchost.exe`**: read the command line (`NtQueryInformationProcess(ProcessCommandLineInformation)`) for `-s <ServiceName>`, then map:
   - `wuauserv`, `BITS`, `DoSvc` → **"Windows Update"** (a real hog, so make it a first-class app)
   - everything else → "Windows services".

Users can split or merge families in the UI ("Treat as separate app").

---

## 8. Identifying websites (per-site rules)

Goal (D7): "YouTube 45%" applies to YouTube traffic **inside any browser**, even with other tabs open.

### 8.1 Signals, best first

| Signal | Where | Visibility | Notes |
|---|---|---|---|
| **TLS ClientHello SNI** | first outbound TCP payload of a :443 flow | high today | Minimal hand-written parser; fuzzed |
| **QUIC Initial SNI** | first outbound UDP :443 Initial packet | high today | QUIC Initial packets are protected with keys derived from a **public** salt + the Destination Connection ID (RFC 9001), so anyone on path can decrypt the Initial and read the ClientHello. YouTube runs mostly on QUIC, so this matters. |
| **DNS answers** | sniffed responses on :53 | medium | Lost when the browser uses Secure DNS (DoH); still useful for apps |
| **IP → domain cache** | built from the above, TTL-bounded | — | Lets later flows to the same CDN IP match instantly |
| **Site pack IP hints** | optional shipped CIDR lists | fallback | Only for big CDNs with stable ranges; low confidence |

### 8.2 Site packs

Shipped as data so they can update without a new build:

```toml
# assets/sitepacks/youtube.toml
id = "youtube"
name = "YouTube"
icon = "youtube.svg"
domains = ["youtube.com", "*.youtube.com", "youtu.be", "*.googlevideo.com", "*.ytimg.com", "youtubei.googleapis.com"]
```

v1 packs: YouTube, Twitch, Netflix, Disney+, Prime Video, Spotify, Kick, Zoom, Google Meet, Teams (web), plus "Add a custom site" (domain wildcard).

### 8.3 Matching and precedence

1. A flow belongs to an **app family** (from PID).
2. If the flow's domain matches a site rule → the flow's class is the **site rule**, **in any app** (decided 2026-11-16: no browser list or per-app switch; a site beats the app's own rule, so the Spotify app counts as Spotify and a YouTube video in Discord as YouTube).
3. Else → the app's rule, else Everything else.

In the UI, sites appear **nested under the browser** ("Chrome ▸ YouTube"), but in the split bar a site can have its own slice.

### 8.4 When the site name is hidden

- **Encrypted Client Hello (ECH)** hides SNI. It's growing (Cloudflare-fronted sites and others). When SNI is the ECH outer name (e.g. `cloudflare-ech.com`), the flow is **unidentified**.
- **Fallback chain:** DNS cache (if not DoH) → IP hint lists → classify as the browser.
- **Honesty in UI:** the browser row shows "3 connections couldn't be identified". The help text explains why and suggests the browser setting that helps ("Use secure DNS: off" helps DNS matching; it doesn't defeat ECH).
- **Future option (backlog):** a tiny browser extension that reports tab domains → highest accuracy, opt-in.

### 8.5 Privacy stance

SnailNet reads only **domain names** in connection setup, **never content**, never leaves the PC, and the site list is visible to the user. The README and first-run say so.

---

## 9. The scheduler: percentages + priorities

This is the heart of the product. It lives in `snailnet-core::scheduler`, is pure Rust, and is driven by a `Clock` trait so it can be simulated exactly.

### 9.1 Model

Per direction (download and upload, each its own tree):

```
Root (rate = shaped line rate R, e.g. 0.92 × estimated capacity)
├─ Class "Steam"         share 40%  ceil —     prio Low
├─ Class "YouTube"       share 45%  ceil —     prio Normal
└─ Class "Everything else" share 15% ceil —    prio (per flow, from rules/Focus boost)
     └─ per-flow fair queues (FQ) inside, weighted by per-app priority
```

- **share**: guaranteed fraction of R when the class is backlogged (D2).
- **ceil** (optional): absolute cap, as a % of R or in Mbps, enforced by a per-class token bucket.
- **prio**: High / Normal / Low, which decides **dequeue order**, not long-run bandwidth (D8).
- If "separate up/down" is off, the same share/ceil applies to both trees.

### 9.2 Algorithm: weighted Deficit Round Robin with priority-ordered service

Each class `c` has:

- `weight_c = share_c`
- `deficit_c` (bytes of credit), `quantum_c = base_quantum × weight_c` (base_quantum ≈ 1514 B × k)
- a token bucket for `ceil_c` (if set)
- a queue (bytes-bounded, CoDel-managed)

Root token bucket at rate `R` gates **total** output (this is what makes download shaping and low latency work).

**Dequeue loop** (whenever root has ≥ one MTU of tokens):

1. Consider the *active set*: classes with a non-empty queue **and** ceil tokens available.
2. Among active classes with `deficit_c ≥ head_packet_size`, pick the **highest priority**; ties are broken round-robin.
3. If none has enough deficit, do one DRR replenish round: `deficit_c += quantum_c` for every active class (idle classes don't accrue, which is what makes it **work-conserving**), then go to 2.
4. Send the head packet; subtract its size from `deficit_c` and from the root tokens.
5. Cap `deficit_c` at `burst_c` (≈ 20–50 ms worth at the class's share). This is the crucial bit: **priority can only spend credit the class has earned**, so High-priority traffic goes *first* but cannot exceed its share over time.

Properties to prove in tests (§19):

- **Shares:** with all classes saturated, measured throughput is within ±3% of shares over a 2 s window.
- **Work-conserving:** an idle class's share is redistributed to backlogged classes **in proportion to their weights**.
- **Ceil:** a class never exceeds ceil by more than one burst.
- **Priority latency:** a High class with a light load (≤ its share) sees queueing delay ≤ 1 packet time + timer granularity even when others are saturated.
- **Starvation-free:** Low-priority classes still get their share.

### 9.3 Inside "Everything else" (and inside any multi-flow class)

Per-flow fair queuing (hash of the 5-tuple → sub-queue, DRR between flows), with flow weights from per-app priority (High 4 : Normal 2 : Low 1). One aggressive unlisted app (a new torrent client, say) can't starve other unlisted apps. Same idea as fq_codel.

### 9.4 Queue management (CoDel + ECN)

Download shaping works by holding inbound packets **after** they crossed the internet, so the sender only slows down if it sees delay or loss. Each queue uses **CoDel** (target 5 ms, interval 100 ms):

- If a packet's sojourn time stays above target for a full interval, the next packet is **ECN-marked** (if the flow negotiated ECN: set CE, fix the IP checksum) or **dropped**.
- Queue hard limit: e.g. 150 ms worth at the class's current rate, or 2 MB, whichever is lower.
- UDP without congestion control (some games, VoIP): such flows are usually tiny and High priority. Bulk UDP beyond its share is simply dropped at the queue limit, so download shaping is **rougher for UDP**, as stated in D3.
- QUIC (YouTube) *does* have congestion control and responds well to CoDel drops.

### 9.5 Express lane

Bypass queues (still counted against root tokens and the class's deficit, which may go briefly negative, then repaid):

- DNS queries/answers
- Pure TCP control packets: SYN, SYN-ACK, FIN, RST, and **ACK-only** packets ≤ 80 bytes (upload ACKs matter for download speed)
- Packets of flows in a class with `prio = High` whose size ≤ 300 B (typical game ticks)

### 9.6 Rule changes are live

Dragging the split bar sends new weights over IPC (debounced 50 ms). The scheduler swaps parameters atomically (`ArcSwap`) with **no packet loss**. Queues persist; deficits are rescaled.

### 9.7 Percentage normalisation rules (UI ↔ core contract)

- Shares are stored as integers in **basis points** (0–10000) and always sum to exactly 10000 per direction.
- "Everything else" has a **minimum of 5%** (it holds DNS, Windows and new apps; starving it breaks things). Its minimum is enforced in core, not just the UI.
- A rule can be **0%** ("only when free"): it gets bandwidth only when no other class wants it, implemented as a weight of epsilon plus Low priority.
- Removing a rule gives its share back to its neighbours in the split bar proportionally.

---

## 10. Line speed: auto-detect, adapt, low-latency mode

### 10.1 First measurement

At first run (with clear consent text): a ~10 s speed test.

- **Built (2026-11-18):** `RunSpeedTest` queues it; the runner stops shaping (state `Measuring`), runs `speedtest.rs` (WinHTTP, `snailnet-win::http`: 6 download / 4 upload connections × 8 s, the 90th percentile of 1 s rates after 2 s warm-up, ping to 1.1.1.1 idle and during the download) and saves the result as the line speed with `[line.measured]`. About 20 s in all. The app offers it in first run ("Measure my connection", the consent being the button and the Cloudflare line above it) and Settings → Connection (Measure; last result; "Ping while downloading"). CLI: `snailnet speedtest`. The reply is an Ack, not the progress/result messages sketched in §12; the result arrives as a config change. Results are per network since §10.3 was built: a test's speeds are the current network's.

- **The proof, built (2026-11-20, service half; §11.10):** the upload phase pings too (`loaded_up_ms`, same side thread and median as the download: `speedtest::loaded_phase`). Right after the test, with shaping back on at the new speed, the runner downloads once more through SnailNet (`speedtest::check`: the download phase again, ~10 s) and pings meanwhile: `with_ms`. So a test is now ~20 s with shaping off plus ~10 s with it on, and it **downloads twice** (the metered note and the app's "about 30 seconds" say so). A check cut short (stop, failure, another network, a newer test or typed speed) records nothing.
  - **Config compatibility is forward only.** `[line.measured]` (`Measured`) has `deny_unknown_fields`, and gained `loaded_up_ms` and `with_ms` (both optional, skipped when absent). An older config loads in the new service (tested); a config the new service wrote with those fields fails to parse in an **older** service, which then falls back to `config.toml.bak` or defaults. Updates only go forward, so this is accepted, not worked around; a downgrade would need the two lines removed by hand.

- Endpoints: configurable list; default **Cloudflare** (`speed.cloudflare.com`, `__down?bytes=` / `__up`), the user's choice 2026-11-18. Run with 4–8 parallel connections, take the 90th-percentile throughput of the last 6 s.
- The test flows are excluded from shaping.
- **Offline / blocked:** fall back to the passive estimate (§10.2) and ask the user to type the speed if they want precision now.
- The result is stored per network (see 10.3).

### 10.2 Adaptive estimate (passive, always on)

Continuous, cheap, no extra traffic:

- Track the **peak sustained throughput** (1 s windows, decaying max over 7 days) per direction.
- Track **latency under load**: a tiny ICMP echo to the default gateway and to **1.1.1.1** (user's choice 2026-11-18) at 1 Hz *only while the line is busy*. The rise is measured over each host's idle baseline (the lowest RTT of the last few minutes). The logic is `snailnet-core::estimator` (built 2026-11-18).
- If throughput repeatedly exceeds the estimate without a latency increase, raise the estimate. If latency rises sharply at a throughput *below* the estimate (line got slower: Wi-Fi, peak hours), lower the shaped rate. This is in the spirit of `cake-autorate`.
- Bounded change rate (≤ 10% per minute up, faster down) so it never oscillates.
- **Never slower (changed 2026-11-20).** Before, one ping sample 30 ms over the baseline in a busy second cut capacity 5%, every such second, down to 30% of the reference, and only a fully loaded, calm line won it back (~12 min), while the factor needed an hour; an idle line never recovered. On a line whose idle ping already jumps (§20: 79 ms average, 229 ms max), browsing could sit at a third of the speed. Now: the rise is the **median of the last 5 busy seconds** and must stay above 30 ms for **3 s** before a cut; the floor is **85%** of the reference (a line truly slower keeps some of its own lag, as it would without SnailNet, rather than being shaped below what it can carry); and while the line isn't busy a cut **drifts back** (half-life 60 s) and a lowered factor gives back 1% per 30 s, up to where it started. Tests in `estimator.rs`: single spikes never cut, a cut comes back once quiet, the rate never falls below 0.85 × 0.80 of the line under random noise.

### 10.3 Per-network profiles

Capacity is stored per network identity (`NLM` network GUID / Wi-Fi SSID + gateway MAC). Home Wi-Fi, phone hotspot and office keep separate speeds. A metered connection (Windows "metered" flag) is shown in the UI.

- **Built (2026-11-18), decisions taken:**
  - **Identity = the router's MAC** (`snailnet-win::network`: `GetBestRoute` to 1.1.1.1 → gateway + adapter, `SendARP` for the MAC; id `gw-xx:…`). Not SSID + MAC: one line reached by Wi-Fi or cable is one line (the estimator absorbs a slower Wi-Fi hop). If ARP fails, `if-{adapter GUID}-{gateway}`. Name = the Wi-Fi SSID (`WlanQueryInterface`), else the adapter alias ("Ethernet"). No COM/NLM, no new crates.
  - **Config:** `[line]` stays the current network's line (all code and the app's `SetLine` unchanged); `[network]` (id, name) says whose it is; `[[networks]]` (id, name, left, line) keeps the others, the 20 most recently left. `Config::switch_network` (`snailnet-core::networks`) does the swap; the factor override comes along, the pin and last test stay with their network.
  - **First network seen adopts the speeds already set** (upgrade path: existing configs keep shaping).
  - **Unknown network = watching only** until measured or typed, not "the last speed": a hotspot's 8 Mbps carried home would cap a fast line (the estimator raises at most 10%/min and 3× the reference), and a home speed carried to a hotspot shapes nothing. The app's notice then says "New network: {name}" with Set speed.
  - **Watcher** (`snailnet-service::netwatch`): looks at the route every 5 s (cheap, sends nothing), identifies only when gateway/adapter change, and tells the hub (`Hub::network_seen`), which saves and wakes the runner. Offline changes nothing. A new network also clears the ping probe's baselines.
  - The app shows "On {network}" under Connection speed and lists the other networks with their speeds in Settings → Connection.
  - **Metered flag, built (2026-11-18):** read by the **app**, not the service, since only the UI uses it: WinRT `NetworkInformation::GetInternetConnectionProfile().GetConnectionCost()` in `app/src-tauri/src/metered.rs` (command `network_metered`); metered = Fixed or Variable cost, roaming, or over the data limit; unknown counts as not metered. Settings → Connection re-reads it when the network or online state changes and shows a note under the line speed: measuring sends and receives for about 20 s, which counts against the data. Nothing is throttled or skipped on a metered network (monthly caps and metered awareness stay in v1.1, "Insight & automation").

### 10.4 Low-latency mode (D5, Smart)

- **On by default.** The root shaper runs at `factor × capacity`, default 0.92 down / 0.90 up. The bottleneck queue then lives in SnailNet (with CoDel) instead of the router's bloated buffer.
- **Auto-tune:** under load, if the RTT increase stays above +15 ms, lower the factor by 1% (floor 0.80); if it stays below +5 ms for 5 min, raise it by 1% (ceiling 0.97).
- **UI:** one toggle, "Low-latency mode: keeps your ping low while downloading. Costs ~5–10% of top speed." Plus a small **"Ping under load"** readout ("Ping 18 ms · without SnailNet ~140 ms", measured during the speed test). **Built as the proof (§11.10, 2026-11-20):** the live ping is the autorate probe's own pings to 1.1.1.1, and "without" is the last speed test on this network, named as such, never "~".
- Off → the root rate equals the estimate, maximum throughput, and shares still work.

---

## 11. v1 features in detail

### 11.1 Rules (core)

| Field | Values | Default |
|---|---|---|
| target | app family / site / custom domain | — |
| share | 0–95% (split bar) | auto-made room for it |
| ceiling | none / % / Mbps, per direction | none |
| priority | High / Normal / Low | Normal |
| separate up/down | bool | off |
| enabled | bool | on |

Creating a rule: click any app row → "Give it a slice". The split bar animates a new segment carved proportionally from existing slices (Everything else first, down to its 5% floor).

### 11.2 Profiles (Control)

- **Built as modes (2026-11-17):** four built-in profiles, `snailnet-core::modes`: **Balanced** (no rules), **Streaming** (Discord/Zoom High, YouTube/Twitch/Netflix slices, launchers and updaters only when free), **Gaming** (Discord High, video Low, downloads only when free; games stay in Everything else), **Quiet downloads** (launchers and updaters only when free). Older configs get the missing ones on load; the user's own Balanced is kept. One click on the Now page or the flyout switches (`SwitchProfile`); "Reset split" puts a mode back to its template (`ResetProfile`, with Undo); Settings → "Reset SnailNet" restores the defaults and shows first run again (`ResetAll`).
- **Duplicate & edit, built (2026-11-18):** "Make a copy" on the Split page copies the active mode as "{mode} copy" (then "… copy 2"; names are unique, any case, 1–40 characters) and **switches to the copy**, so the page then edits it (toast with Undo: delete it and go back). A mode of your own gets Rename (dialog) and Delete (confirm; Balanced takes over if it was on) instead of Reset split, which the service refuses for it (no template). Ids come from the name (lowercase ASCII, `-`), never a built-in id; at most 20 profiles. IPC `CopyProfile` / `RenameProfile` / `DeleteProfile` (`snailnet-core::modes`: `copy_profile`, `rename_profile`, `delete_profile`). Own modes appear after the built-in ones in the tiles, the flyout and the hotkey cycle, with a sliders glyph and "A mode of your own."
- **Hotkey, built (2026-11-18):** `Ctrl+Alt+S` (other keys: see below) moves to the next mode in the order the app shows them (wrapping; modes the config lacks are skipped) and opens the tray flyout, which shows the mode that is on. Registered by the app (`app/src-tauri/src/hotkey.rs`, `tauri-plugin-global-shortcut` =2.4.1), not stored in `config.toml`: it belongs to the Windows user, like Start with Windows, so the app keeps it (an empty `hotkey-off` file in its config folder means off). On by default; Settings → Windows has the switch, which says so when another program already holds the keys.
- **Other keys, built (2026-11-18):** a pick from four, not "press any keys": Ctrl+Alt+S, Ctrl+Shift+Alt+S, Ctrl+Alt+M, Ctrl+Alt+N (`hotkey::CHOICES`), which stay clear of everyday shortcuts (Ctrl+Shift+S is Save as) and need no key recorder. A select beside the switch while it's on; the title says the keys ("Ctrl+Alt+M switches mode"). Kept by the app in a `hotkey-keys` file (absent = default; anything not in the list = default). Picking keys another program holds keeps the old ones registered and says so. Not tried in a real session.

- A profile = a complete rule set + Low-latency setting.
- Shipped presets: **Balanced** (no rules, Low-latency on), **Gaming** (games High, launchers/updaters Low, Windows Update ceiling 20%), **Streaming** (site packs for video Normal, downloads Low), **Downloads** (everything to downloaders, Low-latency off).
- Switch via the tray flyout, the main window header, or a global hotkey (default `Ctrl+Alt+S` cycles; configurable).
- "Duplicate & edit" to make your own. Profiles are stored in config; switching is instant and atomic.

### 11.3 Block per app (Control)

- **Built (2026-11-17), first cut:** `Config.blocked` (family ids, every mode); the fast path drops a blocked app's internet packets in both directions (`Classifier::blocked`), LAN traffic passes, and blocking alone runs the engine without a line speed. In the app: "Block its internet" in an app's Boost menu (with Undo), blocked apps listed under "Using your internet" with Unblock. The SOCKET-layer refusal and the tray counter followed the same day (below).

- A toggle on each app row ("Online / Blocked").
- Implementation: SOCKET layer blocks `connect`/`bind`/`listen` for that family (clean, the app gets a normal network error) **and** NETWORK layer drops packets of already-open flows.
- Blocked apps show a muted row with a lock glyph; the tray shows a counter when ≥ 1 app is blocked.
- **SOCKET refusal and tray counter, built (2026-11-18), decisions taken:**
  - **`connect` only, internet only.** A WinDivert SOCKET handle in DROP mode (`snailnet-service::refuse`) refuses `connect()` of a blocked app's processes to any non-LAN address, so the app gets a normal, immediate network error. `bind`/`listen` aren't refused: blocking leaves the LAN alone, and refusing a bind would break that (and UDP `sendto` without connect is still dropped at the NETWORK layer).
  - **By PID.** WinDivert filters can't name an app, so the filter lists the blocked apps' process ids (≤ 150; the rest are still dropped at the NETWORK layer), from the processes attribution has resolved (`Attribution::processes`, PID + creation time). Checked every 0.5 s; on a change a new handle opens before the old one closes. A reused PID is never refused: the creation time must still match.
  - It sits at priority −100, below the sniffing handles, so they see every connect first: a new process of a blocked app is attributed by its first connect (which gets through and is then dropped at the NETWORK layer) and refused from its next one on.
  - **Fails open:** if the handle won't open, a warning and packet dropping only; when the service stops or dies, the handle closes and nothing is refused. Runs only while the engine runs in blocking mode (an app blocked, or strict mode).
  - WinDivert's filter language has no `!(…)`: each LAN range is written `(remoteAddr < lo or remoteAddr > hi)`. A unit test compiles the longest filter with `WinDivertHelperCompileFilter` (no admin needed).
  - **Tray:** a near-black badge with a white figure in the top-right corner (1–9, then "+"; the warning dot keeps bottom-right) and "· N apps blocked" in the tooltip, while connected and not paused. Not seen at real tray size.
  - **Not yet:** run against the real driver (refusal is untested beyond the filter compiling).

### 11.4 New-app notification (Control)

- The first time an unknown app family goes online, a Windows toast: **"Fortnite went online"**, with buttons *Fine* · *Give it a slice* · *Block*.
- **Default mode: notify-only** (the app is allowed immediately and joins Everything else). This never stalls a launch or update.
- **Strict mode** (setting): hold the app's new connections (SOCKET layer blocks with retry semantics; NETWORK layer holds the SYN up to 20 s) until answered; on timeout, apply the default action.
- Quiet hours: suppress toasts while fullscreen (games) and batch them into one summary toast afterwards.
- Windows' own components (signed by Microsoft, in System32) never prompt.
- **Notify mode, built (2026-11-18), decisions taken:**
  - **The service decides what is new** (`snailnet-service::newapps`). Every family the shaper sees is checked once against `known-apps.txt` in the data folder (one family id per line, plus `since <unix>`: when SnailNet first looked). It's a file of its own, not part of `config.toml`, so meeting an app doesn't bump the config revision or reload the plan.
  - **Quiet (known silently, no prompt):** everything met in the first **10 minutes** after SnailNet first looked (a fresh install doesn't greet every running program); parts of Windows (System, Unknown, services, any exe under the Windows folder, `pkg:Microsoft.Windows…` packages; "in System32" is the test, not the signature, which is slow to read); and apps the user already said something about (a rule in *any* mode, shipped mode templates included, a block, a boost, "It's a game", "Never put it first").
  - Apps are seen only while the engine runs (shaping or blocking). With no line speed nothing is diverted, so nothing is new; accepted.
  - **IPC:** `Event::NewApp { prompt_id, family, name, exe }`, only to clients that list `new-apps` in Hello's `ui_capabilities` (an older app can't decode it); `Snapshot.prompts` lists the open ones (≤ 20, oldest dropped, in memory), so an app started later still shows them. `AnswerPrompt { action: fine | slice | block }`: block blocks the family in every mode (§11.3); fine and slice only close the question; an answered or unknown id is refused.
  - `Settings.new_apps = notify | off` (default notify); switch in Settings → Windows ("Tell me when an app goes online for the first time"). Off still learns apps (turning it back on doesn't flood) and drops the open questions.
  - **App:** `app/src-tauri/src/toast.rs` shows WinRT toasts directly (the `windows` crate already in the tree; `tauri-plugin-notification` has no action buttons on desktop) under the AppUserModelID `net.snailnet.app`, registered in HKCU at start. One toast per app per app run: "{app} went online · It shares Everything else for now." with **Fine · Give it a slice · Block**. "Give it a slice" answers the service and opens the window on the split with a 20% slice for the app (webview event `give-slice`).
  - **Quiet hours:** while a game, a presentation or another fullscreen program is in front (`SHQueryUserNotificationState`), toasts wait; afterwards one summary ("3 apps went online while you played: A, B and C"), or the normal toast if only one waited.
  - Button clicks reach the app through the toast's `Activated` event while the app runs (it lives in the tray). A toast clicked later from Action Center after the app quit does nothing (no COM activator); accepted for v1.
  - **Not yet:** seen in a real window or the real service.
- **Strict mode, built (2026-11-18), decisions taken:**
  - `Settings.new_apps = strict`; in the app a second switch under the notify one, "Wait for my answer first" (shown only while notifications are on).
  - **A hold is a temporary block.** A new app's prompt carries `held: true` (`NewApp.held`, absent from older services); the service adds its family to the plan's blocked list until the answer or **20 s** (`STRICT_HOLD_SECS`), whichever comes first. The existing drop path does the holding: packets of its flows are dropped, so a TCP connect keeps retrying its SYN (Windows retries for about 21 s) and gets through once released. No SOCKET-layer refusal (that's §11.3, item C); "SOCKET layer blocks with retry semantics" above is dropped in favour of this.
  - **Timeout = go online** (notify mode's default); the question stays open, so Block still works later. Fine and Give it a slice release at once, Block blocks for good. Switching back to notify releases every hold; off also drops the questions.
  - **No restart on a hold:** in strict mode the plan says so (`Plan.strict`) and the engine diverts both directions from the start, as for blocking, so adding a held app is a live reconfigure. Strict alone doesn't start the engine: with no line speed and nothing blocked, nothing is seen, so nothing is held.
  - **Accepted gaps:** the first packets of a new app pass until the shaper has attributed its flow and the hub has seen it (up to about a second, the stats poll); a held app's already-open flows break and reconnect. Holds live in memory: a service restart lets held apps go (they're known by then).
  - Toast for a held app: "{app} wants to go online · It waits for your answer, 20 seconds at most." with **Allow · Give it a slice · Block**. During quiet hours the toast waits for the summary, and the app goes online after 20 s.

### 11.5 Focus boost (Smart)

- **Boost by hand, built (2026-11-17):** one app or site at a time goes first (High, at least 50% of the line, no ceiling; the other rules shrink to fit) for 1 h, 4 h or until stopped, in every mode (`Config.boost`, IPC `Boost` / `StopBoost`; the runner ends it on time). In the app: a Boost button on each row of "Using your internet". Automatic focus boost below is still Phase 4.

- **What:** the app in the foreground gets **High priority** temporarily (never a bigger share). Games (detected) get High for their whole session, even when alt-tabbed.
- **Foreground tracking (UI process):** `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` → PID → family → IPC `focus_changed`. Debounced 300 ms so alt-tabbing doesn't churn.
- **Game detection** (any one is enough; confidence shown in UI):
  1. `SHQueryUserNotificationState` = `QUNS_RUNNING_D3D_FULL_SCREEN`, or the foreground window covers its monitor (borderless fullscreen) **and** the process loads `d3d11.dll`/`d3d12.dll`/`vulkan-1.dll`.
  2. The exe lives under a known launcher library (`steamapps\common`, Epic `\Epic Games\`, EA, Ubisoft, Battle.net, Riot, GOG, Xbox `XboxGames`) from **game packs**.
  3. The user marked it "This is a game".
- **Overrides:** per app "Never boost" / "Always High".
- **Optional automation:** "Switch to Gaming profile while a game runs" (off by default, one checkbox).
- **Built (2026-11-18), decisions taken:**
  - **App:** `app/src-tauri/src/focus.rs` polls `GetForegroundWindow` every 300 ms instead of a WinEvent hook (as cheap, and it debounces by itself: a program must be in front two polls running) and sends `FocusChanged` on a change, again every 60 s (a restarted service starts empty). SnailNet's own window is skipped.
  - **Game = exclusive fullscreen D3D** (`SHQueryUserNotificationState`) **or an exe in a launcher library** (`snailnet-core::games`). Borderless fullscreen alone is *not* a game: a browser playing fullscreen video would pass, and the D3D-DLL test doesn't help since browsers load D3D too. Such a window still goes first while in front.
  - **Service** (`snailnet-service::focus`, runtime only): the PID in front and the games (PID + creation time, dropped once exited, checked each second) go into `Plan.focus`; each rx thread resolves them to families when the rules change, and packets of those families get High *flow* priority in their own class (weight 4 in its fair queue, small packets via the express lane). No class and no share changes, so "never a bigger share" holds by construction.
  - `Settings.focus_boost` (on by default) with IPC `SetSettings` and a switch in Settings → Connection ("Put what you're using first").
  - **Gaming mode while playing, built:** `Settings.gaming_while_playing` (off by default; switch in Settings → Connection). While a game runs, the hub plans with the Gaming mode; the mode the user picked stays saved and returns when the game exits. The service says so in every stats tick (`StatsTick.playing`, 4 Hz, so no new event); the app keeps the picked mode checked (it's what comes back) and marks Gaming with an outlined "Playing" flag plus a note: "Gaming mode while you play. {mode} comes back when the game closes." (mode tiles and the tray flyout). Nothing is shown when Gaming is the pick anyway.
  - **Overrides, built (2026-11-18):** `Settings.games` and `Settings.never_first` (family ids; empty lists stay out of the file; one family can't be in both). Set from an app row's Boost menu ("It's a game", "Never put it first"; ticking the ticked one again hands it back to SnailNet; the toast can undo), listed under Settings → Connection → "Your choices". The **service** applies them, not the app: the hub resolves the PID in front to its family with its own `Resolver` (same pack and rules as the shaper; asked only on a focus change). "A game" counts when the process comes to the front (the app repeats the report every 60 s, so a fresh mark catches up); "never first" is checked whenever the plan is made, so it takes effect at once and also stops that app from switching on Gaming mode. **"Always High" is not a separate override:** a rule with High priority already does it, in every mode the rule is in.
  - **Confidence in the UI, built (2026-11-18):** not a number but the reason, since each test is a yes/no: `FocusChanged.why` (`fullscreen` | `library`, absent from older apps, which count as fullscreen) and `StatsTick.games` (`GameTick { family, why }`, one per family, never-first apps left out). The app checks the library first (it's what the program is) and keeps it once seen; the user's "It's a game" mark beats both (`marked`). Shown on the app's row under "Using your internet" in place of its slice status: "Goes first · a game, it ran fullscreen" / "… from a game library" / "Goes first · you said it's a game" (a boost's time left still wins); nothing while "Put what you're using first" is off. Not seen in a real window.

### 11.6 App families (Control)

As specified in §7.2. In the UI, an expandable row shows member processes; "Split out" / "Merge into…" actions.

- **Built (2026-11-18), decisions taken:**
  - **Stored in `config.toml`** as `[families]`: `split = ["steamwebhelper.exe"]` (lowercase program file names) and `[[families.merge]] from / from_name / into / name` (`snailnet-core::families`). IPC `SetFamilies { families, rev }`.
  - **Split out** works by program (exe name), in whichever family it shows up: it becomes the family `split:<exe>`, named "{app} ({exe})". **Merge** works by whole family. Split first, then merge, so moving one program into another app is split + merge. Merges don't chain (the target can't itself be merged away); merging a family that others were merged into moves those along.
  - **Applied in the attribution's view of its apps** (`Attribution::set_families`): app ids stay what the resolver made, so open flows keep their app and change group at once; per-app caches drop with the rules. The hub's own PID → family lookup (focus overrides) applies them too. Rules, blocks and "It's a game" name the grouped family.
  - A split-out program or a merge target never counts as a new app (§11.4).
  - Not an expandable row: the app's Boost menu has "Split out {exe}" (up to 4 programs, shown only when the app has more than one), "Put it back into its app" on a split-out one, and "Count it as part of [app ▾]" (a select over the apps seen). Each says what it did with Undo. Settings → Connection lists "How apps are grouped" with Put back / Separate.
  - Found on the way: blocking (and "It's a game" / "Never first") refused every family id with a `:` in it, so apps known by their path (`c:\…\x.exe`) or Store package (`pkg:…`) couldn't be blocked, a new-app "Block" on them included. Now only rule-target strings (`family:`, `exe:`, `site:`) are refused (`families::is_family_id`).
  - **Not yet:** seen in a real window or the real service.
  - **impeccable audit (2026-11-18, code only, no browser):** of the UI since Phase 4 (Boost menu, Settings lists and switches, the game reason line, toasts). Fixed: "Count it as part of" acted on `change`, which Chromium fires on every arrow key in a closed select, so a keyboard user merged into the first app they arrowed past; now the select picks and a **Move** button does it. The Block item's hover put salmon on `--surface-3` (3.8:1); it keeps the menu ground with a thin salmon ring. App menus scroll inside a short window (`max-height`). The hotkey select no longer rebuilds (focus stayed lost after a choice). Copy: "full screen", "in a launcher's library". Left as is: the app menu is `role="menu"` without arrow-key movement (Tab works) and holds a labelled select; the global reduced-motion rule cuts every animation to 0.01 ms. Contrast and sizes on the rendered page weren't measured (the detector only ran its source rules on `.svelte`).

### 11.7 Live view (core)

- The app list shows ↓/↑ current speed per family/site (1 s smoothed, UI updates at 4 Hz), connection count, and a 60 s micro-sparkline on hover (no persistent graphs in v1).
- The header shows total ↓/↑, capacity, Low-latency status and live ping.
- Sorting: by current speed (default), name, share. Idle apps fade after 30 s and move to an "Idle" group after 5 min.

### 11.8 Pause

- Side rail and tray flyout: **Pause for 15 min / 1 h / until I resume**. Pause state is obvious (amber tray glyph, a notice with Resume).
- **Built (2026-11-17):** `Config.pause`; while paused the plan is empty, so the engine steps aside entirely (no shaping, no blocking, no live stats) and the service reports `ServiceState::Paused { until }`. The runner ends a timed pause by itself. IPC `Pause { minutes }` / `Resume`.

### 11.9 Calls go first (Smart)

- **What:** a live voice or video call goes first in any app, browser or mode, with no rule: Discord, Zoom, Teams, Meet, WhatsApp. Same promise as §11.5: High *flow* priority in its own class (weight 4 in its fair queue, packets ≤ 300 B through the express lane), never a bigger share. Covers the gap the modes leave: Balanced has no call rules, and Streaming/Gaming name only Discord and Zoom, and only by app, so a call in a browser tab or an app not in front had nothing.
- **Built (2026-11-19), decisions taken:**
  - **Spotted by the traffic's shape, not by app name** (`snailnet-core::calls::CallWatch`, pure, simulated-time tests): a UDP flow with ≥ 20 packets a second (both directions together), mean packet ≤ 1 250 B, ≤ 6 Mbps each way, for 3 seconds running. Voice is RTP every 20 ms (~50 pps each way); video calls cap packets near 1 200 B. A game's server stream fits too, which is wanted.
  - **Not calls:** QUIC (remote port 443 with QUIC's fixed bit `0x40` in the first byte; RTP's first byte is `10xxxxxx`, so a call relayed on 443 still counts); bulk UDP over the rate or size caps (ends a call at once); BitTorrent's uTP (an app with more than 4 call-shaped flows at once is a swarm and none count).
  - **Ending:** a call stays one while quiet (muted listener, nobody talking: keep-alives still arrive) and ends 15 s after its last packet; idle flows are forgotten after 60 s; at most 4 096 flows watched.
  - **Per flow, not per app:** only the call's own flow goes first, so a browser's downloads in other tabs don't ride along.
  - **Service:** the rx threads share one `CallWatch` behind a mutex (default one rx thread, so uncontended), taken only for UDP packets of attributed apps. Both directions count when both are diverted; with only one shaped, the one seen is enough. "Never first" apps are skipped (`Plan.never_first`).
  - `Settings.calls_first` (on by default; older configs get it on), `Plan.calls_first`; a switch in Settings → "Calls, games and the app you're using": **Put calls first**.
  - **To the app:** `Seen.calls` (app ids) → the hub's families → `StatsTick.calls` (sorted, once each; empty while the switch is off or not shaping; skipped when empty, so older apps ignore it). The row under "Using your internet" says **"Goes first · in a call"**; a boost's time left and a game's reason win over it. `ServerMsg::Stats` now boxes its tick (clippy `large_enum_variant`); the wire format is unchanged.
  - **Review fixes (2026-11-19):** the classifier watched for calls only when the app wasn't in front (`focused || in_call` short-circuited), so a call lost its history while its window had focus and took ~3 s to come back after alt-tab; it now always watches. "Using your internet" shows the busiest five, so a voice call (~0.04 Mbps) fell off the list behind a download and its label was never seen: an app in a call now takes the place of the quietest one not in a call (`users(…, calls)`; a boost still wins the last place). Its speed read "0.0 Mbps"; app rows now say "<0.1" for a trickle (`rowMbps`).
  - **impeccable audit (2026-11-19, code only, no browser):** detector clean on SettingsPage and WhoCard (regex rules only; contrast and sizes not measured). The new switch row reuses the existing labelled `role="switch"` checkbox pattern and tokens; copy follows its siblings ("Put … first", "Goes first · …").
  - **Not yet:** seen with a real call through the real service (needs the elevated dev service and a call while something downloads), seen in a real window, README and site (with the release that ships it).

### 11.10 The proof: ping with and without SnailNet (Smart)

- **What:** SnailNet's work is invisible, so it shows the result in numbers, measured on this PC and this network: "Ping 19 ms while Steam fills your line · 142 ms without SnailNet", and a grade from A to F before and after. Brief: docs/HANDOFF-wow.md, feature 1.
- **Built (2026-11-20, service half), decisions taken:**
  - **Grades** (`snailnet-core::proof`, pure): rise = loaded − idle ping (`rise`, never negative, `None` unless both exist); **A ≤ 30 ms, B ≤ 60, C ≤ 200, D ≤ 400, F above** (`A_MAX_MS` … `D_MAX_MS`), each edge in the better grade. Waveform's bufferbloat scale, so people can compare; no A+.
  - **The record** (`Measured`, §10.1): `idle_ms`, `loaded_ms` (download, shaping off), `loaded_up_ms` (upload, shaping off), `with_ms` (the check download through SnailNet). All optional; forward-only compatible (§10.1).
  - **The "with SnailNet" check:** `Hub::record_measured` leaves `check_pending = measured.at`. In the shaping session, once shaping has run `CHECK_AFTER` (2 s) and the **download is shaped**, the runner takes it and runs `speedtest::check` on a thread (`Timing.check`, a plain `fn` so tests inject a fake; `Timing.check_after` so they don't wait 2 s), polling `is_finished()` each turn, so the loop never blocks. State stays `Shaping` (no new state); the app learns of it when `with_ms` arrives in the config (`ConfigChanged`). The service's own download is shaped like any app's (Everything else), which is the point.
  - **Only for the test it belongs to:** `Hub::record_check(at, ms)` writes `with_ms` only while `line.measured.at == at`, so a newer test, a typed speed or another network's line wins. A reload that changes the network or `measured.at` cancels the running check; so do stop, failure and a restart. A cancelled check's thread finishes by itself (the WinHTTP download runs to its phase end) and the next check waits for it, so two check downloads never overlap. After a restart (same test, same network) the check is queued again (`retry_check`); after a failure or stop it isn't.
  - **Live ping** (`autorate::Probe::now`): the median of the public host's (1.1.1.1) pings of the last 5 s, at least 2, the latest fresh (≤ 3 s), plus the 5-minute lowest. It goes stale by itself when the line goes quiet (the probe drops to one ping per 30 s). The runner sets it each second (`Hub::set_ping`); `clear_stats` clears it. To the app as `StatsTick.ping: Option<PingTick { now_ms, low_ms }>`, skipped when `None`, so older apps ignore it and a tick without it decodes. No new `ServiceState`, `Event` or `Request` variants: protocol stays v1.
  - **Pings skip SnailNet's queues** (the engine diverts TCP and UDP only), so the number is the line's ping beyond this PC, what apps that go first feel; the words say "Ping", not "your game's ping".
  - **CLI:** `snailnet top` prints `ping: 19 ms (lowest 12 ms)` when the tick has it.
  - **App half** (what to show, the "≥ 20 ms clear" rule, ProofRow, grades in Settings, the toast after Measure): docs/HANDOFF-wow.md 1.5–1.6.
  - **Not yet:** the live check (Measure on the real line; `snailnet top` shows `ping:` during a big download; the saved `measured` has `loaded_up_ms` and `with_ms`; *with* clearly below *without*). Required before release: it is a numeric claim.

### 11.11 The one-tap fix: an app left waiting (Smart)

- **What:** when one app fills the line and another is left waiting, the Now page's sentence offers the cure ("Give Brave 60%"). Brief: docs/HANDOFF-wow.md, feature 2.
- **The signal (built 2026-11-20, scheduler + sim; 2.2 onward not yet):** inside Everything else SnailNet queues fairly per connection (§9.3), so a 16-connection download takes ~16× a one-flow video; nothing reported which app was losing. Now each fq flow remembers its first packet's app tag and counts CoDel drops/ECN marks and queue-limit drops (`acted`); `Scheduler::pressure(out)` returns per tag `{ queued_bytes, acted }` at stats rate. Per-packet cost: one addition; zero allocation on the packet path (retired-owner table, 64 per class, reserved up front; `tests/scheduler_alloc.rs` counts allocations). `acted` is a running total: readers diff ticks with `saturating_sub`.
- **The rule, proven in `snailnet-sim` (`waiting.rs`, `RULE`):** sampled every 250 ms per app over the last 20 ticks (5 s). *Pressed* in a tick = CoDel acted on it **or** any of its bytes are queued. **Waiting** = pressed in **≥ 14 of 20** ticks **and** it got **< ½ of an equal split** of what was delivered in the window, among the apps that were themselves pressed ≥ 14 ticks.
  - Why not CoDel actions alone: at longer RTTs CoDel acts on a starved flow less often (ticks acted: 100 % at 20 ms RTT, 83 % at 50, 43 % at 100, 27 % at 150), while its queue is non-empty far more often (100 / 97 / 82 / 73 %). Why not `HELD_BYTES` (6000) per app: the starved video's queue passed it in only 0–17 % of ticks. Why the share test: the hog is pressed too (CoDel acts on it every tick); only the share tells them apart (Steam ~94 %, the video ~6 %).
  - **Numbers** (25 Mbps line shaped to 23, everything in Everything else, a 16-connection download + a video wanting 8 Mbps, which gets 1.43): flagged after **3.50 s** at 20 and 50 ms RTT, 3.75 s at 100, 4.25 s at 150 (then 92 % of ticks); at 250 ms RTT 5.25 s but only 49 % of ticks (too on-and-off to rely on). Two 8-connection hogs + the video: the video at 3.50 s, neither hog ever.
  - **Never flagged** (0 ticks): the hog, alone or with any waiter; two equal 8-connection downloads; the video alone; a full-buffer video (4 s chunks every 4 s) at 250, 500 and 750 kbps.
  - **Limit:** a top-up video's busy share is about its stream rate / its fair share (~1.35 Mbps here). A 1000 kbps top-up (busy 70 %) is flagged in 123 of 199 ticks; at 1250 kbps it is starved in all but name. 16 of 20 would flag it in 20 ticks but long-RTT waiters only 63 %; 17 of 20, 0 ticks but 47 %. 14 of 20 keeps real waiters reliably flagged; the offer's own 12-of-15 s sustain (2.4) and mutes absorb the rest. With that sustain the offer appears ~15.5 s after the video starts.
- **To the app (2.2, built):** the engine's `Queued` returns its tag, `DirectionStats.pressure` carries `Scheduler::pressure` with each snapshot, and `hub::tag_ticks` sums it per family: `FamilyTick.queued_bytes` (now) and `acted` (since the previous tick), both `serde(default)`. A family with bytes queued but nothing delivered in a tick is listed too (bps 0). `snailnet top` marks `Brave 1.40 (held back 3x)`, the raw signal; the waiting rule itself runs in the app (2.3).
- **In the app (2.3–2.5, built 2026-11-20, not yet seen live):** `waiting.ts` is the sim's rule, fed each direction's raw ticks in `service.svelte.ts` (`service.waiting`); members carry `waiting`, the glance names them ("Brave wants more room too") and their chip says "wants more room". `fix.ts`: `pairOf` = the line full, the busiest app ≥ ½ of the shaped rate (the hog), another app waiting (a waiting member → *give* 60%, as much as fits; a slice that's full → *grow* 15 points); download first, then upload. `FixWatch` offers once the same pair held **12 of the last 15 s** (stalled stream gaps count ≤ 1 s). No offer while the switch is off, a game runs with Gaming standing in, a boost is on, the service isn't shaping, or the pair is muted; an "only when free", off or unrecognised waiter is never offered. `fixer.svelte.ts` runs it in each window (main and flyout) and saves through `saveRules` (rev-checked) to the active mode: "Brave gets 60% of your line now. Undo" shows in place for 10 s (Undo only if the rev is still the fix's; else "the split changed in the meantime"). With unsaved Split-page changes the main window shows "save or discard your split first" instead of the button; the flyout doesn't see the main window's draft, so a fix from there makes that draft stale, which its own bar already handles. Mutes: Not now until local midnight, 10 min after a fix; with the Settings switch ("Suggest a fix when an app is left waiting", on by default) in the app's storage (`snailnet.fix`), not a file like the hotkey: only the UI reads them.
- **Next:** the live check (Steam or curl with many connections + YouTube in Brave, no rules → offer within ~20 s; one click → smooth; Undo restores; YouTube paused → no offer).

---

## 12. Service ↔ app communication (IPC)

### 12.1 Transport

- Named pipe `\\.\pipe\snailnet.v1`, message mode, overlapped I/O (tokio `named_pipe`).
- **Security descriptor:** `SYSTEM: full`, `Administrators: full`, `INTERACTIVE: read/write`; deny `NETWORK` (no remote access). Clients are verified with `GetNamedPipeClientProcessId` + an exe path/signature check that it's `SnailNet.exe` from the install dir.
- Framing: 4-byte little-endian length + **MessagePack** (serde, `rmp-serde`). Max frame 1 MB.

### 12.2 Protocol (v1)

```text
→ Hello { client_version, ui_capabilities }       ← Welcome { service_version, protocol: 1, state }
→ GetSnapshot                                      ← Snapshot { apps, sites, rules, profiles, line, settings }
→ Subscribe { stats_hz: 4 }                        ← StatsTick { t, per_class[], per_family[], totals, ping } (stream)
→ SetRules { profile_id, rules[], rev }            ← Ack { rev } | Error { code, msg }
→ SwitchProfile { id }                             ← Ack
→ SetBlocked { family_id, blocked }                ← Ack
→ FocusChanged { pid, is_game, confidence }        ← (no reply)
→ Pause { until }                                  ← Ack
→ RunSpeedTest                                     ← SpeedTestProgress… / SpeedTestResult
→ AnswerPrompt { prompt_id, action }               ← Ack
                                                    ← Event::NewApp { prompt_id, family } (stream)
                                                    ← Event::LineChanged / SafeMode / EngineError
```

- Every mutating call carries `rev` (optimistic concurrency) so two UIs/CLIs can't silently overwrite each other.
- `protocol` is versioned; the service supports N and N-1.

---

## 13. Configuration and storage

### 13.1 Locations

| What | Where | Owner |
|---|---|---|
| Rules, profiles, settings | `C:\ProgramData\SnailNet\config.toml` | service (ACL: SYSTEM + Admins write; Users read) |
| Network capacity cache | `C:\ProgramData\SnailNet\networks.toml` | service |
| Logs | `C:\ProgramData\SnailNet\logs\service-YYYY-MM-DD.log` (7-day rotation) | service |
| UI prefs (window size, theme override, language) | `%APPDATA%\SnailNet\ui.json` | app |
| Icon cache | `%LOCALAPPDATA%\SnailNet\icons\` | app |
| (v1.1) usage history | `C:\ProgramData\SnailNet\history.sqlite` | service |

Writes are **atomic** (write temp → `ReplaceFileW`), with a `config.toml.bak` of the last good version. A schema `version` field allows migrations.

### 13.2 Config sketch

```toml
version = 1
active_profile = "gaming"

[settings]
low_latency = true
lan_exempt = true
new_app_mode = "notify"     # "notify" | "strict" | "off"
focus_boost = true
hotkey_cycle_profiles = "Ctrl+Alt+S"

[[profiles]]
id = "gaming"
name = "Gaming"
everything_else_share = 1500          # basis points

  [[profiles.rules]]
  target = { kind = "family", id = "steam" }
  share = 4000
  priority = "low"

  [[profiles.rules]]
  target = { kind = "site", id = "youtube" }
  share = 4500
  priority = "normal"

  [[profiles.rules]]
  target = { kind = "family", id = "windows-update" }
  share = 0
  ceiling = { down = "20%", up = "10%" }
  priority = "low"

[families.overrides]
"C:\\Games\\MyIndieGame\\game.exe" = { is_game = true }
```

---

## 14. UI / UX specification

> The direction was picked by the user from real-size mockups: "B · Ledger" (2026-11-16), then "C · The Garden with B's side rail" (2026-11-17, after "match the app theme, the snail icon"), then a premium pass ("more cinematic and premium", animations, pointer cursors). D11–D13 and this section are the brief; `docs/HANDOFF-ui.md` has the history.

### 14.1 Visual language: the Garden

- **From the snail icon:** the deep green of the shell's spiral is the ground (`#0e2b26`), the body's darker green the side rail (`#0a211d`), the shell's bright green (`#2fa88e`) marks what's boosted and the brand, cream (`#efe8d6`) marks the choice that's on (the mode tile, the main action). Same whatever Windows' theme or accent. No Mica (it can't show through WebView2 here).
- **Light and depth:** a soft shell-green light falls from above the content and a deeper green pools low (`--ground-light`). Cards are a step lighter, catch that light on their top edge and sit on real offset shadows (`--shadow-card`, `--shadow-raised`); one elevation per element, no borders. 12–18 px radii.
- **Type:** one family, Atkinson Hyperlegible Next (bundled, OFL), chosen for digits that can't be misread. Page questions 32 px extra-bold with tight tracking, card headings 16 px bold in plain words (no small-capital labels), 15–17 px body, tabular numerals for every speed and percentage.
- **Colour:** warm off-white type in three steps (13:1, 8.9:1, 6:1 on the ground). Slice inks only where they carry meaning (six, ≥ 5.7:1); Everything else is a green-grey stone. Spare room is hatched, never shown by colour alone.
- **The shell:** the icon's spiral is the brand motif: the snail in the side rail, the flyout and empty states, and drawn large and faint behind the Now page's question.
- **Icons:** each app's own icon from its .exe (Rust `app_icon`, cached), a letter tile in the site's colour for websites, four squares for Everything else; Segoe Fluent Icons for UI glyphs; the mode glyphs are drawn SVG.
- **Motion** (one authored moment, the rest explains state): the shell spiral draws itself once when the Now page opens; tiles and cards then arrive in reading order. Pages rise in out of a slight blur; the side rail's highlight slides to the open page; a chosen mode's cream spreads from its glyph; rows of "Using your internet" glide when the order changes; a boost sweeps in its green; menus, dialogs and toasts rise into place; buttons press slightly. Exponential ease-out, 120–560 ms (the spiral 1.8 s). `prefers-reduced-motion` keeps colour and opacity changes and drops travel and blur. Nothing loops.
- **Pointer:** everything clickable shows the pointer cursor; disabled controls show not-allowed.
- **High contrast:** Windows high-contrast mode replaces the palette with system colours.

### 14.2 Window structure

Main window, default 1120 × 740, min 840 × 560, custom title bar (Tauri `decorations: false`, Windows caption buttons drawn in the page).

```
┌───────────────┬──────────────────────────────────────────────────────┐
│ 🐌 SnailNet    │                                           –  □  ✕   │
│               │ What matters right now?                (shell spiral)│
│ ◉ Now         │ [Balanced] [Streaming ✓] [Gaming] [Quiet downloads]  │
│ ≡ Split       │ ┌ Using your internet ──────────┐ ┌ The split ──────┐│
│ ⚙ Settings    │ │ Steam   in Everything else 10.3 [Boost] │ ████▌██▌██ ││
│               │ │ YouTube gets all it asks for 9.8 [Stop] │ YouTube 45%││
│               │ │ Discord …                    2.4 [Boost]│ Brave 30%  ││
│ [❚❚ Pause]    │ │ Blocked: (Epic Unblock)       │ │ [Fine-tune] [Reset]││
│ Your line ✎   │ └───────────────────────────────┘ └─────────────────┘│
│ 25↓ – ↑ Mbps  │ Steam is using the most of your line. YouTube still… │
└───────────────┴──────────────────────────────────────────────────────┘
```

- **Side rail:** the snail and the name; Now · Split · Settings; Pause (Resume with the time left while paused); the line as the user set it (opens the speed dialog). A state that needs attention (§14.8) shows as a notice at the top of whichever page is open, with its action.
- **Now:** "What matters right now?"; four mode tiles (radios); "Using your internet": who moves data, busiest first, each with its status in words and a Boost menu (1 h · 4 h · until I stop it · Block its internet for apps); blocked apps with Unblock; "The split": the mode's shares as one bar and a line each, how many take only what's free, Fine-tune (→ Split) and Reset split (with Undo); at the bottom the sentence on who uses the most and whether anyone is waiting (`ledger.ts` `glance`).
- **Split:** "{Mode} split" with Reset split; the live strip; the slices (the former Ledger rows: icon · name · Mbps and status in words · a thin bar · − % +; settings in place: Goes first, Only when the line is free, a speed limit, Remove; Everything else with its apps and "Give it a slice"); quiet "only when free" rules fold into one row that opens; Add a slice.
- **Settings:** §14.6.
- **Not built yet, so not shown:** notifications, hotkeys. (Speed test and ping readout: Settings → Connection, 2026-11-18.)

### 14.3 Changing a share

- **− / +** move a slice 5% to the next multiple of 5; the others make room in proportion, Everything else never below 5%, a rule never below 1% (`rules.ts` `stepShare`, `split.ts`). Everything else has − / + too.
- **Flyout:** changes show at once and save after 300 ms, so a few quick clicks are one save; a save waits for the previous one's revision, so they never conflict with each other (`ui.svelte.ts`).
- **Split page (2026-11-20, the user asked for a Save button):** edits go into a draft (`draft.ts`, one per window in `ui.svelte.ts`); a bar at the bottom says "Unsaved changes · Nothing changes on your line until you save · Discard · Save" (Ctrl+S saves); changed shares show "was 20%". Save is the rev-checked path; a conflict keeps the draft. If the split changes elsewhere meanwhile, the bar offers "Load the new one" or "Save mine instead". Leaving the page, switching mode, copy/rename/delete/reset ask first in-app (`LeaveDialog`: Save · Don't save · Keep editing).
- **Redesign (2026-11-20, the user found the pages confusing):** Now leads with the live sentence as its heading (then the proof row), "What matters right now?" heads the mode tiles, the split card is "How {mode} shares your line" with "X Mbps now · at least Y when busy", "Fine-tune" is now "Change the split". The Split page says what a slice is in one sentence, has column headings, plainer statuses, and each slice's settings one per line with what they do. A boost is shell-green with a ring, a "Boosted" mark and a bar that shortens as time runs out (the service keeps only the end, so the bar's full length is the shortest menu length, 1 h or 4 h, that fits). Real logos (Simple Icons, CC0, `app/src/lib/brands`, credits in `credits.json`) come before letter tiles; an exe's own icon still wins; brands without a CC0 logo (Disney+, Prime Video, Teams, Edge, Vivaldi) keep the tile; Windows' own traffic shows a monitor glyph. Not yet seen in a real window.
- **Only when the line is free** (0%) is a setting in the row, not the bottom of the − button.
- **The drag bar is back (2026-11-20, the user asked: "the slider like the website")**, reversing its removal on 2026-11-16 (dividers were hard to hit, small slices unreadable, reading it took arithmetic). It sits above the rows as the page's main control; the rows' − / + stay, two ways to the same draft. Each answer to the old problems: the handle between two neighbours trades only between them (sum kept, a rule ≥ 1%, Everything else ≥ 5%), follows the pointer and settles A on a step of 5% on release; it catches a press 32 px wide over the bar's full height (two overlapping handles: the first move picks, so a thin slice can always grow), `touch-action: pan-y`; a segment shows logo + name + %, narrower just %, narrower still nothing (its row says it); while dragging a readout over the handle says "Steam 40% · at least 8 Mbps when busy | YouTube 60% · 12" for the direction shown. Every move edits the draft; faint ticks under the bar mark where the handles were saved. "Only when free" rules aren't on it: a line under it names them. Each handle is a `role="slider"` ("Between Steam and YouTube", arrows / PageUp / PageDown / Home / End, value text naming both sides); hovering a segment or holding a handle lights its rows. The flyout keeps − / + only. Pure parts in `splitDrag.ts` (tested), drawn by `SplitBar.svelte`. Not yet seen in a real window.
- **Redesign 2, built (2026-11-20).** The user chose "Direction 1 · One column" from three real-size mockups (one column, a mixer, plain sentences; in `.impeccable/mocks/redesign/`). DESIGN.md is now the design brief, and the entries above are history. Each page answers one question and shows the line once:
  - **Now:** the sentence, the fix, the ping proof, then "Using your internet", with a thin bar of each app's part of the line.
  - **Split:** the drag bar with live use along its foot (its readout now sits under the bar). The rows hold only the numbers and settings. A mode with no slices gets an empty state with "Give {app} a slice".
  - **Mode switch:** moved to the window's top strip.
  - **Settings:** regrouped, with an index.
  - **Shared changes:** SnailNet's own drawn icons; digits in Segoe UI (no slashed zero); white logo marks; Pause shown only while something is set; a cream warmth where something changed (`flash.ts`).
  - **Removed:** the mode tiles, the split card, the live strip and the first-run split hint.

### 14.4 Tray flyout

A quick panel (380 × 500), anchored above the tray icon, closes on blur and Escape: the snail and Pause (1 h) / Resume (only while something is set, `pause.ts`; the tray icon's right-click menu always has "Pause SnailNet for 1 hour" / "Resume SnailNet", 2026-11-20); the main window's sentence (or the state that needs attention); the four modes; a small live strip; the slices with − % + (quiet "only when free" ones left out); "Open SnailNet".

Tray icon states: normal (snail), paused and safe mode / retrying (amber dot), not connected (faded). Blocked-apps counter: a dark badge top-right (1–9, then +).

The living snail (built 2026-11-20, `tray_icon.rs`): while normal, the snail shows how the line is doing. No animation; it changes only when the mood does, and the tooltip says it in words (never colour alone, §14.9).

| Mood | When | Icon | Tooltip |
|---|---|---|---|
| Calm | line not full | the snail | `SnailNet` |
| Working | full (either direction, ≥ 0.9 × shaped) for 3 s; calm after 10 s not full | cream spiral (cream for what's on) | `SnailNet · line full · ping 19 ms` |
| Struggling | working and ping ≥ 40 ms above its lowest for 5 s; back after 10 s under 25 ms, or at once without a ping | cream spiral on a coral shell | `SnailNet · ping is up 60 ms · slowing the line a little to bring it down` |

The marks win: amber dot and faded snail always sit on the calm art. Art: `icons/tray/*.svg`, rendered to 32/64 px PNGs by `scripts/tray-icons.cjs` and committed. The handoff's "stalks bent down" for struggling didn't read even at 160 px, let alone 16, so struggling is the coral shell instead. **Not yet seen** in the real tray (100 % / 150 %, light / dark taskbar).

### 14.5 First-run experience (≤ 60 seconds)

1. **Welcome:** one sentence ("Split your internet between apps, like 40% for downloads and 60% for YouTube.") plus the privacy line ("SnailNet only reads website names, never content, and nothing leaves your PC").
2. **Measure your connection:** big circular progress, then results ("▼ 100 Mbps ▲ 20 Mbps · ping under load: 140 → 18 ms with Low-latency mode"). Skip → type speeds / measure later.
3. **What matters to you?** Three cards: *Gaming*, *Streaming & calls*, *Just keep downloads polite* → picks the starting profile.
4. **Done.** The main window opens with the live list already filling in. A one-time hint on "The split" card (built 2026-11-18; − / + replaced dragging): "Give an app more with − and +. Fine-tune opens every slice. Share nobody is using goes to whoever needs it, so none is wasted." Got it or Fine-tune puts it away.

### 14.6 Settings (single scrolling page, Windows Settings style)

- **Built (2026-11-17):** Connection speed (Change → the speed dialog); Start with Windows; Reset the {mode} split; Reset SnailNet (confirm; defaults, then first run again); versions of the app and the service. The rest below arrives with its feature.

- **Connection:** measured speeds per network, Re-measure, manual override, Low-latency mode + strength (Auto / Gentle / Strong).
- **Behaviour:** Focus boost, New-app notifications (Notify / Strict / Off), LAN exempt, start with Windows (tray app; service always runs).
- **Hotkeys.** (Built 2026-11-18: the mode hotkey switch, under Windows.)
- **Sites:** manage site packs and custom domains; "Unidentified connections" explainer.
- **Updates:** auto-check, channel (stable/beta), "Check now".
- **Language.**
- **Advanced:** Safe mode, export/import config, open logs folder, diagnostics bundle (redacts nothing sensitive because nothing sensitive is stored, but asks before writing).
- **About:** version, licences (WinDivert LGPL notice), source link.

### 14.7 Microcopy rules

- Plain words: "slice", "goes first", "only when the line is free". Never "QoS class", "token bucket" or "HTB" in the UI.
- Numbers: `36.8 Mbps` (1 decimal below 100, none above); percentages as integers unless Alt-dragging.
- Errors say what happened and what to do: "Couldn't start traffic control. Another app may be using the same driver (e.g. a VPN or game booster). [Show details] [Retry]".

### 14.8 States to design (none may be left as an afterthought)

Empty (no traffic yet) · Loading/connecting to service · Service not running (with a "Start service" button that elevates) · Safe mode · Paused · Offline (no network) · Speed test failed · Unidentified site traffic · Blocked app · Very narrow window · Many apps (100+, virtualised list) · High-contrast mode.

- **Built (2026-11-18):** all but "Speed test failed" (Phase 4). Start SnailNet runs `sc start` through UAC (`service_start.rs`); Offline follows Windows' network state; unidentified traffic is a line under "Using your internet" (Mbps and which apps). Many apps: instead of a virtualised list, "Using your internet" shows the busiest five, quiet "only when free" slices fold into one row and Everything else shows its first apps with "N more".

### 14.9 Accessibility

WCAG 2.2 AA contrast on the dark ground; full keyboard operation (rows, − / + and settings are buttons and native radios); visible focus rings; a share change is announced (polite live region on the percentage), live speeds are not; respects Windows text scaling and high-contrast mode; no information by colour alone (slices carry names and icons, spare room is hatched, statuses are words).

---

## 15. Frontend implementation

- **Svelte 5 (runes) + TypeScript + Vite**, inside Tauri 2.
- **No heavy component library:** a small in-house set of Fluent-like components (Button, ToggleSwitch, SegmentedControl, NumberBox, Flyout, Tooltip, Toast, ListRow, SplitBar). Each comes with light/dark tokens.
- **Tokens:** CSS custom properties (`--surface-layer`, `--stroke-card`, `--text-primary`, `--accent` …). Accent and theme are pushed from Rust (`UISettings`) on change.
- **Charts:** hand-rolled SVG/Canvas sparklines in v1 (tiny); consider `uPlot` for v1.1 history charts.
- **Data flow:** Rust `snailnet-client` subscribes to `StatsTick` and emits a Tauri event to the webview at 4 Hz; Svelte stores hold `apps`, `rules`, `profiles`, `line`. Mutations go through typed `invoke` commands that return the new `rev`.
- **Performance:** keyed, virtualised list; `requestAnimationFrame`-batched number tweens; no layout thrash during split-bar drag (transform-only movement, commit on pointerup + debounced live preview).
- **i18n:** `src/lib/i18n/en.json` with typed keys (generated TS type), ICU-style plurals; no hard-coded strings (lint rule).
- **Windows integration via Tauri plugins:** tray (core), `single-instance`, `autostart`, `notification`, `global-shortcut`, `updater`, `window-state`.

---

## 16. Dependencies

Pin exact versions at project setup after checking current docs (Context7 / crates.io). Candidate list:

**Rust (service/core)**

| Purpose | Crate | Note |
|---|---|---|
| WinDivert bindings | `windivert` / `windivert-sys` | Verify maintenance in Phase 0; otherwise write a thin FFI over `WinDivert.dll` (small API surface) |
| Windows APIs | `windows` (windows-rs) | Process info, timers, pipes security, NLM |
| Windows service | `windows-service` | Service control and install |
| Async | `tokio` | IPC, timers, estimator, not the fast path |
| Channels / sync | `crossbeam-channel`, `arc-swap`, `parking_lot` | Fast path |
| Packet parsing | `etherparse` (or hand-rolled zero-copy) | IPv4/IPv6/TCP/UDP headers |
| DNS parsing | `hickory-proto` (or a minimal parser) | Answers only |
| QUIC Initial decrypt | `hkdf`, `sha2`, `aes-gcm`, `aes` | RFC 9001 Initial keys + header protection |
| Serialisation | `serde`, `toml`, `rmp-serde` | Config and IPC |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` | Rolling files |
| Errors | `thiserror` (libs), `anyhow` (bins) | |
| Tests | `proptest`, `cargo-fuzz` (libFuzzer), `criterion` | |

**Third-party binaries:** WinDivert 2.2.x (`WinDivert.dll`, `WinDivert64.sys`), dual LGPLv3/GPLv2. Ship it as a separate DLL with its licence and an offer of source, which satisfies LGPL even if SnailNet is closed later (D1).

**Frontend:** `svelte@5`, `vite`, `typescript`, `@tauri-apps/api@2`, Tauri plugin JS packages, `@fluentui/svg-icons` (or a curated subset).

---

## 17. Installer, driver, service, updates

### 17.1 Installer (Tauri bundler → NSIS, per-machine)

1. Requires admin once (UAC at install, never afterwards).
2. Installs to `C:\Program Files\SnailNet\`: `SnailNet.exe`, `snailnet-service.exe`, `WinDivert.dll`, `WinDivert64.sys`, licences.
3. Runs `snailnet-service.exe install`, which registers the service `SnailNet` (Automatic (Delayed Start) → or Automatic), with recovery actions *restart after 5 s / 30 s / then stop* and `SERVICE_SID_TYPE_UNRESTRICTED`.
4. Creates `C:\ProgramData\SnailNet\` with the ACLs from §13.1.
5. Registers the tray app autostart (per user, HKCU Run via the autostart plugin).
6. Starts the service, launches the app → first run.

WinDivert installs its own kernel driver service on first `WinDivertOpen` and unloads it when the last handle closes. **Uninstall:** stop the service → (WinDivert unloads) → `snailnet-service.exe uninstall` → remove files; ask whether to keep config.

- **Built (2026-11-18), decisions taken:**
  - **Release-only bundle config.** `app/src-tauri/tauri.release.conf.json` is merged in with `tauri build --config …`; it turns the bundle on (NSIS only, `perMachine` → `C:\Program Files\SnailNet\`), adds the files and the hooks. It stays out of `tauri.conf.json` because `tauri-build` copies `bundle.resources` on every cargo build, and `target\release\snailnet-service.exe` doesn't exist in a plain `cargo clippy`/`test`.
  - **Names:** `mainBinaryName: "SnailNet"` makes the installed app `SnailNet.exe` (cargo's binary stays `snailnet-app.exe`); fixed before the first release because the uninstaller and updates key on it.
  - **Files** (`bundle.resources`, landing next to the app exe, where `WinDivertLib::load` already looks): `snailnet-service.exe` (from `target\release`), `WinDivert.dll` + `WinDivert64.sys` (the pinned 2.2.2 in `third_party/windivert/x64`), `licenses\WinDivert-LICENSE.txt`.
  - **Hooks** (`app/src-tauri/windows/hooks.nsh`): *pre-install* `net stop SnailNet` (an upgrade or update must free the exe and DLL; `net` waits for the stop, `sc` doesn't); *post-install* `snailnet-service.exe install` (re-registers in place on upgrades, creates `C:\ProgramData\SnailNet` with its ACLs) then `net start SnailNet`; a failed install says so in a message box (not in silent mode) and goes on. *Pre-uninstall*: in update mode only stop; otherwise ask "Keep your SnailNet settings?" (Yes by default; silent and passive uninstalls keep them) and run `uninstall` or `uninstall --remove-data`. No new service subcommands were needed.
  - **Autostart (step 5)** stays with the app: the installer runs elevated, so HKCU would be the wrong user's; the finish page launches the app as the user and its first run turns "Start with Windows" on (`autostart.rs`). The template's uninstaller removes the Run value.
  - **One script for a release:** `scripts/build-installer.ps1` builds the service, then the app through the bundler, and writes `latest.json` next to the installer and its `.sig` in `target\release\bundle\nsis\`. `-TargetDir` builds elsewhere when a dev service locks `target\`.

### 17.2 Updates (D18)

- `tauri-plugin-updater` reads `latest.json` from **GitHub Releases**; update packages are **signed with the Tauri updater key** (minisign-style; free, not an Authenticode certificate). Keep the private key offline and back it up.
- Update flow: the app downloads → the NSIS updater stops the `SnailNet` service → replaces files → restarts the service → relaunches the app. Shaping gap: ~2–3 s, fail-open.
- Channels: stable / beta (separate manifest).
- Site packs and game packs can update **independently** (small signed JSON/TOML from the same release feed) without a full app update.
- **Built (2026-11-18), decisions taken:**
  - `tauri-plugin-updater` =2.14.1 (Windows TLS via `native-tls`, no zip), feed `https://github.com/StergiosF/SnailNet/releases/latest/download/latest.json`, Windows `installMode: passive` (a progress bar; the per-machine installer asks UAC once per update). The NSIS hooks above do the service stop/re-register/start, so an update is the installer run with `/UPDATE`.
  - **Key:** generated with `tauri signer generate`, no password; the private key is `%USERPROFILE%\.tauri\snailnet-updater.key` on the dev PC, never in the repo; only the public key is in `tauri.conf.json`. Back it up offline: losing it strands installed copies on their version.
  - **App side:** Rust commands `check_update` / `install_update` (`app/src-tauri/src/updates.rs`, no JS plugin package or capability needed); Settings → About → **Updates** (`UpdateRow.svelte`): "Check for updates" → "SnailNet x.y is out" → Install. Since v0.0.3 a quiet check runs on start and every six hours (`update.svelte.ts`); a found version shows as an Update button in the side rail. Nothing installs without the click.
  - **Pipeline:** `.github/workflows/release.yml` runs the same script on a `v*` tag (or by hand) with the secret `TAURI_SIGNING_PRIVATE_KEY` and puts installer, `.sig` and `latest.json` on a **draft** release; publishing the draft is the go-live (`releases/latest` skips drafts).
  - **Open:** the repository is private, so the feed URL answers 404 to the app until the repo (or a separate public releases repo) is public. Channels (beta) are not built.
- **Pack feed, built (2026-11-18), decisions taken:**
  - **What:** one bundle `packs.toml` carries the three packs as literal strings (`sites`, `families`, `games`), plus `format = 1` and `serial` (UTC `yyyyMMddHHmm` of the publish). The existing pack parsers read them unchanged; a pack missing from the bundle stays the compiled-in one; unknown keys are ignored so a later format-1 bundle may carry more. The game folders moved from code into `assets/games.toml` (`snailnet-core::games::GamePack`) so they can update too.
  - **Signature:** minisign, with the **updater key** (one key to keep safe; the public key compiled into `snailnet-core::packs::PUBLIC_KEY` is the decoded `tauri.conf.json` pubkey). Signed with `tauri signer sign`; the base64 wrapper is decoded to a plain `.minisig`. Checked with `minisign-verify` =0.3.0 (pure Rust, no dependencies, already in the tree through the updater plugin), prehashed signatures only. 1 MB cap.
  - **Newest wins, never back:** a bundle is used only if its serial is above the compiled-in serial (`assets/packs-serial.txt`, set by the publish script) and the one in use. So a new build never runs older lists than it shipped with.
  - **Host:** the rolling GitHub release `packs` (a pre-release, never "latest", so `releases/latest/download/latest.json` stays the app's), `…/releases/download/packs/packs.toml` + `.minisig`. `scripts/publish-packs.ps1` builds, signs, checks (with the real key, through an opt-in core test) and with `-Upload` puts both there.
  - **Fetch:** the service (`snailnet-service::packs`) over WinHTTP (`Https::get`), 2 minutes after start, then daily; 3 h after a failure; a 404 (nothing published, or the repo still private) waits a day. Saved in `%ProgramData%\SnailNet` (checked again on every start, before the config loads). New packs restart shaping once (classifier and site matcher are rebuilt) and send `ConfigChanged`, so the app refetches the site list. The app reads the saved bundle itself (checked) for the game folders in `focus.rs`, once a minute when the file changed.
  - **Setting:** `[settings] pack_updates` (on by default); Settings → About → "Keep website and game lists up to date". Off: no requests; a saved bundle is still used. README's privacy list names this connection.
  - **Site rules and lost packs:** "is this a known site" moved from `Config::validate` (load) to `Config::check_sites` (every change through the hub; the current packs plus the compiled-in ones plus custom sites). A saved rule for a site the packs no longer have matches nothing instead of making the whole config invalid.
  - **Not done:** nothing uploaded yet (the repo is private); publishing is a manual step, not part of `release.yml`. The first bundle was built and checked locally (serial 202611181326, which `assets/packs-serial.txt` now holds).

### 17.3 Code signing (deferred, D1)

Without Authenticode, SmartScreen will show "Windows protected your PC" on first install. That's acceptable for personal use. Before going public: Azure Trusted Signing or an OV/EV certificate; sign `SnailNet.exe`, `snailnet-service.exe` and the installer.

- **Notes (2026-11-18), still deferred:** the updater signature (§17.2) only proves an update came from us; it does nothing for SmartScreen. When a certificate exists: Tauri signs the app exe and the installer through `bundle.windows.signCommand` (Azure Trusted Signing: `trusted-signing-cli`; a certificate: `signtool sign /fd sha256 /tr <timestamp> …`) or `certificateThumbprint`; `snailnet-service.exe` is a resource, not built by Tauri, so `scripts/build-installer.ps1` must sign it with the same command between its cargo build and `tauri build`. In CI the certificate goes in as a secret (Trusted Signing: Azure credentials). `WinDivert.dll` stays as upstream ships it (unsigned); `WinDivert64.sys` keeps its own upstream signature.

---

## 18. Safety, security, and compatibility

### 18.1 Never break the internet

- Fail-open by design (§6.4). Watchdog. Safe mode after a crash loop.
- **Pause** is always one click away (tray).
- `Win + R → services.msc → stop SnailNet` restores normal networking; documented in the README "Help, my internet is broken" section.

### 18.2 Least privilege

- The service runs as LocalSystem (WinDivert needs admin rights). Mitigations: minimal attack surface (IPC is the only input), strict pipe ACL, client verification, bounded message sizes, every IPC input validated in `snailnet-core` (`share` sums, ranges).
- **Parsers are the risky part** (they read attacker-controlled bytes from the internet): bounds-checked Rust only, no `unsafe` in parsers, **continuous fuzzing** of DNS/SNI/QUIC parsers, and per-packet time limits.
- `unsafe` is confined to `snailnet-engine` and `snailnet-win` FFI modules, each block with a `// SAFETY:` justification.

### 18.3 Anti-cheat and antivirus

- Some anti-cheat systems and AVs are suspicious of packet drivers like WinDivert (it's also used by cheat tools and censorship-circumvention tools). **Test in Phase 0** with the games you play (e.g. Riot Vanguard, EAC, BattlEye, FACEIT titles).
- Mitigation: "**Game-safe mode**": SnailNet fully closes all WinDivert handles (the driver unloads) while a listed game runs, keeping only non-driver features. If a specific anti-cheat objects, this is the switch.
- Windows Memory Integrity (HVCI) on: verify WinDivert loads in Phase 0 (it is widely used on Win11, but confirm on your machine).

### 18.4 VPNs and other drivers

- With a VPN, traffic appears twice: inner packets on the virtual adapter (with app PIDs) and outer encrypted packets from the VPN process. **Shape inner, exempt outer**: recognise the VPN client's family (and WireGuard/OpenVPN/IKE ports) and exclude its flows automatically.
- Coexistence with other WinDivert users (GoodbyeDPI, other limiters): handle priorities documented; a diagnostic shows "another packet filter is active".

### 18.5 Privacy

No telemetry in v1. Update checks are the only automatic network calls (plus the speed test, with consent). All data stays local.

---

## 19. Testing strategy

### 19.1 Core (deterministic, runs anywhere)

- **Scheduler simulation** (`snailnet-sim`): traffic generators (saturating TCP-like with AIMD response, CBR UDP game ticks, bursty web, QUIC-like), a simulated clock, and assertions:
  - 40/60 split within ±3% over 2 s when both saturate.
  - Work-conserving redistribution in proportion to weights.
  - Ceil respected within one burst.
  - High-priority light flow: p99 queueing delay ≤ 2 ms at 92% load.
  - No starvation of Low classes; Everything-else floor respected.
  - Live weight changes: no packet loss, convergence within 500 ms.
- **Property tests** (`proptest`): share normalisation always sums to 10000; random rule edits never violate floors; deficits stay bounded.
- **Parser fuzzing** (`cargo-fuzz`): DNS, TLS ClientHello, QUIC Initial; plus a corpus of real captures (pcaps from your own browsing, committed without payloads).
- **Golden tests:** QUIC Initial decryption against RFC 9001 Appendix A test vectors.

### 19.2 Engine and service (Windows)

- `MockEngine` integration tests: rules → classify → schedule → outputs.
- Real-machine test scripts (PowerShell + `iperf3` against a LAN box or a VPS):
  - Two `iperf3` processes renamed `a.exe` / `b.exe` with rules 40/60 → measure.
  - Download + game-like UDP ping test → measure ping under load with and without Low-latency mode.
  - The motivating scenario: **Steam download + YouTube 4K in Chrome** → YouTube never buffers; Steam stays near its share; closing YouTube → Steam recovers within 2 s.
- Soak test: 24 h with rules active; memory flat, no handle leaks, no flow-table growth.
- Failure drills: kill the service mid-download → internet keeps working; corrupt config → loads `.bak`; driver busy → clear error.

### 19.3 UI

- Component tests (Vitest + Testing Library) for SplitBar maths and keyboard behaviour.
- Contract tests: TS IPC types generated from Rust (`ts-rs` or `specta`) so the UI can't drift from the service.
- Visual/manual review per the frontend workflow (impeccable audit); browser automation only when explicitly requested.

### 19.4 CI

GitHub Actions on `windows-latest`: `cargo fmt --check`, `clippy -D warnings`, `cargo test` (core + mocks), `cargo deny` (licences/advisories), frontend lint/typecheck/test, Tauri build artifact. Nightly: short fuzz runs.

---

## 20. Performance budget

| Metric | Target |
|---|---|
| Added latency, uncongested | < 1 ms p50, < 3 ms p99 |
| Ping under load with Low-latency mode | within +10 ms of idle ping |
| Throughput supported | ≥ 500 Mbps on a mid-range 4-core CPU; ≥ 1 Gbps stretch |
| Service CPU | < 3% of one core at 300 Mbps; ~0% idle |
| Service memory | < 60 MB with 5 000 active flows |
| UI memory | < 150 MB (WebView2) while open; tray-only < 40 MB (webview destroyed when hidden) |
| Rule change → effect | < 100 ms |
| Cold start (service ready) | < 1 s after network stack is up |

**Measured 2026-11-18** (release build, `run --console` elevated with a scratch data folder and pipe; 25/40 Mbps line typed, low-latency on; two curl copies `a.exe=40` / `b.exe=60` from Cloudflare; ICMP isn't diverted, so pings show the ISP buffer and TCP connect times show the shaper's own delay):

| Metric | Result | Verdict |
|---|---|---|
| Cold start | pipe answering 77–223 ms after launch | met |
| Service CPU | idle (monitor only) 0%; shaping a quiet line 0.6% of one core; 7.8% of one core at ~11 Mbps in the first run. **Profiled the same day** (CPU per named thread, user vs kernel): ~90% is **kernel** time in the scheduler and rx threads, i.e. WinDivert's receive and reinjection (which runs part of Windows' own TCP/IP work on our thread). SnailNet's own code was ~1.2% of one core. Kernel time was largely per *call*: the scheduler sent ~2.3 packets per call. Fix: no timer spin, naps ≥ 1 ms (§6.5) → ~4 packets per call, scheduler kernel time per packet 28.8 → 14.9 µs, whole service ~47 → ~29 µs per packet (9% of one core at ~3 100 pkts/s) | **not met, and not reachable on WinDivert**: what's left is per-packet reinjection, ~25k pkts/s at 300 Mbps would cost most of a core. Fine for the 25–100 Mbps lines v1 is for; the 300 Mbps budget moves to the own WFP callout driver (§21 "Later"). A whole-PC comparison (same download with and without) couldn't separate it: the PC was ~50% busy across 12 cores from other work during the runs |
| Service memory | 36 MB working set, 28 MB private, 199 handles (a few hundred flows, not 5 000) | met at this size |
| Rule change → effect | IPC round trip 10 ms, "applied to the running shaper" (no restart); the split moving wasn't captured | likely met; not shown end to end |
| Added latency, uncongested | TCP connect min 10.3 → 10.8 ms with shaping | inconclusive (the line's own jitter was far larger) |
| Ping under load | inconclusive: the line was erratic that hour (idle ping avg 79 ms, max 229, before SnailNet ran; downloads 2–21 Mbps), so the shaper was never saturated and the split read ~50/50 | redo on a calm line |
| Throughput ≥ 500 Mbps | not testable on a 25 Mbps line | open |
| UI memory | not measured | open |

---

## 21. Roadmap and milestones

Estimates assume one developer working with Claude Code, part-time-to-steady pace; adjust after Phase 0.

### Phase 0: Spike: prove the hard part (1–2 weeks)

**Goal:** answer "can WinDivert + our scheduler hold a game download to 40% while YouTube plays smoothly?" before any UI exists.

- Workspace skeleton; `snailnet-core::scheduler` minimal (root bucket + DRR, 2 classes).
- `snailnet-cli spike --app steam.exe=40 --app chrome.exe=60 --down 100` as admin.
- FLOW/SOCKET PID mapping; NETWORK divert and reinject; Ctrl+C = fail-open.
- **Checks:** WinDivert loads with HVCI on; your games' anti-cheat tolerates it; measured split within ±5%; ping under load improves.
- **Exit:** a short `docs/phase0-report.md` with numbers. **Go/No-go** for the WinDivert path.

### Phase 1: Engine, scheduler, service (3–4 weeks)

- Full scheduler (priority ordering, ceilings, FQ in Everything else, CoDel/ECN, express lane), simulator, and the test suite from §19.1.
- Flow table, app families (packs + fallbacks + svchost services), PID-reuse safety.
- Windows service (install/uninstall/console mode), config load/save/migrate, logs, watchdog, safe mode.
- IPC server + protocol v1; `snailnet-cli` as the first client.
- **Exit:** rules editable via the CLI, live stats via `snailnet-cli top`, 24 h soak passes.

### Phase 2: Website recognition (2 weeks)

- DNS sniffer, TLS SNI, QUIC Initial SNI, IP→domain cache, site packs, custom domains, unidentified accounting.
- Fuzzing in CI.
- **Exit:** "YouTube 45%" works in Chrome, Edge and Firefox with secure DNS on and off.

### Phase 3: The app (4–5 weeks)

- Tauri shell, custom title bar, Mica, theme/accent bridge, tokens and base components.
- Main window: header, **split bar**, app list with live speeds, rule editor panel.
- Tray icon + flyout; first-run flow; settings page; all states from §14.8.
- i18n scaffolding; accessibility pass; impeccable audit.
- **Exit:** the motivating scenario is solvable with mouse only, in under 15 seconds, by someone who has never seen the app.

### Phase 4: Smart (2–3 weeks)

- Speed test; passive estimator; per-network capacity; Low-latency mode with auto-tune and ping readout.
- Focus boost: foreground hook, game detection, overrides, optional auto-profile.
- **Exit:** ping under load within +10 ms on your line; games get boosted automatically.

### Phase 5: Control (2 weeks)

- Profiles + presets + hotkey; block per app; new-app notifications (notify + strict); family split/merge UI.
- **Exit:** all D9 features complete.

### Phase 6: Ship v1.0 (1–2 weeks)

- NSIS installer with service + driver, uninstall, updater + GitHub Releases pipeline, signed site/game pack feed (**built 2026-11-18**, §17.2; nothing published yet).
- README with the "help, my internet broke" section, licences, privacy statement.
  - **Done (2026-11-18):** `README.md` (what it is, four escalating ways out: Pause, stop the service, uninstall, restart; privacy: what is read, kept, and the only connections made, checked against the code: updates only on click, speed test only on Measure, pings to the router and 1.1.1.1 while shaping, pinned or not; licences). **Third-party notices:** `scripts/third-party-notices.ps1` follows the normal dependencies of `snailnet-app` and `snailnet-service` for Windows x64 (`cargo metadata`) plus the two bundled npm packages, and writes each one's licence and notice files (authors as the copyright line where a crate ships none) to `target\THIRD-PARTY-NOTICES.txt`; `build-installer.ps1` runs it after `npm ci` and the installer ships it as `licenses\THIRD-PARTY-NOTICES.txt` next to WinDivert's. Not committed (~3 MB, 324 packages, changes with every bump). WinDivert: LGPLv3, unmodified, loaded at run time, source link in both files.
  - **Licence (user's choice, 2026-11-18): GPL-3.0-or-later.** `LICENSE` (the canonical GNU text), `license` in the workspace `Cargo.toml` and `app/package.json`, shipped as `licenses\SnailNet-LICENSE.txt`. Everything bundled is compatible (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, MPL-2.0 unmodified, LGPLv3 WinDivert, OFL font). The repo was made public the same day (user's choice) for the update and pack feeds.
- Final perf run against §20; bug bash.
- **Exit:** a clean install on a fresh Windows 10 and a fresh Windows 11 VM works end-to-end; tag `v1.0.0`.

### v1.1: Insight & automation

- Usage history (SQLite; per app/site per hour, rolled up daily/monthly), history charts.
- Schedules ("torrents 80% after 1 am").
- Monthly data caps with warnings and auto-throttle; metered-network awareness.

### Later

- Own WFP callout driver (`WfpEngine`) for > 1 Gbps and ARM64.
- Browser extension for perfect per-tab attribution (ECH-proof).
- Public release: code signing, website, possibly paid "Pro" tier.

---

## 22. Risks and mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Download shaping less precise than upload | High | Medium | Root rate below line (Low-latency), CoDel/ECN, honest UI wording; validated in Phase 0 |
| Anti-cheat flags WinDivert | Medium | High (for gamers) | Test early; Game-safe mode unloads the driver; documented compatibility list |
| ECH/DoH hide website names | Growing | Medium | Multi-signal fallback, visible "unidentified" count, future browser extension |
| AV flags WinDivert as riskware | Medium | Medium | Ship the official signed build unmodified; explain in README; code-sign SnailNet itself before going public |
| PID attribution races (first packets) | Medium | Low | SOCKET-layer early mapping, TCP/UDP table fallback, 50 ms pending window |
| Timer granularity hurts latency | Low–Med | Medium | High-res waitable timers + short spin; measure in Phase 0 |
| CPU cost at high speeds | Low (home lines) | Medium | Batched recv/send, zero-alloc fast path; WFP driver later |
| Service as LocalSystem = high-value target | Low | High | Tiny IPC surface, ACLs, fuzzed parsers, no network listener |
| WinDivert project goes stale | Low | Medium | `PacketEngine` trait; WFP path planned |
| Scope creep | High | Medium | v1 scope locked by D9; everything else into §23 |

---

## 23. Idea backlog

Not committed. Kept so good ideas don't get lost.

- **"Why is my internet slow?" button:** one click shows who's using the line right now and offers a one-tap fix ("Give YouTube a slice?"). *Built as "the one-tap fix" (§11.11, 2026-11-20); live check pending.*
- **Bufferbloat grade (A–F)** in the speed test, before/after SnailNet. Very shareable. *Planned as "the proof", docs/HANDOFF-wow.md feature 1, with a live "ping with and without SnailNet" row.*
- **Living tray snail:** the tray icon shows calm / working / struggling from the line and the ping. *Planned, docs/HANDOFF-wow.md feature 3.*
- **Per-app ping:** show each game's live RTT to its server (from TCP timestamps / UDP request-response pairing).
- **Smart suggestions:** "Steam has been using 95% for 20 min while you watched YouTube, so make a 40/60 split?" *The waiting case is built as the one-tap fix (§11.11).*
- **Auto-pause for calls:** when Teams/Discord/Zoom has an active call, temporarily lower downloaders. *Partly built as §11.9 (the call goes first; downloaders aren't lowered, since going first already keeps the call clear without shrinking anyone's slice).*
- **Explorer context menu:** right-click an .exe → "Limit with SnailNet…".
- **Stream Deck / CLI / local REST** for power users to switch profiles.
- **Share-a-profile** as a small file/link.
- **LAN mode** (much later): run on a home server or router to shape the whole household.

---

## 24. Glossary

| Term | Meaning |
|---|---|
| Share / slice | The guaranteed % of the line a rule gets when it wants it |
| Ceiling | Optional hard maximum, even when the line is free |
| Priority | Who goes first in the queue; lowers latency, doesn't increase share |
| Work-conserving | Unused share is given to others instead of wasted |
| Everything else | The single slice shared by all apps without their own rule |
| Low-latency mode | Shaping slightly below line speed so the router buffer never fills |
| WinDivert | Signed Windows packet-interception driver + library SnailNet builds on |
| FLOW / SOCKET / NETWORK layer | WinDivert layers: PID info / socket events & blocking / actual packets |
| DRR | Deficit Round Robin, the fair-sharing algorithm behind slices |
| CoDel | Controlled Delay, a queue manager that keeps delay low by dropping/marking early |
| ECN | Explicit Congestion Notification: tell the sender to slow down without dropping |
| SNI | The website name a browser sends when opening a secure connection |
| ECH | Encrypted Client Hello: hides the SNI |
| QUIC | UDP-based transport used by YouTube/HTTP3 |

---

## Immediate next steps

1. `cargo new` the workspace per §5; add `third_party/windivert` (pinned 2.2.x).
2. Build the **Phase 0 spike** and write `docs/phase0-report.md` (split accuracy, ping under load, HVCI and anti-cheat results).
3. Go/No-go review of this plan's D4 based on that report, then start Phase 1.
