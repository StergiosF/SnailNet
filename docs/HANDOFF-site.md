# Handoff: revamp the SnailNet landing page

## Done: v2 shipped (2026-11-18)

The revamp below is built and live; the rest of this file is the brief it
was built from, kept for its research and its "why v1 looked bad".

- **Choices the user made:** headless screenshots allowed while building;
  **no measured ping numbers on the page** (the ping picture is an
  unnumbered illustration, labelled "lower is better", its alt text says
  it's a sketch); no app screenshots (visuals are HTML drawn from the
  app's components); design references left to Claude.
- **Composition (top to bottom):** centred hero with the app itself, its
  Now page drawn in HTML from the app's components (side rail, the four
  modes, "Using your internet", the split card; static, `role="img"`);
  "Ping stays low while Steam downloads" (three beats beside the ping
  illustration); "How the split works, in 3 steps" on the lighter band,
  with the live split demo to try under the steps; four features as
  a 2×2 over two screens (website slices, Boost, modes, stepping aside and
  Pause); privacy on a **cream** panel; six open FAQ answers as cards with
  icons; the close on a **shell green** panel (dark button, the snail on
  its trail) and the footer on the ground. Cream and green are panels set
  on the dark ground, never full-width flips; sections size to their
  content (no screen-height minimums). The ping chart and the step bars
  draw once as they scroll into view (none with reduced motion). Deal and reasons are in the
  comment at the top of `site/index.html`.
- **Checked:** `design-taste-frontend` `check.mjs` has no findings at
  1440×900, 1280×800, 1024×768, 820×1180 and 768×1024; perf about 70 KB.
  Accepted against the skill's rules on purpose: the tagline stays the h1
  (brand), the release-button wording ("Coming soon · watch on GitHub")
  stays (tested plumbing), and on a 390 phone sections are taller than one
  screen because the stacked content needs it (the phone hero grows so all
  of the app window stays in view).
- **Tests:** `scripts/site-test.cjs` (jsdom from a temp install; usage in
  its header) covers the download buttons, the demo and the feature tabs: 12 tests.
- **First release, v0.0.1, published 2026-11-18** (tag pushed, Release
  workflow built and drafted it, then published with notes). The site's
  buttons now link `releases/latest/download/SnailNet-setup.exe` and show
  "Version 0.0.1 · 4.4 MB" (checked live). The section "Where things
  stand" below predates it.
- **v0.0.2, published 2026-11-19:** plainer mode words (app, README,
  site) and drawn, centred stepper buttons. Same path: version bumped
  everywhere (the crates pin each other at an exact 0.0.x, so those pins
  move too), tag pushed, Release drafted it, published with notes;
  `releases/latest/download/latest.json` answers 0.0.2.
- **v0.0.3, published 2026-11-19:** steady app lists (stats smoothed,
  quiet apps held 8 s, `app/src/lib/steady.ts`), an Update button in the
  side rail only when a version is out (quiet check on start and every
  6 h), regrouped Settings in plainer words, larger window, installer
  renames an old `snailnet.exe`. The site's and README's privacy lists
  now say the update check runs on its own.
