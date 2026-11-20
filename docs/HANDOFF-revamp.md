# Handoff: SnailNet UI Revamp — Revise Obsidian Jade Mockup (Not Approved Yet)

Start a fresh session with: **"read docs/HANDOFF-revamp.md and revise the Obsidian Jade mockup"**.

Updated 2026-11-20. **Current status: Full-app design audit is DONE; v3 interactive Obsidian Jade mockup is built on disk, NOT approved yet.** Do **not** edit `app/src/` until the user reviews and explicitly approves a revised Obsidian Jade mockup direction.

---

## 1. Current State of the Repo

### Already committed & pushed (verified green)
- **Tray Quit cleanly stops `snailnet-service.exe`**:
  - Added `Request::Stop` to IPC (`crates/snailnet-ipc/src/lib.rs`) and handled it in `crates/snailnet-service/src/ipc.rs` (post-`Hello` from verified in-folder SnailNet binary; replies `Ack` then signals `StopSignal` to flush queues, unload WinDivert, and exit `0` so Windows SCM doesn't restart it).
  - Wired **Quit SnailNet** in `app/src-tauri/src/tray.rs` to hide windows/tray immediately, stop reconnect loop, call `changes::stop_service_blocking()` (3 s cap + non-elevated SCM fallback in `service_start::stop_plain()`), and exit. Closing the main window (`X`) still hides to tray.
  - Updated `README.md` and `PRODUCT.md`.
- **No Svelte/CSS edits have been made yet** — `app/src/` is still untouched v0.0.6 ("Garden").

### Scratch mockups on disk (gitignored under `.impeccable/`)
- **`.impeccable/mocks/obsidian-jade.html`** — **v3 full-app Obsidian Jade interactive prototype (NOT approved yet — revise this next!)**.
  - Clickable URL for Brave: `file:///C:/Users/sfoto/Desktop/SnailNet/.impeccable/mocks/obsidian-jade.html`
  - Covers **Now**, **Split** (interactive 60 fps drag bar), **Settings**, **Tray Flyout (`380 × 500`)**, and **Dialogs** (`Connection speed`, `Add app or website`), with live scenario switches (`Busy · Waiting`, `Calm · Idle`, `Boosted · Streaming`, `Gaming Auto`, `Quiet downloads`) and viewport switches (`Desktop 1280 × 820`, `Min 840 × 560`, `Tray Flyout 380 × 500`).
- **`.impeccable/mocks/now-redesign.html`** — earlier v2 Now-only baseline kept for comparison.

---

## 2. Complete Design Audit of Live `app/src/` (Already Completed)

Full source inspection across all 5 surfaces uncovered these concrete issues that the final revamp must solve (no need to re-audit from scratch):

### A. App Shell & Chrome (`App.svelte`, `Sidebar.svelte`, `TitleBar.svelte`, `ModeSwitch.svelte`, `PauseControl.svelte`, `NoticeBar.svelte`, `app.css`)
1. **Swamp-green chroma muddiness + leaked Garden hexes**: `app.css` (`#0e2b26` ground, `#0a211d` rail, `#143831–#24524a` surfaces, heavy top green wash) swallows the Snail's jade and clashes with cream. Hard-coded Garden colors also leak into 9 files: `brands/index.ts:81-84` (`TILE = "#1b443c"`, `DARK = "#0a211d"`), `SplitBar.svelte:413-420`, `Toast.svelte:76`, `PauseControl.svelte:109-112`, `ConfirmDialog.svelte:89-93`, `Flyout.svelte:248`, `ModeSwitch.svelte:133`, `Snail.svelte:6-26`, and `flash.ts:13`.
2. **44 px top-bar vertical jump & bulky `ModeSwitch`**: `TitleBar.svelte:64-66` stretches from `32px` to `76px` on Now/Split and snaps back to `32px` on Settings. `ModeSwitch.svelte:26-43` duplicates its own separate SVG icon set instead of using `Glyph.svelte`, renders bulky `30×30px` icon squares, and collapses inactive modes to **icon-only** under `1000px` (`ModeSwitch.svelte:213-228`).
3. **Global reduced-motion kill breaks color feedback**: `app.css:585-593` forces `0.01ms !important` on all animations/transitions, breaking `flash.ts` warmth and violating `DESIGN.md` ("drops travel and keeps colour").
4. **Oversized speed card in rail**: `Sidebar.svelte:176-233` shows configured down/up speeds in a `14px`-rounded card at `22px` extra-bold — louder than live app speeds (`15px`) on `Now`.
5. **Flat `NoticeBar`**: Uses raw Segoe Fluent glyphs (`NoticeBar.svelte:9-13`) and identical neutral `--surface-1` backgrounds for info/caution/critical.

### B. Now Screen (`NowPage.svelte`, `WhoCard.svelte`, `BoostButton.svelte`, `ProofRow.svelte`, `ledger.ts`, `modes.ts`)
1. **[P0] Calm-state contradiction**: `ledger.ts:259-263` (`glance()`) ignores traffic `< QUIET_BPS` (`50_000`) and prints *"Nothing is using your internet right now..."*, while `ledger.ts:119` + `modes.ts:166-179` (`users()`) still pass `steady.ts`-held `<0.1 Mbps` apps straight into `WhoCard` under *"Using your internet"* with a broken-looking `2px` dot on a `500px` track.
2. **[P1] Secondary app controls buried in `Boost` — and unreachable while boosted**: `BoostButton.svelte:111-160` buries **Block its internet**, **It's a game**, **Never let it go first**, and **Split/Merge programs** inside the `Boost` popover, and replaces the menu trigger with plain `"Stop boost"` (`L85-92`) while boosted. Must split **Boost** from **`•••`**.
3. **[P1] 60% empty void & Download-only blind spot**: Real default window in `app/src-tauri/tauri.conf.json:18-19` is **`1280 × 820`** (min `840 × 560`), not `1120 × 740`. `NowPage` caps at `880px` single-column, leaving ~60% void, and `App.svelte:59` hardcodes `Now` to download only (no way to see upload saturation).
4. **[P2] Hidden ping proof at rest & orphaned mode summary**: `ProofRow` disappears unless >90% busy for 3 s+; `NowPage.svelte:62-74` leaves `"Balanced doesn't guarantee any app a share yet"` loose at the bottom.

### C. Split Screen (`SplitPage.svelte`, `SplitBar.svelte`, `SliceList.svelte`, `SliceRow.svelte`, `SaveBar.svelte`)
1. **[P1] False empty state on built-in `Quiet downloads`**: `SplitPage.svelte:39-40` treats `sliced === 0` (`share > 0` only) as `empty`, showing `"No app has a guaranteed share yet"` above active spare-only rules (`Steam 0%`, `Epic 0%`).
2. **[P1] Unbounded width**: `SplitPage` has no `max-width`, stretching `SplitBar` and detaching app names from `− % +` steppers by 700+ px on `1280 × 820` / maximized displays.
3. **[P2] Row clutter, floating `SaveBar`, dark Snail**: `SliceRow` stacks priority gradients + chips + box-in-box `.panel` drawer; `SliceList:175-210` uses a `60px` ghost card for `"Add an app or website"`; `SliceList:39` mounts `<Snail>` without `light` (dark-on-dark); `SaveBar:23` uses raw Segoe glyphs inside a tinted square and floats `sticky` mid-page.

### D. Settings Screen (`SettingsPage.svelte`, `UpdateRow.svelte`, `Dropdown.svelte`, `Segmented.svelte`)
1. **[P1] Triple icon repetition + icon-in-a-tinted-square**: Icons appear in `.index`, before every `h2.group-title`, and wrapped in `36×36px` `.ic` boxes on every row (CSS duplicated verbatim in `UpdateRow.svelte:49-66`).
2. **[P2] Nested grade boxes & invisible dark dividers**: `.grades` nests sub-cards + colored letter squares inside the Connection card; `.item + .item` uses `rgb(0 0 0 / 0.18)` which vanishes on Obsidian surfaces.

### E. Tray Flyout, First Run & Dialogs (`Flyout.svelte`, `FirstRun.svelte`, `LineDialog.svelte`, `NameDialog.svelte`, `SpeedFields.svelte`, `AddSlice.svelte`, `Toast.svelte`)
1. **[P1] Broken Pause label in Flyout**: `Flyout.svelte:114` uses `t("pause.1h")` → renders literally **`"For 1 hour"`** without the word `"Pause"`.
2. **[P1] Empty-feeling Flyout in default `Balanced`**: Shows configured split rules instead of active apps — in `Balanced` (0 rules), the `380 × 500` flyout shows one disabled `Everything else 100%` row and ~220 px of blank space.
3. **[P1] Reversed Tab order in `LineDialog` (WCAG 2.4.3)**: `LineDialog.svelte:116-124,154` uses `flex-direction: row-reverse`, walking keyboard Tab backwards (`Save → Cancel → Measure`).
4. **[P2] Dark Snail, inconsistent inputs, loud Toast**: `FirstRun.svelte:104` mounts `<Snail>` without `light`; 3 divergent input styles across `NameDialog`, `AddSlice`, and `SpeedFields`; `Toast.svelte` is a glaring full-cream pill.

### F. Systemic Icons & Vocabulary Drift
- **16 remaining Segoe Fluent glyphs** (`class="icon"`, `&#xE...;`) across 11 files (`AddSlice`, `AppIcon`, `FirstRun`, `Flyout`, `LineDialog`, `NoticeBar`, `ProofRow`, `SaveBar`, `SpeedFields`, `TitleBar`, `Toast`, `WhoCard`) — replace with unified `1.75px`-stroke SVGs in `Glyph.svelte`.
- **Retired vocabulary still in `en.json`**: `"slices"` (`L217`), `"your line"` (`L260`, `L274`, `L282`, `L394`) violate `PRODUCT.md` — replace with `"shares"` and `"your internet"` / `"your speed"`.

---

## 3. User Taste & Locked Direction

- **Theme locked to Obsidian Jade ONLY** — deep mineral obsidian-charcoal planes so living jade (`#2db894` / `#5ee0bd`), warm ivory primary CTA (`#f0ece1`), and real brand app icons have high contrast and calm authority. No light mode, no Deep Spruce, no Garden green wash.
- **Rejected ("AI slop / chaotic")**: Floating bordered card soup, badge/pill spam, icon-in-a-tinted-square syndrome, icons before every heading, walls of bright green Boost buttons, repeating the same metric in multiple places.
- **Target bar**: Elevate visual craft & desktop-software personality so it feels neither like AI-slop cards NOR like a sterile/flat generic wireframe.

---

## 4. Next Session Plan (Start Here!)

### Step 1 — Revise the Obsidian Jade Mockup Until Approved
1. Ask the user what felt off or needs changing in `.impeccable/mocks/obsidian-jade.html` (or if they share specific feedback / reference points, iterate directly on `.impeccable/mocks/obsidian-jade.html`).
2. Keep every `PRODUCT.md` honesty rule intact in the mock:
   - Never fabricate live idle ping when quiet (`StatsTick.ping` is absent when quiet — show clearly dated `config.line.measured` baseline & A–F grades instead).
   - Honor `pause.ts` visibility rules (`Pause` in window rail only when rules/boost/block/pause active; tray always has it).
   - Never show fake sparklines/charts the service doesn't emit.
3. Run silent quality check: `impeccable detect --json .impeccable/mocks/obsidian-jade.html` and verify JS syntax.
4. **Always give the clickable `file:///` URL and Windows path** (shell `Start-Process` runs in another session and won't open Brave on the user's desktop).
5. **Wait for explicit user approval of the mockup** before touching `app/src/`.

### Step 2 — Implement Approved Obsidian Jade System Across `app/`
Once the user approves the revised mockup:
1. Update tokens and base styles in `app/src/app.css` (plus fix `prefers-reduced-motion` so color feedback stays intact, update `brands/index.ts` and `flash.ts` constants, remove all hard-coded Garden hexes).
2. Redesign shell (`App`, `Sidebar`, `TitleBar`, `ModeSwitch`, `PauseControl`, `NoticeBar`) and **Now** (`NowPage`, `WhoCard`, `FixOffer`, `ProofRow`/`ProofBars`, split `BoostButton` into Boost + `•••`, fix `ledger.ts` calm-state contradiction with unit tests).
3. Redesign **Split** (`SplitPage` max-width + fix `Quiet downloads` empty state, `SplitBar`, `SliceList`, `SliceRow`, docked `SaveBar`, `Stepper`, `AddSlice`).
4. Redesign **Settings** (`SettingsPage`, `UpdateRow`, `SpeedFields`, `Dropdown`, `Segmented`) — strip triple icons & `.ic` squares.
5. Redesign **Tray Flyout** (`Flyout.svelte` — fix `"Pause 1h"` label, active apps + Boost in Balanced), **FirstRun** (fix `<Snail light />`), **Dialogs** (fix `LineDialog` tab order, unify inputs), **Toast**, and replace all 16 Segoe Fluent glyphs with `Glyph.svelte` SVGs + clean `en.json` copy.
6. Update `DESIGN.md` and `PLAN.md §14.1` to document the shipped Obsidian Jade system.

---

## 5. Non-Negotiable Constraints & Verification Gates

- **Typography**: Bundled `Atkinson Hyperlegible Next` + unslashed tabular `SnailNet Figures` (`Segoe UI` digits 0–9; never slashed zeros).
- **Accessibility**: WCAG 2.2 AA contrast everywhere, keyboard nav + `:focus-visible`, `prefers-reduced-motion` preserving color-only feedback, Windows `forced-colors: active`, `aria-live="off"` on 4 Hz live numbers.
- **Workflow**: Do **not** run Impeccable's interactive `concept-seed` / `serve-question` decision-page ritual; iterate directly in HTML mocks. PowerShell 5.1 shell: chain with `; if ($LASTEXITCODE -eq 0) { ... }`.

### Frontend gates (run from `app/`):
```powershell
npm run check; if ($LASTEXITCODE -eq 0) { npm run lint }; if ($LASTEXITCODE -eq 0) { npm test }; if ($LASTEXITCODE -eq 0) { npm run build }
```

### Rust gates (run from repo root if Rust/IPC changed):
```powershell
cargo fmt --all --check; if ($LASTEXITCODE -eq 0) { cargo clippy --workspace --all-targets -- -D warnings }; if ($LASTEXITCODE -eq 0) { cargo test --workspace }
```
