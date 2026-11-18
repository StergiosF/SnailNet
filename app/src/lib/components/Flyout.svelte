<script lang="ts">
  // The tray flyout (PLAN.md §14.4): the quickest way to see who is using
  // the line and to change it. The snail, Pause / Resume; the main window's
  // sentence (or the state that needs attention); the modes; a small live
  // strip; the slices with − % + (quiet "only when free" ones left out);
  // and the way to the main window. It hides on Escape and when it loses
  // focus (tray.rs).
  import { getCurrentWindow, Window } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { mbps, targetLabel } from "../format";
  import { t } from "../i18n";
  import { glance, ledger, parts, statusText } from "../ledger";
  import { pause, resume, startService, switchMode } from "../actions";
  import { availableModes } from "../modes";
  import { noticeOf } from "../notice";
  import { QUIET_BPS } from "../ledger";
  import { canStep, stepShare } from "../rules";
  import { service } from "../service.svelte";
  import type { Target } from "../types";
  import { ui } from "../ui.svelte";
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
  const book = $derived(connected ? ledger(ui.rules, service.stats?.down, label) : null);
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
    ),
  );
  const strip = $derived(book ? parts(book) : []);
  const scale = $derived(Math.max(lineBps ?? book?.shaped ?? 0, book?.total ?? 0, 1));
  const modes = $derived(availableModes(service.snapshot?.config.profiles ?? []));
  const active = $derived(service.snapshot?.config.active_profile ?? "");
  const paused = $derived.by(() => {
    const p = service.snapshot?.config.pause;
    return p !== undefined && (p.until === undefined || ui.now < p.until);
  });
  const rows = $derived(
    book?.slices.filter((s) => !(s.rule >= 0 && s.enabled && s.share === null && s.bps < QUIET_BPS)) ?? [],
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
    {#if connected}
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
          onclick={() => void switchMode(m.id)}>{t(m.name)}</button
        >
      {/each}
    </div>
  {/if}

  {#if book && service.stats}
    <div class="bar" aria-hidden="true">
      {#each strip as p (p.key)}
        <i style:width="{(p.bps / scale) * 100}%" style:background={ink(p.ink, p.shade)}></i>
      {/each}
    </div>
  {/if}

  {#if book}
    <ul aria-label={t("split.label")}>
      {#each rows as s (s.key)}
        <li>
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
