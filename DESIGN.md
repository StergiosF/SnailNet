---
name: SnailNet
description: Split this PC's internet between apps by percentage. Its own dark look from the snail icon, "the Garden".
colors:
  ground: "#0e2b26"
  side-rail: "#0a211d"
  surface-1: "#143831"
  surface-2: "#1b443c"
  surface-3: "#24524a"
  text-primary: "#f3eee2"
  text-secondary: "#b8cbc4"
  text-tertiary: "#8fa9a1"
  cream: "#efe8d6"
  cream-hover: "#fbf6ea"
  on-cream: "#0a211d"
  shell: "#2fa88e"
  shell-hover: "#3bbb9f"
  on-shell: "#062520"
  caution: "#eab04e"
  critical: "#f2907e"
  ink-0: "#e8836a"
  ink-1: "#eab04e"
  ink-2: "#9cc7e8"
  ink-3: "#d3a0d8"
  ink-4: "#c9c46e"
  ink-5: "#f0a3a3"
  stone: "#a6b5ad"
typography:
  page-question:
    fontFamily: "SnailNet Figures, Atkinson Hyperlegible Next Variable, Segoe UI, sans-serif"
    fontSize: "32px"
    fontWeight: 800
    lineHeight: "38px"
    letterSpacing: "-0.025em"
  page-title:
    fontFamily: "SnailNet Figures, Atkinson Hyperlegible Next Variable, Segoe UI, sans-serif"
    fontSize: "28px"
    fontWeight: 800
    lineHeight: "34px"
    letterSpacing: "-0.02em"
  card-title:
    fontFamily: "SnailNet Figures, Atkinson Hyperlegible Next Variable, Segoe UI, sans-serif"
    fontSize: "16px"
    fontWeight: 700
  body:
    fontFamily: "SnailNet Figures, Atkinson Hyperlegible Next Variable, Segoe UI, sans-serif"
    fontSize: "15px"
    fontWeight: 400
    lineHeight: 1.45
  label:
    fontFamily: "SnailNet Figures, Atkinson Hyperlegible Next Variable, Segoe UI, sans-serif"
    fontSize: "13px"
    fontWeight: 700
rounded:
  control: "10px"
  segment: "9px"
  card: "16px"
spacing:
  gutter: "32px"
  section: "22px"
  row: "58px"
components:
  button-main:
    backgroundColor: "{colors.cream}"
    textColor: "{colors.on-cream}"
    rounded: "{rounded.control}"
    height: "36px"
  button-soft:
    backgroundColor: "{colors.surface-2}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.control}"
    height: "36px"
  mode-on:
    backgroundColor: "{colors.cream}"
    textColor: "{colors.on-cream}"
    rounded: "{rounded.segment}"
    height: "34px"
  boosted:
    backgroundColor: "{colors.shell}"
    textColor: "{colors.on-shell}"
  card:
    backgroundColor: "{colors.surface-1}"
    rounded: "{rounded.card}"
---

# SnailNet design

The Garden: the snail icon's own colours, chosen by the user from real-size
mockups (2026-11-17). The 2026-11-20 redesign of Now, Split and Settings
("Direction 1 · One column", picked from three mockups, the others a
mixer and plain sentences) changed the structure, not the palette.
Tokens live in `app/src/app.css`; PLAN.md §14 is the history and the
detailed spec.

## Overview

- **Operate, glanced at mid-activity.** People open SnailNet while a game
  downloads or a video stalls, look, change one thing, close it. Every page
  answers one question, top to bottom, in one column:
  - **Now:** "what's happening, and is it OK?" The sentence as the heading,
    the one-tap fix under it, the ping proof, then who is using the line.
  - **Split:** "who gets what?" One sentence on what a slice is, the drag
    bar, the slices.
  - **Settings:** "change how SnailNet behaves", grouped by what people
    come for.
- **One picture of the line per page.** Now shows use (a thin bar per app);
  Split shows the split (the drag bar, live use along its foot). Never a
  second strip, legend or per-row bar saying the same thing again.
