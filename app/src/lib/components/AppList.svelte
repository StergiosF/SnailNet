<script lang="ts">
  // The live app list (PLAN.md §14.2, first cut): one row per app family
  // with its download and upload speed, busiest first. Rule chips, the
  // Online toggle and the rule editor come with the split bar.
  import { appRates, mbps } from "../format";
  import { t } from "../i18n";
  import { service } from "../service.svelte";
  import Snail from "./Snail.svelte";

  let { loading }: { loading: boolean } = $props();
  const rows = $derived(appRates(service.stats?.down, service.stats?.up));
</script>

<section aria-labelledby="active-heading">
  <h2 id="active-heading">{t("list.active")}</h2>

  {#if loading}
    <ul class="card" aria-hidden="true">
      {#each [0.42, 0.3, 0.36] as w, i (i)}
        <li class="row">
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
      {#each rows as r (r.family)}
        <li class="row">
          <span class="name">{r.name}</span>
          <span class="speed num" aria-label="{t('list.down')} {mbps(r.down)} Mbps">
            <i class="icon" aria-hidden="true">&#xE896;</i>{mbps(r.down)}
          </span>
          <span class="speed num up" aria-label="{t('list.up')} {mbps(r.up)} Mbps">
            <i class="icon" aria-hidden="true">&#xE898;</i>{mbps(r.up)}
          </span>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  section {
    padding: 0 var(--gutter) var(--gutter);
  }
  h2 {
    margin: 0 0 8px;
    font-size: 14px;
    line-height: 20px;
    font-weight: 600;
  }
  .card {
    margin: 0;
    padding: 0;
    list-style: none;
    border: 1px solid var(--stroke-card);
    border-radius: var(--radius-card);
    background: var(--surface-card);
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 88px 88px;
    align-items: center;
    gap: 16px;
    min-height: 48px;
    padding: 0 16px;
  }
  .row + .row {
    border-top: 1px solid var(--stroke-divider);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
      grid-template-columns: 1fr 76px 76px;
    }
  }
</style>
