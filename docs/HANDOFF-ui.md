# Handoff: SnailNet app, visual design and UX

Start a fresh session with: **"read docs/HANDOFF-ui.md and continue"**.
Written 2026-11-16. The general project handoff is `docs/HANDOFF.md`;
this file is only about the app's look and usability.

## Status (2026-11-17)

- **Direction now:** "C · The Garden with B's side rail", picked from
  three real-size mockups after the user asked for a revamp that "matches
  the app theme, the snail icon" (the Ledger read as plain and hard to
  use). Then a premium pass ("more cinematic and premium", animations,
  pointer cursors, a bigger window). PLAN.md §14.1–14.6, D12–D13 and
  PRODUCT.md describe it.
- **Built, gates green, pushed:** side rail (Now · Split · Settings,
  Pause, the line); Now page (mode tiles, "Using your internet" with
  Boost / Block, the split in short, the sentence); Split page (the
  Ledger rows, quiet "only when free" rules folded); Settings (speed,
  autostart, both resets, versions); flyout with modes and Pause; the
  service's modes, boost, pause and blocking. Commits 3ebca19 … ee86a4e
  and the speed-dialog fix after them.
- **User feedback so far on the build:** liked the direction ("I like
  now the design"); disliked the speed dialog's fields and the side
  rail's "25 ↓ – ↑ Mbps" (fixed: the fields' wrapper clashed with the
  global `.field` class; the line block now reads Download / Upload in
  words). Next: their screenshots of the rest, then fix what they say.
- `.impeccable/` is untracked impeccable state (config, hook cache,
  old mockups); leave it unless the user asks.

## The task

Make the SnailNet desktop app (`app/`, Tauri 2 + Svelte 5) look
**distinctive, premium and calm**, and make it **easier to use**. The
user's words:

> "the ui looks plain as fuck and looks like windows application I want
> it to be more unique and visually pleasing, overall it is premium and
> not very confusing, the ux can be a bit better I think though"

Then, about the first redesign attempt (see "Rejected" below):

> "the design looks complete AI slop"

The bar: someone opening it should think a small, careful studio made
it, not a template and not an AI. The user gave no reference apps ("I
leave it to you, pick the best approach") and chose **its own dark
look**, not following Windows light/dark or the accent colour.

## What the user said is wrong with the UX (all four confirmed)

1. **Glance: who's hogging.** Opening the app should answer "what is
   using my internet right now, and is it a problem?" instantly.
2. **Quick split change.** Giving the game or YouTube more right now
   should take one or two actions, then close.
3. **The rule editor is heavy.** The side panel stacks five cards (Rule
   on, Slice, Priority, Speed limit with None/%/Mbps per direction).
4. **Numbers are unclear.** "2.5 of 25.0 Mbps ✎", "0.0 not shared out ✎",
   "30% · Normal", "Low latency" pill: the user can't read them at a
   glance.

Also seen in the user's screenshots (2026-11-16, real window, dark):
- The list's "Active" heading mixes live apps with quiet rules (greyed,
  "–" speeds); a quiet site rule shows a globe and pink "45% · Normal".
- The split bar's "Everything else" segment has an inner usage fill
  that reads as a rendering glitch, not as "using 48% of its slice".
- No app icons anywhere: every row is text only. PLAN.md §14.2/§14.3
  wanted real app icons; they are probably the single biggest lever
  for "premium" and for scanning the list. Getting them needs a Rust
  command in the app crate (exe icon via `SHGetFileInfoW` /
  `ExtractIconExW` → PNG data URL, cached by exe path; the IPC
  `AppInfo` already carries the family and exe paths) and bundled
  icons for the sites in `assets/sites.toml`.
- The header shows the line (25) but shaping runs at 23 (factor 0.92):
  two numbers for one idea.
- First run, the line dialog and the tray flyout have never been looked
  at by the user; review them too.

## Rejected so far (don't repeat)

