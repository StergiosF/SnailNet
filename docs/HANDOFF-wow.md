# Handoff: three features that make people say wow

Written 2026-11-20. Start a session with **"read docs/HANDOFF-wow.md and
build feature 1"** (then 3, then 2). Each feature is its own release.

| # | Feature | What the user gets | Release |
|---|---|---|---|
| 1 | **The proof** | "Ping 19 ms while Steam fills your line · 142 ms without SnailNet", and a grade from A to F before and after | v0.0.6 |
| 3 | **The living snail** | The tray snail shows how the line is doing: calm, working, or struggling | v0.0.7 |
| 2 | **The one-tap fix** | "Brave is waiting while Steam fills your line. [Give Brave 60%]" | v0.0.8 |

**Build order is 1 → 3 → 2.** Feature 3 reads the live ping that feature 1
adds, so it is small once 1 is in. Feature 2 is the largest (it needs a new
signal from the scheduler) and stands alone.

**Why these three.** SnailNet's work is invisible: when it succeeds,
nothing happens. Feature 1 shows the result in numbers, feature 3 shows it
at a glance, and feature 2 turns "who's hogging the line?" into one click.
None of them adds a dashboard (PRODUCT.md principle 5): each shows up only
while it has something true to say.

---

## Ground rules for all three

- **Honesty first.** Every number is measured on this PC, on this network,
  and says where it comes from. No estimates dressed as measurements, no
  "up to". If a number isn't there (ICMP blocked, no speed test yet), the UI
  says less, never something made up.
- **Protocol stays v1.** New fields only, `#[serde(default)]` and
  `skip_serializing_if`, so older apps and services keep working. **No new
  `ServiceState` variants and no new `Event` variants**: they're externally
  tagged enums and an older app can't decode an unknown variant.
- **Record decisions in PLAN.md**: §11.10 (proof), §11.11 (fix), §14.4
  (tray states), §10.1 (speed test). This file is the brief; PLAN.md is the
  record.
- **UI work goes through `impeccable`** (no browser unless the user asks;
  say what wasn't seen in a real window). The look is the Garden (PLAN.md
  §14.1); the words follow §14.7.
