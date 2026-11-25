<script lang="ts">
  // The side rail (PLAN.md §14.2): the snail and the name, the pages (Now,
  // Split, Settings), and at the foot Pause (only while SnailNet is doing
  // something) and the internet speed as the user set it, a quiet line that
  // opens the speed dialog. It is a setting, not a live number, so it never
  // outshouts the speeds on Now. When a newer SnailNet is out, an update
  // button pops up there; one click installs it. Its top is part of the
  // window's drag region.
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
    <Snail size={26} light />
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
      <span class="k"><span class="kt">{t("sidebar.line")}</span><span class="pen">{t("settings.line.change")}</span></span>
      {#if hasLine}
        <span class="dirs num">
          <span class="dir"
            ><Glyph name="download" size={14} />{#if line.down_mbps === undefined}<span class="none"
                >{t("line.notSet")}</span
              >{:else}{fig(line.down_mbps)}<small>{t("line.unit")}</small>{/if}</span
          >
          <span class="dir"
            ><Glyph name="upload" size={14} />{#if line.up_mbps === undefined}<span class="none"
                >{t("line.notSet")}</span
              >{:else}{fig(line.up_mbps)}<small>{t("line.unit")}</small>{/if}</span
          >
        </span>
      {:else}
        <span class="unset">{t("sidebar.line.none")}</span>
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
      <Glyph name="download" size={18} />{updateLabel}
    </button>
  {/if}
</aside>

<style>
  .side {
    display: flex;
    flex-direction: column;
    width: var(--side-w);
    flex: none;
    padding: 12px 12px 14px;
    background: var(--surface-side);
    box-shadow: inset -1px 0 0 var(--stroke-card);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 32px;
    padding: 0 8px;
    margin-bottom: 22px;
    font-size: 16px;
    font-weight: 800;
    letter-spacing: -0.01em;
  }
  nav {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  /* The open page's highlight: one soft shape that slides between items. */
  .mark {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 40px;
    border-radius: var(--radius-control);
    background: var(--surface-2);
    transform: translateY(calc(var(--at) * 42px));
    transition: transform 360ms var(--ease-out);
  }
  nav button {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    height: 40px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font-size: 15px;
    font-weight: 600;
    text-align: left;
    transition:
      background-color 120ms linear,
      color 120ms linear;
  }
  nav button:hover {
    background: var(--surface-1);
    color: var(--text-primary);
  }
  nav button.on {
    color: var(--text-primary);
  }
  nav button.on:hover {
    background: transparent;
  }
  nav :global(.glyph) {
    transition: color 200ms linear;
  }
  nav button.on :global(.glyph) {
    color: var(--shell-bright);
  }
  .foot {
    margin-top: auto;
    margin-bottom: 10px;
  }
  /* The internet speed as the user set it: a quiet line at the rail's
     foot over a hairline; "Change" says it opens the speed dialog. */
  .line {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 4px;
    margin-top: auto;
    padding: 12px 10px 4px;
    border: 0;
    border-top: 1px solid var(--stroke-card);
    background: transparent;
    text-align: left;
  }
  .foot + .line {
    margin-top: 0;
  }
  .k {
    display: flex;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-tertiary);
  }
  .kt {
    flex: 1;
  }
  .pen {
    color: var(--text-tertiary);
    transition: color 120ms linear;
  }
  .line:hover .pen {
    color: var(--text-primary);
  }
  /* Two speeds side by side while they fit; a wide upload (111.5 Mbps)
     drops to its own line rather than run past the rail (2026-11-25). */
  .dirs {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 14px;
  }
  .dir {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
    font-size: 15px;
    font-weight: 700;
    color: var(--text-primary);
    transition: color 120ms linear;
  }
  .dir :global(.glyph) {
    color: var(--text-tertiary);
  }
  .line:hover .dir {
    color: var(--accent-hover);
  }
  small {
    margin-left: 2px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-tertiary);
  }
  .none {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .unset {
    font-size: 14.5px;
    font-weight: 700;
    color: var(--caution);
  }
  .update {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 38px;
    margin-top: 10px;
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
