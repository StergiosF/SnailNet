<script lang="ts">
  // The Now page (DESIGN.md, Now): one question, "what's happening, and is
  // it OK?", answered top to bottom in one column.
  //   1. The sentence on how things are going (who uses the most, whether
  //      anyone is waiting) as the page's heading, the one-tap fix under it,
  //      and while the line is full the proof: ping with and without
  //      SnailNet.
  //   2. Who is using the internet, busiest first, each with how much of
  //      the line it takes (a thin bar and words) and a Boost.
  //   3. One line on the mode's split, with the way to it.
  // The mode switch is in the window's top strip (ModeSwitch), and the
  // split lives on its own page: this page shows the line once.
  //
  // Its one authored moment: the snail's shell (the same spiral as the
  // icon) draws itself once, large and faint, behind the heading as the
  // page opens. Then the card arrives.
  import { t } from "../i18n";
  import type { Glance, Ledger } from "../ledger";
  import { modeName } from "../modes";
  import type { Proof } from "../proof";
  import { service } from "../service.svelte";
  import { ui } from "../ui.svelte";
  import FixOffer from "./FixOffer.svelte";
  import Glyph from "./Glyph.svelte";
  import ProofRow from "./ProofRow.svelte";
  import WhoCard from "./WhoCard.svelte";

  let {
    ledger,
    glance,
    proof,
    canEdit,
    lineBps,
  }: { ledger: Ledger | null; glance: Glance | null; proof: Proof | null; canEdit: boolean; lineBps: number | null } =
    $props();

  const mode = $derived(modeName(service.activeProfile));
  // Slices of the mode's own: rules that are on and hold a share.
  const slices = $derived((service.activeProfile?.rules ?? []).filter((r) => r.enabled && r.share > 0).length);
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
  <div class="arrive"><WhoCard {ledger} {canEdit} {lineBps} /></div>
  {#if service.activeProfile && mode}
    <p class="split-line">
      <Glyph name="split" size={16} />
      <span>
        {slices === 0
          ? t("now.noslices", { mode })
          : t(slices === 1 ? "now.slices.one" : "now.slices.many", { mode, count: slices })}
      </span>
      <button type="button" class="link" onclick={() => ui.go("split")}>
        {slices === 0 ? t("now.noslices.go") : t("now.slices.go")}
      </button>
    </p>
  {/if}
</div>

<style>
  .now {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 22px;
    max-width: 880px;
    isolation: isolate;
  }
  /* The shell, behind everything, drawn once. */
  .shell {
    position: absolute;
    z-index: -1;
    top: -120px;
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
  header {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h1 {
    max-width: 22ch;
    margin: 0;
    font-size: 32px;
    line-height: 38px;
    font-weight: 800;
    letter-spacing: -0.025em;
    text-wrap: balance;
  }
  .glance {
    margin: 0;
    max-width: 62ch;
    font-size: 16px;
    line-height: 1.45;
    color: var(--text-secondary);
  }
  .arrive {
    min-width: 0;
    animation: rise-in 520ms 160ms var(--ease-out) backwards;
  }
  .split-line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
    margin: -6px 4px 0;
    font-size: 14px;
    color: var(--text-secondary);
  }
  .split-line :global(.glyph) {
    color: var(--text-tertiary);
  }
  .link {
    padding: 2px 0;
    border: 0;
    background: none;
    color: var(--text-primary);
    font-weight: 700;
    text-decoration: underline;
    text-decoration-color: rgb(239 232 214 / 0.35);
    text-underline-offset: 3px;
    transition: text-decoration-color 160ms linear;
  }
  .link:hover {
    text-decoration-color: var(--accent);
  }
</style>