- **Gates before every commit** (docs/HANDOFF.md "Gates"): `cargo fmt`,
  `clippy -D warnings`, `cargo test` at the root and in `app\src-tauri`;
  `npm run check`, `lint`, `npx vitest run`, `build` in `app\`.
- Commit and push each verified step on `main`.

---

## Feature 1: The proof (ping with and without SnailNet)

### What the user sees

1. **Now page, while the line is full.** A row under the sentence at the
   top:

   ```
   Ping 19 ms while Steam fills your line
   ▓▓░░░░░░░░░░░░░░░░░░░  With SnailNet     19 ms
   ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓  Without, at your speed test   142 ms
   ```

   Two bars on one scale (the longer number fills the width): *with* in
   shell green, *without* muted. It appears once the line has been full for
   3 s, stays 10 s after the line calms down, then fades. Hidden while
   paused, measuring or watching only.
2. **Flyout:** one line under "Using your internet": *"Ping 19 ms · 142
   without SnailNet"*.
3. **Right after Measure** (Settings, first run): the test now takes about
   30 s and ends with the before and after:
   *"Measured 24.6 Mbps down and 4.8 up. While downloading, ping was 142 ms
   without SnailNet and 21 ms with it."* The first run's last step shows the
   same two bars. This is the first-minute wow.
4. **Settings → last speed test:** grades replace today's single ping line:

   ```
   Without SnailNet   D   ping rose 128 ms while downloading
   With SnailNet      A   ping rose 7 ms
   ```

   With a one-line note on what the grade means.
5. **Tray tooltip** (feature 3 uses it): *"SnailNet · line full · ping 19 ms"*.
6. **CLI:** `snailnet top` prints `ping: 19 ms (lowest 12 ms)`, so the live
   check doesn't need the app.

### What's true, and the words that keep it true

- **Live ping** = ICMP echo to `1.1.1.1`, the host the autorate probe
  already pings once a second while the line is busy
  (`snailnet-service::autorate::Probe`). It's the same host the speed test
  pings, so *with* and *without* compare like for like.
- **Pings skip SnailNet's own queues** (the engine captures only TCP and
  UDP). So the number is the line's ping beyond this PC, which is what apps
  that go first (games, calls, the app in front) feel. An app in a full
  slice also waits a few ms in SnailNet's queue (CoDel's target is 5 ms).
  The words say "Ping", not "your game's ping". Ping per app is a separate
  backlog idea.
- **"Without"** comes from the last speed test **on this network**
  (`config.line.measured` belongs to the current network, §10.3), taken
  with shaping off. The UI names it: "Without, at your speed test (12
  Nov)". It's not a guess about right now.
- **Show the comparison only when it says something:** when *without* is
  at least 20 ms above *with*. Otherwise just "Ping 19 ms while Steam fills
  your line", and Settings shows grade A for *without* ("your line keeps
  ping low on its own"). Never claim credit SnailNet didn't earn.
- **Grade scale.** Rise = loaded − idle ping. **A ≤ 30 ms, B ≤ 60, C ≤ 200,
  D ≤ 400, F above.** This is the scale the well-known bufferbloat tests use
  (Waveform's), so people can compare. No A+: the plain scale reads more
  clearly.

### Engineering steps

**1.1 Core: grades and the measured record** (`snailnet-core`)

- New module `snailnet-core::proof` (pure): `enum Grade { A, B, C, D, F }`,
  `fn grade(rise_ms: f64) -> Grade`, the thresholds as named consts, and
  `fn rise(idle, loaded) -> Option<f64>` (never negative). Tests on each
  edge (30.0 → A, 30.1 → B …).
- `Measured` (`rules.rs`) gets two fields, both `Option<f64>`,
  `serde(default, skip_serializing_if)`:
  - `loaded_up_ms`: ping during the upload phase without SnailNet
    (uploads often bloat worse than downloads).
  - `with_ms`: ping during the check download *through* SnailNet (1.3).
- ⚠ `Measured` has `deny_unknown_fields`: an **older** service can't read a
  config with the new fields. Updates only go forward, so accept that and
  write it down in PLAN.md §10.1. Test that an old config (no new fields)
  still loads.

**1.2 Speed test: ping during the upload too** (`speedtest.rs`)

- Ping from a side thread during the upload phase, exactly as during the
  download (`median_ping`, after `WARMUP`) → `loaded_up_ms`.
- Add `pub fn check(cancel) -> Result<Option<f64>>`: download on
  `DOWN_STREAMS` for `PHASE` while pinging; returns the median loaded ping.
  The same code as the download phase, factored out, not copied.

**1.3 The "with SnailNet" check** (`runner.rs`, `hub.rs`)

The speed test runs with shaping off. Right after it, SnailNet starts
shaping at the new speed, downloads once more, and pings meanwhile. That's
the *with* number.

- `Hub`: `check_pending: Option<u64>` (the `measured.at` it belongs to), set
  by `record_measured` on success. `take_check()`, and
  `record_check(at, with_ms)`, which writes `config.line.measured.with_ms`
  **only if `measured.at` still equals `at`** (a newer test or a typed speed
  wins), saves, and sends `ConfigChanged`.
- Runner, inside the shaping session: once shaping has run for
  `CHECK_AFTER` (2 s, so the plan has settled) and a check is pending and
  the download direction is shaped: start `speedtest::check` on a thread.
  Poll `is_finished()` on each loop turn. Cancel it on Stop, on a reload
  that restarts shaping, on a failure, or on a network change. On
  `Ok(Some(ms))`: `record_check`, and log
  `speed test: ping {ms} ms loaded with SnailNet`.
- State stays `Shaping` throughout (no new state). The app learns the
  check finished when `with_ms` arrives in the config.
- The service's own download is shaped like any app (in Everything else).
  That's the point: the number is what SnailNet does to a real download.
- Tests (fake engine): inject the check through `Timing`
  (`check: Option<fn(&AtomicBool) -> Result<Option<f64>>>`, `None` in
  `fast()`). Check that it records; that a stop cancels it with nothing
  recorded; that a newer `measured` wins; and that it doesn't run when
  download isn't shaped.

**1.4 Live ping in the stats** (`autorate.rs`, `hub.rs`, `snailnet-ipc`)

- `Probe` keeps the public host's samples (its `Baseline` already does).
  Add `Probe::now() -> Option<PingNow>`: the **median of the public host's
  samples from the last 5 s** (at least 2), plus the 5-minute lowest. `None`
  when stale, which happens on its own when the line goes quiet (the probe
  drops to one ping every 30 s, and `FRESH` is 3 s). Tests with fixed
  `Instant`s, like the existing baseline tests.
- `Autorate::ping()` passes it through. The runner calls
  `hub.set_ping(autorate.ping())` every `RATE_EVERY`, and `clear_stats`
  clears it.
- IPC: `StatsTick.ping: Option<PingTick>` with
  `PingTick { now_ms: f64, low_ms: f64 }`, `serde(default,
  skip_serializing_if = "Option::is_none")`. Round-trip test, plus a test
  that a tick without `ping` decodes.
- `snailnet top`: print a `ping:` line when present (next to `calls:`,
  `ipc_cmd.rs`).

**1.5 App: what to show** (`app/src/lib/proof.ts`, pure, vitest)

- `types.ts`: `PingTick`, `StatsTick.ping?`, `Measured.loaded_up_ms?`,
  `with_ms?`.
- `proofOf(tick, down: Ledger, up: Ledger, measured) → Proof | null`:
  - Null unless a direction is busy (`ledger.busy`) and `tick.ping` exists.
  - `without` = `loaded_ms` if download is busy; else `loaded_up_ms` if
    only upload is busy. Kept only if `without − now ≥ 20`.
  - `name` = the busiest app or slice (the glance's `top`).
- `ProofWatch` (same pattern as `steady.ts`): shows after 3 s busy, holds
  10 s after, and only updates the number when it moves ≥ 2 ms or 2 s have
  passed (no flicker at 4 Hz).
- Tests: no ping → null; quiet line → null; upload-only uses
  `loaded_up_ms`; a small difference hides *without*; show and hold timing;
  smoothing.

**1.6 App: the UI** (build with `impeccable`)

- `ProofRow.svelte` on the Now page under the glance sentence, as sketched
  above. Tabular figures. Bars grow in once (`motion.ts`, reduced motion =
  no growth). For screen readers: one sentence ("Ping 19 ms with
  SnailNet; 142 ms without it at your last speed test"), `aria-live="off"`
  (live numbers are read on request, §14.9).
- No *without*: "Ping 19 ms while Steam fills your line", plus a quiet
  **Measure to compare** text button (Now page only). It shows only while
  the line is full, so it never nags.
- Flyout: the one line.
- Settings → last speed test: the two grade rows (`proof.grade.*`),
  replacing `ping.body`.
- `measureLine()` (`actions.ts`): after the new `measured` arrives, show
  *"Checking ping with SnailNet… about 10 seconds"* and wait up to 20 s for
  `with_ms`. Then the toast (`measure.done.proof`), or the old
  `measure.done` if the check didn't finish. First run's done step shows
  the bars.
- Speed test texts: "about 20 seconds" → "about 30 seconds"
  (`firstrun.speed.body`, `firstrun.measuring`), and the metered note
  (it downloads a second time).

**Words (en.json, new keys)**

| Key | Text |
|---|---|
| `proof.live` | Ping {now} ms while {name} fills your line |
| `proof.with` | With SnailNet |
| `proof.without` | Without, at your speed test ({date}) |
| `proof.measure` | Measure to compare |
| `proof.sr` | Ping {now} ms with SnailNet; {without} ms without it at your last speed test. |
| `proof.flyout` | Ping {now} ms · {without} without SnailNet |
| `proof.grade.without` | Without SnailNet |
| `proof.grade.with` | With SnailNet |
| `proof.grade.rise` | ping rose {rise} ms while downloading |
| `proof.grade.what` | How much ping rises when the line is full. A is barely at all; F is enough to ruin a call or a match. |
| `proof.grade.calm` | Your line keeps ping low on its own. |
| `measure.checking` | Checking ping with SnailNet… about 10 seconds |
| `measure.done.proof` | Measured {down} Mbps down and {up} up. While downloading, ping was {without} ms without SnailNet and {with} ms with it. |

**1.7 Docs**

README "What it does" (a "Shows it working" bullet), the speed test's
length, and the privacy section (the speed test now downloads twice). Add
the site's feature tab only if it fits. PLAN.md §10.1, §10.4 (the "Ping
under load" readout that §10.4 planned is this), §11.10.

### Edge cases

| Case | Behaviour |
|---|---|
| ICMP blocked / 1.1.1.1 unreachable | No `ping` in the tick: no row, no tooltip ping. The speed test without pings: no grades. |
| No speed test on this network (speed typed) | Live ping only, plus "Measure to compare". |
| Speed test months old | Still shown, with its date. |
| Only upload busy, no `loaded_up_ms` (old test) | Live ping only. |
| Check download fails or is cancelled | `with_ms` absent: Settings shows the *without* grade alone; the toast is the old one. |
| Paused, measuring, safe mode | No row (the probe runs only while shaping). |
| Line full because of the speed test itself | State is Measuring: no row. |
| Network changes mid-check | Cancelled; nothing recorded. |

### Done when

- Unit tests above pass; gates green.
- **Live check** (one UAC click; recipe as in docs/HANDOFF.md "Try it
  live", with a scratch data dir and `--pipe \\.\pipe\snailnet-live`): run
  Measure; `snailnet top` shows `ping:` while a big download runs; the
  saved `measured` has `loaded_up_ms` and `with_ms`; *with* is clearly
  below *without* on the user's line. **Not optional for this feature: it
  makes a numeric claim.** If the live line doesn't show the difference,
  find out why before releasing (that would be a finding about SnailNet
  itself).
- User looks at the Now row and the first run's done step in the real app.

---

## Feature 3: The living snail (tray icon)

### What the user sees

| Mood | When | Icon | Tooltip |
|---|---|---|---|
| **Calm** | line not full | the snail as today | `SnailNet` |
| **Working** | line full and ping fine | the shell's spiral turns cream ("cream for what's on", §14.1): SnailNet is sharing | `SnailNet · line full · ping 19 ms` |
| **Struggling** | line full and ping up ≥ 40 ms for 5 s | eye stalks bent down | `SnailNet · ping is up 60 ms · slowing the line a little to bring it down` |

The existing marks stay on top and win: the amber dot (paused, safe mode,
retrying) and the faded grey snail (no service) always show the calm art
underneath, and the blocked-apps badge keeps its corner. **No animation**:
the icon changes only when the mood changes. The meaning is in the
tooltip too, never in colour alone (§14.9).

*Struggling* is honest. It shows up when the line itself is in trouble
(weak Wi-Fi, a busy ISP), and the tooltip says what SnailNet is doing
about it: the estimator lowers the rate when ping rises (§10.4).

### Engineering steps

**3.1 Mood, pure and tested** (`app/src-tauri/src/tray_icon.rs`)

- `enum Mood { Calm, Working, Struggling }`.
  `fn mood_of(tick: &StatsTick) -> Mood`: full = in either direction, the
  sum of `classes[].bps` ≥ 0.9 × `shaped_bps` (the same `BUSY` as
  `ledger.ts`); rise = `ping.now_ms − ping.low_ms`.
- `MoodWatch` (hysteresis, fed with `t_ms`): Working after 3 s full, back to
  Calm after 10 s not full; Struggling after 5 s at a rise of ≥ 40 ms, back
  to Working after 10 s below 25 ms. No ping → never Struggling. Tests for
  each transition and for flicker at the edges.

**3.2 The art** (`app/src-tauri/icons/tray/`)

- Two variants of `icons/snail.svg`: `snail-working.svg` (spiral stroke
  `#EFE8D6`) and `snail-struggling.svg` (stalks bent forward and down, eyes
  lower, same stroke width). Draw them so they still read at 16 px.
