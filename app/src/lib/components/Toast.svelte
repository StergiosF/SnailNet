<script lang="ts">
  // A short message at the bottom of the window ("The Gaming split is back
  // to how it started. Undo"), read out politely. It goes away by itself
  // after a few seconds (ui.svelte.ts) or when dismissed.
  import { t } from "../i18n";
  import { ui } from "../ui.svelte";
</script>

<div class="slot" role="status" aria-live="polite">
  {#if ui.toast}
    {@const toast = ui.toast}
    <div class="toast">
      <span>{toast.message}</span>
      {#if toast.action}
        <button
          type="button"
          class="act"
          onclick={() => {
            toast.action?.run();
            ui.toast = null;
          }}>{toast.action.label}</button
        >
      {/if}
      <button type="button" class="x" aria-label={t("dismiss")} onclick={() => (ui.toast = null)}>
        <i class="icon" aria-hidden="true">&#xE711;</i>
      </button>
    </div>
  {/if}
</div>

<style>
  .slot {
    position: fixed;
    left: calc(var(--side-w) + var(--gutter));
    right: var(--gutter);
    bottom: 20px;
    display: flex;
    justify-content: center;
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 12px;
    max-width: 560px;
    padding: 8px 8px 8px 16px;
    border-radius: 14px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 14px;
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.4);
    pointer-events: auto;
    animation: rise 180ms var(--ease-out);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  button {
    border: 0;
    background: transparent;
    color: inherit;
  }
  .act {
    height: 32px;
    padding: 0 12px;
    border-radius: 9px;
    font-weight: 800;
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .act:hover,
  .x:hover {
    background: rgb(10 33 29 / 0.08);
  }
  .x {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 9px;
    font-size: 11px;
  }
  @media (forced-colors: active) {
    .toast {
      border: 1px solid CanvasText;
    }
  }
</style>
