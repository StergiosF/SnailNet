---
name: SnailNet
description: Split this PC's internet between apps by percentage. Its own dark look, "Obsidian Jade".
colors:
  ground: "#0f1b18"
  side-rail: "#0a1311"
  surface-1: "#15221e"
  surface-2: "#1c2b27"
  surface-3: "#253631"
  text-primary: "#efece3"
  text-secondary: "#abbcb5"
  text-tertiary: "#80948b"
  cream: "#f0ece1"
  cream-hover: "#fbf8ef"
  on-cream: "#121716"
  shell: "#2db894"
  shell-hover: "#3fcaa5"
  on-shell: "#03241b"
  caution: "#e8b45c"
  critical: "#f0907c"
  ink-0: "#e8836a"
  ink-1: "#eab04e"
  ink-2: "#9cc7e8"
  ink-3: "#d3a0d8"
  ink-4: "#c9c46e"
  ink-5: "#f0a3a3"
  stone: "#a3aea9"
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
  segment: "8px"
  card: "16px"
spacing:
  gutter: "32px"
  section: "22px"
  row: "60px"
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
    height: "28px"
  boosted:
    backgroundColor: "{colors.shell}"
    textColor: "{colors.on-shell}"
  card:
    backgroundColor: "{colors.surface-1}"
    rounded: "{rounded.card}"
---

# SnailNet design

Obsidian Jade: deep mineral charcoal planes so the snail's living jade, the
warm ivory of "what's on" and the apps' own icons carry the colour. Chosen
by the user from an interactive mockup (2026-11-20,
`.impeccable/mocks/obsidian-jade.html`), replacing the Garden
(2026-11-17), whose green planes swallowed the jade and clashed with the
cream. Tokens live in `app/src/app.css`; PLAN.md §14 is the history and the
detailed spec.

## Overview

- **Operate, glanced at mid-activity.** People open SnailNet while a game
  downloads or a video stalls, look, change one thing, close it. Every page
  answers one question first:
  - **Now:** "what's happening, and is it OK?" The sentence as the heading,
    the one-tap fix under it; then "Last 10 minutes" over the list of who
    is using your internet; beside them the ping proof and how the mode
    shares your internet (the user's call, 2026-11-20: Now was too
    empty).
  - **Split:** "who gets what?" One sentence on what a share is, the drag
    bar, the shares.
  - **Settings:** "change how SnailNet behaves", grouped by what people
    come for.
- **One picture of each thing per page.** Now shows use over time (the
  chart) and now (a thin bar per app, in the same colours, so the list is
  the chart's legend), and the mode's split once, small; Split shows the
  split (the drag bar, live use along its foot). Never a second strip,
  legend or per-row bar saying the same thing again.
- **The mode is the app's state,** so its switch sits in the window's top
  strip on every page.
- **Progressive disclosure:** a share's priority, "only use spare speed"
  and limits are behind its chevron; an app's rarer actions (block it, it's
  a game, grouping) behind its `•••`; lists needed once fold.
