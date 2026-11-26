<script lang="ts">
  // A short message at the bottom of the window ("The Gaming split is back
  // to how it started. Undo"), read out politely. It goes away by itself
  // after a few seconds (ui.svelte.ts) or when dismissed.
  import { t } from "../i18n";
  import { ui } from "../ui.svelte";
  import Glyph from "./Glyph.svelte";
</script>

<div class="slot">
  {#if ui.toast}
    {@const toast = ui.toast}
    {@const isError = toast.kind === "error"}
    <div class="toast" class:err={isError} role={isError ? "alert" : "status"}>
      <Glyph name={isError ? "info" : "check"} size={18} />
      <span class="msg">{toast.message}</span>
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
        <Glyph name="close" size={16} />
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
  /* Quiet and dark: a raised surface with a jade tick, not a bright pill. */
  .toast {
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: 560px;
    padding: 6px 6px 6px 14px;
    border-radius: 12px;
    background: var(--surface-3);
    color: var(--text-primary);
    font-size: 14px;
    font-weight: 600;
    box-shadow:
      var(--shadow-raised),
      inset 0 0 0 1px var(--stroke-strong);
    pointer-events: auto;
    animation: rise 220ms var(--ease-out);
  }
  .toast > :global(.glyph) {
    color: var(--shell-bright);
  }
  .toast.err > :global(.glyph) {
    color: var(--critical);
  }
  .msg {
    min-width: 0;
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
    height: 30px;
    padding: 0 12px;
    border-radius: 8px;
    font-weight: 700;
    color: var(--text-primary);
    background: var(--surface-2);
    transition: background-color 120ms linear;
  }
  .act:hover {
    background: var(--surface-1);
  }
  .x {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    color: var(--text-secondary);
    transition:
      background-color 120ms linear,
      color 120ms linear;
  }
  .x:hover {
    background: var(--hover-strong);
    color: var(--text-primary);
  }
  @media (forced-colors: active) {
    .toast {
      border: 1px solid CanvasText;
    }
  }
</style>
