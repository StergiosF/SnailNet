# Handoff: where things stand, and what's next

Written 2026-11-20. Start a session with **"read docs/HANDOFF-next.md and
continue"**. The briefs it points to: `docs/HANDOFF-wow.md` (features 1–3),
PLAN.md §11.10 (the proof), §11.11 (the fix), §14.3 (the split), §14.4 (the tray).

## Where things stand

On `main`, pushed, all gates green (`cargo fmt`, `clippy -D warnings`,
`cargo test --workspace` incl. `app\src-tauri`; in `app\`: `npm run check`,
`lint`, `npx vitest run` = 134 tests, `build`). Version still **0.0.5**:
nothing released since.

| What | State | Left |
|---|---|---|
| **1 The proof**: ping with / without SnailNet, A–F grades | Built, service + app (`192b844`, `75340bc`) | **Live check** (below), the user's look |
| **3 The living snail**: calm / working / struggling tray icon | Built (`943f11f`) | The user's look at the real tray |
| **2 The one-tap fix** | 2.0–2.2 service (`9858dcd`, `751c480`); 2.3–2.6 app (`waiting.ts`, `fix.ts`, `fixer.svelte.ts`, `FixOffer.svelte`) | **Live check**, the user's look |
| **The drag slider** on the Split page | Built (`splitDrag.ts`, `SplitBar.svelte`, PLAN.md §14.3) | the user's look: drag feel, label fitting, the readout |
| No Windows notifications | Built: toasts gone, app doesn't ask for new-app events, Settings switches gone | — |
| Measure again in the Connection speed box | Built | user's look |
| Redesign: Now, Split, Save/Discard, boost look, real logos | Built (`75340bc`, PLAN.md §14.3) | user's look |

**Nothing UI has been seen in a real window** (no-browser rule). Every
screen above needs the user's eyes before release.

## Next, in order

### 1. ~~The drag slider on the Split page~~ (built 2026-11-20; the brief below is kept for the user's look)

"For the split design we should have the slider like the website where the
user can drag it." The website's demo (`site/index.html` `.dbar`,
`site/main.js` from "Dragging: the bar follows the pointer") is a bar of
coloured segments, one per slice, with a **cream handle** between them that
you drag; it snaps to 5 % on release; the handle is a real
`role="slider"` with arrow keys / PageUp / PageDown / Home / End and an
`aria-valuetext` naming both sides; a segment too narrow for its name shows
only its percentage.

This reverses a 2026-11-16 decision (PLAN.md §14.3: "the split bar with
draggable dividers is gone: dividers were hard to hit, small slices were
unreadable, and reading it took arithmetic"). Build it so those three
problems stay solved, and record the reversal in §14.3:

- **Where:** the top of the Split page, above the rows, as the page's main
  control for the active direction (Download / Upload switch stays). The
  rows with − / + stay below it: two ways to the same draft.
- **Handles between neighbours:** dragging the handle between slice A and
  B moves only A and B (their sum stays), so nothing else jumps. Everything
  else is the last segment and keeps its ≥ 5 % floor; a rule's ≥ 1 % floor
  holds (`split.ts` / `rules.ts`, §9.7). Snap to 5 % on release, follow
  the pointer while dragging (like the site).
- **Hard to hit → big targets:** the handle's hit area ≥ 32 px wide and the
  bar's full height (≥ 44 px tall), with the visible cream grip narrower.
  Pointer capture; a vertical swipe still scrolls (`touch-action: pan-y`,
  as the site's CSS does).
- **Small slices unreadable → labels that fit or step aside:** a segment
  shows logo + name + %; narrower, just %; narrower still, nothing, and the
  row below carries it. Hovering / focusing a segment highlights its row.
- **Arithmetic → say it in Mbps too:** while dragging, a small readout over
  the handle: "Steam 40 % · at least 8 Mbps when busy | YouTube 60 % · 12".
- **Into the draft:** every drag changes the draft (`draft.ts`), never
  saves; the Save bar appears as for − / +. "was 20 %" marks show on the bar
  too (a faint tick at the saved position).
- **"Only when the line is free" (0 %) slices** aren't on the bar (they
  have no share); a line under it lists them.
- **Accessibility:** each handle is its own slider ("Between Steam and
  YouTube"), `aria-valuetext` "Steam 40 %, YouTube 60 %"; high-contrast
  styles; reduced motion = no easing.
- **Flyout:** unchanged (− / + only; it's the quick panel).
- **Tests (vitest, pure):** a `splitDrag.ts` with `dragBetween(rules, i,
  pct) → rules` (sum kept, floors held, snap), handle positions from shares,
  label fitting (full / pct / none by width). Build with `impeccable`
  (no browser; say what wasn't seen).

### 2. ~~Finish feature 2 (the one-tap fix), app half~~ (built 2026-11-20; live check below still to do)

`docs/HANDOFF-wow.md` 2.3–2.6, with the rule the sim settled on (PLAN.md
§11.11), **not** the brief's first guess:

- Per family, each 250 ms tick (the app subscribes at 4 Hz, `STATS_HZ`):
  *pressed* = `acted > 0` or `queued_bytes > 0`. **Waiting** = pressed in
  ≥ 14 of the last 20 ticks **and** it got < ½ of an equal split of what
  the pressed apps (≥ 14) delivered in the window. Port
  `crates/snailnet-sim/src/waiting.rs` (`RULE`, `WaitWatch`) to
  `ledger.ts` / a `waiting.ts` with the same tests.
- Then `FixWatch` / `offerOf` / mutes (2.4), the button on the glance
  sentence + Undo + the Settings switch (2.5), docs (2.6). The Split page
  now edits a draft; the fix's one click saves directly (rev-checked) and
  must not clobber an open draft: if a draft is dirty, the offer says
  "Save or discard your split first" instead of a button.
- Live check: Steam (or two `curl`s with many connections) + YouTube in
  Brave, no rules → offer within ~20 s; one click → smooth; Undo restores;
  YouTube paused → no offer.

### 3. The live check for feature 1 (needs the user: one UAC click)

Recipe in docs/HANDOFF.md "Try it live" (scratch data dir, `--pipe
\\.\pipe\snailnet-live`). Run Measure; `snailnet top` shows `ping:` while
a big download runs and `(held back Nx)` on the losing app; the saved
`measured` has `loaded_up_ms` and `with_ms`; **with clearly below
without** on the user's line. `snailnet speedtest` now prints both grades
and waits for `with_ms`, so the CLI alone covers it. If the line shows no
difference, find out why before releasing: the app makes this claim in
numbers.

### 4. Release

The plan said one release per feature; they landed together. Suggested:
**v0.0.6** = proof + living snail + no notifications + redesign + slider,
after the live check and the user's look; **v0.0.7** = the one-tap fix.
Bump the version in the usual places (see the v0.0.5 commit `79ad30e`),
release notes say what wasn't seen live.

## Notes for whoever picks this up

- `Measured` gained fields and has `deny_unknown_fields`: an older service
  can't read a config the new one wrote (forward-only, PLAN.md §10.1).
- The "struggling" tray art is a coral shell, not the brief's drooping
  stalks (unreadable at tray size). Re-render after art changes:
  `scripts/tray-icons.cjs` (instructions at its top).
- Logos: `simple-icons@16.37.0` (devDependency, named imports only, CC0);
  Disney+, Prime Video, Teams, Edge, Vivaldi have no CC0 logo and keep
  letter tiles. Credits: `app/src/lib/brands/credits.json`.
- The boost bar's full length is inferred (the service keeps only the end
  time): the shortest menu length (1 h / 4 h) that fits what's left.
- `FamilyTick.acted` is per tick (diffed in the hub); a family with bytes
  queued but none delivered is listed with `bps: 0`.
