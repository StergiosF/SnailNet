<script lang="ts">
  // "Using your internet" (PLAN.md §14.2): who is moving data right now,
  // busiest first, each with what's happening to it and a Boost. The
  // boosted one says how long it has left.
  import { mbps, targetLabel } from "../format";
  import { t } from "../i18n";
  import type { Ledger } from "../ledger";
  import { boostLeft, liveBoost, users } from "../modes";
  import { service } from "../service.svelte";
  import type { Target } from "../types";
  import { ui } from "../ui.svelte";
  import AppIcon from "./AppIcon.svelte";
  import BoostButton from "./BoostButton.svelte";
  import Snail from "./Snail.svelte";

  let { ledger, canEdit }: { ledger: Ledger | null; canEdit: boolean } = $props();

  const label = (target: Target) => targetLabel(target, service.snapshot?.apps ?? [], service.siteNames);
  const live = $derived(liveBoost(service.snapshot?.config.boost, ui.now));
  const list = $derived(ledger ? users(ledger, live, label) : []);
</script>

<section class="card who" aria-labelledby="who-title">
  <h2 class="eyebrow" id="who-title">{t("who.title")}</h2>
  {#if !ledger}
    <ul aria-hidden="true">
      {#each [0.5, 0.4, 0.55] as w, i (i)}
        <li class="skeleton"><span class="ic"></span><span class="bar" style:width="{w * 100}%"></span></li>
      {/each}
    </ul>
  {:else if list.length === 0}
    <div class="empty">
      <Snail size={40} light />
      <p>{t("who.empty")}</p>
    </div>
  {:else}
    <ul>
      {#each list as u (u.key)}
        <li class:boosted={u.boosted}>
          <AppIcon target={u.target} name={u.name} size={22} boxed />
          <span class="nm">
            <b>{u.name}</b>
            <small>{u.boosted && live ? t("who.boosted", { left: boostLeft(live, ui.now) }) : u.status}</small>
          </span>
          <span class="mb num">{u.bps > 0 ? mbps(u.bps) : ""}<small>{u.bps > 0 ? ` ${t("line.unit")}` : ""}</small></span>
          <BoostButton target={u.target} name={u.name} boosted={u.boosted} disabled={!canEdit} />
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .who {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 16px 12px 10px;
    min-width: 0;
  }
  h2 {
    padding: 0 6px 4px;
  }
  ul {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 12px;
    min-height: 52px;
    padding: 6px 6px;
    border-radius: 12px;
  }
  li.boosted {
    background: linear-gradient(90deg, var(--shell-soft), transparent 80%);
  }
  .nm {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 15px;
    font-weight: 700;
  }
  .nm small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    color: var(--text-secondary);
  }
  li.boosted .nm small {
    color: var(--text-primary);
  }
  .mb {
    font-size: 15px;
    font-weight: 700;
    text-align: right;
    white-space: nowrap;
  }
  .mb small {
    font-size: 12px;
    font-weight: 400;
    color: var(--text-secondary);
  }
  .empty {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 6px 12px;
  }
  .empty p {
    margin: 0;
    color: var(--text-secondary);
    line-height: 1.45;
  }
  .skeleton {
    display: flex;
    gap: 12px;
  }
  .skeleton .ic {
    width: 34px;
    height: 34px;
    border-radius: var(--radius-control);
    background: var(--skeleton);
  }
  .skeleton .bar {
    height: 14px;
    border-radius: 4px;
    background: var(--skeleton);
  }
  @media (forced-colors: active) {
    li.boosted {
      outline: 2px solid Highlight;
      outline-offset: -2px;
    }
  }
</style>
