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
| D11 | % input | **Split bar** with draggable dividers, always 100%, click a segment to type a number | Clearest picture of the whole line |
| D12 | Main screen | Live app list + split bar; **tray flyout** for quick actions | One screen does everything |
| D13 | Style | **Windows 11 native** (Mica, Fluent, system accent, light/dark) | Calm and premium, feels like part of Windows |
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

- Scheduler wake-ups use `CreateWaitableTimerExW(CREATE_WAITABLE_TIMER_HIGH_RESOLUTION)` (Win10 1803+) with ~0.5–1 ms granularity, plus short spin-waits when the next send is < 200 µs away. No global `timeBeginPeriod`.
- Packets are released in micro-batches to amortise syscalls.

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

- Endpoints: configurable list; default a public, no-account HTTP speed-test service with download/upload endpoints. Run with 4–8 parallel connections, take the 90th-percentile throughput of the last 6 s.
- The test flows are excluded from shaping.
- **Offline / blocked:** fall back to the passive estimate (§10.2) and ask the user to type the speed if they want precision now.
- The result is stored per network (see 10.3).

### 10.2 Adaptive estimate (passive, always on)

Continuous, cheap, no extra traffic:

- Track the **peak sustained throughput** (1 s windows, decaying max over 7 days) per direction.
- Track **latency under load**: a tiny RTT probe (ICMP or TCP SYN to the default gateway and to one public anycast host) at 1 Hz *only while the line is busy*.
- If throughput repeatedly exceeds the estimate without a latency increase, raise the estimate. If latency rises sharply at a throughput *below* the estimate (line got slower: Wi-Fi, peak hours), lower the shaped rate. This is in the spirit of `cake-autorate`.
- Bounded change rate (≤ 10% per minute up, faster down) so it never oscillates.

### 10.3 Per-network profiles

Capacity is stored per network identity (`NLM` network GUID / Wi-Fi SSID + gateway MAC). Home Wi-Fi, phone hotspot and office keep separate speeds. A metered connection (Windows "metered" flag) is shown in the UI.

### 10.4 Low-latency mode (D5, Smart)

- **On by default.** The root shaper runs at `factor × capacity`, default 0.92 down / 0.90 up. The bottleneck queue then lives in SnailNet (with CoDel) instead of the router's bloated buffer.
- **Auto-tune:** under load, if the RTT increase stays above +15 ms, lower the factor by 1% (floor 0.80); if it stays below +5 ms for 5 min, raise it by 1% (ceiling 0.97).
- **UI:** one toggle, "Low-latency mode: keeps your ping low while downloading. Costs ~5–10% of top speed." Plus a small **"Ping under load"** readout ("Ping 18 ms · without SnailNet ~140 ms", measured during the speed test).
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

- A profile = a complete rule set + Low-latency setting.
- Shipped presets: **Balanced** (no rules, Low-latency on), **Gaming** (games High, launchers/updaters Low, Windows Update ceiling 20%), **Streaming** (site packs for video Normal, downloads Low), **Downloads** (everything to downloaders, Low-latency off).
- Switch via the tray flyout, the main window header, or a global hotkey (default `Ctrl+Alt+S` cycles; configurable).
- "Duplicate & edit" to make your own. Profiles are stored in config; switching is instant and atomic.

### 11.3 Block per app (Control)

- A toggle on each app row ("Online / Blocked").
- Implementation: SOCKET layer blocks `connect`/`bind`/`listen` for that family (clean, the app gets a normal network error) **and** NETWORK layer drops packets of already-open flows.
- Blocked apps show a muted row with a lock glyph; the tray shows a counter when ≥ 1 app is blocked.

### 11.4 New-app notification (Control)

- The first time an unknown app family goes online, a Windows toast: **"Fortnite went online"**, with buttons *Fine* · *Give it a slice* · *Block*.
- **Default mode: notify-only** (the app is allowed immediately and joins Everything else). This never stalls a launch or update.
- **Strict mode** (setting): hold the app's new connections (SOCKET layer blocks with retry semantics; NETWORK layer holds the SYN up to 20 s) until answered; on timeout, apply the default action.
- Quiet hours: suppress toasts while fullscreen (games) and batch them into one summary toast afterwards.
- Windows' own components (signed by Microsoft, in System32) never prompt.

### 11.5 Focus boost (Smart)

