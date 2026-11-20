<script lang="ts">
  // A slice's share with − and + beside it (PLAN.md §14.2), in 5% steps.
  // The main window's rows and the tray flyout both use it; `compact` is
  // the flyout's size. The figure is a live region, so a screen reader
  // hears the new share after a step. On the Split page a share changed
  // but not saved yet shows what it was underneath. A new figure settles
  // in from just below (DESIGN.md, change feedback); reduced motion: at once.
  import { fly } from "svelte/transition";
  import { t } from "../i18n";
  import { ms } from "../motion";

  let {
    name,
    pct,
    was = null,
    canLess,
    canMore,
    onstep,
    compact = false,
  }: {
    name: string;
    /** The share in whole percent; null for a slice that takes only what is free. */
    pct: number | null;
    /** The saved share in whole percent, while a change isn't saved. */
    was?: number | null;
    canLess: boolean;
    canMore: boolean;
    onstep: (dir: 1 | -1) => void;
    compact?: boolean;
  } = $props();
</script>

<span class="stepper" class:compact>
  <button type="button" class="step" disabled={!canLess} aria-label={t("row.less", { name })} onclick={() => onstep(-1)}
    ><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8h9" /></svg></button
  >
  <span class="fig">
    <output class="n" aria-live="polite" aria-label={pct === null ? t("row.free") : t("row.share", { name, pct })}>
      {#if pct === null}<span class="free">{t("row.free")}</span>{:else}{#key pct}<span
            class="v"
            in:fly={{ y: 4, duration: ms(260) }}>{pct}<small>%</small></span
          >{/key}{/if}
    </output>
    {#if was !== null && !compact}
      <span class="was" aria-label={t("row.was.label", { name, pct: was })}>{t("row.was", { pct: was })}</span>
    {/if}
  </span>
  <button type="button" class="step" disabled={!canMore} aria-label={t("row.more", { name })} onclick={() => onstep(1)}
    ><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8h9M8 3.5v9" /></svg></button
  >
</span>

<style>
  /* One dark well holding − % +, a hairline round it. */
  .stepper {
    display: flex;
    flex: none;
    align-items: center;
    gap: 2px;
    padding: 2px;
    border-radius: var(--radius-control);
    background: var(--surface-side);
    box-shadow: inset 0 0 0 1px var(--stroke-card);
  }
  /* Drawn, not typed: a font's − and + sit off the middle of the button. */
  .step {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 18px;
    line-height: 1;
    transition:
      background-color 120ms linear,
      color 120ms linear;
  }
  .step svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
  }
  .step:hover:not(:disabled) {
    background: var(--surface-3);
    color: var(--text-primary);
  }
  .step:disabled {
    opacity: 0.35;
  }
  .fig {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  /* What it was, hanging just under the figure so the well keeps its size. */
  .was {
    position: absolute;
    top: calc(100% + 4px);
    font-size: 11.5px;
    font-weight: 600;
    white-space: nowrap;
    color: var(--text-tertiary);
  }
  .n {
    width: 52px;
    text-align: center;
    font-size: 15px;
    font-weight: 800;
  }
  .n .v {
    display: inline-block;
  }
  .n small {
    margin-left: 1px;
    font-size: 13px;
    font-weight: 700;
    color: var(--text-secondary);
  }
  .free {
    font-size: 14px;
    font-weight: 600;
    letter-spacing: 0;
    color: var(--text-secondary);
  }

  .compact .step {
    width: 26px;
    height: 26px;
    border-radius: 7px;
  }
  .compact .step svg {
    width: 12px;
    height: 12px;
  }
  .compact .n {
    width: 44px;
    font-size: 14px;
  }
  .compact .n small {
    margin-left: 0;
    font-size: 12px;
  }
  .compact .free {
    font-size: 12px;
  }

  @media (forced-colors: active) {
    .stepper {
      border: 1px solid CanvasText;
    }
    .step {
      border: 1px solid ButtonText;
    }
  }
</style>
