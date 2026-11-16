<script lang="ts">
  // The tray flyout (PLAN.md §14.4): the quickest way to see who is using
  // the line and to give something more. The main window's sentence (or
  // the state that needs attention), a small live strip, every slice with
  // − % +, and the way to the main window. It hides on Escape and when it
  // loses focus (tray.rs).
  import { getCurrentWindow, Window } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { mbps, targetLabel } from "../format";
  import { t } from "../i18n";
  import { glance, ledger, parts, statusText } from "../ledger";
  import { noticeOf } from "../notice";
  import { canStep, stepShare } from "../rules";
  import { service } from "../service.svelte";
  import type { Target } from "../types";
  import { ui } from "../ui.svelte";
  import AppIcon from "./AppIcon.svelte";
  import Snail from "./Snail.svelte";

  onMount(() => {
    service.start().catch((e: unknown) => {
      service.link = { kind: "lost", detail: String(e) };
    });
  });

  const connected = $derived(service.link.kind === "connected" && service.snapshot !== null);
  const label = (target: Target) => targetLabel(target, service.snapshot?.apps ?? [], service.siteNames);
  const book = $derived(connected ? ledger(ui.rules, service.stats?.down, label) : null);
  const lineMbps = $derived(service.snapshot?.config.line.down_mbps);
  const lineBps = $derived(lineMbps === undefined ? null : lineMbps * 1e6);
  const sentence = $derived(book && service.stats ? glance(book, lineBps) : null);
  const notice = $derived(noticeOf(service.link, service.state, () => void openMain()));
  const strip = $derived(book ? parts(book) : []);
  const scale = $derived(Math.max(lineBps ?? book?.shaped ?? 0, book?.total ?? 0, 1));

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
  <p class="app">{t("app.name")}</p>
  {#if notice}
    <h1>{notice.title}</h1>
    <p class="detail" role={notice.severity === "info" ? "status" : "alert"}>{notice.message}</p>
  {:else if sentence}
    <h1>{sentence.title}</h1>
    <p class="detail">{sentence.detail}</p>
  {:else}
    <h1 class="wait">{t("link.connecting")}</h1>
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
      {#each book.slices as s (s.key)}
        {@const pct = s.share === null ? null : Math.round(s.share / 100)}
        <li>
          <AppIcon target={s.target} name={s.name} size={20} />
          <span class="who">
            <b>{s.name}</b>
            <span>{s.bps >= 50_000 ? `${mbps(s.bps)} Mbps · ` : ""}{statusText(s.status)}</span>
          </span>
          {#if s.enabled}
            <button
              type="button"
              class="step"
              disabled={!canStep(ui.rules, s.rule, -1)}
              aria-label={t("row.less", { name: s.name })}
              onclick={() => ui.change(stepShare(ui.rules, s.rule, -1))}>−</button
            >
            <output class="n" aria-live="polite" aria-label={pct === null ? t("row.free") : t("row.share", { name: s.name, pct })}>
              {#if pct === null}<small>{t("row.free")}</small>{:else}{pct}<small>%</small>{/if}
            </output>
            <button
              type="button"
              class="step"
              disabled={!canStep(ui.rules, s.rule, 1)}
              aria-label={t("row.more", { name: s.name })}
              onclick={() => ui.change(stepShare(ui.rules, s.rule, 1))}>+</button
            >
          {:else}
            <span class="n off">{t("row.off")}</span>
          {/if}
        </li>
      {/each}
    </ul>
  {:else}
    <div class="quiet"><Snail size={32} /></div>
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
  .app {
    margin: 0 0 8px;
    font-size: 12px;
    color: var(--text-secondary);
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
  .step {
    flex: none;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text-secondary);
    font-size: 16px;
    line-height: 1;
  }
  .step:hover:not(:disabled) {
    background: var(--surface-3);
    color: var(--text-primary);
  }
  .step:disabled {
    opacity: 0.35;
  }
  .n {
    flex: none;
    width: 50px;
    text-align: center;
    font-size: 18px;
    font-weight: 700;
  }
  .n small {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .n.off {
    width: auto;
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
    font-size: 13px;
    color: var(--critical);
  }
  .open {
    flex: none;
    margin-top: 12px;
  }
  .open .icon {
    font-size: 12px;
  }
</style>
