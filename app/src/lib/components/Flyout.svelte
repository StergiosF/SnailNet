<script lang="ts">
  // The tray flyout (PLAN.md §14.4, Obsidian Jade), 380 × 500: the quickest
  // look and the quickest change. The snail and what SnailNet is doing; the
  // main window's sentence (or the state that needs attention) with the
  // one-tap fix; the modes as four tiles; the proof's one line (live ping
  // while busy, else the last speed test's grades); the apps using the
  // internet now, each with Boost; and at the foot Pause (always here, the
  // tray is where people reach for it) and the way to the main window. It
  // hides on Escape and when it loses focus (tray.rs).
  import { getCurrentWindow, Window } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { mbps, targetLabel } from "../format";
  import { t } from "../i18n";
  import { glance, ledger } from "../ledger";
  import { pause, resume, startService, switchMode } from "../actions";
  import { availableModes, boostLeft, gameWhy, inCall, liveBoost, timeLeft, users } from "../modes";
  import { newNetworkName } from "../line";
  import { noticeOf } from "../notice";
  import { placeMenu } from "../popover";
  import { ProofWatch, grade, proofOf, rise } from "../proof";
  import { service } from "../service.svelte";
  import type { Target } from "../types";
  import { ui } from "../ui.svelte";
  import AppIcon from "./AppIcon.svelte";
  import BoostButton from "./BoostButton.svelte";
  import FixOffer from "./FixOffer.svelte";
  import Glyph, { modeGlyph } from "./Glyph.svelte";
  import Snail from "./Snail.svelte";

  onMount(() => {
    service.start().catch((e: unknown) => {
      service.link = { kind: "lost", detail: String(e) };
    });
    return ui.tick();
  });

  const connected = $derived(service.link.kind === "connected" && service.snapshot !== null);
  const label = (target: Target) => targetLabel(target, service.snapshot?.apps ?? [], service.siteNames);
  const book = $derived(connected ? ledger(ui.rules, service.stats?.down, label, service.waiting.down) : null);
  const upBook = $derived(connected ? ledger(ui.rules, service.stats?.up, label, service.waiting.up) : null);
  const proofWatch = new ProofWatch();
  const proof = $derived.by(() => {
    const s = service.stats;
    if (!connected || !s) return proofWatch.step(null, 0, true);
    const now = proofOf(s, book, upBook, service.snapshot?.config.line.measured);
    return proofWatch.step(now, s.t_ms, s.state !== "Shaping");
  });
  // Ping is only ever shown live while the service measures it (busy);
  // quiet, the last speed test's grades speak instead, never a made-up ping.
  const measured = $derived(service.snapshot?.config.line.measured);
  const grades = $derived.by(() => {
    const without = measured ? rise(measured.idle_ms, measured.loaded_ms) : null;
    const withIt = measured ? rise(measured.idle_ms, measured.with_ms) : null;
    return without === null || withIt === null ? null : { without: grade(without), with: grade(withIt) };
  });
  const proofLine = $derived(
    proof !== null
      ? proof.without === null
        ? t("proof.live", { now: Math.round(proof.now), name: proof.name })
        : t("proof.flyout", { now: Math.round(proof.now), without: Math.round(proof.without) })
      : grades
        ? t("flyout.grades", grades)
        : null,
  );
  const lineMbps = $derived(service.snapshot?.config.line.down_mbps);
  const lineBps = $derived(lineMbps === undefined ? null : lineMbps * 1e6);
  const sentence = $derived(book && service.stats ? glance(book, lineBps) : null);
  const notice = $derived(
    noticeOf(
      service.link,
      service.state,
      { setSpeed: () => void openMain(), resume: () => void resume(), startService: () => void startService() },
      ui.now,
      ui.online,
      newNetworkName(service.snapshot?.config),
    ),
  );
  const boost = $derived(connected ? liveBoost(service.snapshot?.config.boost, ui.now) : null);
  const modes = $derived(availableModes(service.snapshot?.config.profiles ?? []));
  const active = $derived(service.snapshot?.config.active_profile ?? "");
  const picked = $derived(modes.find((m) => m.id === active));
  const pausedUntil = $derived.by(() => {
    const p = service.snapshot?.config.pause;
    return p !== undefined && (p.until === undefined || ui.now < p.until) ? p : null;
  });

  // Who is using the internet now (what the tray is opened for), not the
  // split's rules: in Balanced there are none, and the flyout would be empty.
  const focusBoost = $derived(service.snapshot?.config.settings.focus_boost !== false);
  const callsFirst = $derived(service.snapshot?.config.settings.calls_first !== false);
  const calls = $derived(callsFirst ? (service.stats?.calls ?? []) : []);
  const list = $derived(book ? users(book, boost, label, 4, calls) : []);
  const total = $derived(book?.total ?? 0);
  function sub(u: (typeof list)[number]): string {
    if (u.boosted && boost) return t("who.boost.left", { left: boostLeft(boost, ui.now) });
    const why = gameWhy(u.target, service.stats?.games, focusBoost);
    if (why) return t(`who.game.${why}`);
    return inCall(u.target, service.stats?.calls, callsFirst) ? t("who.call") : u.status;
  }
  const first = (u: (typeof list)[number]) =>
    u.boosted ||
    gameWhy(u.target, service.stats?.games, focusBoost) !== null ||
    inCall(u.target, service.stats?.calls, callsFirst);

  const doing = $derived(
    pausedUntil
      ? t("flyout.state.paused", { left: timeLeft(pausedUntil.until, ui.now) })
      : service.playing && picked
        ? t("mode.playing")
        : connected
          ? t("flyout.state.on")
          : "",
  );

  const uid = $props.id();
  let pauseBtn = $state<HTMLButtonElement | null>(null);
  let pauseMenu = $state<HTMLDivElement | null>(null);
  const pauseChoices: { minutes: number | null; label: string; glyph: "clock" | "infinity" }[] = [
    { minutes: 15, label: t("pause.for.15m"), glyph: "clock" },
    { minutes: 60, label: t("pause.for.1h"), glyph: "clock" },
    { minutes: null, label: t("pause.for.forever"), glyph: "infinity" },
  ];
  function placePause(e: ToggleEvent) {
    if (e.newState === "open" && pauseBtn && pauseMenu) placeMenu(pauseBtn, pauseMenu, "above", "start");
  }
  function pickPause(minutes: number | null) {
    pauseMenu?.hidePopover();
    void pause(minutes);
  }

  async function openMain() {
    const main = await Window.getByLabel("main");
    if (main) {
      await main.show();
      await main.unminimize();
      await main.setFocus();
    }
    await getCurrentWindow().hide();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") void getCurrentWindow().hide();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="flyout">
  <div class="top">
    <Snail size={22} light />
    <span class="app">{t("app.name")}</span>
    {#if doing}
      <span class="state" class:paused={pausedUntil !== null}><i aria-hidden="true"></i>{doing}</span>
    {/if}
  </div>

  <div class="say">
    {#if notice}
      <h1>{notice.title}</h1>
      <p class="detail" role={notice.severity === "info" ? "status" : "alert"}>{notice.message}</p>
    {:else if sentence}
      <h1>{sentence.title}</h1>
      <p class="detail">{sentence.detail}</p>
      <FixOffer canEdit={connected} compact />
    {:else}
      <h1 class="wait">{t("link.connecting")}</h1>
    {/if}
  </div>

  {#if connected && modes.length > 0}
    <div class="modes" role="radiogroup" aria-label={t("mode.label")}>
      {#each modes as m (m.id)}
        <button
          type="button"
          role="radio"
          aria-checked={m.id === active}
          class:on={m.id === active}
          class:playing={m.id === "gaming" && service.playing}
          title={m.body}
          onclick={() => void switchMode(m.id)}
        >
          <Glyph name={modeGlyph(m.id)} size={18} />
          <span>{m.id === "quiet-downloads" ? t("flyout.mode.quiet") : m.name}</span>
        </button>
      {/each}
    </div>
    {#if service.playing && picked}
      <p class="detail note" role="status">{t("mode.playing.note", { mode: picked.name })}</p>
    {/if}
  {/if}

  {#if connected && proofLine}
    <p class="proof num" aria-live="off"><Glyph name="ping" size={18} /><span>{proofLine}</span></p>
  {/if}

  {#if book}
    <div class="list">
      <h2>
        {t("who.title")}
        {#if list.length > 0 && lineBps !== null}
          <span class="num" aria-live="off">{t("who.total", { used: mbps(total), line: mbps(lineBps) })}</span>
        {/if}
      </h2>
      {#if list.length > 0}
        <ul>
          {#each list as u (u.key)}
            <li class:boosted={u.boosted}>
              <AppIcon target={u.target} name={u.name} size={24} />
              <span class="who">
                <b>{u.name}</b>
                <span class:wait={u.waiting && !u.boosted} class:first={first(u)}>{sub(u)}</span>
              </span>
              <span class="mb num" aria-live="off">{mbps(u.bps)}</span>
              <BoostButton target={u.target} name={u.name} boosted={u.boosted} disabled={!connected} />
            </li>
          {/each}
        </ul>
      {:else}
        <p class="empty">{t("who.empty")}</p>
      {/if}
    </div>
  {:else}
    <div class="quiet"><Snail size={32} light /></div>
  {/if}
  {#if ui.error}
    <p class="error" role="alert">{ui.error}</p>
  {/if}

  <div class="foot">
    {#if connected}
      {#if pausedUntil}
        <button type="button" class="btn resume" onclick={() => void resume()}>
          <Glyph name="play" size={18} />{t("pause.resume")}
        </button>
      {:else}
        <button
          bind:this={pauseBtn}
          type="button"
          class="btn"
          popovertarget="{uid}-pause"
          aria-haspopup="menu"
          aria-label={t("pause.label")}
        >
          <Glyph name="pause" size={18} />{t("pause")}
        </button>
        <div bind:this={pauseMenu} id="{uid}-pause" class="popmenu" popover="auto" role="menu" ontoggle={placePause}>
          <p class="title">{t("pause.label")}</p>
          <p class="body">{t("pause.menu.body")}</p>
          {#each pauseChoices as c (c.label)}
            <button type="button" role="menuitem" onclick={() => pickPause(c.minutes)}
              ><Glyph name={c.glyph} size={18} />{c.label}</button
            >
          {/each}
        </div>
      {/if}
    {/if}
    <button type="button" class="btn btn-accent open" onclick={() => void openMain()}>{t("flyout.open")}</button>
  </div>
</div>

<style>
  .flyout {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 16px 6px;
  }
  .app {
    flex: 1;
    font-size: 15px;
    font-weight: 800;
    letter-spacing: -0.01em;
  }
  .state {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    font-size: 12.5px;
    color: var(--text-secondary);
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .state i {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--shell);
  }
  .state.paused i {
    background: var(--caution);
  }
  .say {
    padding: 4px 16px 0;
  }
  h1 {
    margin: 0;
    font-size: 16px;
    line-height: 1.3;
    font-weight: 800;
    letter-spacing: -0.01em;
    text-wrap: balance;
  }
  h1.wait {
    color: var(--text-secondary);
    font-weight: 600;
  }
  .detail {
    margin: 3px 0 0;
    font-size: 13px;
    line-height: 1.4;
    color: var(--text-secondary);
  }
  .note {
    padding: 0 16px;
  }
  /* The modes as four tiles: glyph over name; the one that's on is ivory. */
  .modes {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 4px;
    padding: 12px 12px 0;
  }
  .modes button {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    min-width: 0;
    padding: 8px 2px 7px;
    border: 0;
    border-radius: var(--radius-control);
    background: var(--surface-1);
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 650;
    line-height: 1.15;
    transition:
      background-color 140ms linear,
      color 140ms linear;
  }
  .modes button span {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .modes button:hover:not(.on) {
    background: var(--surface-2);
    color: var(--text-primary);
  }
  .modes button.on {
    background: var(--accent);
    color: var(--on-accent);
  }
  .modes button.on :global(.glyph) {
    color: #13785f;
  }
  /* Gaming standing in while a game runs (§11.5). */
  .modes button.playing:not(.on) {
    color: var(--text-primary);
    box-shadow: inset 0 0 0 1.5px var(--shell);
  }
  .proof {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 10px 12px 0;
    padding: 9px 12px;
    border-radius: 12px;
    background: var(--surface-1);
    font-size: 13px;
    line-height: 1.35;
    color: var(--text-secondary);
  }
  .proof :global(.glyph) {
    color: var(--shell-bright);
  }
  .list {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    padding: 12px 6px 0;
  }
  h2 {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    margin: 0;
    padding: 0 10px 6px;
    font-size: 13px;
    font-weight: 700;
    color: var(--text-secondary);
  }
  h2 span {
    font-weight: 600;
    color: var(--text-tertiary);
  }
  ul {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 0;
    list-style: none;
    overflow-y: auto;
  }
  li {
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 10px;
    min-height: 46px;
    padding: 3px 4px 3px 10px;
    border-radius: var(--radius-control);
    transition: background-color 120ms linear;
  }
  li:hover {
    background: var(--surface-1);
  }
  li.boosted {
    background: var(--shell-soft);
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
    white-space: nowrap;
  }
  .who b {
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 14px;
    font-weight: 700;
  }
  .who span {
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 12.5px;
    color: var(--text-secondary);
  }
  .who span.wait {
    color: var(--caution);
  }
  .who span.first {
    color: var(--shell-bright);
  }
  .mb {
    font-size: 13.5px;
    font-weight: 700;
  }
  .empty {
    margin: 0;
    padding: 4px 10px;
    font-size: 13.5px;
    color: var(--text-secondary);
  }
  .quiet {
    flex: 1;
    display: grid;
    place-items: center;
  }
  .error {
    margin: 8px 16px 0;
  }
  .foot {
    display: flex;
    flex: none;
    gap: 8px;
    padding: 10px 12px 12px;
    border-top: 1px solid var(--stroke-card);
  }
  .foot .btn {
    flex: 1;
  }
  .foot .resume {
    background: color-mix(in srgb, var(--caution) 16%, var(--surface-2));
    color: var(--caution);
  }
  .foot .resume:hover {
    background: color-mix(in srgb, var(--caution) 24%, var(--surface-2));
  }
  @media (forced-colors: active) {
    .modes button,
    .proof {
      border: 1px solid ButtonText;
    }
    .modes button.on {
      forced-color-adjust: none;
      background: Highlight;
      color: HighlightText;
    }
    li.boosted {
      outline: 1px solid Highlight;
    }
  }
</style>
