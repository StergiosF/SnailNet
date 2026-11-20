<script lang="ts">
  // The proof's two bars (feature 1, §11.10): ping with SnailNet over ping
  // without it at the last speed test, on one scale (the longer number
  // fills the width). Each bar has its words and number above it; with
  // in jade, without in coral. The numbers carry the meaning, never the
  // colour alone (§14.9). Bars grow in once (motion.ts); later changes
  // ease to their new length. Decorative for screen readers: whoever
  // shows it gives one sentence.
  import { shortDate } from "../format";
  import { t } from "../i18n";
  import { grow } from "../motion";

  let { withMs, withoutMs, at }: { withMs: number; withoutMs: number; at: number } = $props();

  const scale = $derived(Math.max(withMs, withoutMs, 1));
  const rows = $derived([
    { key: "with", label: t("proof.with"), ms: withMs, delay: 0 },
    { key: "without", label: t("proof.without", { date: shortDate(at) }), ms: withoutMs, delay: 90 },
  ]);
</script>

<div class="bars" aria-hidden="true">
  {#each rows as r (r.key)}
    <div class="row">
      <span class="label">{r.label}</span>
      <span class="ms num">{t("proof.ms", { ms: Math.round(r.ms) })}</span>
      <span class="track"
        ><i class={r.key} style:width="{Math.max(2, (r.ms / scale) * 100)}%" in:grow|global={{ delay: r.delay }}></i></span
      >
    </div>
  {/each}
</div>

<style>
  .bars {
    display: grid;
    gap: 10px;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: baseline;
    row-gap: 5px;
    column-gap: 10px;
  }
  .label {
    overflow: hidden;
    font-size: 13px;
    line-height: 18px;
    color: var(--text-secondary);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ms {
    font-size: 13.5px;
    line-height: 18px;
    font-weight: 700;
    color: var(--text-primary);
    white-space: nowrap;
  }
  .track {
    grid-column: 1 / -1;
    display: block;
    height: 6px;
    border-radius: 3px;
    background: rgb(255 255 255 / 0.05);
    overflow: hidden;
  }
  .track i {
    display: block;
    height: 100%;
    border-radius: 3px;
    transform-origin: left center;
    transition: width 320ms var(--ease-out);
  }
  .with {
    background: var(--shell);
  }
  .without {
    background: var(--critical);
  }
  @media (forced-colors: active) {
    .track {
      border: 1px solid CanvasText;
    }
    .track i {
      forced-color-adjust: none;
    }
    .with {
      background: Highlight;
    }
    .without {
      background: CanvasText;
    }
  }
</style>
