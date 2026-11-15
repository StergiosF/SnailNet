# Handoff: SnailNet, next steps

Start a fresh session with: **"read docs/HANDOFF.md and continue"**.

## Where things stand (2026-11-15)

- **Plan:** `PLAN.md` is the source of truth (decisions D1–D20, roadmap §21).
- **Git:** everything is on **`main`**. The user works only on `main`: no
  feature branches. `main` is in sync with `origin/main` (pushed 2026-11-14;
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
| Real scenario | Elevated: `.\target\release\snailnet.exe spike --down 25 --app steam.exe,steamwebhelper.exe=40:low --app brave.exe=60`, then a Steam download + YouTube in Brave | split within ±5%, YouTube doesn't buffer |
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

1. ~~**Scheduler, full version**~~ **Done (2026-11-14, commit 19fc7af,
   pushed).** `snailnet-core::scheduler` now has priority-ordered
   DRR with a burst cap (idle classes bank ≤ 30 ms of credit, which is
   what lets a light High flow go first), per-class ceilings, fq_codel-style
   per-flow queues in every class (4:2:1 weights, new-flow list), CoDel
   5 ms/100 ms with ECN (`parse::ip::set_ce`), the express lane (credit
   charged on entry, so it can't dodge a share), `shares` (basis points,
   5% floor, proportional removal), and `LiveConfig` (`ArcSwap`) +
   `Scheduler::sync`/`reconfigure` (classes matched by name; removed rules'
   packets move to Everything else; nothing lost). 58 core tests, including
   proptests for share lists and random op sequences (deficits bounded,
   no packet leaked or duplicated). `spike` uses it: flow hashes, ECN, and
   `--app exe=40:low` / `:high` priority suffixes.
2. ~~**`snailnet-sim` crate**~~ **Done (2026-11-14, commit 5d19690,
   pushed).** `crates/snailnet-sim`: closed-loop traffic models (Reno AIMD TCP
   with optional ECN, paced QUIC-like with 0.7 backoff, CBR game/ping, web
   transfers with slow start and think times) on `SimClock`, behind a
   modelled ISP link (line rate, 150 ms FIFO buffer). Every drop is
   signalled to its sender; each run checks `sent = delivered + lost + in
   network`. `tests/plan.rs` proves the §19.1 list: 40/60 within ±3% over
   every 2 s window with RTTs 15–120 ms; idle share redistributed by
   weight; ceiling within one burst; light High flow p99 ≤ 2 ms in the
   scheduler at 92% load (also with a 1 ms timer, also as a High app in
   Everything else); Low class and the 5% floor hold; live weight change
   and rule removal lose nothing and converge within 500 ms; runs are
   repeatable.

   Findings worth knowing:
   - A class whose only sender is **one Reno flow** can't fill its share
     (its window drops below the BDP after each halving; CoDel keeps ~5 ms
     queued). The slack goes to other classes, so 40/60 reads ~44/56 with
     one flow each; with 4 flows per class it's exact. Pinned by
     `a_lone_reno_flow_underfills_its_share_and_the_rest_is_not_wasted`.
     100 ms windows swing with the TCP sawtooth; judge splits over ≥ 500 ms.
   - `cargo run --release -p snailnet-sim` sweeps the shaping factor for
     the Phase 0 scenario (25 Mbps, Steam 4×TCP + YouTube QUIC + browsing,
     ping). At 1.00 the ISP buffer fills (ping +131 ms, split 59/41, wrong
     way round). At 0.92: split exact, ping +1.4 ms p50 but **+28 ms p99**;
     the scheduler's part is ≤ 0.6 ms. The tail is **slow-start bursts of
     new connections** (each ack releases two packets, about twice the
     shaped rate) overflowing the ISP buffer before the shaper sees them.
     Without the browsing flow, p99 is ~3 ms. Lower factors shrink the tail
     (0.85: +11.5 ms; 0.80: +6.8 ms p99). Compare with the real
     `--factor 0.85` run in step 1.
3. ~~**Fast path**~~ **Done (2026-11-14).** In `snailnet-engine`:
   `pool` (two shelves of pre-allocated buffers, 8192 × 2 KiB + 64 × 64 KiB;
   a `PacketBuf` returns itself on drop; empty shelf → the packet passes
   straight through instead of allocating), `fastpath` (`FastPath::start`
   over any `PacketEngine`: 1–2 `rx` threads → bounded channel → one
   `scheduler` thread per process with a scheduler per shaped direction →
   `SendEx`; a `watchdog` that fails open when the scheduler's heartbeat is
   older than 500 ms), and `mock::MockEngine`. Fail-open is
   `WinDivertShutdown(RECV)`: per the WinDivert docs, new packets stop
   being queued (so they bypass us) while already-queued ones still drain,
   and the rx threads forward those unshaped. On stop or fail-open the
   scheduler flushes its queues at full speed, so nothing we hold is lost.
   Pure TCP control packets now take the core's express lane (`control`)
   instead of bypassing. `spike` runs on it (`--rx-threads 1|2`; it exits
   with "failed open: …" if the watchdog fired). 7 real-thread tests in
   `crates/snailnet-engine/tests/fastpath.rs` (rate, 40/60 split, ACK
   jumps the queue, LAN/unshaped bypass, stop flushes, stalled scheduler
   fails open and loses nothing, empty pool passes through), stable over
   15 repeated runs.

   Real-driver run (2026-11-14, `scripts/phase0-synthetic.ps1`, 25 Mbps,
   factor 0.92, a.exe=40 / b.exe=60, one curl stream each): no fail-open,
   clean stop, 99–100% attributed by FLOW, ~0–3 passthrough pkts/s.
   - Split while both ran: 41–45 / 55–59 (target 40/60 after "everything
     else" goes idle), so within ±5%. a gets a little extra because one Reno
     flow can't fill b's share, the same as `snailnet-sim` found. Mid-run
     (14–21 s) it drifted to ~48/52, and b went to 0 from 22 s for the rest
     of the run (its curl stopped, cause unknown: maybe Cloudflare limiting
     repeat requests). a then took the whole line, 20–23 Mbps.
   - Ping 1.1.1.1 (ICMP isn't diverted, so this measures the ISP buffer):
     idle 10 ms avg; under load avg 38, max 221, 1 of 22 lost. Most samples
     are 9–46 ms; the outliers (timeout, 221, 74, 53) line up with
     connections ending or restarting, which are the slow-start bursts the
     simulator predicted. Next: the same run with `-Factor 0.85`, and
     longer downloads so connections don't churn.
4. ~~**Classification**~~ **Done (2026-11-14).** `snailnet-core::classify`:
   `flow` (FlowTable: FLOW events > SOCKET connect/accept > SOCKET
   bind/listen by local port > 1 s port snapshot; SOCKET entries leave on
   close, FLOW on delete; capped), `family` (Resolver over a
   `ProcessSource`: pack `assets/families.toml` → `svchost -s` → generic
   host's nearest real ancestor (parent must be older than the child) →
   Store package → same folder + same publisher (never inside Windows or
   `steamapps\common`) → the exe alone; cached by (pid, creation time), so
   a reused PID is resolved afresh), `Attribution` (resolves on the event
   threads, interns (family, exe) as `AppId`; rx threads do one read-locked
   lookup) and `Pending` (per rx thread, 50 ms window, bounded).
   `snailnet-win::process::WinProcesses` supplies creation time, parent,
   command line (svchost only), package family, embedded-signature
   publisher and FileDescription. The CLI got `events.rs` (flow-sniffer,
   socket-sniffer, port-table threads); `--app` takes families
   (`--app steam=40:low --app brave=60`) or exes; `check` prints each
   flow's family. Real-driver run (synthetic, 25 Mbps, 0.92): SOCKET layer
   opens fine; 100% of packets attributed (0% pending/unknown); split
   39–42 / 58–61 while both ran (b's curl stopped at ~21 s again); ping
   idle 10 ms, loaded avg 22 / max 47, 0 lost (one sample; the last run
   was 38/221 — less connection churn this time, not a classify effect).
5. ~~**Windows service**~~ **Done (2026-11-14); smoke-tested as a real
   service 2026-11-15** (`scripts/service-smoke.ps1`, elevated): install
   sets recovery 5 s / 30 s restart, 24 h reset, non-crash flag TRUE, SID
   unrestricted; defaults → monitor only; `down_mbps = 25` → "shaping down
   at 23.0 Mbps"; clean stop, no `running` marker; ACL SYSTEM/Admins full,
   Users read; uninstall `--remove-data` removes both. The first run found
   that install opened the service without `SERVICE_START`, which
   `ChangeServiceConfig2` needs for restart actions (Access denied, and the
   steps after it were skipped); fixed. `crates/snailnet-service` (bin `snailnet-service`):
   `install` (LocalSystem, auto start, recovery restart 5 s / 30 s / none,
   also on non-crash failure; re-running updates it) / `uninstall
   [--remove-data]` / `run --console [--data-dir]` / hidden `service`.
   `store`: `config.toml` in `%ProgramData%\SnailNet` (ACL: SYSTEM + Admins
   full, Users read, `snailnet-win::fs::protect_dir`), saved via tmp +
   fsync + `ReplaceFileW` (old file → `.bak`); load falls back to `.bak`,
   then defaults; a broken file is kept as `.bad`; a newer-version file is
   left alone. `snailnet-core::rules`: the config model (profiles, rules
   with family/exe targets, share bp, priority, ceilings, line speed,
   low-latency factors) → `Plan` (scheduler classes). `crashguard`:
   `running` marker + `crashes.txt`; 3 crashes or fail-opens in 10 min →
   Safe mode (no handles). `runner`: load → guard → shape → on fail-open
   count it, wait 10 s, retry; no line speed → monitor only. `logging`:
   `logs\service.YYYY-MM-DD.log`, 7 kept, `SNAILNET_LOG` filter; a panic
   logs then aborts. `shaper` (moved out of the CLI's `events.rs`):
   sniffers + classifier + fast path; `spike` and `check` now use it.
   19 tests (store, crash guard, runner with a scripted fake engine).
6. ~~**IPC**~~ **Done (2026-11-14).** `crates/snailnet-ipc`: protocol v1
   types (`ClientMsg{id, Request}`, `ServerMsg::{Reply, Stats, Event}`,
   `ServiceState`, `Snapshot{rev, state, config, apps}`, `StatsTick`),
   framing (4-byte LE length + MessagePack via `rmp_serde::to_vec_named`,
   so structs are maps; 1 MB cap checked before allocating), and a
   blocking `Client` (Hello on connect, replies matched by id, stats and
   events queued). Service: `hub` (config + `rev` + state + latest stats +
   apps; every change checks `rev` → `Conflict`, validates → `Invalid`,
   saves atomically, bumps `rev`, wakes the runner, broadcasts
   `ConfigChanged`) and `ipc` (tokio current-thread runtime on its own
   thread; keeps one instance listening; first instance uses
   `FILE_FLAG_FIRST_PIPE_INSTANCE`; remote clients rejected; ≤ 16
   clients; 5 s write timeout; Hello first, protocol 1 only).
   `PIPE_SDDL`: deny NETWORK; SYSTEM, Admins and Owner Rights full;
   interactive users `0x12019b` (read/write data, **no** create-instance,
   so they can't add a fake server instance). The owner ACE is needed:
   without it the creator can't add a second instance (the tests found
   this). `SetLine` is an addition to §12.2 (no estimator yet).
   `SetBlocked`/`FocusChanged`/`Pause`/`RunSpeedTest`/`AnswerPrompt` exist
   but answer `Unsupported`.
   Runner: a config change **restarts shaping** with the new plan (flushes
   queued packets; not counted as a failure); no line speed → waits for a
   change, so `snailnet line --down 25` starts shaping. Safe mode ignores
   changes until a restart. States are published to the hub.
   CLI (no admin needed): `status`, `apps`, `rules [--profile P] [--clear]
   steam=40:low game.exe=10`, `profile [ID]`, `line [--down] [--up]
   [--factor] [--clear]`, `top [--hz] [--count]`; hidden `--pipe` to talk
   to `snailnet-service run --console --pipe NAME`.
   Verified end to end without admin (console service on a scratch dir
   and a test pipe): status, rules saved (shown as 38/57/5), overcommit
   refused, line set → shaping attempted → fail-open counted → state
   "retrying" visible in `status` and `top`. **Verified against the real
   service while shaping** (2026-11-15, in the smoke test): `status` shows
   "shaping", line 25 Mbps, 21 apps seen; `top` streams ticks. That run
   showed 26 Mbps against a 23 Mbps cap: the hub divided the engine's
   counters (published every 100 ms) by its own 250 ms poll gap, so each
   tick could be off by ±40%. Snapshots now carry `taken: Instant` and the
   hub and `spike` compute rates between those stamps (a re-read snapshot
   changes nothing). Still to do: `snailnet top` during a real download.

   Deferred, worth doing before Phase 1 exit:
   - **Live rule swap without restart** (§9.6): the scheduler has
     `LiveConfig`, but `FastPath` takes fixed configs and `AppClassifier`
     caches app → class. A restart flushes, so nothing is lost, but
     attribution state is rebuilt (the port snapshot covers open flows).
   - ~~**Per-app stats**~~ **Done (2026-11-15).** `Classifier::classify`
     returns `(PacketMeta, Tag)`; the service's tag is the `AppId`
     (`NO_TAG` when unknown). The scheduler thread counts bytes *sent* per
     tag (dequeue and the stop flush), published as `DirectionStats::tags`.
     The hub maps tags to apps (`apps()` is in `AppId` order) and sends
     `DirStats::families` (`FamilyTick { family, name, bps }`, busiest
     first, idle ones omitted, unknown → `unknown`/"Unknown").
     `snailnet top` prints the top 5 under each direction. Not yet seen
     with real traffic.
   - **Client verification** (§12.1: client PID → exe path/signature),
     and the CLI checking that the server is the service: not done; the
     DACL is the only gate. The pipe is byte mode, not message mode (the
     framing makes them equivalent).
   - The hub saves `config.toml` on the IPC thread (blocking, but tiny).
7. **Phase 1 exit:** the deferred items above, then the 24 h soak (flat
   memory, no handle leaks; the 60 s log line prints the sizes).

**Phase 1 exit:** rules editable via the CLI (done), live stats via
`snailnet top` (done, per class), 24 h soak passes (flat memory, no
handle leaks).

Then Phase 2 (website recognition), Phase 3 (Tauri UI), as in PLAN.md §21.

## What exists

| Crate | Role | Tests |
|---|---|---|
| `snailnet-core` | `Clock` (real + sim), full §9 scheduler (priority DRR, ceilings, FQ + CoDel/ECN, express lane, live reconfig, `shares`), IPv4/IPv6/TCP/UDP parser + ECN marking, `classify` (flow table, app families, pending window), `rules` (config model → scheduler plan). No OS deps. | 93 |
| `snailnet-engine` | `PacketEngine` trait, `WinDivertLib`/`WinDivert` (runtime-loaded FFI via `libloading`), `Address` (80-byte `WINDIVERT_ADDRESS`), batch buffers, `pool` (pre-allocated packet buffers), `fastpath` (rx / scheduler / watchdog threads), `MockEngine` | 10 + 7 |
| `snailnet-win` | process facts (`WinProcesses`: path, creation time, parent, command line, package, publisher, description), PID-by-local-port snapshot (IP Helper), high-res waitable timer, `fs` (atomic replace with backup, data-folder ACL, `SecurityAttributes` from SDDL) | 11 |
| `snailnet-ipc` | protocol v1 types, framing, blocking `Client`, pipe SDDL | 5 |
| `snailnet-service` (bin `snailnet-service`) | Windows service: install/uninstall, config store, crash guard + Safe mode, logs, supervisor, `hub` + `ipc` (pipe server), `shaper` (FLOW/SOCKET/port threads + classifier + fast path) | 34 |
| `snailnet-cli` (bin `snailnet`) | IPC client (`status`, `apps`, `rules`, `profile`, `line`, `top`); Phase 0 `check` (sniff-only driver/PID/family test) and `spike` (shaping via `snailnet_service::shaper`) | 6 |
| `snailnet-sim` (bin `snailnet-sim`) | closed-loop traffic models + ISP link on `SimClock`; §19.1 property tests; shaping-factor sweep | 5 + 10 |

`assets/families.toml` is the app-family pack (compiled into core with
`include_str!`; its tests check it parses).

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
1.1.1.1 alongside. That recipe is now `scripts/phase0-synthetic.ps1`
(`-OutDir`, `-Down`, `-Factor`, `-Duration`); it needs a release build.

## Gotchas

- Output of `snailnet top` captured through Windows PowerShell 5.1 shows
  `↓` as `Γåô` (OEM code page); a real console shows it fine.

- **The WinDivert driver stays loaded** after a run (`sc query WinDivert` →
  RUNNING), which locks `third_party/windivert/x64/WinDivert64.sys`. Git
  can't rewrite that file until it's unloaded: `sc stop WinDivert`
  (elevated) or reboot. Avoid checkouts that touch it.
- `spike` still excludes DNS by the filter rather than sending it express
  (Phase 2 sniffs DNS). The Phase 0 numbers in the report were measured with the *old*
  scheduler and rx loop (tail drop, no CoDel, ACKs bypassed); re-run the
  synthetic test to see what CoDel and the express lane do to the ping gap.
- With `--rx-threads 2`, two receive batches can overtake each other, so
  packets of one flow may be reordered. Keep 1 unless rx is the bottleneck.
- "Everything else" keeps a 5% floor, so 40/60 rules now shape as exactly
  38/57/5 (`shares::normalize`; the old scheduler gave 38.1/57.1/4.8).
- Only `WinDivert64.sys` carries an embedded signature; `WinDivert.dll`
  is unsigned (the publisher test reads the `.sys`).
- Family resolution order matters for folder grouping: a pack app
  "teaches" its folder + publisher, so an unlisted signed helper seen
  *before* its main exe gets its own folder family instead. Rare; fine
  until the UI's split/merge (§11.6).
- The driver's signing cert (a Chengdu company, the one upstream used for
  2.2.2) has expired; its timestamp keeps it valid, but some AVs block it.
- The user's browser is **Brave**. The line is about 25 Mbps down; upload
  not measured yet.

## Working agreements

- Work and commit directly on `main`; commit when asked; push only when asked.
- No browser automation unless explicitly asked.
- Shell gotchas: Git Bash mangles a `\\.\pipe\…` argument on its way to
  an exe, so drive pipe tests from PowerShell; and PowerShell variable
  names are case-insensitive (`$p` and `$P` are the same variable).
- Library or API docs: look them up with Context7 first.
- Ask interview-style questions through the options window
  (AskUserQuestion), recommended option first.
- A **GateGuard** hook blocks the first Write of each new file, and some
  shell commands, until you state who calls the file, what data it touches,
  and the user's verbatim instruction (or, for destructive commands, which
  files change and how to roll back). State that, then retry the identical
  call.