- **v0.0.4, published 2026-11-19:** calls go first (PLAN.md §11.9),
  released before a live call test (user's choice). The site's "Games and
  calls go first" tab and the README say so.
- **The snail** was also fixed the same day: its eye stalks now grow from
  the head (app icon set, tray, Snail.svelte, README pictures, favicons,
  og.png).

---

Start a fresh session with: **"read docs/HANDOFF-site.md and revamp the
landing page"**. Written 2026-11-18, after v1 of the page shipped. The
project handoff is `docs/HANDOFF.md`; the app's look is
`docs/HANDOFF-ui.md`, `PRODUCT.md` and PLAN.md §14. (The first page's
build brief is this file's git history, commit 0b4797e.)

## The ask (user's words, 2026-11-18)

> "I like the readme the website looks bad I don't like it I want a
> complete revamp or something to be a lot more visually pleasing and
> understandable and a bit more effective to the user, search the internet
> for ideas and practices and do the revamp."

Earlier the same day, about v1: the cream button's white glow was disliked
(fixed); "overall the page look a bit boring and not very user friendly
and a bit monotoned"; asked for "a better color pallet that matches our app
theme". The **README** (rewritten that day) is liked: its structure and
voice are a good model for the page's words.

This is a **redesign**, not a polish: keep the product truth, the brand
commitments and the plumbing below; replace the composition and the look of
v1. Don't polish v1.

## Where things stand

- Live: <https://stergiosf.github.io/SnailNet/>, from `site/`
  (`index.html`, `styles.css`, `main.js`, `fonts/`, `assets/icons/`,
  favicons, `og.png`). Deployed by `.github/workflows/pages.yml` on pushes
  to `main` that touch `site/**` (Pages is enabled, build type workflow).
- Repo About, homepage and topics are set (2026-11-18).
- No `v*` release exists yet: GitHub's `releases/latest` answers 404.

### Keep (plumbing that works and is tested)

- **The download logic in `site/main.js`** (`checkRelease`/`showRelease`):
  buttons start on the releases page (never a 404); with a release carrying
  `SnailNet-setup.exe` they link to
  `https://github.com/StergiosF/SnailNet/releases/latest/download/SnailNet-setup.exe`
  and show "Version x · y MB"; an older release falls back to the
  versioned `*_x64-setup.exe`; with no release they read
  "Coming soon · watch on GitHub" and link the repo; offline / rate-limited
  they stay as they are. 10-minute sessionStorage cache in try/catch.
  `scripts/build-installer.ps1` writes the fixed-name copy and
  `release.yml` uploads it. Keep the `data-download`, `data-download-label`
  and `data-release` hooks (or port the logic with them).
- **The split demo's model** in `main.js` (20 Mbps line so 5% = 1 Mbps;
  Steam wants everything, the video about 9 Mbps; statuses in the app's own
  words from `app/src/lib/i18n/en.json` `status.*`; drag with pointer
  capture and `touch-action: pan-y`, keyboard slider, − / + by 5%, Pause
  the video, polite live region). A jsdom test of all of it was in the
  last session's scratchpad (gone); rewrite one beside the page if the demo
  stays (`site/` must stay buildless; put tests outside it, e.g.
  `scripts/site-test.cjs` with jsdom from a temp install).
- Self-hosted **Atkinson Hyperlegible Next** (latin woff2 + `OFL.txt`), the
  real brand icons in `site/assets/icons/` (fetched with the
  design-taste-frontend `icons.mjs`; `icons-credits.json` beside them;
  footer line "Product names and logos are trademarks of their owners").
- Meta: title, description, canonical, Open Graph image (`og.png`, the
  snail on the ground colour, 1200×630), favicons from the snail.

### Why v1 looked bad (diagnosis, so the revamp doesn't repeat it)

1. **It was never looked at.** v1 was verified only through code (the
   no-browser rule). Nobody saw the real render at 1440, 768 or 390 before
   it shipped. This is the main cause; see "Process" below.
2. **Text-left / small-window-right hero**: the app window was rendered at
   ~600 px with 12–14 px UI text, so it read as a busy thumbnail, not as
   the product. The study below found centred heroes with a big visual
   *below* the headline on the vast majority of good tool pages.
3. **Everything was a dark green card on a dark green ground**: one
   lightness band for the whole page, so sections blurred together
   ("monotoned"). The fix added coloured glows, which is decoration, not
   structure.
4. **Too many words and too many small UI fragments**: each section
   explained instead of showing one thing big.
5. **No proof**: nothing said *why it's worth installing*. A real measured
   result exists (below) and wasn't used.

## Research (2026-11-18): what good tool pages do

Sources: Evil Martians' study of 100+ developer-tool landing pages (2025),
HowdyGo's survey of 200+ SaaS sites (2024), general 2026 trend write-ups
(vendor blogs; directional only).

- **Structure that works:** hero → trust → features (problem-first) →
  social proof → FAQ → final CTA. "No salesy language; clever and simple
  wins." Centred layout in a max-width container; flashy interactions are
  avoided; typography, clear layout and breathing room carry it.
- **Hero:** centred big headline, short sub-line, **two CTAs** (a specific
  primary such as "Download for Windows", a visually lighter secondary such
  as GitHub), then **a large product visual directly below**. The visual's
  options by maturity: animated product UI, static product UI, switchable
  UIs, a live embedded UI for narrow tools.
- **Show the product, big:** 88% of polled buyers won't act without seeing
  the product; most sites still only show a small screenshot. A real
  screenshot or short real video beats mock-ups; an un-gated interactive
  demo beats both (vendor A/B tests, treat lifts as directional).
- **Trust for individual-user / open-source tools:** numbers instead of
  logos (GitHub stars, downloads), or one short quote. "Open source users
  are users, too." Don't invent any.
- **Features: lead with the problem**, not a list. Layouts that work:
  big screenshots with short text, alternating rows, bento, numbered
  "how it works" steps (good when the product has "magic" not visible in
  the UI, which SnailNet has).
- **FAQ** is the one supporting block nearly every page needs (accordion,
  near the end: "Do you store my data?", "Will it slow my internet?").
- **Final CTA:** a full-width block on a **contrasting background**, one
  short line, one button.
- **Dark is fine** (most tool pages default to it) **if the page has more
  than one lightness band**: deep ground with vivid accents, plus at least
  one light or colour-field section for rhythm.

Sources: [Evil Martians: we studied 100 dev tool landing pages](https://evilmartians.com/chronicles/we-studied-100-devtool-landing-pages-here-is-what-actually-works-in-2025),
[HowdyGo: your landing page has to show your product](https://web.howdygo.com/blog/show-your-product),
[Navattic: interactive demo conversions](https://navattic.com/blog/increase-conversions-for-interactive-demos),
[SaaSFrame: Raycast download page](https://www.saasframe.io/examples/raycast-download-page),
[Lapa Ninja: open-source category](https://www.lapa.ninja/category/open-source/),
[Netguru: web design trends 2026](https://www.netguru.com/blog/top-web-design-trends).

## The proof SnailNet has (ask before using it)

`docs/phase0-report.md`, "Ping under load", **real scenario 2026-11-15** on
the developer's ≈ 25 Mbps fibre line: Steam download + YouTube 4K in Brave.

- Without SnailNet: ping **avg 226 ms, max 688 ms, 2 of 20 lost**.
- With SnailNet (Steam 40% low priority, Brave 60%): **avg 22 ms, p50
  17 ms, max 77 ms, 0 lost**. Idle was 9–11 ms.

That is the page's strongest message ("ping during a Steam download:
226 ms → 22 ms"), but PRODUCT.md calls these numbers "engineering, not
marketing", it's one run on one line, and it was the Phase 0 spike, not the
release build. **Ask the user** whether it may be shown, worded honestly
("Measured on one 25 Mbps home line: …", a footnote naming the setup).
Without a yes, show the effect as a labelled illustration, no numbers.

## Direction for the revamp (proposal; confirm in the first message)

Keep the Garden (it's the app's identity, chosen by the user) but give the
page **rhythm**: deep-green sections, one **cream** section (the accent as a
ground, dark text on it), one colour-field band in the shell's green, and
the slice inks used **big** (as the split bar itself) rather than as glows.

1. **Hero, centred:** the snail, the tagline as h1 ("slow lane" may stay
   mint), one sentence, Download + GitHub, the facts as chips. Below it,
   **the product big**: the split bar at hero scale (inks, percentages,
   names you can read from across the room), ideally live (drag it right
   there: the demo *is* the hero, per the research on live UIs), with the
   app window or flyout behind/around it. Real screenshots replace the
   rendering when the user can take them.
2. **Proof band** (if allowed): the before/after ping in two big numbers,
   or a simple two-line ping trace (226 ms jagged vs 22 ms flat), labelled.
3. **The problem in one line → the fix:** three short scenarios, each a
   picture: "A game download starts" / "The stream keeps playing" /
   "Ping stays low".
4. **How it works, numbered (3 steps, it has "magic"):** 1 Give each app a
   slice → 2 Busy line: everyone gets their slice → 3 Spare room goes to
   whoever needs it. Animated once, on interaction, not on scroll.
5. **Features as a bento** (4 cells max, each one big visual, a 3–6 word
   title, one sentence): website slices in any browser (real YouTube /
   Twitch / Netflix icons), games and calls first (Boost menu), modes
   (the four tiles, clickable), Pause / steps aside.
6. **Privacy on the cream section:** "Nothing about you leaves your PC",
   the four connections SnailNet makes (README "Privacy"), big and plain.
7. **FAQ accordion** (`<details>`), from the README's "Questions" (closing
   the window, slower internet, browsers, anti-cheat untested, uploads,
   cost).
8. **Final CTA, full-width colour field** (shell green or cream): one line,
   one Download button, the three install steps small beneath (SmartScreen
   honestly: More info → Run anyway).
9. Footer: links, licence, font credit, trademark line.

Words: take them from the README (liked) and `en.json`; ≤ ~60 words a
screen; headings name a thing. Same "don't claim" list as before: no
speeds above what was measured, no "zero CPU", nothing about anti-cheat
except "not tested yet", no macOS/Linux/ARM, no invented users, quotes,
stars or download counts.

## Process for the fresh session

1. **First message: ask the user** (AskUserQuestion, recommended first):
   - *"May I look at the page in a browser (headless screenshots at 1440,
     768, 390) while I build it?"* → **Yes (Recommended)**. The standing
     rule `~/.claude/rules/no-browser-automation.md` makes this opt-in; v1
     failed because nobody looked. With a yes, the design-taste-frontend
     checkers (`consistency.mjs`, `check.mjs`, `shoot.mjs`, `perf.mjs`) and
     impeccable's rendered `detect` may run.
   - *"May the page show the measured ping result (226 → 22 ms, one home
     line, footnoted)?"*
   - *"Can you take screenshots of the app now (Now page, Split page, tray
     flyout)?"* (Win+Shift+S) If not, render from the components as v1 did
     but much larger, fewer elements.
2. **References:** search the Details catalog (`mcp__details__search_inspirations`,
   project context: "free open-source Windows app that shares a home
   internet connection between apps by percentage, for gamers and
   streamers; one-page download site") for a centred software hero with a
   big product visual, a bento feature grid, and a full-width final CTA;
   **let the user pick** (the tool requires it), one reference per section.
3. **Direction:** `design-taste-frontend` (`deal.mjs`) for the hero layout
   *within* the Garden brand (palette and type are fixed by PRODUCT.md; deal
   only structure). It's a redesign, so impeccable's new-work flow applies:
   replace v1's composition, don't extend it.
4. **Build** with `impeccable` (Persuade mode). Plain HTML/CSS/JS in
   `site/`, no framework, no build step; keep the plumbing above.
5. **Look, fix, look again** at 1440, 1024, 768, 390 (if allowed), then
   `impeccable audit`. Check: contrast on every new pair (the cream section
   especially), forced-colors, reduced motion, keyboard on the demo, no
   horizontal scroll at 360, total weight (v1 was about 70 KB).
6. Commit + push on `main` (user delegated; no branches); the Pages
   workflow deploys; `curl` the live URL; report what was and wasn't seen.

## Notes from v1 worth keeping

- The design hooks' ignores for v1 (`cramped-padding`, `nested-cards`,
  `icon-tile-stack` on `site/index.html`) live in `.impeccable/config.json`,
  which is gitignored, so it's local only. Drop them if the revamp removes
  the patterns they covered.
- v1 used the README's facts accurately; the README's sections "What it
  does", "Download and install", "Using SnailNet", "The words SnailNet
  uses" and "Questions" are the vetted source for page copy.
- `PRODUCT.md`'s "Capabilities and Constraints" says hotkeys,
  notifications and per-network speeds are not built, but the app's
  Settings page has all three (hotkey, new-app notice, other networks).
  Trust the code; suggest updating PRODUCT.md.
- `scripts/readme-images.cjs` draws `docs/images/*.svg` from the app's
  tokens; the same drawing code can seed large page visuals.
