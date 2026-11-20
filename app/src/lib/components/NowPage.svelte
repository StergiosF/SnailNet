<script lang="ts">
  // The Now page (PLAN.md §14.2), read in the order a glance asks:
  //   1. What's happening on my line? The sentence on how things are going
  //      (who uses the most, whether anyone is waiting) is the page's
  //      heading; under it, while the line is full, the proof: ping with
  //      SnailNet and without it (ProofRow).
  //   2. Which mode is on, and what else could it be? "What matters right
  //      now?" over the four mode tiles.
  //   3. What can I do? Who is using the internet, each with a Boost,
  //      beside how the mode shares the line, with the way to change it.
  //
  // Its one authored moment: the snail's shell (the same spiral as the
  // icon) draws itself once, large and faint, behind the question as the
  // page opens. Then the tiles and cards arrive in reading order.
  import { t } from "../i18n";
  import type { Glance, Ledger } from "../ledger";
  import type { Proof } from "../proof";
  import ModeTiles from "./ModeTiles.svelte";
  import SplitCard from "./SplitCard.svelte";
  import FixOffer from "./FixOffer.svelte";
  import ProofRow from "./ProofRow.svelte";
  import WhoCard from "./WhoCard.svelte";

  let {
    ledger,
    glance,
    proof,
    canEdit,
  }: { ledger: Ledger | null; glance: Glance | null; proof: Proof | null; canEdit: boolean } = $props();
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
  <header>
    <h1 aria-live="off">{glance ? glance.title : t("now.heading")}</h1>
    {#if glance?.detail}
      <p class="glance" aria-live="off">{glance.detail}</p>
    {/if}
    <FixOffer {canEdit} />
    {#if proof}
      <ProofRow {proof} />
    {/if}
  </header>
  <section class="modes" aria-labelledby="modes-h">
    <div class="sec-head">
      <h2 id="modes-h">{t("now.title")}</h2>
      <p>{t("now.modes.body")}</p>
    </div>
    <ModeTiles disabled={!canEdit} />
  </section>
  <div class="cols">
    <div class="arrive" style:--d="220ms"><WhoCard {ledger} {canEdit} /></div>
    <div class="arrive" style:--d="280ms"><SplitCard {ledger} {canEdit} /></div>
  </div>
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
    max-width: 24ch;
    margin: 4px 0 0;
    font-size: 32px;
    line-height: 38px;
    font-weight: 800;
    letter-spacing: -0.025em;
    text-wrap: balance;
  }
  .modes {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 10px;
  }
  .sec-head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 2px 14px;
  }
  .sec-head h2 {
    margin: 0;
    font-size: 20px;
    line-height: 26px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .sec-head p {
    margin: 0;
    font-size: 14px;
    color: var(--text-secondary);
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
  header {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .glance {
    margin: 0;
    max-width: 68ch;
    font-size: 16px;
    line-height: 1.45;
    color: var(--text-secondary);
  }
  @media (max-width: 860px) {
    .cols {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
