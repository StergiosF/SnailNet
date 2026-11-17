<script lang="ts">
  // A slice's share with − and + beside it (PLAN.md §14.2), in 5% steps.
  // The main window's rows and the tray flyout both use it; `compact` is
  // the flyout's size. The figure is a live region, so a screen reader
  // hears the new share after a step.
  import { t } from "../i18n";

  let {
    name,
    pct,
    canLess,
    canMore,
    onstep,
    compact = false,
  }: {
    name: string;
    /** The share in whole percent; null for a slice that takes only what is free. */
    pct: number | null;
    canLess: boolean;
    canMore: boolean;
    onstep: (dir: 1 | -1) => void;
    compact?: boolean;
  } = $props();
</script>

<span class="stepper" class:compact>
  <button type="button" class="step" disabled={!canLess} aria-label={t("row.less", { name })} onclick={() => onstep(-1)}
    >−</button
  >
  <output class="n" aria-live="polite" aria-label={pct === null ? t("row.free") : t("row.share", { name, pct })}>
    {#if pct === null}<span class="free">{t("row.free")}</span>{:else}{pct}<small>%</small>{/if}
  </output>
  <button type="button" class="step" disabled={!canMore} aria-label={t("row.more", { name })} onclick={() => onstep(1)}
    >+</button
  >
</span>

<style>
  .stepper {
    display: flex;
    flex: none;
    align-items: center;
    gap: 6px;
  }
  .step {
    width: 34px;
    height: 34px;
    border: 0;
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--text-secondary);
    font-size: 18px;
    line-height: 1;
    transition: background-color 120ms linear;
  }
  .step:hover:not(:disabled) {
    background: var(--surface-3);
    color: var(--text-primary);
  }
  .step:disabled {
    opacity: 0.35;
  }
  .n {
    width: 72px;
    text-align: center;
    font-size: 26px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }
  .n small {
    margin-left: 1px;
    font-size: 16px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .free {
    font-size: 14px;
    font-weight: 600;
    letter-spacing: 0;
    color: var(--text-secondary);
  }

  .compact {
    gap: 10px;
  }
  .compact .step {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    font-size: 16px;
  }
  .compact .n {
    width: 50px;
    font-size: 18px;
    letter-spacing: 0;
  }
  .compact .n small {
    margin-left: 0;
    font-size: 12px;
  }
  .compact .free {
    font-size: 12px;
  }

  @media (forced-colors: active) {
    .step {
      border: 1px solid ButtonText;
    }
  }
</style>