- **Quiet and premium** (PRODUCT.md principle 5): no nagging, no
  dashboards nobody asked for. One Windows notification only, once ever:
  the first time the window is closed, "SnailNet is still running" in the
  tray (the user's call, 2026-11-20).
- **Start with Windows is asked, once:** after first run on a fresh
  install, a small dialog "Start SnailNet with Windows?" (Start with
  Windows · Not now). Settings → Windows and the tray menu change it
  later and always agree.

## Colors

- **Planes ("Blend", the user's pick, 2026-11-20: Obsidian with the old
  Garden's light):** the ground is charcoal with a breath of green
  (`#0f1b18`), the side rail and title strip a step darker (`#0a1311`),
  the one panel per page part a step lighter (surface-1), controls another
  (surface-2, surface-3). A soft jade light falls from above the content
  and pools low on the left (`--ground-light`); never a green wash on the
  planes themselves.
- **Ivory is "what's on":** the mode that's on, the one main action per
  view, focus rings, and the change warmth (below).
- **Jade is "goes first / boosted"** and the brand: the snail, a boosted
  row's wash and tag, the Boost glyph under the pointer, the open page's
  glyph in the rail, switches that are on.
- **Slice inks** (ink-0 to ink-5) only where they carry meaning: a share's
  segment, its dot and its use bar. Everything else is stone. Spare room is
  hatched, never colour alone.
- **Caution amber** for "waiting for speed" and Resume; **critical coral**
  for blocked apps and Reset everything.
- **Text in three steps** on the ground: 15:1, 8.6:1, 5:1. Tertiary is for
  units, hints, quiet glyphs and disabled text.

## Typography

- **Atkinson Hyperlegible Next** (bundled, OFL) for every word.
- **Digits are Windows' Segoe UI:** a `SnailNet Figures` face covering
  0–9 leads the stack, one face per weight. Atkinson slashes its zero and
  the user read it as "Ø" (their choice, 2026-11-20). Tabular numerals
  everywhere.
- **Scale:**
  - Now's sentence: 32 / 38, extra-bold, -0.025em.
  - Page titles: 28 / 34.
  - Panel and group titles: 16 bold, in plain words; never small capitals,
    never an eyebrow above a heading.
  - Body: 15–16 px. Labels and statuses: 13–14 px.
- **Plain, everyday words** (the user's call, 2026-11-20): "share", not
  "slice"; "your internet" or "speed", not "the line"; "Priority";
  "Only use spare speed"; Everything else. Statuses are short: "using
  all of its share", "waiting for speed", "most of your speed".

## Layout

- **Window:** 1280 × 820 by default, minimum 840 × 560. A 212 px side
  rail holds the snail and the name; Now · Split · Settings; Pause, only
  while SnailNet is doing something; the internet speed.
- **The top strip** is 44 px tall on every page (nothing jumps between
  pages): the mode switch at the gutter, the caption buttons at the right,
  everything else the window's drag region.
- **Pages:** a 32 px gutter (20 px under 900 px). Now is two columns when
  wide (the chart and the list, and a 316 px side column of two panels:
  the ping and the mode's split), one
  when narrow. Split caps at 860 px. Settings sets a 168 px index beside
  groups up to 680 px wide; narrow, the index steps aside.
- **Rhythm:** 22 px between a page's parts, 60 px rows, 8–14 px inside a
  row.

## Elevation & Depth

- **Softly lifted panels.** One panel per page part, lifted off the ground
  by a soft offset shadow (`--shadow-panel`, the Garden's depth; no
  border); rows inside it are parted by hairlines (`--stroke-card`) and
  lift one surface step on hover. Menus, dialogs and the toast alone are
  raised (`--shadow-raised`).
- **No side stripes, no cards on cards, no icon in a tinted square.**
  Notices are tinted by their tone with the glyph in full colour; a boosted
  row is a jade wash from the left with a Boosted tag.
- The shell spiral is drawn large and faint behind Now's heading. It is the
  page's one authored moment.

## Shapes

- Radii: 10 px controls, 8 px segments, 12 px rows and notices, 16 px
  panels. Small tags only where they mean something (Boosted).
- **Icons are SnailNet's own** (`Glyph.svelte`): drawn on a 24 box with a
  1.75 stroke, round ends, in the current text colour.
  - Now: the shell's spiral. Split: a bar cut in three. Settings: two
    sliders. Boost: two chevrons, "goes first". Pause: two lines. Internet
    speed: three arcs over a dot.
  - The modes: Balanced is a circle shared in three; Streaming a screen
    with play; Gaming a pad; Quiet downloads an arrow that comes down
    slowly, dotted. The user's own modes share a sliders glyph.
  - Never emoji, unicode stand-ins or Segoe Fluent glyphs. Caption buttons
    are drawn as thin strokes like Windows' own.
- **App icons:**
  - The program's own .exe icon wins.
  - Otherwise the brand's Simple Icons mark (CC0) in **white on the
    brand's tile**, as the real app icons are. Only a tile so light that
    white would vanish (Kick) takes the dark mark.
  - No logo: a letter tile. Never a look-alike.

## Components

- **Mode switch** (`ModeSwitch.svelte`): compact segmented radios (28 px
  in a 3 px dark track), each mode always named with its glyph; the one
  that's on is ivory with its glyph in deep jade. Narrow windows drop the
  glyphs, never the names. Each mode's one line is its tooltip and
  description; while a game runs a quiet "Playing" note sits beside it.
- **Last 10 minutes** (`Timeline.svelte`, `history.ts`): Now's main
  picture. One stacked band per app (the five busiest, then "Other
  apps"), each in its share's ink or a stone step, your speed as a dashed
  line, "10 min ago · 5 min ago · now" under it. Marks with a short label
  where something happened: a mode switch, a boost, a share given, an app
  that started. Hover reads any moment; hovering a row lights its band.
  Download / Upload and the total live in its header and drive the list
  too. The app keeps it from the stats it already gets, in 5 s steps,
  only while it runs: it starts empty, says so, and never invents data.
- **How {mode} shares it** (`SplitSummary.svelte`, Now's side column):
  the mode's shares as one small bar, a line each with priority, "at least
  N Mbps" and what it uses now; apps on spare speed counted in a
  sentence; "See the split". Balanced says every app gets a fair turn.
- **Using your internet** (`WhoCard.svelte`): one panel, busiest first.
  Each row: icon, name, short status, a 3 px bar of its part of your
  speed (its band's colour from the chart), Mbps, **Boost** and **`•••`**. Boost is a
  quiet text button that turns jade under the pointer; `•••` holds the
  rarer actions, reachable while boosted. Apps barely using anything are
  not listed: an "open but idle" line says so.
- **Ping proof** (Now's side column): live ping with SnailNet and the
  speed test's ping without it, as two bars, only while something fills
  your internet; when quiet, the speed test's A–F grades with its date.
  Never an invented idle ping.
- **Drag bar** (`SplitBar.svelte`): one ink-tinted segment per share with
  its name, percentage and guaranteed Mbps inside; Everything else
  hatched; ivory handles (open hand, closed while dragging); live use as a
  thin line along each segment's foot.
- **Share rows** (`SliceList.svelte`, `SliceRow.svelte`): one panel,
  hairlines between rows. Icon, name, ink dot, status (priority as words:
  "High priority ·"), − % +, a chevron. Opened, its options sit flat in
  the row, one per line (Priority, Only use spare speed, Speed limit,
  Remove). Spare-only apps fold under "N apps that only use spare speed".
  Everything else opens to its apps; one click gives an app a share.
  "Add an app or website" is the list's last row.
- **Save bar:** docked to the bottom of the page while there are unsaved
  changes; ivory Save (Ctrl+S), quiet Discard.
- **Menus** (`.popmenu`): every item leads with a grey glyph that lights
  on hover; danger items are coral.
- **Settings rows** lead with one quiet grey glyph, no tile; headings and
  the index carry none. The speed test's grades are big plain letters side
  by side (A jade, F coral).
- **Internet speed** (side rail): a quiet line over a hairline, "↓ 25
  Mbps ↑ 5 Mbps", with Change. A setting, never louder than live speeds.
- **Notices** (`NoticeBar.svelte`): tinted by tone (info blue, caution
  amber, critical coral), glyph in full colour, one action.
- **Empty split:** what a share is, "Give {app} a share" for the apps
  using your internet now, another app or website.
- **Pause:** a quiet rail item, shown while a rule is on in the mode
  that's on, a boost runs, an app is blocked, or SnailNet is paused
  (`pause.ts`); Resume is amber with the time left. The tray always has it.
- **Toast:** a quiet dark bar with a jade check, only when Undo is needed.
- **Change warmth** (`flash.ts`): anything the user changes warms with
  ivory at 13% and fades back in 1.1 s, where it happened (a share, a
  priority, a boost, a new share). It is colour only, so reduced motion
  keeps it. A new percentage settles in from 4 px below.
- **Motion** everywhere else explains state:
  - exponential ease-out, 120–560 ms;
  - pages rise in when you switch to them (and only then); the rail's
    highlight slides;
  - rows glide when the order changes; nothing loops.
  - `prefers-reduced-motion` drops travel and keeps colour.

## Do's and Don'ts

- **Do** answer the page's one question first. Add a second picture of the
  same thing only if the first can't carry it.
- **Do** put a setting behind its row's Settings when people need it once.
- **Do** say what changed in place: the warmth on the row, not a toast,
  unless it needs Undo.
- **Don't** add a strip, legend or per-row bar that repeats the drag bar.
- **Don't** use side stripes, cards on cards, icons in tinted squares, or an
  icon before every heading.
- **Don't** use a pop-up or Windows notification for anything, except
  the one-time "still running in the tray" notification and the one-time
  "Start with Windows?" question.
- **Don't** draw look-alike logos, or put the brand's mark in black on its
  colour.
- **Don't** put a kicker or eyebrow above a heading. Don't make a giant
  hero number, gradient text or decorative glass.
- **Don't** let SnailNet make your internet slower than it is without it
  (PLAN.md §6.5, §10.2): the UI never offers a setting that would.
