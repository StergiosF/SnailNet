<script lang="ts">
  // A state that needs attention (PLAN.md §14.8: not running, watching
  // only, retrying, safe mode…), at the top of whichever page is open,
  // with its one action. Each severity has its own tone: a faint tint of
  // it over the panel and its glyph in full colour (never a side stripe).
  import type { Notice } from "../ui.svelte";
  import Glyph, { type GlyphName } from "./Glyph.svelte";

  let { notice }: { notice: Notice } = $props();

  const glyph: Record<Notice["severity"], GlyphName> = { info: "info", caution: "clock", critical: "power" };
</script>

<div class="notice {notice.severity}" role={notice.severity === "info" ? "status" : "alert"}>
  <span class="gl"><Glyph name={glyph[notice.severity]} /></span>
  <div class="words">
    <b>{notice.title}</b>
    <p>{notice.message}</p>
  </div>
  {#if notice.action}
    <button type="button" class="btn" class:btn-accent={notice.severity === "critical"} onclick={notice.action.onclick}
      >{notice.action.label}</button
    >
  {/if}
</div>

<style>
  .notice {
    --tone: var(--ink-2);
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 14px 12px 16px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--tone) 7%, var(--surface-1));
  }
  .caution {
    --tone: var(--caution);
  }
  .critical {
    --tone: var(--critical);
  }
  .gl {
    display: flex;
    flex: none;
    align-self: flex-start;
    margin-top: 1px;
    color: var(--tone);
  }
  .words {
    flex: 1;
    min-width: 0;
  }
  b {
    font-size: 15px;
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
