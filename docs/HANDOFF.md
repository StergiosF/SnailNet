# Handoff: SnailNet, next steps

Start a fresh session with: **"read docs/HANDOFF.md and continue"**.

## Start here (2026-11-18, pack feed; newest)

- **Signed site/game pack feed done** (PLAN.md §17.2 "Pack feed"):
  `snailnet-core::packs` (bundle `packs.toml` = format, serial, the
  three pack texts; minisign with the updater key; newest serial wins,
  never back; `current()` / `install()`), game folders now
  `assets/games.toml`, service `packs.rs` (daily fetch from the GitHub
  release `packs`, saved in ProgramData, restart shaping once), app
  `focus.rs` reads the saved bundle, Settings → About switch
  (`pack_updates`, on). Site "known" check moved to
  `Config::check_sites` (changes only).
- **Publishing:** `scripts/publish-packs.ps1` (`-Upload` puts it on the
  release; not run, repo private). It bumps `assets/packs-serial.txt`:
  commit that with the assets. Change a pack → run it, commit, upload.
- Not seen in a real window or run in the real service. The switch
  follows the existing switches' markup; no impeccable pass for it.

### Next, in order

1. Final perf run against §20 (needs the elevated real service).
2. With the user: the real-service test ("First thing next session"
   further down), VM installs, tag `v1.0.0`; making the repo public
   (licence first), then `publish-packs.ps1 -Upload`.

## Earlier: 2026-11-18, latest session

Pushed on `main`: `05fb091` E, `b70cae1` audit fixes, `4a45626` README +
notices. Decisions in PLAN.md §11.2, §11.5, §11.6, §21 Phase 6.

- **E done, so Phase 5 is complete.** The game reason ("Goes first · a game, it ran
  full screen" / "in a launcher's library" / "you said it's a game") on
  the row under "Using your internet": `FocusChanged.why`,
  `StatsTick.games` (`GameTick`, `GameWhy`), `Focus::games`. Hotkey keys
  from four presets (`hotkey::CHOICES`, `hotkey-keys` file, Settings →
  Windows select; commands `hotkey_info` / `set_hotkey_keys`).
- **impeccable audit done** (code only, no browser): merge select now
  pick + Move (arrow keys merged by accident), Block hover contrast,
  long menus scroll, hotkey select focus. Left: menu arrow-key movement,
  global reduced-motion cut. Rendered contrast wasn't measured.
- **Phase 6:** README (help section, privacy, licences) and
  `scripts/third-party-notices.ps1` (run by `build-installer.ps1`,
  shipped as `licenses\THIRD-PARTY-NOTICES.txt`) done. The installer
  hasn't been built since.

### Next, in order

1. **Signed site/game pack feed** (§21 Phase 6): packs update without a
   new build. Needs decisions: where it's hosted (same GitHub releases as
   the updater?), signature (ed25519/minisign, which crate: Context7
   first), how the service fetches (WinHTTP via `snailnet-win::http`
   exists), and falling back to the compiled-in packs.
2. Final perf run against §20 (needs the elevated real service).
3. With the user: the real-service test ("First thing next session"
   further down), VM installs, tag `v1.0.0`.

### Needs the user (new)

- **SnailNet's own licence**, before the repo goes public for the update
  feed (README says "not chosen yet; all rights reserved").
- Everything under "Needs the user" below still stands.

## Earlier: 2026-11-18, end of session

Everything is committed and pushed on `main` (last code commit:
`bfb2b16`). The user leaves decisions to Claude: pick, decide, record in
PLAN.md, run the gates, commit + push each verified step. No branches.

### State

- **Phase 5 (Control) is done except E.** This session: B strict mode
  (`7836402`), C SOCKET refusal + tray counter (`998d75d`), D family
  split/merge (`bfb2b16`). Decisions for each are in PLAN.md §11.3,
  §11.4, §11.6; short notes in "Session notes" below.
- **Phase 6:** installer, updater and `release.yml` exist (built, never
  run).
- **Nothing since Phase 4 has run in the real service or been seen in a
  real window.** Every automated gate passes. Riskiest untested parts:
  the SOCKET refusal against the real driver (`refuse.rs`), strict-mode
  holds, WinRT toasts, the tray badge at real size, the installer.

### Next, in order

1. **E (small, app only):** game-detection confidence in the UI (§11.5:
   why SnailNet thinks it's a game: fullscreen, launcher library, or
   "you said so"); choosing another hotkey than `Ctrl+Alt+S` (§11.2,
   `app/src-tauri/src/hotkey.rs`, Settings → Windows).
2. **impeccable audit** of the UI added since Phase 4 (Boost menu
   grouping items, Settings lists and switches, toast wording). No
   browser unless the user asks; say what wasn't seen.
3. **Phase 6 leftovers (§21):** signed site/game pack feed (packs update
   without a new build); README with the "help, my internet broke"
   section, licences (WinDivert's licence and the bundle; see
   `deny.toml`), privacy statement; final perf run against §20.
4. **With the user (elevated / VMs):** the real-service test under
   "First thing next session" further down; then the Phase 6 exit: a
   clean install on fresh Windows 10 and 11 VMs, then tag `v1.0.0`.

### Needs the user

- Make the repo (or a releases repo) public for the update feed (the
  `TAURI_SIGNING_PRIVATE_KEY` secret is set, 2026-11-18); back up
  `%USERPROFILE%\.tauri\snailnet-updater.key`; bump the version from
  0.0.1.
