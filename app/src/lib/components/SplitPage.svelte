<script lang="ts">
  // The Split page (PLAN.md §14.2, §14.3): the active mode's split in full,
  // to fine-tune it: the live strip, each slice with − % + and its settings
  // in place, Everything else's apps, Add a slice, Reset split. "Make a
  // copy" starts a mode of your own (§11.2); one of your own has Rename and
  // Delete instead of Reset, since there's no starting split to go back to.
  import { copyMode, deleteMode, renameMode, resetSplit } from "../actions";
  import { t } from "../i18n";
  import type { Ledger } from "../ledger";
  import { isOwnMode, modeName } from "../modes";
  import { service } from "../service.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import NameDialog from "./NameDialog.svelte";
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
  const active = $derived(service.activeProfile);
  const own = $derived(active !== null && isOwnMode(active.id));
  let renaming = $state(false);
  let deleting = $state(false);
  let busy = $state(false);

  async function rename(name: string) {
    if (!active) return;
    busy = true;
    if (await renameMode(active.id, name)) renaming = false;
    busy = false;
  }

  async function remove() {
    if (!active) return;
    busy = true;
    await deleteMode(active.id, mode);
    busy = false;
    deleting = false;
  }
</script>

<div class="head">
  <h1>{mode ? t("split.title", { mode }) : t("nav.split")}</h1>
  <button type="button" class="btn btn-quiet" disabled={!canEdit} onclick={() => void copyMode()}>
    <i class="icon" aria-hidden="true">&#xE8C8;</i>{t("mode.copy")}
  </button>
  {#if own}
    <button type="button" class="btn btn-quiet" disabled={!canEdit} onclick={() => (renaming = true)}>
      <i class="icon" aria-hidden="true">&#xE8AC;</i>{t("mode.rename")}
    </button>
    <button type="button" class="btn btn-quiet" disabled={!canEdit} onclick={() => (deleting = true)}>
      <i class="icon" aria-hidden="true">&#xE74D;</i>{t("mode.delete")}
    </button>
  {:else}
    <button type="button" class="btn btn-quiet" disabled={!canEdit} onclick={() => void resetSplit()}>
      <i class="icon" aria-hidden="true">&#xE777;</i>{t("split.reset")}
    </button>
  {/if}
</div>
<NameDialog open={renaming} name={active?.name ?? ""} {busy} onsave={(n) => void rename(n)} oncancel={() => (renaming = false)} />
<ConfirmDialog
  open={deleting}
  title={t("mode.delete.title", { mode })}
  body={t("mode.delete.body")}
  confirm={t("mode.delete")}
  {busy}
  onconfirm={() => void remove()}
  oncancel={() => (deleting = false)}
/>
{#if ledger && showStrip}
  <UsageStrip {ledger} {lineBps} />
{/if}
<SliceList {ledger} {loading} {canEdit} />

<style>
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
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