- Render them to PNG at the size `default_window_icon()` gives the tray
  today (check: probably 32 × 32; add 64 × 64 if Windows picks the larger
  one at 150–200 % scaling). Use a small `scripts/tray-icons.cjs` (dev
  dependency `@resvg/resvg-js`; Context7 first) and commit the PNGs, so the
  build needs no Node step. Load them with `include_bytes!` and decode them
  with Tauri's `Image::from_bytes` (check which feature it needs).
- `paint` / `paint_badge` draw on whichever base the mood picks.

**3.3 Wiring** (`lib.rs`, `tray_icon.rs`)

- `TauriSink::stats` feeds the `MoodWatch` (kept in `Shared`) and calls
  `refresh_tray` **only when the mood changes**. The tooltip with the ping
  updates at most every 2 s.
- `Shown` caches `(Status, badge, Mood)`. `update()` takes the mood, and
  Offline or Warning force Calm.
- Tooltip text is built in Rust (`tooltip()`), with tests. Keep it short:
  Windows cuts tray tooltips at 127 characters.

**3.4 Docs:** README "The tray" paragraph; PLAN.md §14.4's tray table.

### Done when

- Tests pass; gates green.
- The user looks at the tray at 100 % and 150 % scaling, on a light and a
  dark taskbar, during a big download (Working) and, if it can be
  provoked, a weak Wi-Fi (Struggling). **Not seen until the user looks**:
  say so in the release notes if released before that.

