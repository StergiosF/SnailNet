# Handoff: SnailNet, next steps

Start a fresh session with: **"read docs/HANDOFF.md and continue"**.

## Where things stand (2026-11-14)

- **Plan:** `PLAN.md` is the source of truth (decisions D1–D20, roadmap §21).
- **Git:** everything is on **`main`**. The user works only on `main`: no
  feature branches. `main` is 3 commits ahead of `origin/main` (not pushed;
  push only when asked).
- **Phase 0 is mostly done.** Results are in `docs/phase0-report.md`:
  - `snailnet check` (elevated): the WinDivert 2.2 driver loads **with HVCI
    on**, Defender is silent, 28/28 flows attributed, IPv4/IPv6 FLOW address
    decoding confirmed.
  - Synthetic `spike` (two curl copies `a.exe=40` / `b.exe=60`,
    `--down 25`): held **38/57/5 within ±2%**, recovered in ~2–4 s when one
    stopped, attributed 100% of packets, failed open cleanly.
  - Ping under load: 61 → 25 ms avg (234 → 54 max). **Not yet** at the §20
    target (≤ idle + 10 ms ≈ 21 ms).

## Next steps, in order

### 1. Finish Phase 0 (needs the user at the machine)

| Task | How | Done when |
|---|---|---|
| Real scenario | Elevated: `.\target\release\snailnet.exe spike --down 25 --app steam.exe,steamwebhelper.exe=40 --app brave.exe=60`, then a Steam download + YouTube in Brave | split within ±5%, YouTube doesn't buffer |
| Lower factor | Same run with `--factor 0.85` plus `ping -t 1.1.1.1` | loaded ping recorded; is it ≤ 21 ms? |
| Anti-cheat | Run `spike` (or `check`) while launching the user's games | per-game result in the report |
| CPU | Task Manager → `snailnet.exe` during the real run | % recorded |
| Upload | `--up <Mbps>` after measuring upload | split recorded |
| **Go/No-go** | Fill in the report's Decision line; record the outcome in PLAN.md's D4 row | decision written |

You can run the elevated steps yourself (see "Running elevated" below), but
the real-app and anti-cheat runs need the user to drive the apps.

### 2. Phase 1: engine, scheduler, service (PLAN.md §21, 3–4 weeks)

Suggested order: each step stays testable on its own, and the pure-Rust
ones come first.

1. **Scheduler, full version** (`snailnet-core::scheduler`, §9): priority
   ordering with the deficit cap (`burst_c`), per-class ceilings (token
   bucket), per-flow fair queues inside "Everything else" (weights 4:2:1),
   CoDel (5 ms / 100 ms) with ECN marking, express lane, live weight swap
   (`arc-swap`) with no packet loss. Tests from §19.1 + `proptest`
   (shares sum to 10000, floors hold, deficits bounded).
   **Start here:** it's pure Rust, needs no admin, and is the most likely
   fix for the ping gap.
2. **`snailnet-sim` crate** (§5, §19.1): traffic models (AIMD TCP, CBR
   game UDP, bursty web, QUIC-like) on `SimClock`; asserts p99 queueing delay
   ≤ 2 ms for a light High-priority flow at 92% load, plus convergence
   < 500 ms after a weight change.
3. **Fast path** (§4, §6.3): slab buffer pool (no per-packet `Vec`), 1–2 rx
   threads, scheduler thread, watchdog that closes the NETWORK handle if the
   scheduler stalls > 500 ms.
4. **Classification** (§6.3, §7): flow table keyed by (pid, process
   creation time), SOCKET-layer early mapping, 50 ms pending window, app
   families (packs in `assets/`, same-folder+publisher fallback, parent chain
   for generic hosts, `svchost -s` → "Windows Update").