- **What:** the app in the foreground gets **High priority** temporarily (never a bigger share). Games (detected) get High for their whole session, even when alt-tabbed.
- **Foreground tracking (UI process):** `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` → PID → family → IPC `focus_changed`. Debounced 300 ms so alt-tabbing doesn't churn.
- **Game detection** (any one is enough; confidence shown in UI):
  1. `SHQueryUserNotificationState` = `QUNS_RUNNING_D3D_FULL_SCREEN`, or the foreground window covers its monitor (borderless fullscreen) **and** the process loads `d3d11.dll`/`d3d12.dll`/`vulkan-1.dll`.
  2. The exe lives under a known launcher library (`steamapps\common`, Epic `\Epic Games\`, EA, Ubisoft, Battle.net, Riot, GOG, Xbox `XboxGames`) from **game packs**.
  3. The user marked it "This is a game".
- **Overrides:** per app "Never boost" / "Always High".
- **Optional automation:** "Switch to Gaming profile while a game runs" (off by default, one checkbox).

### 11.6 App families (Control)

As specified in §7.2. In the UI, an expandable row shows member processes; "Split out" / "Merge into…" actions.

### 11.7 Live view (core)

- The app list shows ↓/↑ current speed per family/site (1 s smoothed, UI updates at 4 Hz), connection count, and a 60 s micro-sparkline on hover (no persistent graphs in v1).
- The header shows total ↓/↑, capacity, Low-latency status and live ping.
- Sorting: by current speed (default), name, share. Idle apps fade after 30 s and move to an "Idle" group after 5 min.

### 11.8 Pause

- Tray flyout and main header: **Pause for 15 min / 1 h / until I resume**. Shaping off, monitoring on. Pause state is obvious (amber tray glyph).

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

> Design workflow (from the global rules): use `design-taste-frontend`'s `references/app-screens.md` to set the screen direction (this is an app screen, not a landing page), then build and audit with `impeccable` before calling UI work done. The decisions in this plan (D11–D13) are the brief.

### 14.1 Visual language: Windows 11 native

- **Material:** Mica window background (Tauri `windowEffects: ["mica"]`; Windows 10 fallback: Acrylic, else a solid layer colour). Content sits on subtle layer cards (`1px` stroke, 8 px radius), as in Windows Settings.
- **Type:** Segoe UI Variable (Display for the 28 px title, Text for body); 14 px body, 12 px caption; tabular numerals for all speeds.
- **Colour:** neutral Fluent greys; **system accent colour** (read via `UISettings` in the Rust side → CSS variable) for the active profile, focused states and the user's own slices. Each split-bar slice gets a calm, distinguishable hue from a fixed palette that passes contrast in both themes; app icons sit on the slice.
- **Theme:** follows the system light/dark live; manual override in settings.
- **Icons:** Fluent System Icons (MIT), 16/20 px.
- **Motion:** 150–250 ms ease-out for layout, numbers tween over 300 ms, split-bar segments spring gently while dragging. Respect `prefers-reduced-motion`. Nothing loops or pulses except the live-speed digits.
- **Mascot:** a tiny snail glyph in the tray and on empty states only. Never in the way.

### 14.2 Window structure

Main window, default 960 × 640, min 720 × 480, custom title bar (Tauri `decorations: false` with Windows snap layouts preserved via the caption-button area).

```
┌─────────────────────────────────────────────────────────────────────┐
│ 🐌 SnailNet     [ Gaming ▾ ]          ▼ 42.1 / 100   ▲ 3.2 / 20 Mbps │
│                                       ● Low latency  · ping 18 ms  ⏸ │
├─────────────────────────────────────────────────────────────────────┤
│  DOWNLOAD ⇄ UPLOAD (linked)                                          │
│  ┌──────────────────────┬─────────────────────────┬───────────────┐  │
│  │ ◼ Steam   40%        │ ▶ YouTube   45%         │ Everything 15%│  │
│  └──────────────────────┴─────────────────────────┴───────────────┘  │
│                                                                      │
│  ACTIVE                                                     Sort ▾   │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │ [ic] Steam              ▼ 36.8  ▲ 0.4   40% · Low     ●Online ▸│  │
│  │ [ic] Chrome ▸ YouTube   ▼ 4.9   ▲ 0.2   45% · Normal  ●Online ▸│  │
│  │ [ic] Fortnite  🎮 boost ▼ 0.3   ▲ 0.1   Everything · High   ▸  │  │
│  │ [ic] Discord            ▼ 0.1   ▲ 0.1   Everything · Normal ▸  │  │
│  └────────────────────────────────────────────────────────────────┘  │
│  IDLE (6) ▸                                                          │
│                                                     ⚙ Settings      │
└─────────────────────────────────────────────────────────────────────┘
```

- **Header:** profile switcher, live totals vs capacity, Low-latency pill (click → explanation popover + toggle), live ping, Pause.
- **Split bar:** see 14.3. The "linked" chip toggles between one shared bar and two stacked bars (Download / Upload).
- **App list:** one row per family or site. Row anatomy: icon · name (+ badges: 🎮 Game, ⚡ Boosted, 🔒 Blocked) · ↓/↑ speed (tabular, 1 decimal) · slice chip (`40% · Low` or `Everything · Normal`) · Online/Blocked toggle · chevron.
- **Row expand / side panel** (the rule editor): share (number field synced to the bar), ceiling (None / % / Mbps per direction), priority (segmented control High · Normal · Low), "This is a game" / "Never boost", separate up/down, members (processes or domains), "Split out / Merge", delete rule.

### 14.3 The split bar (signature component)

- A full-width bar, 44 px high; each segment shows icon + name + % (collapsing to icon + % below 80 px, then a tooltip only).
- **Drag a divider** → the two neighbours trade share (1% steps; hold `Shift` for 5%; `Alt` for 0.1%). Live Mbps hint under the cursor ("≈ 36.8 Mbps").
- **Click a segment** → inline number input; typing 40 rebalances the *other* segments proportionally, never Everything else below 5%.
- **Drag a segment** onto another to reorder (purely visual).
- **Drop an app row onto the bar** → creates a rule with a sensible starting slice (taken from Everything else first).
- **Live fill:** each segment shows a subtle inner fill for actual current usage vs its slice, so you *see* sharing happen (YouTube using 25% of its 45%, with Steam's fill visibly extending into the unused part as a hatched overflow).
- **Keyboard:** Tab focuses dividers; ←/→ moves by 1% (Shift 5%); Enter on a segment opens the number input; screen-reader labels like "Divider between Steam 40 percent and YouTube 45 percent".

### 14.4 Tray flyout

A Windows 11-style quick panel (360 × ~420), anchored above the tray icon, Mica background, closes on blur:

```
┌──────────────────────────────┐
│ SnailNet        ▼ 42 ▲ 3 Mbps │
│ ● Low latency · ping 18 ms    │
│ [Balanced][Gaming][Streaming] │  ← profile pills
│ Top now                       │
│  Steam      ▼ 36.8   40%      │
│  YouTube    ▼  4.9   45%      │
│  Fortnite   ▼  0.3   ⚡ High  │
│ [⏸ Pause 15 min]  [Open ↗]    │
└──────────────────────────────┘
```

Tray icon states: normal (snail), paused (amber), blocked apps present (small dot), safe mode (warning).

### 14.5 First-run experience (≤ 60 seconds)

1. **Welcome:** one sentence ("Split your internet between apps, like 40% for downloads and 60% for YouTube.") plus the privacy line ("SnailNet only reads website names, never content, and nothing leaves your PC").
2. **Measure your connection:** big circular progress, then results ("▼ 100 Mbps ▲ 20 Mbps · ping under load: 140 → 18 ms with Low-latency mode"). Skip → type speeds / measure later.
3. **What matters to you?** Three cards: *Gaming*, *Streaming & calls*, *Just keep downloads polite* → picks the starting profile.
4. **Done.** The main window opens with the live list already filling in. A one-time coach mark on the split bar: "Drag to share. Unused share is never wasted."

### 14.6 Settings (single scrolling page, Windows Settings style)

- **Connection:** measured speeds per network, Re-measure, manual override, Low-latency mode + strength (Auto / Gentle / Strong).
- **Behaviour:** Focus boost, New-app notifications (Notify / Strict / Off), LAN exempt, start with Windows (tray app; service always runs).
- **Hotkeys.**
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

### 14.9 Accessibility

WCAG 2.2 AA contrast in both themes; full keyboard operation; visible focus rings (Fluent style); live speed regions throttled for screen readers (announce on request, not 4× per second); respects Windows text scaling; no information by colour alone (slices also carry labels/icons).

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

### 17.2 Updates (D18)

- `tauri-plugin-updater` reads `latest.json` from **GitHub Releases**; update packages are **signed with the Tauri updater key** (minisign-style; free, not an Authenticode certificate). Keep the private key offline and back it up.
- Update flow: the app downloads → the NSIS updater stops the `SnailNet` service → replaces files → restarts the service → relaunches the app. Shaping gap: ~2–3 s, fail-open.
- Channels: stable / beta (separate manifest).
- Site packs and game packs can update **independently** (small signed JSON/TOML from the same release feed) without a full app update.

### 17.3 Code signing (deferred, D1)

Without Authenticode, SmartScreen will show "Windows protected your PC" on first install. That's acceptable for personal use. Before going public: Azure Trusted Signing or an OV/EV certificate; sign `SnailNet.exe`, `snailnet-service.exe` and the installer.

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

- NSIS installer with service + driver, uninstall, updater + GitHub Releases pipeline, signed site/game pack feed.
- README with the "help, my internet broke" section, licences, privacy statement.
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

- **"Why is my internet slow?" button:** one click shows who's using the line right now and offers a one-tap fix ("Give YouTube a slice?").
- **Bufferbloat grade (A–F)** in the speed test, before/after SnailNet. Very shareable.
- **Per-app ping:** show each game's live RTT to its server (from TCP timestamps / UDP request-response pairing).
- **Smart suggestions:** "Steam has been using 95% for 20 min while you watched YouTube, so make a 40/60 split?"
- **Auto-pause for calls:** when Teams/Discord/Zoom has an active call, temporarily lower downloaders.
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
