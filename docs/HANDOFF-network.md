# Handoff: no internet at startup, and game detection (2026-11-23)

Commit `0fed51e` on `main`. This file explains what was wrong, what changed
and how to check it, so a second agent can review the change on its own.
It has not been released: the version is still 0.0.9.

## The two reports

1. **No internet at startup.** With SnailNet set to start with Windows,
   Windows couldn't connect to the internet. Quitting the app (which stops
   `snailnet-service` over IPC) brought the internet back at once.
2. **CS2 not detected.** A friend's SnailNet didn't treat Counter-Strike 2
   as a game.

## What was found (and what was not)

**The exact cause of report 1 was not reproduced or proven.** Evidence
looked at:

- `C:\ProgramData\SnailNet\logs\service.*.log` and the Windows System
  event log. The PC almost never fully restarts. It uses Modern Standby
  (sleep and wake), so "starting with Windows" is usually *waking up*. The
  service keeps running across sleep with its WinDivert handles open.
- The one real boot (2026-11-20 01:13 local, 2026-11-19 23:13 UTC in the
  log) shows shaping working normally from the first second.
- 2026-11-22 04:40 UTC: 0.0.9 was installed over 0.0.8, shaping started,
  the user paused about 7 s later and quit about 30 s after that. That fits
  "no internet", but the log has no error that explains it.

So the fix closes every way found by reading the code in which the service
could keep traffic from getting through while it runs, especially at boot
and on wake:

| # | Weak spot | Why it matches the report |
|---|---|---|
| A | The service started diverting before Windows had a network (adapter, Wi-Fi, DHCP still coming up at boot or on wake). | It ran in the window where the PC was trying to get online. |
| B | Nothing restarted shaping after a wake or a network change. Handles, queues and attribution carried on across sleep and route changes. | Stale state on wake. Quitting (a fresh start) fixed it. |
| C | **A failed WinDivert send was only counted. Its packets were lost, and the fast path never failed open.** If Windows kept refusing reinjected packets (for example after an adapter reset), every diverted packet was dropped for as long as the service ran. | Exactly "no internet until the service stops". The watchdog only watches the scheduler's heartbeat, which keeps beating, so it never fired. |
| D | DHCP, DHCPv6 and NTP went through WinDivert. They were classed as LAN and bypassed, but they were still captured and reinjected. | Address assignment and the clock matter most at boot and on wake. |
| E | The service had no dependencies and could race the Base Filtering Engine and TCP/IP at boot. | Boot ordering. |

## What changed

### Network (snailnet-service, snailnet-engine)

- **A: wait for a route before diverting.**
  `crates/snailnet-service/src/runner.rs`: `wait_for_route` (about line 635)
  is called before every `engine.start` (line 421), only when
  `timing.watch_network` (that is off in tests). If a route already exists
  it returns at once. Otherwise it polls `snailnet_win::network::route()`
  every 1 s for up to `ROUTE_WAIT` (60 s), then waits `ROUTE_SETTLE` (5 s).
  If there's still no route after 60 s it shapes anyway. State shows
  `Starting` while it waits.
- **B: restart on a route change or a wake.**
  `crates/snailnet-service/src/netwatch.rs` calls `hub.route_changed()`
  when the route differs from the last check (including back online after
  being offline), or when wall time jumped more than 5 s + `WOKE_AFTER`
  (30 s) between its 5 s checks. A jump like that means the PC slept. The
  hub (`hub.rs`: `route_changed` / `take_route_change`, lines 199-206) sets
  a flag and requests a reload. In the runner's shaping loop,
  `Wake::Reload if hub.take_route_change()` (line 451) breaks out with
  `Outcome::Reload`. That is a restart, not a failure, so it doesn't count
  toward Safe mode. The flag is cleared just before each start (line 425).
- **C: fail open on repeated send failures.**
  `crates/snailnet-engine/src/fastpath.rs`: `Shared::send` counts failed
  send calls in a row (`send_fail_streak`). Any success resets it. At
  `SEND_FAIL_LIMIT` (64) it calls `fail_open` with "N sends in a row
  failed (...)". The runner then retries in 10 s, and 3 strikes in 10
  minutes is Safe mode, as before.
  The mock has `MockEngine::fail_sends(bool)`, and the test is
  `sends_that_keep_failing_fail_open` in
  `crates/snailnet-engine/tests/fastpath.rs`.
- **D: the filter skips DHCP, DHCPv6 and NTP.**
  `crates/snailnet-service/src/shaper.rs` `network_filter` (about line 791)
  adds
  `(tcp or (localPort != 67 and localPort != 68 and localPort != 546 and localPort != 547 and remotePort != 123))`.
  Note that WinDivert rejects `!( … )`, which the first draft used. The
  test `the_network_filter_compiles_and_leaves_dhcp_and_ntp_alone`
  compiles all four filter variants with `WinDivertHelperCompileFilter`.
