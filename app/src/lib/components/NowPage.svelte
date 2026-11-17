<script lang="ts">
  // The Now page (PLAN.md §14.2): "What matters right now?" and the four
  // modes; who is using the internet, each with a Boost; the split in
  // short; and at the bottom one plain sentence on how things are going
  // (who uses the most, and whether anyone is waiting).
  import { t } from "../i18n";
  import type { Glance, Ledger } from "../ledger";
  import ModeTiles from "./ModeTiles.svelte";
  import SplitCard from "./SplitCard.svelte";
  import WhoCard from "./WhoCard.svelte";

  let {
    ledger,
    glance,
    canEdit,
  }: { ledger: Ledger | null; glance: Glance | null; canEdit: boolean } = $props();
</script>

<div class="now">
  <h1>{t("now.title")}</h1>
  <ModeTiles disabled={!canEdit} />
  <div class="cols">
    <WhoCard {ledger} {canEdit} />
    <SplitCard {ledger} {canEdit} />
  </div>
  {#if glance}
    <p class="glance" aria-live="off"><b>{glance.title}</b> {glance.detail}</p>
  {/if}
</div>

<style>
  .now {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  h1 {
    margin: 0;
    font-size: 26px;
    line-height: 32px;
    font-weight: 800;
    letter-spacing: -0.015em;
  }
  .cols {
    display: grid;
    grid-template-columns: minmax(0, 1.3fr) minmax(0, 1fr);
    gap: 14px;
    align-items: start;
  }
  .glance {
    margin: 0;
    padding: 12px 16px;
    border-radius: 14px;
    background: var(--surface-side);
    font-size: 14px;
    line-height: 1.45;
    color: var(--text-secondary);
  }
  .glance b {
    color: var(--text-primary);
    font-weight: 700;
  }
  @media (max-width: 860px) {
    .cols {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