---

## Feature 2: The one-tap fix

### What the user sees

When one app fills the line and another is left waiting (for 15 s), the
sentence at the top of the Now page already says *"Steam is using the
most of your line. Brave wants more room too."* Now it also offers the
cure:

```
Steam is using the most of your line. Brave is waiting.
[ Give Brave 60% ]   Not now
```

One click saves the rule in the active mode. A toast says *"Brave gets 60%
of your line now."* with **Undo**. The flyout shows the same, compact.
That's the moment the user opens SnailNet because YouTube is buffering:
the fix is already waiting there.

- **Waiter without a slice** → "Give {name} 60%" (`addRule(rules, target,
  6000)`; if the other slices leave less room, as much as fits, and the
  button says the real number).
- **Waiter whose slice is full** (status *held*) → "Make {name}'s slice
  {pct}%", 15 points more, up to what fits (`setRuleShare`).
- **No offer** when: the waiter is "only when free", off, unknown or
  unidentified traffic; a game is running with Gaming on (`playing`: the
  user isn't looking at the split, and the wrong mode would change); a
  Boost is on; paused or not shaping; the user can't edit (no rev yet).
- **Not now** mutes that pair (hog, waiter) until tomorrow. After a fix is
  applied, the pair is muted for 10 minutes so it can't ping-pong.
- **Settings → "Suggest a fix when an app is left waiting"** (on by
  default): *"A button in SnailNet's window, never a pop-up."* No toasts:
  the offer appears only where the user is already looking (principle 5).

**Known limit, said plainly:** zero-config, SnailNet only names websites
while a site rule exists (§8), so the waiter is the browser ("Brave"), not
"YouTube". Once a site rule exists, sites show up as themselves. Naming
sites always is a separate decision (cost and privacy, §8.5), not part of
this.

### The missing signal, and why it comes first

Today only **slices** can say "wants more" (`ClassTick.queued_bytes`,
`status.held`). Inside Everything else, SnailNet queues fairly **per
connection** (§9.3), so Steam with 16 connections takes about 16× what a
browser with one QUIC flow gets. That's exactly the hog case, and today
nothing reports which member is waiting.

But a queue-bytes threshold won't catch it: CoDel keeps a slow flow's queue
near 5 ms, which is about 900 bytes at 1.5 Mbps, far under `HELD_BYTES`
(6000). The honest signal is **CoDel acting on the app**: drops and ECN
marks mean "this app keeps asking for more than it gets".

**2.0 Prove the signal in the simulator first** (`snailnet-sim`): 25 Mbps
line, everything in Everything else. A 16-flow bulk download plus one
QUIC-like video flow that wants 8 Mbps → the video must show up as waiting
within 15 s. Negative cases: the video with a full buffer (on/off bursts)
must **not** show as waiting, and neither must the hog itself, since only
the side that is losing should count. Tune the rule here, before any UI
work.

### Engineering steps

**2.1 Per-app pressure in the scheduler** (`snailnet-core::scheduler`)

- Each flow sub-queue (`fq.rs` `Flow`) remembers the tag of its first
  packet (the app part, as `tags.rs` defines it). Count CoDel drops and ECN
  marks per tag, cumulatively, plus the queued bytes now.
- `Scheduler::pressure(&self, out: &mut Vec<(Tag, Pressure)>)` with
  `Pressure { queued_bytes, acted }`. It goes through the flows (a few
  hundred at most) at the stats rate, not per packet; the per-packet cost
  is one counter increment at the drop/mark site, which already exists.
- Tests: the per-tag queued bytes add up to the class's `queued_bytes`;
  drops land on the right tag; no allocation on the packet path (keep the
  zero-alloc fast path, §20).

**2.2 Through the engine to IPC** (`shaper.rs`, `hub.rs`, `snailnet-ipc`)

- `EngineSnapshot` carries the pressure per tag; `hub::tag_ticks` sums it
  per family. `FamilyTick` gains `queued_bytes: u64` and `acted: u64` (CoDel
  actions since the previous tick), both `serde(default)`.
- `snailnet top` marks waiting families: `brave 1.4 Mbps (waiting)`.

**2.3 App: members can wait** (`ledger.ts`)

- `Member.waiting`: the rule proven in 2.0 (CoDel acted on it in most of
  the last ~5 s of ticks, or queued above `HELD_BYTES`). Smoothed by the
  same pattern as `steady.ts`.
- `glance()`: waiting members count as waiting (today only slices can be);
  the member row says *"wants more room"*.
- Tests in `ledger.test.ts`.

**2.4 App: the offer** (`app/src/lib/fix.ts`, pure, vitest)

- `FixWatch` (time-based, fed each tick): candidate = hog (the busiest
  entity, ≥ 50 % of the shaped rate) + waiter (another entity that is
  waiting, can be offered) in the same direction, the line full. Offer
  when the **same pair** held for ≥ 12 of the last 15 s.
- `offerOf(rules, ledger, pair) → { label, rules: Rule[] } | null`: the
  new rules (through `addRule` / `setRuleShare`, so the split stays valid,
  §9.7) and the button's words.
- Mutes: "not now" until local midnight; applied: 10 min. Kept in the
  app's own data folder, next to the hotkey choice (same mechanism as
  `hotkey-keys`), with the on/off switch.