- Run the elevated dev service and look at the app (steps further down).
  The old dev service may still lock `target\debug\snailnet-service.exe`
  ("Access is denied" on build): stop it first.
- VM tests of the installer.

### Gates (run before every commit)

- Repo root: `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace`.
- `app\src-tauri`: `cargo fmt --check`, `cargo clippy --all-targets --
  -D warnings`, `cargo test`.
- `app\`: `npm run check`, `npm run lint`, `npx vitest run`,
  `npm run build`.
- `runner::tests::no_line_speed_means_no_shaping` (20 ms timing) can
  fail under load; it passes alone.

### Working notes

- GateGuard hook: before the first edit of each file (and the first
  shell command, and anything it calls destructive) state importers,
  affected API, data and the user's words, then retry. `app/**` is
  exempt.
- Commit from Bash with `git commit -F - <<'EOF' … EOF` (quotes break in
  PowerShell 5.1). Edit scripts with backslashes: Python raw strings,
  written to a file first (long heredocs with quotes can break Bash).
- No repo prettier config: don't run prettier on existing files (it
  reflows them). New TS files are fine at `--print-width 120`.
- WinDivert filters have no `!(…)`; check filters with
  `WinDivertLib::check_filter` (no admin) in a unit test.
- Rules in `~/.claude/rules`: no browser automation unless asked; UI
  work through `impeccable`; Context7 before using a library API.

## Session notes (2026-11-18, later)

- **B, strict mode (§11.4), done:** `new_apps = "strict"`; a held app's
  family joins `Plan.blocked` until answered or 20 s (`Newcomers` holds,
  `NewApp.held`); timeout lets it online, the question stays; `Plan.strict`
  makes the engine divert both ways so a hold never restarts shaping.
  App: "Wait for my answer first" switch, held toast text. Not seen in a
  real window or service.

- **C, SOCKET refusal + tray counter (§11.3), done:** `refuse.rs`
  (DROP-mode SOCKET handle listing blocked apps' PIDs, internet connects
  only, rebuilt every 0.5 s, fails open), `Attribution::processes`,
  `WinDivertLib::check_filter`; tray badge + tooltip count. Never run
  against the real driver.

- **D, family split/merge (§11.6), done:** `snailnet-core::families`
  (`[families]` split by exe / merge by family), `Attribution::set_families`
  (ids kept, view regrouped), IPC `SetFamilies`; Boost menu items + a
  Settings list with undo. Fixed: blocking path/`pkg:` families was
  refused by validation. Not seen in a real window.

Next: **E** small (game-detection confidence in the UI §11.5, another
hotkey §11.2); impeccable audit of today's UI is still owed. "Needs the
user" below still stands.

## Start here (2026-11-18, night)

Pushed on `main` (last: b9e0c61). Done this session (decisions in PLAN.md):

- **A, new-app notifications, notify mode (§11.4):** `snailnet-service::newapps`
  (`known-apps.txt`, 10-minute quiet learning, Windows parts and apps with
  rules stay quiet), `Event::NewApp` (cap `new-apps`), `Snapshot.prompts`,
  `AnswerPrompt` fine/slice/block; app `toast.rs` (WinRT toasts, AUMID
  `net.snailnet.app`, summary after fullscreen games), `give-slice` opens the
  split; Settings → Windows switch. Not seen in a real window or service.
- **Phase 6 (subagent, 5cc9d75 + b9e0c61):** NSIS installer
  (`tauri.release.conf.json`, `windows/hooks.nsh`, `scripts/build-installer.ps1`),
  updater (`updates.rs`, Settings → About → Updates), `release.yml`.
  The installer builds but was never run. The updater key lives at
  `%USERPROFILE%\.tauri\snailnet-updater.key` (not in the repo).
- **Needs the user:** make the repo (or a releases repo) public for the
  update feed; add the `TAURI_SIGNING_PRIVATE_KEY` secret; back up the key;
  bump the version from 0.0.1; test the installer on clean Win10/11 VMs.

Next: **B strict mode** (hold a new family by putting it in the plan's
blocked list for ≤ 20 s; dropped SYNs retry by themselves; note that apply()
restarts the shaper when blocking starts), then **C** (the WinDivert SOCKET
layer is opened SNIFF today; refusing `connect` needs a non-sniff handle, so
weigh fail-closed risk; tray counter), **D** family split/merge, **E** small.

## Start here (2026-11-18, end of day)

Everything is committed and pushed on `main`. The user leaves decisions
to Claude (pick, decide, record in PLAN.md, commit + push each verified
step) and wants the next session to **run a subagent alongside** to
finish the plan faster. Work stays on `main`: no branches, so no
worktrees either; split the work by files instead (below).

### Done today (decisions in PLAN.md)

- §11.5: Gaming-while-playing shown in the app (`StatsTick.playing`);
  per-app overrides "It's a game" / "Never put it first"
  (`Settings.games` / `never_first`, applied in the service's hub, which
  resolves the focused PID's family itself; Boost menu + Settings list).
- §10.3: metered note (app command `network_metered`, WinRT).
- §11.2: `Ctrl+Alt+S` cycles modes and opens the flyout
  (`app/src-tauri/src/hotkey.rs`, plugin `tauri-plugin-global-shortcut`
  =2.4.1, off-switch in Settings → Windows); **Duplicate & edit**
  (IPC `CopyProfile`/`RenameProfile`/`DeleteProfile`; Split page: Make
  a copy, Rename, Delete; own modes in tiles/flyout/hotkey).
- Gates at the end: `cargo clippy --workspace --all-targets -D
  warnings`, `cargo fmt --check`, `cargo test --workspace` (347 passed),
  `npm run check` / `lint` / `npx vitest run` (88) / `build` in `app\`.
- **Nothing from 2026-11-18 has been seen in a real window or run in the
  real service** (see "First thing next session" further down; the old
  dev service still locks `target\debug\snailnet-service.exe`).

### What's left of the plan (PLAN.md §21)

Phase 5 (Control), exit "all D9 features complete":

| # | Work | Touches | Notes |
|---|---|---|---|
| A | **New-app notification, notify mode** (§11.4): first time an unknown family goes online → toast "{app} went online" with Fine · Give it a slice · Block; no toasts while a game is fullscreen, one summary after; Microsoft components in System32 never prompt | service (`shaper.rs` family seen, `hub.rs` remembers "known" families in config, new IPC event), `snailnet-ipc`, app (`bridge.rs` event, toast code, Settings switch Notify/Off) | Biggest item. Check Context7 for `tauri-plugin-notification` (Windows action buttons may need WinRT `ToastNotification` directly via the `windows` crate) |
| B | **Strict mode** (§11.4): hold new connections of an unknown app until answered (≤ 20 s), then default action | service shaper/SOCKET layer, `AnswerPrompt` IPC (exists, answers Unsupported) | After A; same files, so same agent |
| C | **Block at the SOCKET layer + tray counter** (§11.3): refuse `connect`/`bind`/`listen` of blocked families; tray shows a count when ≥ 1 app is blocked | `snailnet-service::shaper`, `snailnet-engine`, `app/src-tauri/src/tray_icon.rs` | Shares shaper.rs with A/B: do it in the same stream as A/B, or after |
| D | **Family split/merge UI** (§11.6) | `snailnet-core::classify::family` overrides in config, app rows | Mostly core + app UI |
| E | Small: game-detection confidence in the UI (§11.5); choosing another hotkey (§11.2) | app only | |

Phase 6 (Ship v1.0): **installer** (Tauri bundler + installing the
service and WinDivert files, uninstall cleans up), **updater**, signing
notes; PLAN.md §15–§17 and §21 have the details.

### Suggested split for the subagent

- **Main session:** stream A → B → C (all in the service's shaper, hub
  and IPC plus the app's bridge; one owner avoids conflicts there).
- **Subagent (general-purpose, same tree, on `main`):** Phase 6
  installer + updater (new files: bundler config in
  `app/src-tauri/tauri.conf.json`, scripts under `scripts/`, an
  install step for the service), or D if Phase 6 needs the user's
  choices first (signing certificate, update host). Tell it: don't
  touch `crates/snailnet-service/src/{shaper,hub,ipc}.rs`,
  `crates/snailnet-ipc`, `app/src-tauri/src/bridge.rs`; run the gates;
  commit only its own files with `git commit <paths>` (never `git add
  -A`, never amend/rebase), message ending with the Co-Authored-By line;
  record decisions in PLAN.md (edit only its own section) and report
  back. The main session pushes.
- Both building at once is fine (cargo locks the target dir), just
  slower. Only one of them should run the elevated dev service.

### Working notes (still true)

- GateGuard hook: before the first edit of each file (and first shell
  command) state importers/callers, affected API, data, and the user's
  words, then retry the same call. `app/**` is exempt locally.
- Commit messages with double quotes break in Windows PowerShell 5.1;
  commit from Bash with `git commit -F - <<'EOF' … EOF`.
- Rules in `~/.claude/rules`: no browser automation unless asked; UI
  goes through the `impeccable` skill (its full audit hasn't been run
  on today's UI); Context7 before using a library API.

## Earlier: 2026-11-18, late

Everything is committed and pushed on `main`. The user leaves decisions
to Claude: pick the next step, decide, record it in PLAN.md, commit and
push each verified step.

### Done this session (details and decisions in PLAN.md)

- **Speeds per network (§10.3):** router MAC = network id, SSID/adapter
  = name (`snailnet-win::network`); `[network]` + `[[networks]]` in
  config (`snailnet-core::networks`); `snailnet-service::netwatch`
  swaps the line on a change (route checked every 5 s); a new network
  is watching-only with a "New network: {name}" notice; Settings →
  Connection shows "On {network}" and the other networks.
- **Focus boost (§11.5):** the app polls the window in front
  (`app/src-tauri/src/focus.rs`, 300 ms, debounced, repeated every
  60 s) → `FocusChanged`; game = exclusive fullscreen or an exe in a
  launcher library (`snailnet-core::games`). The service
  (`snailnet-service::focus`) puts those PIDs in `Plan.focus`; the
  shaper gives their families High *flow* priority (no share change).
  Games stay first until they exit.
- **Gaming mode while playing:** off by default; plans with Gaming while
  a game runs, the picked mode stays saved.
- New IPC `SetSettings`; Settings → Connection has the switches "Put
  what you're using first" (on) and "Gaming mode while you play" (off).

### Not tested for real (user's call, 2026-11-18)

The user ticked the real-service test as seen and chose to move on.
**Nothing of Phase 4 has run inside the real service or been seen in a
real window.** Every automated check passes (cargo clippy/fmt/test,
app check/lint/vitest/build, Tauri clippy/test). The old dev service
(elevated, PID from 2026-11-17) still locks `target\debug\snailnet-service.exe`, so `cargo build -p snailnet-service` fails with
"Access is denied" until it's stopped (Ctrl+C in its console, or an
elevated `Stop-Process`). The test steps are under "First thing next
session" below; do them whenever the user wants.

### Next work, in order (no user needed)

1. ~~**Show the Gaming switch-over in the app**~~ **Done (2026-11-18):**
   `StatsTick.playing`; outlined "Playing" flag on Gaming + a note in the
   mode tiles and flyout (PLAN.md §11.5). Not seen in a real window.
2. ~~**Per-app overrides (§11.5)**~~ **Done (2026-11-18):**
   `[settings] games` / `never_first`, applied in the service (hub
   resolves the focused PID's family); Boost menu items + "Your choices"
   in Settings. "Always High" = a High-priority rule (exists). Not seen
   in a real window.
3. ~~**Metered network note (§10.3)**~~ **Done (2026-11-18):** app
   command `network_metered` (WinRT `ConnectionCost`), a note under the
   line speed in Settings → Connection. Not seen in a real window.
4. Phase 5 (PLAN.md §21): ~~hotkey~~ **done 2026-11-18** (Ctrl+Alt+S
   cycles modes + opens the flyout; `app/src-tauri/src/hotkey.rs`; not
   tried in a real session). Duplicate & edit also done (see the newest
   section). Left: new-app toasts notify + strict (§11.4), blocking at the SOCKET layer
   and the tray counter (§11.3), family split/merge UI (§11.6). Then
   Phase 6 (installer, updater).

### Still owed (need the user)

Phase 4 exit check (ping under load within +10 ms; games boosted
automatically), Phase 3 exit test (someone new, mouse only, < 15 s),
Phase 2 exit check (YouTube 45% in three browsers), Phase 1 soak run 3,
the anti-cheat test.

## Start here (earlier 2026-11-18)

Everything below is committed and pushed on `main` (last: 5df79b0).
The user leaves decisions to Claude: pick the next step, decide, record
the decision in PLAN.md, commit and push each verified step.

### Where we are

- **Phase 3 (the app): built.** Done this session: "Start SnailNet" on
  the not-running notice (UAC, `app/src-tauri/src/service_start.rs`), an
  Offline notice, hidden-name website traffic under "Using your
  internet", a one-time hint on "The split", and no focus ring on the
  button a dialog hands focus back to, Enter included
  (`app/src/lib/focusRing.ts`, user's request). **Owed:** the Phase 3
  exit test (someone new solves the motivating scenario, mouse only,
  < 15 s); needs the user and a tester.
- **Phase 4 (Smart): half built.**
  - `snailnet-core::estimator`: pure per-direction logic (ping-driven,
    like cake-autorate): calm + pressing raises ≤ 10%/min, a > 30 ms
    rise cuts 5%/s to 30% of the reference, Low-latency factor tunes
    0.80–0.97, capped at 3× the reference.
  - `snailnet-win::ping` (ICMP + gateway) and `snailnet-win::http`
    (WinHTTP: system TLS/proxy, no new crates).
  - `snailnet-service::autorate`: probe thread pings the router and
    1.1.1.1 (user's choice) at 1 Hz while busy, 30 s idle; baseline =
    lowest RTT of 5 min; the runner feeds it each second and applies the
    rate live on a ≥ 2% move (`Config::plan_live`). `line.pinned` = keep
    typed speeds. Log line: `line speed: shaping at down …`.
  - `snailnet-service::speedtest`: `RunSpeedTest` → runner stops shaping
    (state `Measuring`), tests Cloudflare (user's choice) ~20 s, saves
    speeds + `[line.measured]` (at, down/up, idle/loaded ping). CLI
    `snailnet speedtest`. One real run: 22.0 / 25.7 Mbps, 11 / 40 ms.
  - App: "Measure my connection" in first run; Settings → Connection has
    Measure, last result, "Ping while downloading", "Follow the line
    automatically" (off = pinned); a Measuring notice.
  - **Speeds per network (§10.3), built later on 2026-11-18:** network =
    the router's MAC (`snailnet-win::network`), name = Wi-Fi SSID or
    adapter; `[network]` + `[[networks]]` in config
    (`snailnet-core::networks::switch_network`); `snailnet-service::netwatch`
    checks the route every 5 s and swaps the line; a new network is
    watching-only ("New network: {name}" notice) until measured or typed;
    Settings → Connection shows "On {network}" and the other networks.
    Decisions in PLAN.md §10.3. Metered flag not done. Not yet seen in
    the real service or a real window.
- **Nothing of Phase 4 has run inside the real service yet.** The dev
  service in `target\debug` was running all session with the user's
  real config (shaping at 23 Mbps), which locks the exe, so it has the
  old build. A background `npm run tauri dev` also died (exit 101).

### First thing next session (needs the user, elevated)

1. Stop the dev service, `cargo build -p snailnet-service`, start it
   again (`target\debug\snailnet-service.exe run --console`), then
   `npm run tauri dev` in `app\`.
2. Look at the new screens (user's eyes; no browser automation by
   default): Settings → Connection, the Start button (stop the service
   to see it), the split hint, the hidden-traffic line.
3. Check the log for `network: on "…"` at start (the existing speeds
   are adopted for this network); Settings → Connection shows "On …".
   Joining a phone hotspot should log `new here` and show the New
   network notice; coming back restores the home speed.
4. Focus boost: with a download running, bring a game (Steam library,
   or exclusive fullscreen) to the front; it should feel snappier, and
   it stays first when alt-tabbed until it exits. Settings → Connection
   → "Put what you're using first" turns it off.
5. Click Measure; then start a big download and watch the log for
   `line speed:` steps and the ping staying low. Tune the estimator's
   constants if it oscillates.

### Next work, in order (no user needed)

1. ~~Speeds per network~~ (built; see above). Left: the metered flag.
2. ~~Focus boost~~ (built 2026-11-18, PLAN.md §11.5). Left: per-app
   overrides ("This is a game", "Never boost"), the Gaming-profile
   automation.
3. Phase 4 exit check with the user: ping under load within +10 ms on
   their line; games boosted automatically.
4. Then Phase 5 (profiles/hotkey, new-app notifications), Phase 6
   (installer, updater).

### Still owed from earlier phases (need the user)

Phase 2 exit check (YouTube 45% in Chrome/Edge/Firefox, secure DNS on and
off), Phase 1 soak run 3 (24 h, fixed build), the anti-cheat test.

### Working notes

- A GateGuard hook asks for facts (callers, no duplicate, data shape,
  the user's words) before the first Bash call and before creating any
  new file; state them, then retry.
- Rules: `~/.claude/rules`: no browser automation unless asked; UI work
  goes through the `impeccable` skill (PRODUCT.md is its context; no
  DESIGN.md yet). Context7 for library docs.
- Checks: `cargo clippy --workspace --all-targets`, `cargo fmt --all`,
  `cargo test --workspace` (the service's lib tests build separate exes,
  so they run even while the dev service locks its exe); in `app\`:
  `npm run check`, `npm run lint`, `npx vitest run`, `npm run build`.
  Network tests are `#[ignore]` (run with `--ignored`).
- "LF will be replaced by CRLF" warnings on commit are harmless.

## Start here (2026-11-17)

- **Latest:** the app was redesigned ("the Garden", `docs/HANDOFF-ui.md`)
  and the service gained **modes** (four built-in profiles), **boost**,
  **pause** and **blocking an app** (PLAN.md §11.2, 11.3, 11.5, 11.8).
  All pushed on `main`. The dev service in `target\debug` was last run
  with the user's real config (it shapes their traffic at 23 Mbps while
  running; stopping needs elevation).
- **The user approved the redesigned app (2026-11-17).** Next: the
  Phase 2 exit check, the 24 h soak, and the anti-cheat test below; the
  Phase 3 exit test (the motivating scenario, mouse only, < 15 s, by
  someone new to the app); then Phase 4 (speed test, low-latency tuning,
  automatic focus boost).

## Start here (2026-11-16)

- **Where we are:** Phase 1 built except its exit soak. Phase 2 code is
  complete (incl. per-site stats rows, name counts over IPC, fuzz targets,
  CI) but its **exit test in a real browser is still owed**. **Phase 3
  (the app) has started:** `app/` holds the Tauri 2 + Svelte 5 shell.
- **Installed:** the service is installed in `C:\Program Files\SnailNet`
  (build of b7282cc) with line 25 Mbps down and rules `site:youtube=45
  brave=30`, but **stopped and set to manual start** (user's choice).
  To test: elevated `sc start SnailNet`; afterwards `sc config SnailNet
  start= auto` or uninstall. `snailnet top` (unelevated, from that folder)
  now prints `names:` and `sites:` lines.
- **Git:** `main`, ~15 commits ahead of `origin/main`, not pushed (push
  only when asked). CI (`.github/workflows/ci.yml`, `fuzz.yml`) has never
  run on GitHub yet.
- **Next, in order (pick with the user):**
  1. **Phase 3 continued** (no user needed). Done 2026-11-16: the split
     bar (§14.3), the **rule editor** (side panel: give a slice, on/off,
     share or "only when free", priority, ceilings per direction,
     remove; picker for sites + seen apps; list rows open it, quiet
     rules stay listed), **system accent** via `UISettings`
     (`window.rs`), **Windows 10 fallback** (solid layer colour, no
     Acrylic). Open user questions: Acrylic on Win10 after all? Snap
     layouts need a hack (wry hosts WebView2 windowed; see the comment
     in `TitleBar.svelte`): do it, or wait for upstream? (Defaults
     taken meanwhile: solid colour; wait.) Also done: **line speed**
     (`line.rs` `set_line`, `LineDialog` from the header figures and the
     Watching-only bar), **first run** (`FirstRun.svelte`: welcome →
     type speeds or later → done; `localStorage` `snailnet.firstRun.done`),
     **tray + flyout** (`tray.rs`: menu Open/Quit, left click toggles the
     `flyout` window placed in the work area by the icon, hides on blur;
     closing the main window hides it; single instance via
     `tauri-plugin-single-instance` =2.5.5; `Flyout.svelte`, picked in
     `main.ts` by window label). **Tray icon states** (`tray_icon.rs`:
     normal; amber dot for safe mode / retrying; faded grey when not
     connected; tooltip says which. Paused and blocked-apps dot wait for
     those service features). **Autostart + start minimised**
     (`autostart.rs`, `tauri-plugin-autostart` =2.7.0, HKCU Run with
     `--minimized`; main window starts hidden and is shown unless that
     flag is present; release builds turn it on once on first launch
     (marker file `autostart-default-applied` in the app config dir);
     tray menu has a checkable "Start with Windows"; commands
     `autostart_enabled` / `set_autostart` for a future settings page).
     Seen in a real window 2026-11-16: Mica couldn't show through
     WebView2 (white page), so the windows are now opaque with a solid
     ground (commit 0f932a5). The user then rejected the look ("plain,
     looks like a Windows application") and a first redesign ("AI
     slop"). **Next: the redesign in `docs/HANDOFF-ui.md`**, then the
     settings page (§14.6).
  2. **Phase 2 exit check** (needs the user): start the service, play
     YouTube in Brave with secure DNS on and off, watch `snailnet top`
     (`names:`/`sites:` lines) and the log; add missing domains to
     `assets/sites.toml` (compiled in: rebuild + reinstall after).
  3. **Soak run 3** (24 h, pass-through) for the Phase 1 exit.
  4. Anti-cheat test (Phase 0, user skipped for now).

### The app (Phase 3) — how to run and where things are

- `app/src-tauri` is the `snailnet-app` crate, a **workspace member** so
  its dev exe lands in `target\debug` next to `snailnet-service.exe`: the
  service only accepts clients in its own folder. Dev loop: elevated
  `target\debug\snailnet-service.exe run --console`, then `npm run tauri
  dev` in `app/`. The installed service in Program Files will reject the
  dev app (`Link::Rejected` InfoBar).
- Rust: `bridge.rs` (one pipe connection, reconnect with backoff, emits
  `link` / `snapshot` / `service-state` / `stats`; tested with fakes),
  `lib.rs` (Tauri setup, `current` command for late page loads).
- Svelte: `src/lib/service.svelte.ts` (runes store), `types.ts` (mirrors
  the IPC serde JSON — keep in step with `snailnet-ipc`), `format.ts`
  (§14.7 number rules, vitest), `i18n/en.json` (all strings), components
  `TitleBar`, `Header`, `InfoBar`, `AppList`, `Snail`, `SplitBar`,
  `RuleEditor` (+ `Segmented`, `ToggleSwitch`). `rules.ts` holds the
  rule edits as pure functions (a rule is a bar *slice* when enabled with
  share > 0; giving one carves from Everything else first), `editor.svelte.ts`
  which panel is open, `system.ts` the accent/backdrop. The IPC `Snapshot`
  now carries `sites` (every site a rule may name). Tokens in
  `src/app.css` (Fluent light/dark/high contrast; Segoe Fluent Icons).
- Gate: `npm run check`, `npm run lint`, `npm test` in `app/` plus the
  cargo gate. Versions pinned exactly (Tauri 2.12.5, Svelte 5.57.3, Vite
  8.3.8, TS 6.0.3 — svelte-check doesn't support TS 7 yet).
- Design workflow: `PRODUCT.md` (impeccable) records users/brand; the
  visual world is pinned by PLAN.md §14.1 (Windows 11 native). The UI has
  **not been looked at in a real window yet** (no browser/screenshot
  unless the user asks).
- `.claude/settings.local.json` (uncommitted, local) sets
  `GATEGUARD_EXEMPT_GLOBS=app/**`.

## Where things stand (Phase 0/1 detail, 2026-11-15)

- **Plan:** `PLAN.md` is the source of truth (decisions D1–D20, roadmap §21).
- **Git:** everything is on **`main`**. The user works only on `main`: no
  feature branches. Push only when asked.
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
| ~~Real scenario~~ **done 2026-11-15** (factor 0.85): 40/60, YouTube 4K didn't buffer, ping 226 → 22 ms avg (p50 17, max 77), CPU 1.1% of all cores, 32 MB | Elevated: `.\target\release\snailnet.exe spike --down 25 --app steam.exe,steamwebhelper.exe=40:low --app brave.exe=60`, then a Steam download + YouTube in Brave | split within ±5%, YouTube doesn't buffer |
| ~~Lower factor~~ | **Synthetic run done 2026-11-15**: `phase0-synthetic.ps1 -Factor 0.85` → split 39–40/60–61, loaded ping avg 20 / max 39 vs idle 9, 0 lost (meets §20 on average). Repeat in the real scenario | loaded ping recorded in the real run too |
| Anti-cheat (**user skipped for now**, 2026-11-15) | Run `spike` (or `check`) while launching the user's games | per-game result in the report |
| ~~CPU~~ **done**: 1.08% of 12 logical cores at 21 Mbps | Task Manager → `snailnet.exe` during the real run | % recorded |
| ~~Upload~~ **done 2026-11-15**: line ≈ 40 Mbps ↑ (fiber; ↓ really is ≈ 25). Synthetic `--up 40 --factor 0.85`: 40/60, ping avg 12 vs 10 idle (unshaped upload: 410 ms) | `--up <Mbps>` after measuring upload | split recorded |
| ~~**Go/No-go**~~ **Go, 2026-11-15** (anti-cheat later); written in the report and PLAN.md D4 | Fill in the report's Decision line; record the outcome in PLAN.md's D4 row | decision written |

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
   - ~~**Live rule swap without restart**~~ **Done (2026-11-15).**
     `FastPath` keeps a `LiveConfig` per shaped direction (the scheduler
     thread `sync`s each pass); `FastPath::reconfigure` validates both
     configs first and refuses a change of shaped directions
     (`ReconfigureError::DirectionsChanged`). The classifier now holds
     `Arc<dyn Rules>` behind `LiveRules` (mutex + generation; rx threads
     reload and clear their app → class cache when the generation moves).
     `Shaper::reconfigure` publishes scheduler first, then rules.
     `Running::apply` (runner): a config change is applied live; a new or
     dropped direction or a LAN-shaping change restarts as before. Verified
     on the real driver in the smoke test (`rules brave=60`, `line --down
     30` → "applied to the running shaper" twice, rate 27.6 Mbps, Brave's
     traffic moved to its class within a tick, no restart).
     Per-app rows in `top` were seen there too (Brave, Claude Code).
   - ~~**Per-app stats**~~ **Done (2026-11-15).** `Classifier::classify`
     returns `(PacketMeta, Tag)`; the service's tag is the `AppId`
     (`NO_TAG` when unknown). The scheduler thread counts bytes *sent* per
     tag (dequeue and the stop flush), published as `DirectionStats::tags`.
     The hub maps tags to apps (`apps()` is in `AppId` order) and sends
     `DirStats::families` (`FamilyTick { family, name, bps }`, busiest
     first, idle ones omitted, unknown → `unknown`/"Unknown").
     `snailnet top` prints the top 5 under each direction.
   - ~~**Client verification**~~ **Done (2026-11-15)**, user's choice:
     "same folder as the service". `snailnet-win::pipe`: the service gets
     each client's PID (`GetNamedPipeClientProcessId`) and refuses it
     unless its exe is in the service exe's folder (logged: "IPC: refused
     a client: … is not in …"; the client sees `ClientError::Rejected`).
     The client can't read a SYSTEM service's image path as a normal user
     (tested: `OpenProcess` fails), so it checks the pipe's **owner**
     instead: SYSTEM, Administrators or the same user, else
     `ClientError::Impostor`. Verified with a console service: the CLI in
     `target\release` is accepted, a copy elsewhere is refused.
     Signature checks wait for signed builds. The pipe is byte mode, not
     message mode (the framing makes them equivalent).
   - The hub saves `config.toml` on the IPC thread (blocking, but tiny).
7. **Phase 1 exit:** the deferred items above are done (2026-11-15); what
   remains is the **24 h soak** (flat memory, no handle leaks; the 60 s
   log line prints the sizes).
   **Soak running since 2026-11-15 05:25 local (03:25Z)**, user's choice
   "pass-through": the service is installed from a copy in
   `C:\Program Files\SnailNet` (service exe, CLI, WinDivert DLL + sys; the
   CLI must live there, see client verification), so `target\release`
   stays free for builds. Line 1000/1000 Mbps at factor 1.0, no app rules:
   the whole engine sees every packet but never limits. (Monitor-only was
   rejected: it opens no WinDivert handles, so it would test nothing.)
   Baseline right after start: working set 33.5 MB, 135 handles.
   A per-user scheduled task **`SnailNetSoak`** runs
   `scripts/soak-sample.ps1` every 10 min for 2 days (headless via
   `conhost --headless`) → `%LOCALAPPDATA%\SnailNetSoak\soak.csv`
   (`time,pid,ws_mb,private_mb,handles,threads`; pid 0 = service not
   running). First row: 33.6 MB WS, 26.6 MB private, 125 handles, 11 threads.
   Check (see the restart below for the current end time): the CSV (look for steady growth in
   private_mb / handles; a new pid means a restart) and the service's 60 s
   log lines in `C:\ProgramData\SnailNet\logs\` (table sizes, pool free,
   overflow; Users can read it).
   **Check-in 2026-11-15 17:50:** the PC **slept 06:09 → 17:47** (an app
   called `SetSuspendState`), so the soak had only ~45 min awake; the task
   missed 70 runs. The service **survived S3 sleep/resume**: same PID, no
   fail-open, no restart, attributing again right after wake (38 MB WS,
   132 handles; tables jumped to ~235 flows / 185 sockets / 265 ports after
   resume, during the reconnect burst — watch that they come back down).
   The first log line after resume averaged "over 41962 s": `Instant`
   counts sleep time but thread waits don't. That exposed a **watchdog
   race**: on resume, if the watchdog looked before the scheduler beat, it
   would see an 11 h "stall" and fail open for nothing (it didn't this
   time). Fixed in commit 5787780: `fastpath::StallMeter` counts only
   silence the watchdog was awake for (≤ 2 ticks per look; any beat
   resets), with unit tests.
   **Soak restarted 2026-11-15 18:05 local** with the fixed build (PID
   7180, same pass-through config). Baseline: 35.1 MB WS, 26.4 MB private,
   135 handles, 14 threads. The first run's CSV is kept as `soak-run1.csv`.
   Check after **2026-11-16 18:05**. Sleep during the run is fine, since it
   tests the fix; judge by awake hours, and look for any "stalled" fail-open
   right after a resume in the log.
   **Check-in 2026-11-16 00:30: the run-2 soak shows a leak.** Private
   memory 26.4 → 32.5 MB in 6.25 h and still rising; handles flat. The
   60 s log shows the FLOW table growing steadily, 7 → 1083 entries,
   while Windows had only 73 TCP connections and 53 UDP endpoints: dead
   entries whose FLOW-deleted / socket-close never matched (cause not
   pinned down; the cap is 131 072, so nothing stopped it). **Fixed in
   core** (`FlowTable::set_ports`): an entry whose local port is missing
   from 3 consecutive port snapshots (~3 s) is dropped; a protocol with no
   rows in the snapshot (failed table read) is never aged. The running
   soak still has the old build, so it can't pass; a run 3 with this build
   is needed.
   **Soak stopped 2026-11-16 00:37 (user: "Stop it, no soak for now").**
   Torn down: service uninstalled with `--remove-data`, `C:\Program
   Files\SnailNet` deleted, WinDivert stopped, task `SnailNetSoak`
   unregistered. Evidence kept in `%LOCALAPPDATA%\SnailNetSoak\`
   (`soak-run1.csv`, `soak-run2.csv`, `run2-logs\`). **Phase 1 exit still
   needs a passing 24 h soak (run 3)** with the fixed build: reinstall the
   copy in Program Files, pass-through config, re-register the sampler
   task (`scripts/soak-sample.ps1`), and watch "tracking N flows" stay
   near the live connection count.
   (Old cleanup steps, already done:) `Unregister-ScheduledTask SnailNetSoak -Confirm:$false`;
   elevated: `& 'C:\Program Files\SnailNet\snailnet-service.exe' uninstall --remove-data`,
   delete that folder, `sc stop WinDivert`.

**Phase 1 exit:** rules editable via the CLI (done), live stats via
`snailnet top` (done, per class), 24 h soak passes (flat memory, no
handle leaks).

Then Phase 2 (website recognition), Phase 3 (Tauri UI), as in PLAN.md §21.

### 3. Phase 2: website recognition (started 2026-11-16, during the soak)

Pure-Rust parts were done during the soak; the soak has since been
stopped, so driver runs are free again.

1. ~~**TLS ClientHello SNI**~~ **Done.** `snailnet-core::parse::tls`:
   `from_tcp` (TLS records, joins a hello split across records/segments,
   `NeedMore` until complete, gives up past `MAX_HELLO` = 16 KiB) and
   `from_handshake` (bare message, for QUIC CRYPTO frames). Returns
   `ClientHello { sni, ech }`: name lowercased and validated (`host_name`),
   `ech` set when `encrypted_client_hello` is present (the SNI is then the
   outer name → unidentified, §8.4). 12 tests incl. proptests (garbage,
   corrupted/truncated hellos, any name at any record size).
2. ~~**QUIC Initial SNI**~~ **Done.** `snailnet-core::parse::quic`:
   `open_initial(datagram)` opens a client Initial of QUIC v1 or v2
   (RFC 9001 / 9369 keys from the public salt + DCID; header protection
   removed, AES-128-GCM tag checked) and returns its CRYPTO frames;
   `CryptoAssembler` joins out-of-order / overlapping frames across
   packets (Chrome shuffles them; a PQ hello needs two Initials), capped
   at `MAX_HELLO`; feed `prefix()` to `tls::from_handshake`. Crypto is
   pure-Rust RustCrypto (aes-gcm 0.11, aes 0.9, hkdf 0.13, sha2 0.11).
   Tests: RFC key vectors for v1 and v2, the RFC A.2 header mask, a
   seal → open round trip, a shuffled two-packet hello, tamper/version/
   type/truncation rejection, proptests (garbage, corrupted headers,
   assembler vs. a reference model).
3. ~~**DNS answers**~~ **Done.** `parse::dns::answers(msg)` → `Resolution
   { names (CNAME chain, question first), addrs (ip, ttl) }` for A/AAAA
   responses; backward-only compression pointers, hop limit, capped.
4. ~~**Site packs, matcher, cache**~~ **Done.** `assets/sites.toml` (the 10
   v1 sites; **first draft**, check in the exit test), `sites::SitePack`
   / `SiteMatcher` (longest pattern wins; custom sites override shipped
   ones) / `DomainCache` (newest answer wins, TTL clamped 5 min–1 h,
   FIFO-capped).
5. ~~**Per-flow names**~~ **Done.** `PacketInfo` has `payload_offset` +
   `payload(pkt)`. `sniff::FlowNames::observe` reads a :443 flow's first
   outbound payloads (TLS ≤ 12 segments, QUIC ≤ 6 packets) → `Named::
   Pending` / `Known { names, via: Sni | Dns | Ech | None }`; ECH or no
   SNI → the DNS cache; other ports → the cache at once; capped (a full
   table still reads one-packet hellos), `forget` on flow end, `sweep`
   after 15 min idle.
6. ~~**Wiring**~~ **Done (unit-tested; not yet run on the driver).**
   User decision 2026-11-16: **a site rule applies in every app and beats
   the app's rule** (PLAN.md D7, §8.3). Config gains `[[sites]]` custom
   sites; a site rule must name a known site; `Plan::site_class` /
   `has_site_rules`. `Classifier::classify(pkt, info, outbound)` gets the
   packet bytes. Service `shaper`: `PlanRules` (plan + `SiteMatcher`)
   replaces `impl Rules for Plan`; `SiteState` (`FlowNames` + `DomainCache`,
   shared, lock order cache → names) is consulted only while site rules
   exist; a `dns-sniffer` thread (H4: NETWORK, SNIFF|RECV_ONLY, `inbound
   and !loopback and (udp.SrcPort == 53 or tcp.SrcPort == 53)`; TCP needs
   the whole length-prefixed message in one segment) fills the cache; if
   it can't open, shaping goes on with SNI only (logged). The 60 s log
   gains "site names (packets): sni/dns/ech/none/naming; tracking N named
   flows, M DNS addresses". CLI: `snailnet rules site:youtube=45`.
7. **Next (needs the user at the machine):**
   install the service from this build, `snailnet rules site:youtube=45
   brave=...`, play YouTube in Brave (and Chrome/Edge/Firefox if
   installed) with secure DNS on and off; check `snailnet top` shows the
   `site:youtube` class and the log's name mix; note what lands in
   "none" and add missing domains to `assets/sites.toml`. Still to do
   after that: per-site rows in stats (tags are per app today; the class
   rates already show `site:…`), unidentified counts over IPC for the UI
   (§8.4), and fuzzing in CI (cargo-fuzz targets for tls/quic/dns).

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
Always use `-File` with a script: an inline `-Command` passed through
`Start-Process -ArgumentList` gets its quoting mangled and silently does
nothing. Delete a folder holding `WinDivert64.sys` only after `sc stop
WinDivert`; while the driver is loaded the file is locked.

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
