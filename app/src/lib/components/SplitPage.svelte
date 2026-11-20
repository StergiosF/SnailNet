<script lang="ts">
  // The Split page (DESIGN.md, Split): one question, "who gets what".
  // Read top to bottom: the mode's split and its own buttons, one sentence
  // on what a slice is, the drag bar (the one picture of the split, live
  // use along its foot), then the slices with − % + and their settings
  // folded until opened. A mode with no slices gets an empty state that
  // says what a slice is and offers one to the apps using the line now.
  //
  // "Make a copy" starts a mode of your own (§11.2); one of your own has
  // Rename and Delete instead of Reset, since there's no starting split to
  // go back to. Changes wait in a draft until Save (SaveBar, ui.svelte.ts);
  // the mode's own buttons ask first when there are unsaved changes.
  import { copyMode, deleteMode, renameMode, resetSplit } from "../actions";
  import { t } from "../i18n";
  import type { Ledger } from "../ledger";
  import { isOwnMode, modeName } from "../modes";
  import { addRule, sameTarget, setEnabled } from "../rules";
  import { service } from "../service.svelte";
  import type { Target } from "../types";
  import { ui } from "../ui.svelte";
  import AppIcon from "./AppIcon.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Glyph from "./Glyph.svelte";
  import NameDialog from "./NameDialog.svelte";
  import SaveBar from "./SaveBar.svelte";
  import SliceList from "./SliceList.svelte";
  import SplitBar from "./SplitBar.svelte";

  let { ledger, loading, canEdit }: { ledger: Ledger | null; loading: boolean; canEdit: boolean } = $props();

  const mode = $derived(modeName(service.activeProfile));
  const active = $derived(service.activeProfile);
  const own = $derived(active !== null && isOwnMode(active.id));
  let renaming = $state(false);
  let deleting = $state(false);
  let busy = $state(false);

  /** Slices with a share of their own (in the draft, if editing). */
  const sliced = $derived(ui.rules.filter((r) => r.enabled && r.share > 0).length);
  const empty = $derived(ledger !== null && !loading && sliced === 0);
  /** Who could have a slice: the apps in Everything else now, busiest first. */
  const offers = $derived(
    (ledger?.slices.at(-1)?.members ?? []).filter((m) => !m.fixed && m.bps >= 50_000).slice(0, 3),
  );

  function give(target: Target) {
    const rules = ui.rules;
    const i = rules.findIndex((r) => sameTarget(r.target, target));
    ui.edit(i >= 0 ? setEnabled(rules, i, true) : addRule(rules, target));
    ui.open = `${target.kind}:${target.id.toLowerCase()}`;
  }

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
  <button type="button" class="btn btn-quiet" disabled={!canEdit} onclick={() => ui.guard(() => void copyMode())}>
    <Glyph name="copy" size={16} />{t("mode.copy")}
  </button>
  {#if own}
    <button type="button" class="btn btn-quiet" disabled={!canEdit} onclick={() => ui.guard(() => (renaming = true))}>
      {t("mode.rename")}
    </button>
    <button type="button" class="btn btn-quiet" disabled={!canEdit} onclick={() => ui.guard(() => (deleting = true))}>
      {t("mode.delete")}
    </button>
  {:else}
    <button type="button" class="btn btn-quiet" disabled={!canEdit} onclick={() => ui.guard(() => void resetSplit())}>
      <Glyph name="reset" size={16} />{t("split.reset")}
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

{#if empty}
  <section class="card empty" aria-labelledby="empty-h">
    <span class="em-ic" aria-hidden="true"><Glyph name="split" size={22} /></span>
    <div class="words">
      <h2 id="empty-h">{t("split.empty.title")}</h2>
      <p>{t("split.empty.body")}</p>
      {#if canEdit}
        <div class="pick">
          {#each offers as m (m.key)}
            <button type="button" class="btn" onclick={() => give(m.target)}>
              <AppIcon target={m.target} name={m.name} size={18} />{t("split.empty.give", { name: m.name })}
            </button>
          {/each}
          <button type="button" class="btn btn-quiet" onclick={() => (ui.adding = true)}>
            <Glyph name="plus" size={16} />{t("split.empty.other")}
          </button>
        </div>
      {/if}
    </div>
  </section>
  {#if ui.rules.length > 0}
    <SliceList {ledger} {loading} {canEdit} />
  {/if}
{:else}
  <p class="intro">{t("split.intro")}</p>
  {#if ledger && !loading}
    <SplitBar {ledger} {canEdit} />
  {/if}
  <SliceList {ledger} {loading} {canEdit} />
{/if}
<SaveBar />

<style>
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 6px;
  }
  h1 {
    flex: 1;
    margin: 0;
    font-size: 28px;
    line-height: 34px;
    font-weight: 800;
    letter-spacing: -0.02em;
  }
  .head .btn {
    gap: 8px;
  }
  .intro {
    max-width: 62ch;
    margin: 6px 0 0;
    font-size: 15px;
    line-height: 1.5;
    color: var(--text-secondary);
  }
  .empty {
    display: flex;
    align-items: flex-start;
    gap: 16px;
    margin-top: 24px;
    padding: 22px 24px 24px;
    animation: rise-in 480ms var(--ease-out) backwards;
  }
  .em-ic {
    display: grid;
    flex: none;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 12px;
    background: var(--shell-soft);
    color: var(--shell-bright);
  }
  .words {
    min-width: 0;
  }
  .empty h2 {
    margin: 0;
    font-size: 18px;
    line-height: 24px;
    font-weight: 800;
    letter-spacing: -0.01em;
  }
  .empty p {
    max-width: 60ch;
    margin: 6px 0 0;
    line-height: 1.5;
    color: var(--text-secondary);
  }
  .pick {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 18px;
  }
</style>