- **The mode is the app's state,** so its switch sits in the window's top
  strip above Now and Split, not inside either page.
- **Progressive disclosure:** a slice's priority, "only when the line is
  free", limits and grouping are behind its Settings; lists needed once
  (other networks, marked apps, grouping) fold.
- **Quiet and premium** (PRODUCT.md principle 5): no nagging, no
  dashboards nobody asked for, never a Windows notification.

## Colors

- **Ground and surfaces:** the shell's deep green is the ground, the body's
  darker green the side rail; cards a step lighter (surface-1), controls
  another (surface-2, surface-3). A soft shell-green light falls from above
  (`--ground-light`).
- **Cream is "what's on":** the mode that's on, the one main action per
  view, focus rings, and the change warmth (below).
- **Shell green is "goes first / boosted"** and the brand: a boosted row,
  the Boost glyph, the open page's glyph in the rail, a group's glyph.
- **Slice inks** (ink-0 to ink-5, all at least 5.7:1 on the ground) only
  where they carry meaning: a slice's segment and its use bar. Everything
  else is stone. Spare room is hatched, never colour alone.
- **Caution amber** for "waiting for room" and Resume; **critical coral**
  for blocked apps and Reset everything.
- **Text in three steps** on the ground: 13:1, 8.9:1, 6:1. Tertiary is for
  rings and disabled text only.

## Typography

- **Atkinson Hyperlegible Next** (bundled, OFL) for every word.
- **Digits are Windows' Segoe UI:** a `SnailNet Figures` face covering
  0–9 leads the stack, one face per weight. Atkinson slashes its zero and
  the user read it as "Ø" (their choice, 2026-11-20). Tabular numerals
  everywhere.
- **Scale:**
  - Now's sentence: 32 / 38, extra-bold, -0.025em.
  - Page titles: 28 / 34.
  - Card and group titles: 16 bold, in plain words; never small capitals,
    never an eyebrow above a heading.
  - Body: 15–16 px. Labels and statuses: 13–14 px.
