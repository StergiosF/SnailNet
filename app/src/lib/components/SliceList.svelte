<script lang="ts">
  // The slices (PLAN.md §14.2): the active profile's rules in order, then
  // Everything else with its apps, then "Add a slice". Rules that take only
  // what's free and are quiet right now fold into one row that opens (the
  // modes have many: every launcher and updater). While the service is
  // still answering, three quiet placeholder rows. (No column headings:
  // the page's sentence says what a percentage means.)
  import { t } from "../i18n";
  import type { Ledger, Slice } from "../ledger";
  import { ui } from "../ui.svelte";
  import Glyph from "./Glyph.svelte";
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
    <ul class="card rows">
      {#each shown.slice(0, -1) as s (s.key)}
        <SliceRow slice={s} {canEdit} />
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
            <span class="chev" class:open={unfold} aria-hidden="true"><Glyph name="down" size={18} /></span>
          </button>
        </li>
        {#if unfold}
          {#each rest as s (s.key)}
            <SliceRow slice={s} {canEdit} />
          {/each}
        {/if}
      {/if}
      {#each shown.slice(-1) as s (s.key)}
        <SliceRow slice={s} {canEdit} />
      {/each}
    </ul>
    {#if canEdit}
      <button type="button" class="add" onclick={() => (ui.adding = true)}>
        <span class="plus" aria-hidden="true"><Glyph name="plus" size={18} /></span>{t("list.add.long")}
      </button>
    {/if}
  {/if}
  {#if ui.error}
    <p class="error" role="alert">{ui.error}</p>
  {/if}
</section>

<style>
  .list {
    margin-top: 24px;
    padding-bottom: 8px;
  }
  /* The shares in one card, a hairline between rows. */
  ul {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .rows {
    overflow: hidden;
  }
  .rows > :global(li + li) {
    border-top: 1px solid rgb(0 0 0 / 0.2);
  }
  .fold button {
    display: flex;
    align-items: center;
    gap: 14px;
    width: 100%;
    min-height: 64px;
    padding: 12px 16px;
    border: 0;
    background: transparent;
    text-align: left;
    transition: background-color 120ms linear;
  }
  .fold button:hover {
    background: var(--hover-soft);
  }
  .stack {
    display: grid;
    place-items: center;
    grid-template-columns: repeat(3, 6px);
    gap: 4px;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-control);
    background: rgb(0 0 0 / 0.16);
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-tertiary);
  }
  .words {
    display: flex;
    flex-direction: column;
    flex: 1;
    gap: 2px;
    min-width: 0;
  }
  .words b {
    font-size: 15px;
    font-weight: 600;
  }
  .words small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13.5px;
    color: var(--text-secondary);
  }
  .chev {
    display: flex;
    color: var(--text-tertiary);
    transition: transform 200ms var(--ease-out);
  }
  .chev.open {
    transform: rotate(180deg);
  }
  .skeleton {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 68px;
    padding: 0 16px;
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
    height: 60px;
    margin-top: 10px;
    padding: 0 16px;
    border: 0;
    border-radius: var(--radius-card);
    background: transparent;
    color: var(--text-secondary);
    font-weight: 600;
    text-align: left;
    box-shadow: inset 0 0 0 1.5px rgb(255 255 255 / 0.07);
    transition:
      background-color 120ms linear,
      color 120ms linear;
  }
  .add:hover {
    background: var(--hover-soft);
    color: var(--text-primary);
  }
  .plus {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-control);
    background: rgb(0 0 0 / 0.16);
    color: var(--text-secondary);
  }
  .add:hover .plus {
    background: var(--accent);
    color: var(--on-accent);
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
    .add {
      border: 1.5px solid ButtonText;
    }
  }
</style>
