# Handoff: where things stand, and what's next

Written 2026-11-20 (second pass, after the user looked at the UI in the
mock preview). Start a session with **"read docs/HANDOFF-next.md and
continue"**. Older briefs it points to: `docs/HANDOFF-wow.md` (features
1–3), PLAN.md §11.10 (the proof), §11.11 (the fix), §14.3 (the split),
§14.4 (the tray).

## Where things stand

On `main`, pushed. App gates green: in `app\` `npm run check`, `lint`,
`npx vitest run` (174 tests), `build`. No Rust changed since `aa5a025`
(Rust gates were green then). Version still **0.0.5**: nothing released
since.

| What | State | Left |
|---|---|---|
| **The drag bar** on the Split page | Built (`57f4800`, `splitDrag.ts`, `SplitBar.svelte`, PLAN.md §14.3) | Goes into the redesign (below) |
| **1 The proof**: ping with / without SnailNet | Built (`192b844`, `75340bc`) | **Live check** |
| **2 The one-tap fix** | Built end to end (`f4c8825`, `7e4e3e4`, `e5e6e8a`): `waiting.ts`, `fix.ts`, `fixer.svelte.ts`, `FixOffer.svelte`, Settings switch | **Live check** |
| **3 The living snail** (tray moods) | Built (`943f11f`) | The user's look at the real tray |
| Boost menu off-screen / cut off | Fixed (`41ec569`, `popover.ts` `menuSpot`, tested) | — |
| Windows services / Update icon | Windows glyph (`3b8d49a`) | Part of the icon work below |

### The mock preview (new, use it)

`cd app; npm run dev`, then open `http://localhost:5173/mock.html`
(`?view=flyout` for the tray panel, `?mode=streaming` for a mode with
slices). `src/mock.ts` answers the app's commands with Tauri's
`mockIPC` from a made-up snapshot and sends stats 4×/s: Steam filling a
25 Mbps line, Brave left waiting. Save, the fix and Undo change the
made-up snapshot, so the pages react. Dev only: not in the build. The
browser is allowed when the user asks for it (`no-browser-automation.md`).

## What the user asked for (2026-11-20, in their words, then the plan)

