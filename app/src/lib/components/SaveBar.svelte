<script lang="ts">
  // The Split page's unsaved changes (PLAN.md §14.3): docked at the foot of
  // the page while the draft differs from what's saved, full width with a
  // hairline above it, so it reads as part of the window rather than a
  // card floating over the list. Save (ivory, Ctrl+S) sends it
  // (rev-checked), Discard drops it. When the saved split changed
  // somewhere else meanwhile it says so, and offers the new one or saving
  // this one over it. It slides up once, as the first change is made;
  // reduced motion keeps only the fade.
  import { t } from "../i18n";
  import { ui } from "../ui.svelte";
  import Glyph from "./Glyph.svelte";

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s" && ui.dirty && !ui.committing) {
      e.preventDefault();
      void ui.commit();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

{#if ui.dirty}
  <div class="bar" class:stale={ui.stale} class:failed={ui.draftError !== null} role="region" aria-label={t("draft.title")}>
    {#if ui.stale || ui.draftError}
      <span class="mark" aria-hidden="true"><Glyph name="info" size={20} /></span>
    {/if}
    <div class="words" aria-live="polite">
      {#if ui.draftError}
        <b>{t("draft.title")}</b>
        <span class="err">{ui.draftError}</span>
      {:else if ui.stale}
        <b>{t("draft.stale.title")}</b>
        <span>{t("draft.stale.body")}</span>
      {:else}
        <b>{t("draft.title")}</b>
        <span>{t("draft.body")}</span>
      {/if}
    </div>
    {#if ui.stale}
      <button type="button" class="btn btn-quiet" disabled={ui.committing} onclick={() => ui.discard()}
        >{t("draft.reload")}</button
      >
      <button type="button" class="btn btn-accent" disabled={ui.committing} onclick={() => void ui.commit()}
        >{t("draft.keep")}</button
      >
    {:else}
      <button type="button" class="btn btn-quiet" disabled={ui.committing} onclick={() => ui.discard()}
        >{t("draft.discard")}</button
      >
      <button
        type="button"
        class="btn btn-accent save"
        disabled={ui.committing}
        aria-keyshortcuts="Control+S"
        onclick={() => void ui.commit()}>{t("draft.save")}<kbd aria-hidden="true">Ctrl+S</kbd></button
      >
    {/if}
  </div>
{/if}

<style>
  /* Docked: sticky to the foot of the scrolling page, out to the window's
     edges (past the page's gutter and bottom padding). */
  .bar {
    position: sticky;
    bottom: 0;
    z-index: 2;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 12px;
    margin: 28px calc(-1 * var(--gutter)) -28px;
    padding: 12px var(--gutter);
    border-top: 1px solid var(--stroke-strong);
    background: color-mix(in srgb, var(--surface-side) 92%, transparent);
    backdrop-filter: blur(10px);
    animation: dock 340ms var(--ease-out) backwards;
  }
  @keyframes dock {
    from {
      opacity: 0;
      transform: translateY(100%);
    }
  }
  .mark {
    display: flex;
    flex: none;
    color: var(--caution);
  }
  .failed .mark {
    color: var(--critical);
  }
  .words {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 2px 10px;
    flex: 1;
    min-width: 0;
    font-size: 14px;
    line-height: 1.35;
  }
  .words b {
    font-size: 14.5px;
    font-weight: 700;
  }
  .words span {
    color: var(--text-secondary);
  }
  .words .err {
    color: var(--text-primary);
  }
  .btn {
    flex: none;
  }
  .save {
    min-width: 96px;
  }
  kbd {
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    opacity: 0.55;
  }
  @media (forced-colors: active) {
    .bar {
      border-top: 2px solid Highlight;
    }
  }
</style>
