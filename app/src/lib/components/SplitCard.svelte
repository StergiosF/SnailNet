<script lang="ts">
  // The split in short (PLAN.md §14.2), titled with the mode it belongs
  // to: one bar of the mode's shares in their inks, a line per slice (its
  // share, what it moves now and the least it gets when the line is busy),
  // how many take only what's free, and the ways on: Change the split (the
  // Split page) and Reset split. Until put away, a one-time
  // hint says how shares change and that unused share is never wasted
  // (§14.5 step 4; − / + replaced dragging, §14.3).
  import { resetSplit } from "../actions";
  import { mbps, percent } from "../format";
  import { t } from "../i18n";
  import type { Ledger, Slice } from "../ledger";
  import { modeName, summary } from "../modes";
  import { service } from "../service.svelte";
  import { ui } from "../ui.svelte";

  let { ledger, canEdit }: { ledger: Ledger | null; canEdit: boolean } = $props();

  const s = $derived(ledger ? summary(ledger) : null);
  const mode = $derived(modeName(service.activeProfile));

  function fineTune() {
    ui.dismissCoach();
    ui.go("split");
  }
  const ink = (x: Slice) => (x.ink >= 0 ? `var(--ink-${x.ink})` : "var(--stone-0)");
  const use = (x: Slice) =>
    x.kept === null
      ? t("card.row.now", { used: mbps(x.bps) })
      : `${t("card.row.now", { used: mbps(x.bps) })} · ${t("card.row.kept", { kept: mbps(x.kept) })}`;
</script>

<section class="card split" aria-labelledby="split-title">
  <h2 class="card-title" id="split-title">{mode ? t("card.split", { mode }) : t("nav.split")}</h2>
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
          <span class="nm">
            <span class="name">{x.name}</span>
            <small class="num">{use(x)}</small>
          </span>
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
  {#if ui.coach && s && canEdit}
    <div class="coach" role="note" aria-labelledby="coach-title">
      <div class="words">
        <b id="coach-title">{t("coach.title")}</b>
        <p>{t("coach.body")}</p>
      </div>
      <button type="button" class="btn btn-quiet" onclick={() => ui.dismissCoach()}>{t("coach.ok")}</button>
    </div>
  {/if}
  <div class="actions">
    <button type="button" class="btn" onclick={fineTune}>
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
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.25);
    transition: width 700ms var(--ease-out);
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
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
    font-size: 14px;
  }
  .sw {
    align-self: start;
    width: 10px;
    height: 10px;
    margin-top: 5px;
    border-radius: 3px;
  }
  .nm {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .nm .name,
  .nm small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .nm .name {
    font-weight: 600;
  }
  .nm small {
    font-size: 13px;
    color: var(--text-secondary);
  }
  b {
    min-width: 44px;
    text-align: right;
    font-size: 18px;
    font-weight: 700;
  }
  .free {
    margin: 0;
    font-size: 13px;
    color: var(--text-secondary);
  }
  .coach {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 12px 12px 14px;
    border-radius: 12px;
    background: var(--shell-soft);
    animation: rise-in 420ms var(--ease-out) 600ms backwards;
  }
  .coach .words {
    flex: 1;
    min-width: 0;
  }
  .coach p {
    margin: 0;
    font-size: 13px;
    line-height: 1.45;
    color: var(--text-secondary);
  }
  .coach b {
    display: block;
    margin-bottom: 2px;
    font-size: 14px;
    color: var(--text-primary);
  }
  .coach .btn {
    flex: none;
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
    .coach {
      border: 1px solid CanvasText;
    }
    .bar i,
    .sw {
      forced-color-adjust: none;
      border: 1px solid CanvasText;
    }
  }
</style>