1. **Windows 11 Fluent native** (PLAN.md §14.1, the current code):
   "plain, looks like a Windows application". Mica is gone anyway (it
   can't show through WebView2 here, commit 0f932a5); the page paints a
   solid ground.
2. **"Tide"** (artifact https://claude.ai/artifact/AKHsaevoGtTHXHgkNc9VMo,
   made this session): "complete AI slop". Treat it as the list of
   tells to avoid, all of which it had:
   - navy-black ground with a teal/cyan accent and a status pill with a
     glowing dot: the default "dark AI dashboard";
   - trendy display + UI font pairing (Sora + Manrope) chosen for mood,
     not purpose;
   - everything a rounded pill or a 16–26 px rounded card, all the same
     weight, no hierarchy from the shapes;
   - a grey helper paragraph under every block explaining the UI;
   - a cute governing metaphor (liquid in glass chambers) doing
     decoration instead of information;
   - a giant hero number like a SaaS metrics page;
   - "+2.1 borrowed" badges and other chip clutter;
   - controls for features that don't exist yet (profile picker, Pause).
   An earlier round of card mockups for a decision page (split-flap
   "Departure Board", "Liquid Levels", in `.impeccable/mocks/decision/`)
   was never chosen; the folder is scratch from this session and can be
   deleted.

## How to work with this user

- **Don't run impeccable's direction ritual** (concept-seed, the decision
  page server, comp rounds): the user said "forget impeccable". Design
  judgment is yours. `design-taste-frontend` 2.5 builds marketing sites
  and has no app-screen reference, so it doesn't fit either.
- **Ask with AskUserQuestion** (options window), recommended option
  first. Keep questions few; the user leaves taste to you but rejects
  what looks generic.
- **Show before building the whole thing.** Make two or three genuinely
  different directions as static mockups of the **main window at its
  real size (960 × 640)** with the user's real data (below), and let the
  user pick. Each must differ in structure and character, not just
  colour. For each, write one line on why it fits SnailNet and not any
  other app.
- **No browser automation or screenshots unless the user asks** in that
  turn. The user looks and pastes screenshots. For mockups: write HTML
  files and tell the user the paths to open, or publish an artifact, or
  ask whether you may screenshot them.
- **Commit and push** each finished, verified step on `main` without
  asking (no branches). Confirm before anything destructive.
- Library or API docs (Svelte, Tauri): look them up with Context7.
- A GateGuard hook blocks the first Write of each new file until you
  state who calls it, what data it touches and the user's instruction.
  `app/**` is exempt locally.

## Real data to design with

The user's machine: line ≈ 25 Mbps down / 40 Mbps up (fibre),
shaping factor 0.92 (23 Mbps shaped), browser **Brave**, apps seen:
Discord, Claude Code, Brave, Steam. Config: profile "Balanced", rules
`site:youtube = 45%` and `brave = 30%`, Everything else 25% (5% floor).
Typical live moment: YouTube 4K in Brave ~10 Mbps, Discord 2.5, a Steam
download wanting everything.

## Product truth (don't break)

Read `PRODUCT.md` (users, purpose, principles) and PLAN.md §14 (UI
spec). Keep:
- Percentages first, numbers second. Shares are guarantees, never
  cages: idle share flows to whoever needs it.
- Vocabulary: "slice", "goes first", "only when the line is free",
  "Everything else". Never "QoS", "class", "token bucket".
- A site rule (YouTube) applies in every app and beats the app's rule.
- States the UI must show (§14.8): starting, shaping, unshaped (no line
  speed), retrying, safe mode, service not running, client rejected,
  empty, unidentified traffic, many apps.
- Accessibility (§14.9): WCAG AA contrast, full keyboard (the split bar
  dividers move with ←/→), visible focus, no meaning by colour alone,
  screen-reader labels, reduced motion.
- §14.1 (Windows 11 native look) is **overridden** by the user: once a
  direction is agreed, rewrite §14.1 and the brand line in PRODUCT.md
  ("Native Windows 11 look") to the new direction.
- Not built yet, so don't show working controls for them: profiles
  (only "Balanced" exists), pause, block, speed test, notifications.
  `SetBlocked`/`Pause`/`RunSpeedTest` answer `Unsupported`.

## The code

- `app/src/App.svelte` (main window), `app/src/main.ts` (picks the main
  window or `Flyout.svelte` by window label).
- Components in `app/src/lib/components/`: `TitleBar` (custom, frameless
  window; caption buttons drawn in the page), `Header` (profile, line
  figures, opens `LineDialog`), `InfoBar` (states), `SplitBar` (drag
  dividers, keyboard), `AppList`, `RuleEditor` (side panel, also the
  picker for a new slice), `FirstRun`, `Flyout`, `SpeedFields`,
  `Segmented`, `ToggleSwitch`, `Snail` (glyph).
- Logic, keep and reuse: `service.svelte.ts` (runes store: link,
  snapshot, stats, `siteNames`, `saveRules`, `saveShares`, `saveLine`),
  `rules.ts` (pure rule edits, list rows, picker; tested), `format.ts`
  (number rules; tested), `line.ts`, `editor.svelte.ts`,
  `lineDialog.svelte.ts`, `types.ts` (mirrors the IPC JSON; keep in step
  with `crates/snailnet-ipc`).
- Look: tokens in `app/src/app.css` (Fluent light/dark/high-contrast
  today; replace freely), strings in `app/src/lib/i18n/en.json` (all UI
  copy goes there).
- Rust side (`app/src-tauri/src/`): `bridge.rs` (pipe to the service),
  `lib.rs` (commands `current`, `set_rules`), `line.rs`, `tray.rs` +
  `tray_icon.rs` (tray, flyout window 360 × 420), `window.rs` (Windows
  accent via `UISettings`; drop it if the new look ignores the accent),
  `autostart.rs`. Window: 960 × 640, min 720 × 480, `decorations: false`,
  opaque.

## Run it

1. Service (elevated, the user clicks UAC): a script in the scratchpad
   running `target\debug\snailnet-service.exe run --console` with output
   to a file, started with `Start-Process powershell -Verb RunAs
   -WindowStyle Hidden -ArgumentList '-NoProfile -ExecutionPolicy Bypass
   -File <script>'`. It uses `C:\ProgramData\SnailNet\config.toml` (the
   user's real config: it **shapes their real traffic** at 23 Mbps while
   it runs). Stopping it also needs elevation (`Stop-Process` in an
   elevated script). The installed service (`SnailNet`, stopped, manual)
   would reject a dev app.
2. `cargo build --workspace`, then `npm run tauri dev` in `app/` (run in
   the background; Vite HMR for Svelte, restarts on Rust changes).
3. Gates: in `app/`: `npm run check`, `npm run lint`, `npm test`; root:
   `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --
   -D warnings`, `cargo test --workspace`.

## Done when

- The user picked a direction from real-size mockups and says the
  built app looks premium and not generic (their screenshots, not your
  opinion).
- The four UX complaints are answered in the built app: glance, quick
  change, light editor, readable numbers.
- Main window, rule editor, line dialog, first run, flyout and every
  §14.8 state are in the new look; light theme dropped or redesigned
  to match.
- Gates green; PLAN.md §14.1 and PRODUCT.md updated; `docs/HANDOFF.md`
  updated; committed and pushed on `main`.
