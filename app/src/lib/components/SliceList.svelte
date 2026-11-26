<script lang="ts">
  // The shares (PLAN.md §14.2, DESIGN.md Split): one flat panel, hairlines
  // between rows. The active mode's rules with a share, in order; then the
  // rules that only use spare speed, folded under one row that says how
  // many (Quiet downloads, every launcher and updater); Everything else
  // with its apps; and "Add an app or website" as a plain row at the foot.
  // While the service is still answering, three quiet placeholder rows.
  // (No column headings: the page's sentence says what a percentage means.)
  import { t } from "../i18n";
  import type { Ledger, Slice } from "../ledger";
  import { ui } from "../ui.svelte";
  import Glyph from "./Glyph.svelte";
  import SliceRow from "./SliceRow.svelte";
  import Snail from "./Snail.svelte";

  let { ledger, loading, canEdit }: { ledger: Ledger | null; loading: boolean; canEdit: boolean } = $props();

  /** A rule that only uses spare speed (share 0), and is on. */
  const spare = (s: Slice) => s.rule >= 0 && s.enabled && s.share === null;
  const other = $derived(ledger?.slices.at(-1) ?? null);
  const rules = $derived(ledger?.slices.filter((s) => s.rule >= 0 && !spare(s)) ?? []);
  const spares = $derived(ledger?.slices.filter(spare) ?? []);
  // Folded until asked, unless one of them is moving now or has its
  // settings open: then it shows, so the page never hides what's busy.
  let unfoldAsked = $state<boolean | null>(null);
  const unfold = $derived(unfoldAsked ?? spares.some((s) => s.active || ui.open === s.key));
  const empty = $derived(
    ledger !== null && ledger.slices.length === 1 && (other?.members.length ?? 0) === 0 && ledger.total < 50_000,
  );
</script>

<section class="list" aria-label={t("split.label")} aria-busy={ui.saving}>
  {#if loading || !ledger}
    <ul class="card rows" aria-hidden="true">
      {#each [0.36, 0.28, 0.44] as w, i (i)}
        <li class="skeleton"><span class="ic"></span><span class="bar" style:width="{w * 100}%"></span></li>
      {/each}
    </ul>
  {:else}
    {#if empty}
      <div class="empty">
        <Snail size={40} light />
        <p><strong>{t("list.empty.title")}</strong>{t("list.empty.body")}</p>
      </div>
    {/if}
    <ul class="card rows">
      {#each rules as s (s.key)}
        <SliceRow slice={s} {canEdit} />
      {/each}
      {#if other}
        <SliceRow slice={other} {canEdit} />
      {/if}
      {#if spares.length > 0}
        <li class="fold">
          <button type="button" aria-expanded={unfold} onclick={() => (unfoldAsked = !unfold)}>
            <span class="chev" class:open={unfold} aria-hidden="true"><Glyph name="chevron" size={16} /></span>
            <span class="words">
              <b>{t(spares.length === 1 ? "fold.title.one" : "fold.title", { count: spares.length })}</b>
              {#if !unfold}
                <small>{spares.map((s) => s.name).slice(0, 4).join(", ")}{spares.length > 4 ? "…" : ""}</small>
              {/if}
            </span>
          </button>
        </li>
        {#if unfold}
          {#each spares as s (s.key)}
            <SliceRow slice={s} {canEdit} />
          {/each}
        {/if}
      {/if}
      {#if canEdit}
        <li class="addrow">
          <button type="button" class="add" onclick={() => (ui.adding = true)}>
            <span class="plus" aria-hidden="true"><Glyph name="plus" size={18} /></span>{t("list.add.long")}
          </button>
        </li>
      {/if}
    </ul>
  {/if}
</section>

<style>
  .list {
    margin-top: 22px;
    padding-bottom: 8px;
  }
  ul {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 6px;
    list-style: none;
  }
  /* Hairlines between rows, inset past the icon; gone around a hovered row. */
  .rows > :global(li) {
    position: relative;
  }
  .rows > :global(li + li)::before {
    content: "";
    position: absolute;
    top: 0;
    left: 60px;
    right: 12px;
    height: 1px;
    background: var(--stroke-card);
  }
  .rows > :global(li.fold)::before,
  .rows > :global(li.addrow)::before {
    left: 12px;
  }
  .fold button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 48px;
    padding: 6px 14px;
    border: 0;
    border-radius: 12px;
    background: transparent;
    color: var(--text-secondary);
    text-align: left;
    transition:
      background-color 120ms linear,
      color 120ms linear;
  }
  .fold button:hover {
    color: var(--text-primary);
  }
  .fold button:focus-visible,
  .add:focus-visible {
    outline-offset: -2px;
  }
  .words {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 2px 10px;
    flex: 1;
    min-width: 0;
  }
  .words b {
    font-size: 14px;
    font-weight: 650;
  }
  .words small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    color: var(--text-tertiary);
  }
  .chev {
    display: flex;
    transition: transform 240ms var(--ease-out);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .skeleton {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 62px;
    padding: 0 14px;
  }
  .skeleton .ic {
    width: 32px;
    height: 32px;
    border-radius: 9px;
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
    min-height: 52px;
    padding: 8px 14px;
    border: 0;
    border-radius: 12px;
    background: transparent;
    color: var(--text-secondary);
    font-weight: 700;
    text-align: left;
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
    flex: none;
    place-items: center;
    width: 32px;
    height: 32px;
    color: var(--shell-bright);
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
  @media (forced-colors: active) {
    .rows > :global(li + li)::before {
      background: CanvasText;
    }
  }
</style>