- Tests: sustain window, pair changes reset it, every "no offer" case
  above, the share that fits when slices are crowded, mutes expire.

**2.5 App: the UI** (build with `impeccable`)

- Now page: the button and "Not now" sit on the glance sentence (no new
  card: the sentence already says what's wrong, so the cure goes next to
  it). Flyout: the same, compact.
- Click → the existing save path (`actions.ts`, with the rev and the
  conflict handling) → the toast with **Undo** (`Toast.action`). Undo puts
  back the previous rules if the rev still matches; otherwise it says the
  split changed meanwhile.
- Settings → the switch in the group with the calls and games switches.

**Words (en.json, new keys)**

| Key | Text |
|---|---|
| `fix.waiting` | {waiter} is waiting. |
| `fix.give` | Give {name} {pct}% |
| `fix.grow` | Make {name}'s slice {pct}% |
| `fix.later` | Not now |
| `fix.done` | {name} gets {pct}% of your line now. |
| `fix.undo` | Undo |
| `fix.undone` | Back to how it was. |
| `fix.stale` | The split changed in the meantime, so nothing was undone. |
| `fix.setting.title` | Suggest a fix when an app is left waiting |
| `fix.setting.body` | A button in SnailNet's window, never a pop-up. |
| `status.member.waiting` | wants more room |

