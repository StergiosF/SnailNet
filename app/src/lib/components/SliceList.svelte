<script lang="ts">
  // The slices (PLAN.md §14.2): the active profile's rules in order, then
  // Everything else with its apps, then "Add a slice". While the service
  // is still answering, three quiet placeholder rows.
  import { t } from "../i18n";
  import type { Ledger } from "../ledger";
  import { ui } from "../ui.svelte";
  import SliceRow from "./SliceRow.svelte";
  import Snail from "./Snail.svelte";

  let { ledger, loading, canEdit }: { ledger: Ledger | null; loading: boolean; canEdit: boolean } = $props();

  const other = $derived(ledger?.slices.at(-1) ?? null);
  const empty = $derived(
    ledger !== null && ledger.slices.length === 1 && (other?.members.length ?? 0) === 0 && ledger.total < 50_000,
  );
</script>

<section class="list" aria-label={t("split.label")} aria-busy={ui.saving}>
  {#if loading || !ledger}
    <ul aria-hidden="true">
      {#each [0.36, 0.28, 0.44] as w, i (i)}
        <li class="skeleton"><span class="ic"></span><span class="bar" style:width="{w * 100}%"></span></li>
      {/each}
    </ul>
  {:else}
    {#if empty}
      <div class="empty">
        <Snail size={40} />
        <p><strong>{t("list.empty.title")}</strong>{t("list.empty.body")}</p>
      </div>
    {/if}
    <ul>
      {#each ledger.slices as s (s.key)}
        <SliceRow slice={s} shaped={ledger.shaped} {canEdit} />
      {/each}
    </ul>
    {#if canEdit}
      <button type="button" class="add" onclick={() => (ui.adding = true)}>
        <span class="plus" aria-hidden="true">+</span>{t("list.add.long")}
      </button>
    {/if}
  {/if}
  {#if ui.error}
    <p class="error" role="alert">{ui.error}</p>
  {/if}
</section>

<style>
  .list {
    margin: 24px calc(var(--gutter) - 12px) 0;
    padding-bottom: 24px;
  }
  ul {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .skeleton {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 64px;
    padding: 0 12px;
  }
  .skeleton .ic {
    width: 40px;
    height: 40px;
    border-radius: var(--radius-control);
    background: var(--skeleton);
  }
  .skeleton .bar {
    height: 14px;
    border-radius: 4px;
    background: var(--skeleton);
  }
  .add {
    display: flex;
    align-items: center;
    gap: 14px;
    width: 100%;
    height: 56px;
    margin-top: 4px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--radius-card);
    background: transparent;
    color: var(--text-secondary);
    text-align: left;
    transition: background-color 120ms linear;
  }
  .add:hover {
    background: var(--surface-1);
    color: var(--text-primary);
  }
  .plus {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-control);
    box-shadow: inset 0 0 0 1.5px var(--surface-3);
    font-size: 20px;
  }
  .empty {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 8px 12px 16px;
  }
  .empty p {
    margin: 0;
    color: var(--text-secondary);
  }
  .empty strong {
    display: block;
    color: var(--text-primary);
    font-weight: 600;
  }
  .error {
    margin: 12px 12px 0;
  }
  @media (forced-colors: active) {
    .plus {
      border: 1.5px solid ButtonText;
    }
  }
</style>
