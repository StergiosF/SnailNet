<script lang="ts">
  // The tray flyout (PLAN.md §14.4): the quickest way to see who is using
  // the line and to change it. The snail, Pause / Resume; the main window's
  // sentence (or the state that needs attention) and, while the line is
  // full, the proof's one line (ping with and without SnailNet); the modes; a small live
  // strip; the boost, if one is on (who goes first, the time left in words
  // and as a shortening bar, Stop); the slices with − % + (quiet "only
  // when free" ones left out); and the way to the main window. Its − / +
  // save by themselves: it's the quick panel (ui.svelte.ts `change`). It hides on Escape and when it loses
  // focus (tray.rs).
  import { getCurrentWindow, Window } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { mbps, targetLabel } from "../format";
  import { t } from "../i18n";
  import { glance, ledger, parts, statusText } from "../ledger";
  import { pause, resume, startService, stopBoost, switchMode } from "../actions";
  import { availableModes, boostLeft, boostShare, liveBoost } from "../modes";
  import { newNetworkName } from "../line";
  import { noticeOf } from "../notice";
  import { pauseShown } from "../pause";
  import { flash } from "../flash";
  import { ProofWatch, proofOf } from "../proof";
  import { canStep, stepShare } from "../rules";
  import { service } from "../service.svelte";
  import type { Target } from "../types";
  import { ui } from "../ui.svelte";
  import FixOffer from "./FixOffer.svelte";
  import AppIcon from "./AppIcon.svelte";
  import Snail from "./Snail.svelte";
  import Stepper from "./Stepper.svelte";

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
  const proofLine = $derived(
    proof === null
      ? null
      : proof.without === null
        ? t("proof.live", { now: Math.round(proof.now), name: proof.name })
        : t("proof.flyout", { now: Math.round(proof.now), without: Math.round(proof.without) }),
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
  const strip = $derived(book ? parts(book) : []);
  const boost = $derived(connected ? liveBoost(service.snapshot?.config.boost, ui.now) : null);
  const boostBar = $derived(boost ? boostShare(boost, ui.now) : null);
  const scale = $derived(Math.max(lineBps ?? book?.shaped ?? 0, book?.total ?? 0, 1));
  const modes = $derived(availableModes(service.snapshot?.config.profiles ?? []));
  const active = $derived(service.snapshot?.config.active_profile ?? "");
  const picked = $derived(modes.find((m) => m.id === active));
  const paused = $derived.by(() => {
    const p = service.snapshot?.config.pause;
    return p !== undefined && (p.until === undefined || ui.now < p.until);
  });
  const rows = $derived(
    book?.slices.filter((s) => !(s.rule >= 0 && s.enabled && s.share === null && !s.active)) ?? [],
  );

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
  const ink = (i: number, shade: number) => (i >= 0 ? `var(--ink-${i})` : `var(--stone-${shade})`);
</script>

<svelte:window onkeydown={onKey} />

<div class="flyout">
  <div class="top">
    <Snail size={20} light />
    <span class="app">{t("app.name")}</span>
    {#if connected && pauseShown(service.snapshot?.config, ui.now)}
      <button
        type="button"
        class="pause"
        class:paused
        onclick={() => void (paused ? resume() : pause(60))}
        >{paused ? t("pause.resume") : t("pause.1h")}</button
      >
    {/if}
  </div>
  {#if notice}
    <h1>{notice.title}</h1>
    <p class="detail" role={notice.severity === "info" ? "status" : "alert"}>{notice.message}</p>
  {:else if sentence}
    <h1>{sentence.title}</h1>
    <p class="detail">{sentence.detail}</p>
    <FixOffer canEdit={connected} compact />
    {#if proofLine}
      <p class="proof num" aria-live="off">{proofLine}</p>
    {/if}
  {:else}
    <h1 class="wait">{t("link.connecting")}</h1>
  {/if}

  {#if connected && modes.length > 0}
    <div class="modes" role="radiogroup" aria-label={t("mode.label")}>
      {#each modes as m (m.id)}
        <button
          type="button"
          role="radio"
          aria-checked={m.id === active}
          class:on={m.id === active}
          class:playing={m.id === "gaming" && service.playing}
          onclick={() => void switchMode(m.id)}>{m.name}</button
        >
      {/each}
    </div>
    {#if service.playing && picked}
      <p class="detail" role="status">{t("mode.playing.note", { mode: picked.name })}</p>
    {/if}
  {/if}

  {#if book && service.stats}
    <div class="bar" aria-hidden="true">
      {#each strip as p (p.key)}
        <i style:width="{(p.bps / scale) * 100}%" style:background={ink(p.ink, p.shade)}></i>
      {/each}
    </div>
  {/if}

  {#if boost}
    <div class="boost">
      <AppIcon target={boost.target} name={label(boost.target)} size={18} />
      <span class="bw">{t("flyout.boost", { name: label(boost.target), left: boostLeft(boost, ui.now) })}</span>
      <button
        type="button"
        aria-label={t("boost.stop.label", { name: label(boost.target) })}
        onclick={() => void stopBoost()}>{t("boost.stop")}</button
      >
      {#if boostBar !== null}
        <span class="left" aria-hidden="true"><i style:width="{boostBar * 100}%"></i></span>
      {/if}
    </div>
  {/if}

  {#if book}
    <ul aria-label={t("split.label")}>
      {#each rows as s (s.key)}
        <li use:flash={`${s.share}|${s.enabled}`}>
          <AppIcon target={s.target} name={s.name} size={20} />
          <span class="who">
            <b>{s.name}</b>
            <span
              >{s.bps >= 50_000
                ? t("row.moving", { mbps: mbps(s.bps), status: statusText(s.status) })
                : statusText(s.status)}</span
            >
          </span>
          {#if s.enabled}
            <Stepper
              name={s.name}
              pct={s.share === null ? null : Math.round(s.share / 100)}
              canLess={canStep(ui.rules, s.rule, -1)}
              canMore={canStep(ui.rules, s.rule, 1)}
              onstep={(dir) => ui.change(stepShare(ui.rules, s.rule, dir))}
              compact
            />
          {:else}
            <span class="off">{t("row.off")}</span>
          {/if}
        </li>
      {/each}
    </ul>
  {:else}
    <div class="quiet"><Snail size={32} light /></div>
  {/if}
  {#if ui.error}
    <p class="error" role="alert">{ui.error}</p>
  {/if}

  <button type="button" class="btn open" onclick={() => void openMain()}>
    {t("flyout.open")}<i class="icon" aria-hidden="true">&#xE8A7;</i>
  </button>
</div>

<style>
  .flyout {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 16px;
    overflow: hidden;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
  }
  .app {
    flex: 1;
    font-size: 13px;
    font-weight: 700;
    color: var(--text-secondary);
  }
  .pause {
    height: 28px;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: var(--surface-1);
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 700;
  }
  .pause:hover {
    background: var(--surface-2);
  }
  .pause.paused {
    background: var(--caution);
    color: #2a1d05;
  }
  .modes {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 6px;
    margin-top: 14px;
  }
  .modes button {
    height: 34px;
    border: 0;
    border-radius: 9px;
    background: var(--surface-1);
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 700;
    transition: background-color 140ms linear, color 140ms linear;
  }
  .modes button:hover:not(.on) {
    background: var(--surface-2);
    color: var(--text-primary);
  }
  .modes button.on {
    background: var(--accent);
    color: var(--on-accent);
  }
  /* Gaming standing in while a game runs (§11.5). */
  .modes button.playing {
    color: var(--accent);
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  @media (forced-colors: active) {
    .modes button,
    .pause {
      border: 1px solid ButtonText;
    }
    .modes button.on {
      forced-color-adjust: none;
      background: Highlight;
      color: HighlightText;
    }
  }
  h1 {
    margin: 0;
    font-size: 18px;
    line-height: 1.3;
    font-weight: 700;
    text-wrap: balance;
  }
  h1.wait {
    color: var(--text-secondary);
    font-weight: 600;
  }
  .detail {
    margin: 4px 0 0;
    font-size: 13px;
    line-height: 1.4;
    color: var(--text-secondary);
  }
  .proof {
    margin: 6px 0 0;
    font-size: 13px;
    line-height: 1.4;
    font-weight: 700;
    color: var(--shell-hover);
  }
  @media (forced-colors: active) {
    .proof {
      color: CanvasText;
    }
  }
  .bar {
    display: flex;
    gap: 2px;
    flex: none;
    height: 8px;
    margin-top: 14px;
    border-radius: 3px;
    background: var(--surface-1);
    overflow: hidden;
  }
  .bar i {
    display: block;
    flex: 0 1 auto;
    min-width: 2px;
    border-radius: 2px;
  }
  .boost {
    position: relative;
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
    padding: 8px 8px 10px 10px;
    border-radius: var(--radius-control);
    overflow: hidden;
    background: linear-gradient(90deg, rgb(47 168 142 / 0.26), rgb(47 168 142 / 0.08));
    box-shadow: inset 0 0 0 1px rgb(47 168 142 / 0.4);
    animation: rise-in 420ms var(--ease-out) backwards;
  }
  .bw {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    font-weight: 700;
  }
  .boost button {
    flex: none;
    height: 26px;
    padding: 0 10px;
    border: 0;
    border-radius: 7px;
    background: var(--shell);
    color: var(--on-shell);
    font-size: 12px;
    font-weight: 700;
  }
  .boost button:hover {
    background: var(--shell-hover);
  }
  .boost .left {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 3px;
    background: rgb(47 168 142 / 0.18);
  }
  .boost .left i {
    display: block;
    height: 100%;
    background: var(--shell);
    transition: width 1200ms linear;
  }
  @media (forced-colors: active) {
    .boost {
      border: 1px solid Highlight;
    }
    .boost .left i,
    .boost button {
      forced-color-adjust: none;
      background: Highlight;
      color: HighlightText;
    }
  }
  ul {
    flex: 1;
    min-height: 0;
    margin: 12px -8px 0;
    padding: 0;
    list-style: none;
    overflow-y: auto;
  }
  li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 48px;
    padding: 0 8px;
    border-radius: var(--radius-control);
  }
  li:hover {
    background: var(--surface-1);
  }
  .who {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    white-space: nowrap;
  }
  .who b {
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 14px;
    font-weight: 600;
  }
  .who span {
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 12px;
    color: var(--text-secondary);
  }
  .off {
    flex: none;
    font-size: 13px;
    color: var(--text-secondary);
  }
  .quiet {
    flex: 1;
    display: grid;
    place-items: center;
  }
  .error {
    margin: 8px 0 0;
  }
  .open {
    flex: none;
    margin-top: 12px;
  }
  .open .icon {
    font-size: 12px;
  }
</style>
