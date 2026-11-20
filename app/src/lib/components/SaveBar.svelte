<script lang="ts">
  // The Split page's unsaved changes (PLAN.md §14.3): a bar that stays at
  // the bottom of the page while the draft differs from what's saved.
  // Save sends it (rev-checked), Discard drops it. When the saved split
  // changed somewhere else meanwhile it says so, and offers the new one
  // or saving this one over it. It rises in once, as the first change is
  // made; reduced motion keeps only the fade. Ctrl+S saves too.
  import { t } from "../i18n";
  import { ui } from "../ui.svelte";

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
    <i class="icon mark" aria-hidden="true">{ui.stale || ui.draftError ? "" : ""}</i>
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
      <button type="button" class="btn btn-accent save" disabled={ui.committing} onclick={() => void ui.commit()}
        title={t("draft.save.key")}>{t("draft.save")}</button
      >
    {/if}
  </div>
{/if}

<style>
  .bar {
    position: sticky;
    bottom: 16px;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 16px;
    padding: 12px 12px 12px 16px;
    border-radius: var(--radius-card);
    background: var(--surface-3);
    box-shadow: var(--shadow-raised);
    animation: rise-in 360ms var(--ease-out) backwards;
  }
  .mark {
    flex: none;
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 9px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 14px;
  }
  .stale .mark {
    background: var(--caution);
  }
  .words {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    font-size: 14px;
    line-height: 1.35;
  }
  .words b {
    font-size: 15px;
    font-weight: 700;
  }
  .words span {
    color: var(--text-secondary);
  }
  /* Red text falls under 4.5:1 on this surface: the mark carries the
     colour, the words stay light. */
  .words .err {
    color: var(--text-primary);
  }
  .failed .mark {
    background: var(--critical);
  }
  .btn {
    flex: none;
  }
  .save {
    min-width: 96px;
  }
  @media (forced-colors: active) {
    .bar {
      border: 2px solid Highlight;
    }
  }
</style>
