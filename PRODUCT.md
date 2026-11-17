# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

(A Windows 10/11 x64 desktop app: Tauri 2 shell, Svelte 5 UI rendered in
WebView2. Windows conventions for behaviour (caption buttons, tray,
keyboard, high contrast); its own look, not Windows'.)

## Users

Home users who game and stream on a shared, limited home connection (the
first user's line is about 25 Mbps down). The motivating moment: a game
download takes the whole line and YouTube stops buffering, or ping spikes
mid-match. They want downloads, video, calls and games to coexist without
learning networking. Not network administrators. A public release is
planned after v1; the first user is the developer.

## Product Purpose

SnailNet splits this PC's internet between apps and websites by
percentage, with priorities for low latency. Success: the motivating
scenario is solved with the mouse only, in under 15 seconds, by someone who
has never seen the app (PLAN.md §21 Phase 3 exit), and the internet never
breaks because of SnailNet.

## Positioning

Percentages, not KB/s: drag a bar so the game download gets 40% and
YouTube 60%. Shares are guarantees under contention, never cages: idle
share flows to whoever needs it. Per-website rules (YouTube, Twitch…) work
inside any browser. Existing tools (NetLimiter, NetBalancer) speak in
priorities and absolute limits and make the user do the arithmetic.

## Operating Context

- A Windows service (LocalSystem) does all shaping; the app is a client
  over a named pipe (`\\.\pipe\snailnet.v1`, protocol v1). If the app is
  closed, shaping continues.
- The app lives in the tray (flyout for quick changes) with a main window
  for rules; autostarts at login, minimised.
- Used mid-activity: while a game launches, a download runs, a video
  stalls. Glanced at, adjusted, closed.
- `snailnet` CLI exists for diagnostics; the UI is the product.

## Capabilities and Constraints

- Rules target an app family (`steam`, `brave`) or a site (`youtube`); a
  site rule applies in every app and beats the app's rule.
- Everything else keeps a 5% floor. Priorities: High · Normal · Low.
- Line speed is set by hand today; the speed test and estimator are
  Phase 4. Modes (four built-in profiles), Boost, Pause and blocking an
  app are built; notifications, hotkeys and the speed test are not.
  `RunSpeedTest`, `FocusChanged`, `AnswerPrompt` answer Unsupported.
- Service states the UI must show: Starting, Shaping, Unshaped (no line
  speed), Retrying (failed open), Safe mode, Paused, not running.
- Some HTTPS traffic can't be named (ECH, no SNI): "unidentified", shown
  honestly, never hidden.
- Vocabulary: "slice", "goes first", "only when the line is free",
  "Everything else". Never "QoS class", "token bucket", "HTB".

## Brand Commitments

- Name: **SnailNet**. Tagline: "Put the bandwidth hogs in the slow lane."
- The snail is the brand: in the side rail, the tray, the flyout, empty
  states, and its shell spiral drawn faint behind the Now page. Never in
  the way of a control.
- Its own dark look from the snail icon, "the Garden" (PLAN.md §14.1,
  D11–D13): shell greens, cream for what's on, soft light and depth,
  hyperlegible type, motion that explains. Chosen by the user from
  real-size mockups (2026-11-17) after Windows 11 native ("looks like a
  Windows application") and the plainer Ledger.

## Evidence on Hand

No screenshots, testimonials, users or benchmarks for marketing yet.
Measured numbers live in `docs/phase0-report.md` (engineering, not
marketing). Do not fabricate usage or proof.

## Product Principles

1. Percentages first, numbers second.
2. Never waste bandwidth: shares are guarantees, not cages.
3. Never break the internet: fail open, one click pauses everything.
4. Zero-config useful.
5. Quiet and premium: no nagging, no fake urgency, no dashboards nobody
   asked for.

## Accessibility & Inclusion

WCAG 2.2 AA contrast on the dark ground; full keyboard operation; visible
focus; live speeds throttled for screen readers (on request,
not 4× a second); respects Windows text scaling and high-contrast mode; no
information by colour alone (PLAN.md §14.9).
