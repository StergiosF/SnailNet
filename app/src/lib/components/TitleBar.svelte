<script lang="ts">
  // Custom title bar (PLAN.md §14.2): one height on every page, so nothing
  // jumps when the page changes. It holds the mode switch (`lead`) at the
  // page's gutter, or the snail and the name when there's no side rail to
  // carry them (first run), and Windows 11 caption buttons drawn as thin
  // strokes like the system's own. Everything around the switch stays the
  // drag region. Snap layouts on hover over Maximize are not available:
  // with decorations off, WebView2 owns the mouse over the page and only
  // reports drag / no-drag regions, never a maximize button. Win+Z still
  // opens them.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount, type Snippet } from "svelte";
  import { t } from "../i18n";
  import Snail from "./Snail.svelte";

  let { brand = false, lead }: { brand?: boolean; lead?: Snippet } = $props();

  const win = getCurrentWindow();
  let maximized = $state(false);

  onMount(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const sync = async () => {
      const m = await win.isMaximized();
      if (!cancelled) maximized = m;
    };
    void sync();
    void win.onResized(sync).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  });
</script>

<header class="titlebar" class:lead={lead !== undefined} data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    {#if brand}
      <Snail size={18} light />
      <span data-tauri-drag-region>{t("app.name")}</span>
    {/if}
    {#if lead}{@render lead()}{/if}
  </div>
  <div class="caption">
    <button type="button" aria-label={t("window.minimize")} onclick={() => win.minimize()}>
      <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5h10" /></svg>
    </button>
    <button
      type="button"
      aria-label={maximized ? t("window.restore") : t("window.maximize")}
      onclick={() => win.toggleMaximize()}
    >
      <svg viewBox="0 0 10 10" aria-hidden="true">
        {#if maximized}
          <path d="M2.5 2.5V.5h7v7h-2" /><rect x=".5" y="2.5" width="7" height="7" />
        {:else}
          <rect x=".5" y=".5" width="9" height="9" />
        {/if}
      </svg>
    </button>
    <button type="button" class="close" aria-label={t("window.close")} onclick={() => win.close()}>
      <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 0l10 10M10 0 0 10" /></svg>
    </button>
  </div>
</header>

<style>
  .titlebar {
    display: flex;
    align-items: stretch;
    height: var(--titlebar-h);
    flex: none;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    padding-left: 14px;
    font-size: 13px;
    font-weight: 700;
    color: var(--text-secondary);
  }
  .lead .brand {
    padding-left: var(--gutter);
  }
  .caption {
    display: flex;
  }
  .caption button {
    display: grid;
    place-items: center;
    width: 46px;
    border: 0;
    padding: 0;
    background: transparent;
    color: var(--text-secondary);
    transition: background-color 83ms linear;
  }
  .caption svg {
    width: 10px;
    height: 10px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1;
    shape-rendering: crispEdges;
  }
  .caption .close svg {
    shape-rendering: geometricPrecision;
  }
  .caption button:hover {
    background: var(--fill-subtle-hover);
    color: var(--text-primary);
  }
  .caption button:active {
    background: var(--fill-subtle-pressed);
    color: var(--text-secondary);
  }
  .caption .close:hover {
    background: var(--close-hover);
    color: #ffffff;
  }
  .caption .close:active {
    background: color-mix(in srgb, var(--close-hover) 90%, transparent);
    color: rgb(255 255 255 / 0.7);
  }
  .caption button:focus-visible {
    outline-offset: -2px;
  }
  @media (max-width: 900px) {
    .lead .brand {
      padding-left: 20px;
    }
  }
</style>
