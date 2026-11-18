# Handoff: SnailNet landing page

Start a fresh session with: **"read docs/HANDOFF-site.md and build the
landing page"**. Written 2026-11-18. The project handoff is
`docs/HANDOFF.md`; the app's look is `docs/HANDOFF-ui.md`, `PRODUCT.md`
and PLAN.md §14.

## The ask (user's words, 2026-11-18)

> "I want one page landing marketing page to download the application, it
> should be premium and visually pleasing and not overwhelming but it
> should market the product in a great way."

One page. Its job: make a home gamer/streamer on a shared connection get
it in five seconds, trust it, and press Download.

## Decisions already taken (Claude, user delegates; change only with reason)

- **Where:** `site/` at the repo root, plain static HTML + CSS + a little
  JS (no framework, no build step), deployed by a new workflow
  `.github/workflows/pages.yml` (GitHub Pages via Actions, on pushes to
  `main` that touch `site/**`). URL: `https://stergiosf.github.io/SnailNet/`.
  Enabling Pages in the repo settings is `gh api` work (the repo is
  public); do it, it is reversible.
- **Download link that never changes:** make the release also carry the
  installer under a fixed name, `SnailNet-setup.exe` (copy it in
  `scripts/build-installer.ps1` and upload it in `release.yml` beside the
  versioned file). The button links to
  `https://github.com/StergiosF/SnailNet/releases/latest/download/SnailNet-setup.exe`.
  A small script asks `api.github.com/repos/StergiosF/SnailNet/releases/latest`
  for the version and size to show under the button; if that fails, the
  button still works. **No `v*` release exists yet**: until one does, the
  button must not lead to a 404. Show "Coming soon · watch on GitHub" (or
  link the releases page) when the API says there is no release, and say
  so to the user.
- **Design workflow** (`~/.claude/rules/frontend-design-workflow.md`): a
  new public page, so **direction first with `design-taste-frontend`**
  (run its `draw.mjs`, pick the direction that carries SnailNet's own
  idea), then **build and audit with `impeccable`**. The project's own
  rules win where they disagree: the brand below is fixed.
- **No browser automation** unless the user asks
  (`~/.claude/rules/no-browser-automation.md`). Verify with code, an HTML
  validator, contrast arithmetic, and `curl` against a local server; say
  plainly what wasn't seen in a real viewport, and offer the user a
  local preview command (`npx serve site` or `python -m http.server`).

## Brand (fixed, from the app; PRODUCT.md "Brand Commitments", PLAN.md §14.1)

- **Name** SnailNet. **Tagline** "Put the bandwidth hogs in the slow lane."
- **The snail is the brand:** `app/src-tauri/icons/snail.svg` (and the
  PNGs beside it). Its shell spiral may sit faint behind the hero, as in
  the app's Now page.
- **"The Garden" palette** (`app/src/app.css`): ground `#0e2b26`, side
  `#0a211d`, surfaces `#143831` / `#1b443c` / `#24524a`, text
  `#f3eee2` / `#b8cbc4`, **cream `#efe8d6` for the one strong action**
  (the Download button), the shell's bright green for "goes first". Read
  the rest of the tokens there; reuse, don't invent.
- **Type:** Atkinson Hyperlegible Next (variable; the app bundles it
  from `@fontsource-variable/atkinson-hyperlegible-next`, OFL: self-host
  the woff2 in `site/`, keep its licence). One family, tabular numbers.
- **Voice:** plain, warm, short (README and `app/src/lib/i18n/en.json`
  are the reference). Percentages, not jargon. No "revolutionary", no
  "AI".

## What to say (true today; every claim must match the code/README)

1. **Hero:** the tagline; one sentence: SnailNet shares your PC's
   internet between apps and websites by percentage, so the game download
   and YouTube both keep going. Download button + "Free · open source
   (GPL-3.0) · Windows 10 and 11, 64-bit". The product beside it.
2. **The moment it fixes:** a game download eats the line, the stream
   buffers, ping spikes mid-match. One line, not a wall of text.
3. **How it feels (the centrepiece):** the split bar. Drag it: 40% to the
   download, 60% to YouTube, and the share is a guarantee when the line
   is busy, never a cage (what one doesn't use, the other gets). A small
   live demo in the page is the best "real product" here.
4. **Three things, no more:** website rules that work in any browser
   (YouTube, Twitch, Netflix…); games and calls go first (focus boost,
   low-latency shaping just below the line speed); modes switched in one
   click or Ctrl+Alt+S (Balanced, Streaming, Gaming, Quiet downloads).
5. **Trust:** it steps aside rather than break your internet (fails
   open, Safe mode, Pause; README "Help, my internet broke"); nothing
   about you leaves your PC (README privacy: no telemetry, no account;
   the only connections it makes are listed there).
6. **Download again** + requirements + links: GitHub, README, licence.
   **The SmartScreen note:** the installer isn't code-signed yet (PLAN.md
   §17.3), so Windows shows "Windows protected your PC": say "More info →
   Run anyway" honestly, near the button, small.

Don't claim: speeds above what was measured, "zero CPU", anything about
anti-cheat (untested), macOS/Linux/ARM, or numbers from PLAN.md §20 that
aren't met.

## Product visuals

Real product UI, not invented screens. Best: **screenshots from the user**
(Win+Shift+S) of the Now page, the Split page and the tray flyout, taken
with the real app; ask for them at the start and build with placeholders
of the right size meanwhile. Fallback: a faithful HTML rendering built
from the app's own components and tokens (`app/src/lib/components/`,
`app.css`), e.g. the split bar and a mode tile. Never a generic dashboard
mock.

## Quality bar

- Not overwhelming: one idea per screen, generous space, at most one
  motion moment worth noticing (the split bar settling), everything else
  quiet. `prefers-reduced-motion` respected.
- Fast: no framework, self-hosted font subset, images as AVIF/WebP with
  sizes; aim for a tiny page (checked through code, not a browser,
  unless asked).
- Accessible: real headings, alt text, focus rings, contrast checked
  against the tokens (the app's comments give the ratios), works at
  360 px wide and on a 27" screen. Light mode isn't needed (the Garden is
  dark by design), but Windows high contrast mode must stay usable.
- Meta: title, description, Open Graph image (the snail on the ground
  colour), favicon from the snail.

## Steps

1. Read PRODUCT.md, README.md, `app/src/app.css`, `snail.svg`.
2. `design-taste-frontend`: direction (draw.mjs), within the brand above.
3. Ask the user for the three screenshots (AskUserQuestion, recommended
   first: "I'll take them now").
4. Build `site/` with `impeccable`; then the stable installer name in
   `build-installer.ps1` + `release.yml`, and `pages.yml`.
5. `impeccable` audit; fix what it finds.
6. Commit + push on `main` (no branches); enable Pages; `curl` the live
   URL; report what was and wasn't seen.
