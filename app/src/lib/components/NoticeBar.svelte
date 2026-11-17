<script lang="ts">
  // A state that needs attention (PLAN.md §14.8: not running, watching
  // only, retrying, safe mode…), at the top of whichever page is open,
  // with its one action.
  import type { Notice } from "../ui.svelte";

  let { notice }: { notice: Notice } = $props();

  const glyph = { info: "", caution: "", critical: "" };
</script>

<div class="notice {notice.severity}" role={notice.severity === "info" ? "status" : "alert"}>
  <i class="icon" aria-hidden="true">{glyph[notice.severity]}</i>
  <div class="words">
    <b>{notice.title}</b>
    <p>{notice.message}</p>
  </div>
  {#if notice.action}
    <button type="button" class="btn btn-accent" onclick={notice.action.onclick}>{notice.action.label}</button>
  {/if}
</div>

<style>
  .notice {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 16px;
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .icon {
    flex: none;
    font-size: 20px;
    color: var(--text-secondary);
  }
  .caution .icon {
    color: var(--caution);
  }
  .critical .icon {
    color: var(--critical);
  }
  .words {
    flex: 1;
    min-width: 0;
  }
  b {
    font-size: 16px;
  }
  p {
    margin: 2px 0 0;
    font-size: 14px;
    line-height: 1.45;
    color: var(--text-secondary);
  }
  .btn {
    flex: none;
  }
  @media (forced-colors: active) {
    .notice {
      border: 1px solid CanvasText;
    }
  }
</style>
