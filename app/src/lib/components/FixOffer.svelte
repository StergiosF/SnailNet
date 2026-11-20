<script lang="ts">
  // The one-tap fix on the sentence (PLAN.md §11.11): under "Steam is
  // using the most of your line. Brave wants more room too.", the cure in
  // one button and a way to say not now. After a click, what it did and
  // Undo, in the same place. With unsaved changes on the Split page, no
  // button (it would save over them): a line saying so instead. `compact`
  // is the flyout's size.
  import { fixer } from "../fixer.svelte";
  import { t } from "../i18n";

  let { canEdit, compact = false }: { canEdit: boolean; compact?: boolean } = $props();

  const o = $derived(fixer.offer);
  const words = $derived(
    o === null
      ? null
      : o.pair.kind === "give"
        ? { text: t("fix.give", { name: o.pair.name, pct: o.pct }), label: t("fix.give.label", { name: o.pair.name, pct: o.pct }) }
        : { text: t("fix.grow", { name: o.pair.name, pct: o.pct }), label: t("fix.grow.label", { name: o.pair.name, pct: o.pct }) },
  );
</script>

<div class="fix" class:compact role="status">
  {#if fixer.done}
    <p class="done">
      <span>{fixer.done.message}</span>
      {#if fixer.done.undo}
        <button type="button" class="btn btn-quiet" disabled={fixer.busy} onclick={() => void fixer.undo()}>{t("fix.undo")}</button>
      {/if}
    </p>
  {:else if o && words && canEdit}
    {#if fixer.blockedByDraft}
      <p class="note">{t("fix.draft")}</p>
    {:else}
      <div class="acts">
        <button
          type="button"
          class="btn btn-accent cure"
          disabled={fixer.busy}
          aria-label={words.label}
          onclick={() => void fixer.apply()}>{words.text}</button
        >
        <button type="button" class="btn btn-quiet" aria-label={t("fix.later.label")} onclick={() => fixer.later()}
          >{t("fix.later")}</button
        >
      </div>
    {/if}
  {/if}
</div>

<style>
  .fix:empty {
    display: none;
  }
  .fix {
    margin-top: 6px;
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    animation: fix-in 260ms var(--ease-out);
  }
  .cure {
    font-weight: 700;
  }
  .done,
  .note {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 10px;
    margin: 0;
    font-size: 15px;
    line-height: 1.45;
    color: var(--text-primary);
  }
  .note {
    color: var(--text-secondary);
  }
  .compact .done,
  .compact .note {
    font-size: 13px;
  }
  .compact .acts {
    gap: 6px;
  }
  @keyframes fix-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
  }
</style>
