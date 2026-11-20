<script lang="ts">
  // The side rail (PLAN.md §14.2): the snail and the name, the pages (Now,
  // Split, Settings), and at the bottom Pause and the line as the user set
  // it, which opens the speed dialog. When a newer SnailNet is out, an
  // update button pops up there; one click installs it. Its top is part of the window's drag
  // region.
  import { t } from "../i18n";
  import { lineDialog } from "../lineDialog.svelte";
  import { pauseShown } from "../pause";
  import { service } from "../service.svelte";
  import { ui, type Page } from "../ui.svelte";
  import { updates } from "../update.svelte";
  import Glyph, { type GlyphName } from "./Glyph.svelte";
  import PauseControl from "./PauseControl.svelte";
  import Snail from "./Snail.svelte";

  const pages: { id: Page; label: string; glyph: GlyphName }[] = [
    { id: "now", label: t("nav.now"), glyph: "now" },
    { id: "split", label: t("nav.split"), glyph: "split" },
    { id: "settings", label: t("nav.settings"), glyph: "settings" },
  ];

  const line = $derived(service.snapshot?.config.line ?? {});
  // Pause only while SnailNet is doing something the user set up (or is
  // paused, for Resume); the tray menu always has it (pause.ts).
  const showPause = $derived(pauseShown(service.snapshot?.config, ui.now));
  const canSet = $derived(service.link.kind === "connected" && service.snapshot !== null);
  const hasLine = $derived(line.down_mbps !== undefined || line.up_mbps !== undefined);
  const fig = (v: number | undefined) => (v === undefined ? "–" : String(Math.round(v * 10) / 10));
  const updateLabel = $derived(
    updates.status.kind === "available"
      ? t("sidebar.update.available", { version: updates.status.version })
      : t("sidebar.update.installing"),
  );
</script>

<aside class="side">
  <div class="brand" data-tauri-drag-region>
    <Snail size={30} light />
    <span data-tauri-drag-region>{t("app.name")}</span>
  </div>

  <nav aria-label={t("nav.label")} style:--at={pages.findIndex((p) => p.id === ui.page)}>
    <span class="mark" aria-hidden="true"></span>
    {#each pages as p (p.id)}
      <button
        type="button"
        class:on={ui.page === p.id}
        aria-current={ui.page === p.id ? "page" : undefined}
        onclick={() => ui.go(p.id)}
      >
        <Glyph name={p.glyph} />{p.label}
      </button>
    {/each}
  </nav>

  {#if canSet}
    {#if showPause}
      <div class="foot">
        <PauseControl disabled={!canSet} />
      </div>
    {/if}
    <button
      type="button"
      class="line"
      onclick={() => lineDialog.show()}
      aria-label={hasLine
        ? t("sidebar.line.label", { down: fig(line.down_mbps), up: fig(line.up_mbps) })
        : t("header.line.set")}
    >
      <span class="k">{t("sidebar.line")}<i class="icon pen" aria-hidden="true">&#xE70F;</i></span>
      {#if hasLine}
        {#each [{ label: t("sidebar.down"), v: line.down_mbps }, { label: t("sidebar.up"), v: line.up_mbps }] as row (row.label)}
          <span class="row">
            <span class="dirname">{row.label}</span>
            {#if row.v === undefined}
              <span class="none">{t("line.notSet")}</span>
            {:else}
              <span class="v num">{fig(row.v)}<small>{t("line.unit")}</small></span>
            {/if}
          </span>
        {/each}
      {:else}
        <span class="v unset">{t("sidebar.line.none")}</span>
      {/if}
    </button>
  {/if}
  {#if updates.available || updates.status.kind === "installing"}
    <button
      type="button"
      class="update"
      disabled={updates.busy}
      aria-live="polite"
      onclick={() => void updates.install()}
    >
      <i class="icon" aria-hidden="true">&#xE896;</i>{updateLabel}
    </button>
  {/if}
</aside>

<style>
  .side {
    display: flex;
    flex-direction: column;
    width: var(--side-w);
    flex: none;
    padding: 12px 12px 16px;
    background: var(--surface-side);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px 24px;
    font-size: 18px;
    font-weight: 800;
    letter-spacing: -0.01em;
  }
  nav {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  /* The open page's highlight, one shape that slides between items. */
  .mark {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 42px;
    border-radius: var(--radius-control);
    background: var(--shell-soft);
    transform: translateY(calc(var(--at) * 44px));
    transition: transform 360ms var(--ease-out);
  }
  nav button {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    height: 42px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font-size: 15px;
    text-align: left;
    transition: background-color 120ms linear, color 120ms linear;
  }
  nav button:hover {
    background: var(--fill-subtle-hover);
    color: var(--text-primary);
  }
  nav button.on {
    color: var(--text-primary);
    font-weight: 700;
  }
  nav button.on:hover {
    background: transparent;
  }
  nav :global(.glyph) {
    transition: color 200ms linear;
  }
  nav button.on :global(.glyph) {
    color: var(--shell);
  }
  .line {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 4px;
    /* At the foot of the rail; under Pause when Pause shows. */
    margin-top: auto;
    padding: 10px 12px 12px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    text-align: left;
    transition: background-color 120ms linear;
  }
  .foot {
    margin-top: auto;
  }
  .foot + .line {
    margin-top: 8px;
  }
  .line:hover {
    background: var(--fill-subtle-hover);
  }
  .k {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 2px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }
  .dirname,
  .none {
    font-size: 13px;
    color: var(--text-secondary);
  }
  .v {
    font-size: 16px;
    font-weight: 700;
  }
  .v.unset {
    font-weight: 600;
    color: var(--caution);
  }
  small {
    margin-left: 2px;
    font-size: 13px;
    font-weight: 400;
    color: var(--text-secondary);
  }
  .pen {
    font-size: 11px;
    opacity: 0;
    transition: opacity 160ms linear;
  }
  .line:hover .pen,
  .line:focus-visible:not([data-quiet-focus]) .pen {
    opacity: 1;
  }
  .update {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 38px;
    margin-top: 8px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--radius-control);
    background: var(--accent);
    color: var(--on-accent);
    font-size: 14px;
    font-weight: 700;
    text-align: left;
    transition: background-color 120ms linear;
    animation: rise-in 420ms var(--ease-out) backwards;
  }
  .update:hover:not(:disabled) {
    background: var(--accent-hover);
  }
  .update .icon {
    font-size: 13px;
  }
  .update:disabled {
    opacity: 0.7;
  }
  @media (forced-colors: active) {
    .update {
      outline: 2px solid Highlight;
    }
    .side {
      border-right: 1px solid CanvasText;
    }
    .mark {
      display: none;
    }
    nav button.on {
      outline: 2px solid Highlight;
      outline-offset: -2px;
    }
  }
</style>
