<script lang="ts">
  // The Split page (PLAN.md §14.2, §14.3): the active mode's split in full,
  // to fine-tune it: the live strip, each slice with − % + and its settings
  // in place, Everything else's apps, Add a slice, Reset split.
  import { resetSplit } from "../actions";
  import { t } from "../i18n";
  import type { Ledger } from "../ledger";
  import { modeName } from "../modes";
  import { service } from "../service.svelte";
  import SliceList from "./SliceList.svelte";
  import UsageStrip from "./UsageStrip.svelte";

  let {
    ledger,
    lineBps,
    loading,
    canEdit,
    showStrip,
  }: { ledger: Ledger | null; lineBps: number | null; loading: boolean; canEdit: boolean; showStrip: boolean } =
    $props();

  const mode = $derived(modeName(service.activeProfile));
</script>

<div class="head">
  <h1>{mode ? t("split.title", { mode }) : t("nav.split")}</h1>
  <button type="button" class="btn btn-quiet" disabled={!canEdit} onclick={() => void resetSplit()}>
    <i class="icon" aria-hidden="true">&#xE777;</i>{t("split.reset")}
  </button>
</div>
{#if ledger && showStrip}
  <UsageStrip {ledger} {lineBps} />
{/if}
<SliceList {ledger} {loading} {canEdit} />

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 16px;
  }
  h1 {
    flex: 1;
    margin: 0;
    font-size: 26px;
    line-height: 32px;
    font-weight: 800;
    letter-spacing: -0.015em;
  }
  .icon {
    font-size: 14px;
  }
</style>