- **Plain, everyday words** (the user's call, 2026-11-20): "share", not
  "slice"; "your internet" or "speed", not "the line"; "Priority";
  "Only use spare speed"; Everything else. Statuses are short: "using
  all of its share", "waiting for speed", "most of your speed".

## Layout

- **Window:** 1120 × 740 by default, minimum 840 × 560. A 208 px side
  rail holds:
  - the snail and the name;
  - Now · Split · Settings;
  - Pause, only while SnailNet is doing something;
  - the line.
- **The top strip** is 32 px tall with the caption buttons. On Now and
  Split it grows to 76 px to hold the mode switch at the 32 px gutter;
  everything around the switch stays the window's drag region.
- **Pages:** one column with a 32 px gutter. Now caps at 880 px. Settings
  sets a 184 px index beside groups up to 680 px wide; under 980 px the
  index steps aside.
- **Rhythm:** 22 px between a page's parts, 58 px rows, 8–14 px inside a
  row.

## Elevation & Depth

- **One elevation per element, never a border with it.** Cards sit on
  `--shadow-card` (an offset soft shadow, with the light catching the top
  edge); menus, dialogs and the save bar on `--shadow-raised`.
- Rows inside a card are flat: hover lifts them one surface step.
- The shell spiral draws itself once, large and faint (about 9% opacity),
  behind Now's heading. It is the page's one authored moment.

## Shapes

- Radii: 10 px controls, 9 px segments, 16 px cards. Pills only for small
  marks (Boosted, Playing, High priority).
- **Icons are SnailNet's own** (`Glyph.svelte`): drawn on a 24 box with a
  1.75 stroke, round ends, in the current text colour.
  - Now: the shell's spiral. Split: a bar cut in three. Settings: two
    sliders. Boost: two chevrons, "goes first". Pause: two lines.
  - The modes: Balanced is a circle shared in three; Streaming a screen
    with play; Gaming a pad; Quiet downloads an arrow that comes down
    slowly, dotted. The user's own modes share a sliders glyph.
  - Never emoji or unicode stand-ins. A few Segoe Fluent glyphs remain
    inside menus and dialogs until they're redrawn.
- **App icons:**
  - The program's own .exe icon wins.
  - Otherwise the brand's Simple Icons mark (CC0) in **white on the
    brand's tile**, as the real app icons are. Only a tile so light that
    white would vanish (Kick) takes the dark mark.
  - No logo: a letter tile. Never a look-alike.

## Components

- **Mode switch** (`ModeSwitch.svelte`): segmented radios, 44 px tall,
  in a 76 px top strip. Each mode's mark is bold and filled, as on the
  first mode tiles, on a small chip; the one that's on is cream spreading
  from its mark, its chip shell green. Each mode's one line is its tooltip and
  description; Gaming says Playing while a game runs.
- **Using your internet** (`WhoCard.svelte`): busiest first. Each row has
  its icon, name, short status, a 4 px bar of its part of the line (its
  slice's ink, stone in Everything else), Mbps and Boost. A boosted row is
  shell-green with a ring, a Boosted mark and a bar that shortens.
- **Drag bar** (`SplitBar.svelte`): one segment per slice, cream handles,
  live use as a 4 px line under each segment. The readout shows under the
  bar while dragging.
- **Share rows** (`SliceList.svelte`, `SliceRow.svelte`): one card,
  hairlines between rows. Icon, name, status, − % +, a chevron. Priority
  tints the row (High a shell-green wash from the left with a "High
  priority" chip, Low a shade darker). Opened: a dark panel with one
  control per line (Priority, Only use spare speed, Speed limit), the
  explanation in the tooltip, Remove under it. Everything else opens to
  its apps; one click gives an app a share.
- **Drag bar** cursors: an open hand over a handle, a closed hand while
  dragging. Live use is a thin line inside each segment's foot.
- **Menus** (`.popmenu`): every item leads with a grey glyph that lights
  on hover; hover is a cream wash, danger a coral one. No native
  `<select>`: lists open in place, or `Dropdown.svelte`.
- **Boost** is shell green on a soft green tile (`--shell-bright`).
- **Settings rows** lead with a quiet grey glyph on a dark tile. The
  speed test's grades sit side by side, With SnailNet tinted green.
- **Internet speed** (side rail): a small card, download and upload
  side by side as big figures over "Mbps down" and "Mbps up".
- **Empty split:** what a slice is, "Give {app} a slice" for the apps
  using the line now, another app or website, and a 30% example in Mbps.
- **Pause:** shown in the window while a rule is on in the mode that's on,
  a boost runs, an app is blocked, or SnailNet is paused (`pause.ts`). The
  tray menu always has it.
- **Change warmth** (`flash.ts`): anything the user changes warms with
  cream at 13% and fades back in 1.1 s, where it happened:
  - a slice's share, priority, on/off or limit;
  - a boost starting or stopping;
  - a new slice, from the fix.

  It is colour only, so reduced motion keeps it. A new percentage settles
  in from 4 px below.
- **Motion** everywhere else explains state:
  - exponential ease-out, 120–560 ms;
  - pages rise in and the rail's highlight slides;
  - rows glide when the order changes; nothing loops.
  - `prefers-reduced-motion` drops travel and keeps colour.

## Do's and Don'ts

- **Do** answer the page's one question first. Add a second picture of the
  same thing only if the first can't carry it.
- **Do** put a setting behind its row's Settings when people need it once.
- **Do** say what changed in place: the warmth on the row, not a toast,
  unless it needs Undo.
- **Don't** add a strip, legend or per-row bar that repeats the drag bar.
- **Don't** use a pop-up or Windows notification for anything.
- **Don't** draw look-alike logos, or put the brand's mark in black on its
  colour.
- **Don't** put a kicker or eyebrow above a heading. Don't make a giant
  hero number, gradient text or decorative glass.
- **Don't** let SnailNet make the line slower than it is without it
  (PLAN.md §6.5, §10.2): the UI never offers a setting that would.
