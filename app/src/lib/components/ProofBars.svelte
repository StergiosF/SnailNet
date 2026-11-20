<script lang="ts">
  // The proof's two bars (feature 1, §11.10): ping with SnailNet next to
  // ping without it at the last speed test, on one scale (the longer number
  // fills the width). With in the shell's green, without in a muted stone.
  // The numbers carry the meaning, never the colour alone (§14.9). Bars
  // grow in once (motion.ts); later changes ease to their new length.
  // Decorative for screen readers: whoever shows it gives one sentence.
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
    <span class="track"
      ><i class={r.key} style:width="{Math.max(2, (r.ms / scale) * 100)}%" in:grow|global={{ delay: r.delay }}></i></span
    >
    <span class="label">{r.label}</span>
    <span class="ms num">{t("proof.ms", { ms: Math.round(r.ms) })}</span>
  {/each}
</div>

<style>
  .bars {
    display: grid;
    grid-template-columns: minmax(96px, 1fr) auto auto;
    align-items: center;
    column-gap: 14px;
    row-gap: 8px;
    max-width: 560px;
  }
  .track {
    display: block;
    height: 8px;
    border-radius: 4px;
    background: rgb(255 255 255 / 0.05);
    overflow: hidden;
  }
  .track i {
    display: block;
    height: 100%;
    border-radius: 4px;
    transform-origin: left center;
    transition: width 320ms var(--ease-out);
  }
  .with {
    background: var(--shell);
  }
  .without {
    background: var(--stone-3);
  }
  .label {
    font-size: 13px;
    line-height: 18px;
    color: var(--text-secondary);
    white-space: nowrap;
  }
  .ms {
    justify-self: end;
    font-size: 15px;
    line-height: 20px;
    font-weight: 700;
    color: var(--text-primary);
    white-space: nowrap;
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