- **E: service dependencies.** `crates/snailnet-service/src/install.rs`
  sets the dependencies to `BFE`, `Tcpip` and `nsi`. The installer
  (`app/src-tauri/windows/hooks.nsh`, line 79) runs
  `snailnet-service.exe install` on every install and update, so existing
  installs are re-registered.

### Game detection

- **Pack:** `assets/game-exes.toml` lists known game exes (lowercase
  file names, about 50 popular games including `cs2.exe`).
  `assets/games.toml` adds the libraries `\rockstar games\`,
  `\amazon games\library\`, `\itch\apps\`, `\wargaming.net\games\` and
  `\games\`, plus the Rockstar launcher and Social Club folders as
  launchers. `crates/snailnet-core/src/games.rs`: `GamePack::from_parts`
  joins the two; `is_game_path` matches the exe name first, then the
  libraries. The signed bundle carries the exes as their own `game_exes`
  key (see the review below for why they are not in `games`).
- **Service:** `hub.rs` `focus_changed` (line 742). If the app didn't flag
  the window in front as a game, the service checks the exe itself
  (`image_path` plus `in_game_library`) and treats it as
  `GameWhy::Library`.
- **App:** `app/src-tauri/src/focus.rs` `game_why` (line 144). Besides the
  library check and exclusive D3D fullscreen, there's a new borderless
  fullscreen test. It requires all of these:
  - the exe isn't in `NOT_GAMES` (browsers, video players, the desktop,
    PowerPoint and similar);
  - the window in front has no title bar and covers its monitor, within
    8 px (`foreground_covers_screen`, `covers`), and isn't
    Progman/WorkerW/Shell_TrayWnd;
  - the process has d3d9/d3d11/d3d12/vulkan-1/opengl32 loaded
    (`draws_3d`). If Windows won't let us look (anti-cheat), that counts as
    yes.

  This adds the `Win32_System_Diagnostics_ToolHelp` feature in
  `app/src-tauri/Cargo.toml`.

## How to verify

Code checks (already passing at `0fed51e`):

```
cargo fmt --all --check
cargo clippy --workspace --all-targets      # no lints
cargo test --workspace                      # all pass
cargo test -p snailnet-engine --test fastpath sends_that_keep_failing
cargo test -p snailnet-service --lib the_network_filter
cargo test -p snailnet-core games
cargo test -p snailnet-app focus
```

Things a reviewer should look at closely:

1. **Can a route-change restart count as a failure?** On a reload the
   runner stops shaping, and the flush may send to an adapter that's gone.
   If that hits 64 failed send *calls* in a row, `stop()` returns a
   failure, and `(Outcome::Reload, Some(f))` counts it as a strike. That
   is unlikely, because a flush is a handful of batched calls, but check
   it.
2. **Wake detection** depends on wall time jumping between netwatch
   rounds. A forward clock change of more than 30 s (NTP, the user) also
   restarts shaping. That's harmless, but it is a restart.
3. **`route()` is IPv4 only** (`GetBestRoute` to a probe address). An
   IPv6-only network waits the full 60 s, then shapes.
4. **`\games\`** is broad: any exe under a folder named `Games` counts as
   a game. A false positive only means that app goes first. It never gets
   a bigger share.
5. **Borderless fullscreen** can also flag a fullscreen non-game that
   isn't in `NOT_GAMES` and loads a 3D runtime. The impact is the same as
   in 4.
6. **The root cause wasn't proven.** C is the strongest match. Confirm or
   rule it out on a real machine (below).

Not checked yet (needs a real PC, admin rights and a release):

- A cold boot and a sleep/wake with "Start with Windows" on. The service
  log should show `no route to the internet yet; waiting…`, then
  `network is up; shaping in 5 s`, and after a wake
  `network: the PC woke up; restarting shaping`.
- `sc qc SnailNet` after updating should list `BFE Tcpip nsi` under
  DEPENDENCIES.
- If the internet ever drops while shaping, the log should now show
  `… sends in a row failed …` followed by a fail-open, not a silent
  outage.
- CS2 in front, both windowed and borderless: the app should show it as a
  game going first.
- No version bump or release was made. Users only get this from a new
  release (0.0.10).

## Review (2026-11-23, second agent)

Checked `0fed51e` against the code, the tests (fmt, clippy, all tests
pass) and this PC's logs (`C:\ProgramData\SnailNet\logs`) and event log.

### What holds

- A–E do what they say. `install` uses `change_config` on an existing
  service, so the new dependencies do reach updated installs.
- Question 1 (a reload counted as a strike): it can't happen through the
  flush. `shutdown()` is `WINDIVERT_SHUTDOWN_RECV` only, so sends keep
  working while stopping.

### What the logs say about the cause

- **C never happened on this PC.** All 794 one-minute reports from
  2026-11-17 to 11-21 say `send errors 0`. C is a sound safety net, but
  it isn't shown to be the cause.
- **On 2026-11-22 the service was paused 7 s after starting, and stayed
  paused 23 s before the quit.** A pause closes every WinDivert handle. If
  the internet was still down while paused, diverting wasn't the cause.
  Ask the user whether it came back on Pause or only on Quit.
- **The 0.0.5 service died 4 times on 2026-11-20** (System log 7031/7034
  at 00:37:56, 01:03:33, 01:21:06 and 02:55:10 UTC; SCM restarted it
  after 5 s and 30 s, then gave up). There is no `panic:` line, no WER
  report and no Application error, so it was probably killed during dev
  work, not a crash. Not explained; watch for it.
- In 0.0.5 autorate shaped a ~26 Mbps line down to 4.7–6 Mbps (11-19,
  11-20). That is already fixed by the floor in `f374868` (0.0.6).

### Fixed in this review

1. **Blocking "Windows services" or "System" cut Windows off.** Any
   family could be blocked from the ••• menu. "Windows services" holds
   the DHCP client, the DNS client and NlaSvc (Windows' connectivity
   check), so the refuser refused their internet connects and Windows
   said "No internet" until SnailNet stopped. Pause or Quit fixed it. That
   is the reported symptom; this PC's config blocks nothing, so it wasn't
   the cause here, but it could be for others. Now `rules::can_block`
   drops those two from the plan (a config that lists them stays valid),
   and the menu no longer offers Block for them.
2. **The next pack publish would have frozen pack updates for every
   install up to 0.0.9.** One signed bundle carries all packs, and a
   bundle is used only if every pack in it parses. `GamePack` is
   `deny_unknown_fields`, so a `games` text with `exes` fails on 0.0.9
   and the whole bundle (sites, app families too) is dropped. The exes
   now live in `assets/game-exes.toml`, the bundle carries them as
   `game_exes` (older builds ignore unknown bundle keys), and
   `publish-packs.ps1` writes that key. A test parses `games.toml` the
   way 0.0.9 does.
3. **A stale plan after the route wait.** The plan was made before the
   wait of up to 65 s, so a pause or new settings from meanwhile were
   applied only after shaping had started. Now the runner plans again
   after waiting.
4. `NOT_GAMES` gains terminals, editors, call, photo and PDF apps.
   Borderless fullscreen false positives matter more than the doc said:
   a detected game stays first **for as long as the process runs**, in
   the background too, not only while in front.

### Still open

- **Wake detection may not fire on Modern Standby.** Services keep
  running through it, so the netwatch thread may never see a wall-clock
  jump. A reliable signal is `SERVICE_ACCEPT_POWEREVENT`
  (`PBT_APMRESUMEAUTOMATIC`) or `RegisterSuspendResumeNotification`. The
  route-change path still covers a wake that drops Wi-Fi.
- **`route()` is `None` for an on-link default route** (full-tunnel
  WireGuard/wintun VPNs, PPPoE on the PC), not only IPv6-only. Each
  service start then waits the full 60 s, and netwatch sees "offline".
- After a route-change restart, `wait_for_route` returns at once (the
  route is already there), so the 5 s settle applies only at boot.
- `cod.exe` and `aces.exe` are generic names; any program called that
  counts as a game.

## Follow-up on "Still open" (2026-11-23, third pass)

Each item above, in order. fmt, clippy and all tests pass.

- **Wake signal.** The service now accepts power events
  (`ServiceControlAccept::POWER_EVENT`, `svc.rs`). On
  `PBT_APMRESUMEAUTOMATIC` it logs `power: the PC woke up; restarting
  shaping` and calls `hub.route_changed()`, the same path netwatch uses.
  The clock-jump check in netwatch stays as a fallback. If both fire for
  one wake, the second is usually absorbed: the flag is cleared just
  before each start, and the restart now waits 5 s first (below). Whether
  Windows sends the resume event on *this* PC's Modern Standby wakes is
  not confirmed. Look for the log line after a wake.
- **On-link routes.** `network::route()` now reports a route with no next
  hop (or the probe itself as next hop) as online:
  `Route.gateway` is `Option<Ipv4Addr>`, `None` for on-link. So a
  full-tunnel VPN or PPPoE on the PC no longer makes every start wait
  60 s, and netwatch sees it as online. `identify` returns `None` for such
  a route, so netwatch doesn't switch networks: the line stays the last
  network's, as it did before when these looked offline. Turning such a
  VPN on or off is a route change and restarts shaping.
- **Settle after a route-change restart.** When a session ends on a route
  change or a wake, the next start waits `ROUTE_SETTLE` (5 s) even though
  the route is already there (`network changed; shaping again in 5 s`),
  then plans again. Going through the paused/unshaped branch drops the
  pending settle.
- **Generic exe names.** `cod.exe` and `aces.exe` are out of
  `game-exes.toml`. Their folders are libraries in `games.toml` instead:
  `\call of duty\` (Battle.net installs) and `\warthunder\win64\` (the
  Gaijin launcher's install; Steam installs were already covered). Tests
  cover both, and that a `cod.exe` or `aces.exe` elsewhere is not a game.

Still not checked on a real machine: everything in "Not checked yet"
above, plus the power-event line after a Modern Standby wake and a start
with a full-tunnel VPN up (it should not wait 60 s).
