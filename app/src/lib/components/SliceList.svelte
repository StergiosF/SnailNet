<script lang="ts">
  // The slices (PLAN.md §14.2): the active profile's rules in order, then
  // Everything else with its apps, then "Add a slice". Rules that take only
  // what's free and are quiet right now fold into one row that opens (the
  // modes have many: every launcher and updater). While the service is
  // still answering, three quiet placeholder rows. Over the rows, what
  // the columns are, so the percentage says what it means.
  import { t } from "../i18n";
  import type { Ledger, Slice } from "../ledger";
  import { ui } from "../ui.svelte";
  import SliceRow from "./SliceRow.svelte";
  import Snail from "./Snail.svelte";

  let { ledger, loading, canEdit }: { ledger: Ledger | null; loading: boolean; canEdit: boolean } = $props();

  const other = $derived(ledger?.slices.at(-1) ?? null);
  /** Quiet "only when free" rules (unless one's settings are open). */
  const folded = (s: Slice) =>
    s.rule >= 0 && s.enabled && s.share === null && !s.active && ui.open !== s.key;
  const shown = $derived(ledger?.slices.filter((s) => !folded(s)) ?? []);
  const rest = $derived(ledger?.slices.filter(folded) ?? []);
  let unfold = $state(false);
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
    <div class="heads" aria-hidden="true">
      <span>{t("split.list.who")}</span>
      <span class="r">{t("split.list.share")}</span>
    </div>
    <ul>
      {#each shown.slice(0, -1) as s (s.key)}
        <SliceRow slice={s} shaped={ledger.shaped} {canEdit} />
      {/each}
      {#if rest.length > 0}
        <li class="fold">
          <button type="button" aria-expanded={unfold} onclick={() => (unfold = !unfold)}>
            <span class="stack" aria-hidden="true">
              {#each rest.slice(0, 3) as s (s.key)}<span class="dot"></span>{/each}
            </span>
            <span class="words">
              <b>{t(rest.length === 1 ? "fold.title.one" : "fold.title", { count: rest.length })}</b>
              <small>{rest.map((s) => s.name).slice(0, 4).join(", ")}{rest.length > 4 ? "…" : ""}</small>
            </span>
            <i class="icon chev" class:open={unfold} aria-hidden="true">&#xE70D;</i>
          </button>
        </li>
        {#if unfold}
          {#each rest as s (s.key)}
            <SliceRow slice={s} shaped={ledger.shaped} {canEdit} />
          {/each}
        {/if}
      {/if}
      {#each shown.slice(-1) as s (s.key)}
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
    margin: 20px -12px 0;
    padding-bottom: 8px;
  }
  /* What the columns are: who, and what the percentage means. */
  .heads {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    padding: 0 12px 8px;
    margin-bottom: 4px;
    border-bottom: 1px solid var(--stroke-card);
    font-size: 13px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .heads .r {
    text-align: right;
  }
  ul {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .fold button {
    display: flex;
    align-items: center;
    gap: 14px;
    width: 100%;
    padding: 12px;
    border: 0;
    border-radius: var(--radius-card);
    background: transparent;
    text-align: left;
    transition: background-color 120ms linear;
  }
  .fold button:hover {
    background: var(--surface-1);
  }
  .stack {
    display: grid;
    place-items: center;
    grid-template-columns: repeat(3, 8px);
    gap: 4px;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-control);
    background: var(--surface-1);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-tertiary);
  }
  .words {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .words b {
    font-size: 16px;
    font-weight: 600;
  }
  .words small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 14px;
    color: var(--text-secondary);
  }
  .chev {
    font-size: 12px;
    color: var(--text-secondary);
    transition: transform 200ms var(--ease-out);
  }
  .chev.open {
    transform: rotate(180deg);
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
