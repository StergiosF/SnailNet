<script lang="ts">
  // The live app list (PLAN.md §14.2): one row per app family or website
  // with traffic now, busiest first, then the rules whose target is quiet.
  // Each row shows its speeds and its slice chip ("40% · Low",
  // "Everything else") and opens the rule editor; "Add a slice" opens the
  // picker for anything not listed.
  import { editor } from "../editor.svelte";
  import { mbps, percent, targetLabel } from "../format";
  import { t } from "../i18n";
  import { listRows, sameTarget, shownShare, type Row } from "../rules";
  import { service } from "../service.svelte";
  import Snail from "./Snail.svelte";

  let { loading }: { loading: boolean } = $props();

  const rules = $derived(service.activeProfile?.rules ?? []);
  const names = $derived({
    ...Object.fromEntries((service.snapshot?.sites ?? []).map((s) => [s.id, s.name])),
    ...service.siteNames,
  });
  const rows = $derived(
    listRows(service.stats?.down, service.stats?.up, rules, (target) =>
      targetLabel(target, service.snapshot?.apps ?? [], names),
    ),
  );
  const canEdit = $derived(service.link.kind === "connected" && service.snapshot !== null);
  const selected = $derived(editor.panel?.mode === "edit" ? editor.panel.target : null);

  function chip(row: Row): string {
    const rule = rules[row.rule];
    if (!rule) return t("list.chip.everything");
    if (!rule.enabled) return t("list.chip.off");
    const share = shownShare(rules, row.rule);
    return t("list.chip", {
      share: share === null ? t("list.chip.free") : percent(share),
      priority: t(`priority.${rule.priority}`),
    });
  }
</script>

<section aria-labelledby="active-heading">
  <div class="heading">
    <h2 id="active-heading">{t("list.active")}</h2>
    {#if canEdit}
      <button type="button" class="add" onclick={() => editor.open({ mode: "pick" })}>
        <i class="icon" aria-hidden="true">&#xE710;</i>{t("list.add")}
      </button>
    {/if}
  </div>

  {#if loading}
    <ul class="card" aria-hidden="true">
      {#each [0.42, 0.3, 0.36] as w, i (i)}
        <li class="row skeleton">
          <span class="bar" style:width="{w * 100}%"></span>
        </li>
      {/each}
    </ul>
  {:else if rows.length === 0}
    <div class="card empty">
      <Snail size={40} />
      <p><strong>{t("list.empty.title")}</strong>{t("list.empty.body")}</p>
    </div>
  {:else}
    <ul class="card">
      {#each rows as r (r.key)}
        {@const ruled = r.rule >= 0}
        <li>
          {#if r.fixed || !canEdit}
            <div class="row" title={r.fixed ? t("list.unknown") : undefined}>
              {@render content(r, ruled)}
            </div>
          {:else}
            <button
              type="button"
              class="row"
              class:selected={selected !== null && sameTarget(selected, r.target)}
              class:idle={r.idle}
              aria-expanded={selected !== null && sameTarget(selected, r.target)}
              onclick={() => editor.open({ mode: "edit", target: r.target })}
            >
              {@render content(r, ruled)}
            </button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

{#snippet content(r: Row, ruled: boolean)}
  <span class="name">
    {#if r.target.kind === "site"}<i class="icon kind" aria-hidden="true">&#xE774;</i>{/if}
    <span class="label">{r.name}</span>
  </span>
  <span class="speed num" aria-label="{t('list.down')} {mbps(r.down)} Mbps">
    <i class="icon" aria-hidden="true">&#xE896;</i>{r.idle ? "–" : mbps(r.down)}
  </span>
  <span class="speed num up" aria-label="{t('list.up')} {mbps(r.up)} Mbps">
    <i class="icon" aria-hidden="true">&#xE898;</i>{r.idle ? "–" : mbps(r.up)}
  </span>
  <span class="chip num" class:ruled>{r.fixed ? "" : chip(r)}</span>
  <i class="icon chevron" aria-hidden="true">{r.fixed || !canEdit ? "" : ""}</i>
{/snippet}

<style>
  section {
    padding: 0 var(--gutter) var(--gutter);
  }
  .heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 32px;
    margin-bottom: 8px;
  }
  h2 {
    margin: 0;
    font-size: 14px;
    line-height: 20px;
    font-weight: 600;
  }
  .add {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    padding: 0 12px;
    border: 1px solid var(--stroke-card);
    border-radius: var(--radius-control);
    background: var(--surface-card);
    color: var(--text-primary);
    font: inherit;
  }
  .add:hover {
    background: var(--fill-subtle-hover);
  }
  .add .icon {
    font-size: 12px;
  }
  .card {
    margin: 0;
    padding: 0;
    list-style: none;
    border: 1px solid var(--stroke-card);
    border-radius: var(--radius-card);
    background: var(--surface-card);
  }
  li + li {
    border-top: 1px solid var(--stroke-divider);
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 80px 80px 128px 16px;
    align-items: center;
    gap: 16px;
    width: 100%;
    min-height: 48px;
    padding: 0 12px 0 16px;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
  }
  .row.skeleton {
    display: flex;
  }
  li:first-child .row {
    border-top-left-radius: var(--radius-card);
    border-top-right-radius: var(--radius-card);
  }
  li:last-child .row {
    border-bottom-left-radius: var(--radius-card);
    border-bottom-right-radius: var(--radius-card);
  }
  button.row:hover {
    background: var(--fill-subtle-hover);
  }
  button.row:active {
    background: var(--fill-subtle-pressed);
  }
  button.row:focus-visible {
    outline-offset: -2px;
  }
  .row.selected {
    background: var(--accent-tint);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .row.idle .label,
  .row.idle .speed {
    color: var(--text-tertiary);
  }
  .name {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .kind {
    font-size: 14px;
    color: var(--text-secondary);
  }
  .speed {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 6px;
  }
  .speed .icon {
    font-size: 12px;
    color: var(--text-secondary);
  }
  .up {
    color: var(--text-secondary);
  }
  .chip {
    justify-self: end;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    line-height: 16px;
    color: var(--text-secondary);
  }
  .chip.ruled {
    color: var(--accent-text);
    font-weight: 600;
  }
  .chevron {
    font-size: 12px;
    color: var(--text-secondary);
  }
  .bar {
    height: 12px;
    border-radius: 2px;
    background: var(--skeleton);
  }
  .empty {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 20px;
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
  @media (max-width: 760px) {
    .row {
      grid-template-columns: minmax(0, 1fr) 68px 68px 104px 16px;
      gap: 12px;
    }
  }
</style>
