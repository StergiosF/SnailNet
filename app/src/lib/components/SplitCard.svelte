<script lang="ts">
  // The split in short (PLAN.md §14.2): one bar of the mode's shares in
  // their inks, a line per slice (what it moves against what its share
  // keeps for it), how many take only what's free, and the ways on:
  // Fine-tune (the Split page) and Reset split.
  import { resetSplit } from "../actions";
  import { mbps, percent } from "../format";
  import { t } from "../i18n";
  import type { Ledger, Slice } from "../ledger";
  import { summary } from "../modes";
  import { ui } from "../ui.svelte";

  let { ledger, canEdit }: { ledger: Ledger | null; canEdit: boolean } = $props();

  const s = $derived(ledger ? summary(ledger) : null);
  const ink = (x: Slice) => (x.ink >= 0 ? `var(--ink-${x.ink})` : "var(--stone-0)");
  const use = (x: Slice) =>
    x.kept === null ? `${mbps(x.bps)} ${t("line.unit")}` : `${mbps(x.bps)} / ${mbps(x.kept)} ${t("line.unit")}`;
</script>

<section class="card split" aria-labelledby="split-title">
  <h2 class="eyebrow" id="split-title">{t("card.split")}</h2>
  {#if s}
    <div class="bar" aria-hidden="true">
      {#each s.slices as x (x.key)}
        <i style:width="{(x.share ?? 0) / 100}%" style:background={ink(x)}>
          {#if (x.share ?? 0) >= 1200}{percent(x.share ?? 0)}{/if}
        </i>
      {/each}
    </div>
    <ul>
      {#each s.slices as x (x.key)}
        <li>
          <i class="sw" style:background={ink(x)} aria-hidden="true"></i>
          <span class="nm">{x.name}</span>
          <span class="use num">{use(x)}</span>
          <b class="num">{percent(x.share ?? 0)}</b>
        </li>
      {/each}
    </ul>
    {#if s.free > 0}
      <p class="free">{s.free === 1 ? t("card.free.one") : t("card.free.many", { count: s.free })}</p>
    {/if}
  {:else}
    <div class="bar skeleton" aria-hidden="true"></div>
  {/if}
  <div class="actions">
    <button type="button" class="btn" onclick={() => (ui.page = "split")}>
      {t("card.finetune")}<i class="icon" aria-hidden="true">&#xE72A;</i>
    </button>
    <button type="button" class="btn btn-quiet" disabled={!canEdit} onclick={() => void resetSplit()}>
      {t("split.reset")}
    </button>
  </div>
</section>

<style>
  .split {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 18px;
    min-width: 0;
  }
  .bar {
    display: flex;
    gap: 2px;
    height: 28px;
    border-radius: 8px;
    overflow: hidden;
  }
  .bar.skeleton {
    background: var(--skeleton);
  }
  .bar i {
    display: flex;
    align-items: center;
    padding-left: 8px;
    font-style: normal;
    font-size: 12px;
    font-weight: 800;
    color: #0f2420;
    overflow: hidden;
    white-space: nowrap;
    transition: width 400ms var(--ease-out);
  }
  ul {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto 40px;
    align-items: center;
    gap: 10px;
    font-size: 14px;
  }
  .sw {
    width: 10px;
    height: 10px;
    border-radius: 3px;
  }
  .nm {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .use {
    font-size: 13px;
    color: var(--text-secondary);
  }
  b {
    text-align: right;
    font-weight: 700;
  }
  .free {
    margin: 0;
    font-size: 13px;
    color: var(--text-secondary);
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: auto;
    padding-top: 4px;
  }
  .actions .icon {
    font-size: 11px;
  }
  @media (forced-colors: active) {
    .bar i,
    .sw {
      forced-color-adjust: none;
      border: 1px solid CanvasText;
    }
  }
</style>