Do these before the release. A and B are a **redesign**: follow
`frontend-design-workflow.md` (direction first, from
`design-taste-frontend`'s `references/app-screens.md`; then build and
audit with `impeccable`). DESIGN.md doesn't exist yet, so write it as
part of this. Keep the Garden (PLAN.md §14.1: the snail's greens, cream
for what's on, Atkinson Hyperlegible) unless the user says otherwise:
the complaint is structure and clarity, not the palette.

### A. Now and Split pages: complete redesign

> "I want a complete redesign on our theme on the now page and the split
> page, they both look way too confusing and not very practical to the
> user, a new user will get overwhelmed."

What makes them confusing today (seen in the mock):

- **The Split page says the same thing four times:** the live "Right
  now" strip with its legend, the drag bar, a thin bar in every row, and
  Everything else's app chips. On top: column headings, an intro
  paragraph, a hint under the bar, "Only when the line is free: …", a
  fold row, and the Save bar over the list.
- **The Now page stacks five ideas:** the sentence (+ the fix button),
  the ping proof, four mode tiles, "Using your internet" with Boost
  buttons, and a second split card ("How Balanced shares your line")
  with its own bar, coach and buttons.
- Status words are long and technical-sounding ("not using 9.2 Mbps of
  its slice, so others get it", "15.4 Mbps over its slice, from room
  nobody needs", "using its whole slice and wants more").
- With no rules (Balanced), the Split page is one grey bar saying
  "Everything else 100%": nothing to do, nothing explained.

Direction to propose (put it to the user; it's their product taste):

- **One job per page.** Now = "what's happening, and is it OK?" (the
  sentence, the fix, who's using the line). Split = "who gets what"
  (one control, the drag bar; rows only hold settings).
- **One picture of the split, not four.** The drag bar is the split;
  live use can show inside it (a fill per segment) instead of a second
  strip; rows drop their own bars.
- **Progressive disclosure:** priority, limits, "only when free" and
  groups behind a row's Settings; the mode tiles smaller, or a switcher.
- **A good empty state** for a mode with no slices: what a slice is, and
  one button to give the app using the most right now a slice.
- Shorter status words (§14.7 microcopy rules still hold).
- Show 2–3 directions as real-size mockups (the user chose the Garden
  that way, 2026-11-17), then build the chosen one 1:1.

### B. Settings: revamp

> "the settings tab need a revamp because they are chaotic"

Today (`SettingsPage.svelte`) it's one long page of cards: Connection,
"What goes first" (focus boost, calls first, Gaming while playing, the
new fix switch, the first list), apps grouping, Windows, About, Reset.
Regroup by what people come for (my line; what goes first; apps;
startup and hotkey; help and about; start over), with fewer words per
item and rarely used things folded. Same design pass as A.

### C. Icons: a black shape in the tiles, and they don't look like the real ones

> "the icons still have a black element in them… brave for example look
> at the real icon"

- **Cause:** `brands/index.ts` `markOn` draws the mark white or
  near-black, whichever contrasts more with the brand colour. On Brave's
  orange (#FB542B), dark wins, so the lion is drawn black. And Simple
  Icons are one-colour silhouettes: the real Brave icon is an orange
  lion with white, Discord's is white on blurple, and so on.
- **Fix direction:** in the real app the **program's own .exe icon**
  (`icon.rs`, `icons.forTarget`) is the true icon and already wins.
  Check that it loads for every family on the user's PC (the mock has
  none, so the mock always shows logo tiles). For the logo fallback,
  either always draw a white mark on the brand tile (simplest, and it
  matches most real app icons), or bundle full-colour official marks
  where their licence allows (check each: Simple Icons is CC0, brand
  assets often aren't). Never draw look-alikes.

### D. A better icon family

> "I think we can do a lot better on our icon family too"

The mode tiles (Balanced, Streaming, Gaming, Quiet downloads) use
generic Fluent glyphs in squares. Draw a small custom set in the snail's
world (one stroke weight, authored SVG; the craft floor bans emoji and
unicode stand-ins) for the modes, the side rail (Now, Split, Settings),
Pause and Boost. Part of the redesign.

### E. Pause only when SnailNet is doing something

> "the pause button should appear only when snailnet is being used; when
> I have not enabled any boost or any split or anything at all it should
> not appear"

- Show Pause when any of these hold: the active mode has a slice (an
  enabled rule with share > 0), a non-normal priority or a limit; a
  Boost is on; an app is blocked; or it's already paused (so Resume
  stays reachable). Hide it otherwise, in the side rail
  (`PauseControl.svelte`) and the flyout.
- **Tell the user one thing first:** even with no rules SnailNet still
  shapes: it keeps ping low under load (that's the A–F grade). Hiding
  Pause then leaves no one-click "off" for that. Ask whether that's
  fine, or whether Pause should then live in the tray menu or Settings
  (principle 3: "one click pauses everything").

### F. Show a change with something subtle

> "when I change something, I give a boost or I change priority etc.,
> it should be shown the change with something subtle visually"

A short highlight on the thing that changed, where it is: the row's
background warms for about 1 s and fades, the new number settles in, a
boosted row's green sweeps in once (it already does in WhoCard). One
pattern for every change (boost, priority, share, limit, mode switch,
the fix); with `prefers-reduced-motion`, colour only. A small helper
(e.g. `flash.ts`, keyed by slice key and what changed) so pages don't
each invent one.

### Smaller things seen in the mock (fix in the redesign)

- The drag bar's readout floats over the "Right now" legend above it.
- The glance can say "It runs on room Everything else isn't using.
  Brave wants more room too." while Brave sits inside Everything else:
  the two halves contradict each other.
- **Slashed zeros:** Atkinson Hyperlegible draws 0 with a slash on
  purpose (0 ≠ O), and the user's screenshots read it as "Ø". The font
  has no alternate zero (every feature was tried). Either keep it or use
  a different face for figures only. Ask the user.
- The fix offer appeared about 11 s after load in the mock (the 12-of-15 s
  rule). Fine, but check it live.

## Still to do from before

1. **Live check of the fix (feature 2):** Steam (or `curl` with many
   connections) plus YouTube in Brave, no rules: the offer appears
   within ~20 s; one click makes it smooth; Undo restores the split;
   with YouTube paused, no offer. Known: the flyout can't see the main
   window's unsaved draft (PLAN.md §11.11).
2. **Live check of the proof (feature 1, one UAC click):** recipe in
   docs/HANDOFF.md "Try it live" (scratch data dir, `--pipe
   \\.\pipe\snailnet-live`). `snailnet speedtest` prints both grades.
   The ping **with** SnailNet should be clearly below **without** on the
   user's line; if it isn't, find out why before releasing.
3. **The user's look** at the tray moods (100 % / 150 % scaling, light
   and dark taskbar).
4. **Release:** v0.0.6 after A–F and the live checks (one release for
   everything is fine now). Bump the version as in `79ad30e`; the
   release notes say what wasn't seen live.

## Notes for whoever picks this up

- The installed v0.0.5 has no CLI, and the dev CLI can't talk to the
  installed service (only programs in its folder may connect). On
  2026-11-20 the user's internet felt slow while nothing was downloading
  (Brave browsing, Claude Code). I suggested Pause to test whether
  SnailNet's line speed is set too low, and to re-measure if so.
- `Measured` has `deny_unknown_fields`: an older service can't read a
  config the new one wrote (forward-only, PLAN.md §10.1).
- Tray art: `scripts/tray-icons.cjs` re-renders it after changes.
- Logos: `simple-icons@16.37.0`, named imports only, CC0; credits in
  `app/src/lib/brands/credits.json`.
- How the user works (memory): commit on `main` and push finished steps;
  decide small things yourself, but put taste decisions (the redesign
  direction) to the user with the question tool, recommended option
  first; never Windows notifications.
