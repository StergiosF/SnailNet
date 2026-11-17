<script lang="ts">
  // The Now page (PLAN.md §14.2): "What matters right now?" and the four
  // modes; who is using the internet, each with a Boost; the split in
  // short; and at the bottom one plain sentence on how things are going
  // (who uses the most, and whether anyone is waiting).
  //
  // Its one authored moment: the snail's shell (the same spiral as the
  // icon) draws itself once, large and faint, behind the question as the
  // page opens. Then the tiles and cards arrive in reading order.
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
  <svg class="shell" viewBox="0 0 512 512" aria-hidden="true">
    <circle class="rim" cx="224" cy="250" r="150" pathLength="1" />
    <path
      class="coil"
      pathLength="1"
      d="M224 250m-22 0a22 22 0 1 1 44 0a50 50 0 1 1-100 0a78 78 0 1 1 156 0a106 106 0 1 1-212 0"
    />
  </svg>
  <h1>{t("now.title")}</h1>
  <ModeTiles disabled={!canEdit} />
  <div class="cols">
    <div class="arrive" style:--d="220ms"><WhoCard {ledger} {canEdit} /></div>
    <div class="arrive" style:--d="280ms"><SplitCard {ledger} {canEdit} /></div>
  </div>
  {#if glance}
    <p class="glance arrive" style:--d="340ms" aria-live="off"><b>{glance.title}</b> {glance.detail}</p>
  {/if}
</div>

<style>
  .now {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 18px;
    isolation: isolate;
  }
  /* The shell, behind everything, drawn once. */
  .shell {
    position: absolute;
    z-index: -1;
    top: -96px;
    right: -24px;
    width: 420px;
    height: 420px;
    pointer-events: none;
    fill: none;
    stroke: var(--shell);
    stroke-linecap: round;
  }
  .rim {
    stroke-width: 2;
    stroke-opacity: 0.1;
    stroke-dasharray: 1;
    animation: draw 1400ms var(--ease-out) backwards;
  }
  .coil {
    stroke-width: 10;
    stroke-opacity: 0.09;
    stroke-dasharray: 1;
    animation: draw 1800ms 120ms var(--ease-out) backwards;
  }
  @keyframes draw {
    from {
      stroke-dashoffset: 1;
    }
    to {
      stroke-dashoffset: 0;
    }
  }
  h1 {
    margin: 4px 0 0;
    font-size: 32px;
    line-height: 38px;
    font-weight: 800;
    letter-spacing: -0.025em;
    text-wrap: balance;
  }
  .arrive {
    min-width: 0;
    animation: rise-in 520ms var(--d, 0ms) var(--ease-out) backwards;
  }
  .cols {
    display: grid;
    grid-template-columns: minmax(0, 1.3fr) minmax(0, 1fr);
    gap: 14px;
    align-items: start;
  }
  .glance {
    margin: 0;
    padding: 14px 18px;
    border-radius: 14px;
    background: rgb(10 33 29 / 0.72);
    box-shadow: var(--shadow-card);
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
