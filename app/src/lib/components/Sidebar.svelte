<script lang="ts">
  // The side rail (PLAN.md §14.2): the snail and the name, the pages (Now,
  // Split, Settings), and at the bottom Pause and the line as the user set
  // it, which opens the speed dialog. Its top is part of the window's drag
  // region.
  import { t } from "../i18n";
  import { lineDialog } from "../lineDialog.svelte";
  import { service } from "../service.svelte";
  import { ui, type Page } from "../ui.svelte";
  import PauseControl from "./PauseControl.svelte";
  import Snail from "./Snail.svelte";

  const pages: { id: Page; label: string; glyph: string }[] = [
    { id: "now", label: t("nav.now"), glyph: "" },
    { id: "split", label: t("nav.split"), glyph: "" },
    { id: "settings", label: t("nav.settings"), glyph: "" },
  ];

  const line = $derived(service.snapshot?.config.line ?? {});
  const canSet = $derived(service.link.kind === "connected" && service.snapshot !== null);
  const hasLine = $derived(line.down_mbps !== undefined || line.up_mbps !== undefined);
  const fig = (v: number | undefined) => (v === undefined ? "–" : String(Math.round(v * 10) / 10));
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
        onclick={() => (ui.page = p.id)}
      >
        <i class="icon" aria-hidden="true">{p.glyph}</i>{p.label}
      </button>
    {/each}
  </nav>

  {#if canSet}
    <div class="foot">
      <PauseControl disabled={!canSet} />
    </div>
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
        <span class="v"
          >{fig(line.down_mbps)}<i class="icon dir" aria-hidden="true">&#xE896;</i>{fig(line.up_mbps)}<i
            class="icon dir"
            aria-hidden="true">&#xE898;</i
          ><small>{t("line.unit")}</small></span
        >
      {:else}
        <span class="v unset">{t("sidebar.line.none")}</span>
      {/if}
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
  nav .icon {
    width: 18px;
    font-size: 16px;
    transition: color 200ms linear;
  }
  nav button.on .icon {
    color: var(--shell);
  }
  .line {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    margin-top: 6px;
    padding: 10px 12px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    text-align: left;
    transition: background-color 120ms linear;
  }
  .foot {
    margin-top: auto;
  }
  .line:hover {
    background: var(--fill-subtle-hover);
  }
  .k {
    display: flex;
    align-items: center;
    gap: 8px;
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
  .dir {
    margin: 0 8px 0 3px;
    font-size: 11px;
    color: var(--text-secondary);
  }
  .pen {
    font-size: 11px;
  }
  @media (forced-colors: active) {
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