5. **Windows service** (`snailnet-service`, §4, §17): `windows-service`
   crate, `install` / `uninstall` / `run --console`; config TOML in
   `C:\ProgramData\SnailNet\` with atomic writes + `.bak` (§13); `tracing`
   logs with 7-day rotation; safe mode after 3 crashes in 10 minutes.
6. **IPC** (`snailnet-ipc`, §12): named pipe `\\.\pipe\snailnet.v1` with
   the ACL, length-prefixed MessagePack, protocol v1 messages; make
   `snailnet-cli` a client (`status`, `rules`, `top`).

**Phase 1 exit:** rules editable via the CLI, live stats via
`snailnet top`, 24 h soak passes (flat memory, no handle leaks).

Then Phase 2 (website recognition), Phase 3 (Tauri UI), as in PLAN.md §21.

## What exists

| Crate | Role | Tests |
|---|---|---|
| `snailnet-core` | `Clock` (real + sim), root token bucket + weighted DRR scheduler, IPv4/IPv6/TCP/UDP parser. No OS deps. | 23 |
| `snailnet-engine` | `PacketEngine` trait, `WinDivertLib`/`WinDivert` (runtime-loaded FFI via `libloading`), `Address` (80-byte `WINDIVERT_ADDRESS`), batch buffers | 6 |
| `snailnet-win` | exe name by PID, PID-by-local-port snapshot (IP Helper), high-res waitable timer | 4 |
| `snailnet-cli` (bin `snailnet`) | `check` (sniff-only driver/PID test), `spike` (shaping) | 6 |

WinDivert 2.2.2 is pinned in `third_party/windivert/` (hashes and signature
notes in its README). The DLL is found via `$SNAILNET_WINDIVERT_DIR`, then
the exe's folder, then that repo path.

## Verify

```powershell
cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo test --workspace
cargo build --release
```

## Running elevated

This shell isn't elevated. Write a `.ps1` in the scratchpad that runs the
command and redirects output to a file, then run
`Start-Process powershell -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '-NoProfile -ExecutionPolicy Bypass -File <script>'`
and read the file. The user clicks Yes on the UAC prompt. Tell them first.

Synthetic test recipe: copy `C:\Windows\System32\curl.exe` to `a.exe` and
`b.exe`, download `https://speed.cloudflare.com/__down?bytes=25000000`
(100 MB returns 403; use one stream per process, because parallel streams
stall) under `spike --app a.exe=40 --app b.exe=60 --duration 32`, and ping
1.1.1.1 alongside. Worth adding to the repo as `scripts/phase0-synthetic.ps1`.

## Gotchas

- **The WinDivert driver stays loaded** after a run (`sc query WinDivert` →
  RUNNING), which locks `third_party/windivert/x64/WinDivert64.sys`. Git
  can't rewrite that file until it's unloaded: `sc stop WinDivert`
  (elevated) or reboot. Avoid checkouts that touch it.
- Phase 0 shortcuts, which Phase 1 replaces: no priority, ceilings or
  CoDel/ECN; only DNS and pure TCP control packets bypass the queues; tail
  drop at ~50 ms of share; a `Vec` allocated per packet; no SOCKET layer.
- "Everything else" keeps a 5% floor, so 40/60 rules shape as 38.1/57.1/4.8.
- The driver's signing cert (a Chengdu company, the one upstream used for
  2.2.2) has expired; its timestamp keeps it valid, but some AVs block it.
- The user's browser is **Brave**. The line is about 25 Mbps down; upload
  not measured yet.

## Working agreements

- Work and commit directly on `main`; commit when asked; push only when asked.
- No browser automation unless explicitly asked.
- Library or API docs: look them up with Context7 first.
- Ask interview-style questions through the options window
  (AskUserQuestion), recommended option first.
- A **GateGuard** hook blocks the first Write of each new file, and some
  shell commands, until you state who calls the file, what data it touches,
  and the user's verbatim instruction (or, for destructive commands, which
  files change and how to roll back). State that, then retry the identical
  call.
