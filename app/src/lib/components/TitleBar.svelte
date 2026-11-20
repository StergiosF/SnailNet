<script lang="ts">
  // Custom title bar (PLAN.md §14.2): a drag region (with the snail and
  // the name only when there's no side rail to carry them, as in first
  // run) and Windows 11 caption buttons drawn with Segoe Fluent Icons like
  // the system's own. Snap layouts on hover over Maximize are not available:
  // with decorations off, WebView2 owns the mouse over the page and only
  // reports drag / no-drag regions, never a maximize button, and every
  // workaround is a hack (native overlay window or faked Win+Z). Win+Z
  // itself still opens them.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount, type Snippet } from "svelte";
  import { t } from "../i18n";
  import Snail from "./Snail.svelte";

  // `lead`: what the main window puts in the strip (the mode switch, on
  // Now and Split); the strip then grows to hold it, and the rest of it
  // stays the drag region.
  let { brand = false, lead }: { brand?: boolean; lead?: Snippet } = $props();

  const win = getCurrentWindow();
  let maximized = $state(false);

  onMount(() => {
    let unlisten: (() => void) | undefined;
    const sync = async () => (maximized = await win.isMaximized());
    void sync();
    void win.onResized(sync).then((u) => (unlisten = u));
    return () => unlisten?.();
  });
</script>

<header class="titlebar" class:tall={lead !== undefined} data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    {#if brand}
      <Snail size={16} light />
      <span data-tauri-drag-region>{t("app.name")}</span>
    {/if}
    {#if lead}{@render lead()}{/if}
  </div>
  <div class="caption">
    <button type="button" aria-label={t("window.minimize")} onclick={() => win.minimize()}>
      <i class="icon" aria-hidden="true">&#xE921;</i>
    </button>
    <button
      type="button"
      aria-label={maximized ? t("window.restore") : t("window.maximize")}
      onclick={() => win.toggleMaximize()}
    >
      <i class="icon" aria-hidden="true">{maximized ? "" : ""}</i>
    </button>
    <button type="button" class="close" aria-label={t("window.close")} onclick={() => win.close()}>
      <i class="icon" aria-hidden="true">&#xE8BB;</i>
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
  .titlebar.tall {
    height: 76px;
  }
  .tall .brand {
    padding: 16px 0 8px var(--gutter);
  }
  .tall .caption {
    align-self: flex-start;
    height: var(--titlebar-h);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    padding-left: 14px;
    font-size: 12px;
    line-height: 16px;
    color: var(--text-secondary);
  }
  .caption {
    display: flex;
  }
  .caption button {
    width: 46px;
    border: 0;
    padding: 0;
    background: transparent;
    color: var(--text-secondary);
    font-size: 10px;
    transition: background-color 83ms linear;
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
</style>