**2.6 Docs:** README (Now page: "the fix"), PLAN.md §11.11 (rules,
thresholds from the sim, mutes), and §23 ("Why is my internet slow?" and
"Smart suggestions" are built as this).

### Done when

- The sim proves the signal (2.0) and the thresholds are written in
  PLAN.md with the numbers from the sim.
- Unit tests pass; gates green.
- **Live check:** Steam (or two `curl` copies with many connections)
  downloading, YouTube in Brave with no rules → the offer appears within
  ~20 s; one click → YouTube plays smoothly; Undo restores the split. With
  YouTube paused (buffer full) → no offer.

---

## What changes, all in one place

| Where | Change | Compatible? |
|---|---|---|
| `StatsTick.ping` | new `Option<PingTick>` | yes (default) |
| `FamilyTick.queued_bytes`, `.acted` | new `u64` | yes (default) |
| `Measured.loaded_up_ms`, `.with_ms` | new `Option<f64>` | forward only (`deny_unknown_fields`: an older service can't read the config) |
| `ServiceState`, `Event`, `Request` | **unchanged** | — |
| Speed test | ~20 s → ~30 s, downloads twice | metered note updated |
| App prefs | fix switch + mutes (app data folder) | app only |
| Tray | three moods, two new PNGs | app only |

## Risks

| Risk | Mitigation |
|---|---|
| 1.1.1.1 rate-limits or deprioritises ICMP, so pings are noisy | Median of 5 s; *without* shown only when ≥ 20 ms clear of *with*; no ping → say less |
| The live check shows no difference on the user's line (little bufferbloat) | The words handle it ("your line keeps ping low on its own"); still ship, since the grade is honest either way |
| The fix's signal misfires (offers when nothing's wrong) | Proven in the sim first; 12 of 15 s sustain; mutes; switch in Settings |
| The fix edits the wrong mode | No offer while `playing`; it edits the active profile through the rev-checked path |
| Tray art unreadable at 16 px | User checks at real sizes before release; the tooltip carries the meaning anyway |
| More moving parts in the shaping loop (the check thread) | Cancelled on every exit path; tested with the fake engine; never blocks the loop |

## Not in this plan (later)

- Ping per app (a game's own server ping), from TCP timestamps or UDP
  request/reply pairs.
- A rolling "with SnailNet" grade from everyday use ("this week: A").
- Naming websites without a site rule (so the fix can say "YouTube").
- An opt-in toast for the fix while the window is closed.
- Sharing the grade as an image.
