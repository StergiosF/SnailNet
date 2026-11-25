<script lang="ts">
  // The Now page (DESIGN.md, Now; Obsidian Jade): "what's happening, and
  // is it OK?".
  //   1. The hero, full width: the sentence on how things are going as the
  //      page's heading, and the one-tap fix under it.
  //   2. The main column: "Last 10 minutes" (Timeline, with the Download /
  //      Upload choice), then who is using the internet now (WhoCard),
  //      busiest first, each row in its band's colour; hovering a row
  //      lights its band.
  //   3. A side column of two panels: the ping, and how the mode shares
  //      your internet (SplitSummary). While the line is full the ping is
  //      live, with and without SnailNet (ProofRow); at rest it's the last
  //      speed test's grades, dated. Never an idle ping the service didn't
  //      give.
  // Wide windows put 2 and 3 side by side; narrow ones stack them.
  //
  // Its one authored moment: the snail's shell (the same spiral as the
  // icon) draws itself once, large and faint, behind the heading as the
  // page opens.
  import { canMeasure, measureLine } from "../actions";
  import { shortDate } from "../format";
  import { t } from "../i18n";
  import type { Glance, Ledger } from "../ledger";
  import { bandColors } from "../history";
  import { grade, rise, type Grade, type Proof } from "../proof";
  import { service } from "../service.svelte";
  import { timeline } from "../timeline.svelte";
  import { ui } from "../ui.svelte";
  import FixOffer from "./FixOffer.svelte";
  import Glyph from "./Glyph.svelte";
  import ProofRow from "./ProofRow.svelte";
  import SplitSummary from "./SplitSummary.svelte";
  import Timeline from "./Timeline.svelte";
  import WhoCard from "./WhoCard.svelte";

  let {
    ledger,
    glance,
    proof,
    canEdit,
    lineBps,
  }: { ledger: Ledger | null; glance: Glance | null; proof: Proof | null; canEdit: boolean; lineBps: number | null } =
    $props();

  // "Last 10 minutes" in the direction shown, and each band's colour,
  // which its row in the list shares.
  const bands = $derived.by(() => {
    void timeline.rev;
    return timeline.history.bands(ui.dir);
  });
  const colors = $derived(bandColors(bands));
  let lit = $state<string | null>(null);

  // The last speed test's grades (§11.10): how much ping rose while
  // downloading, without SnailNet and, after the check, with it.
  const measured = $derived(service.snapshot?.config.line.measured);
  type GradeRow = { key: string; who: string; grade: Grade; rise: number };
  const grades = $derived.by((): GradeRow[] => {
    const m = measured;
    if (!m) return [];
    const without = rise(m.idle_ms, m.loaded_ms);
    if (without === null) return [];
    const out: GradeRow[] = [
      { key: "without", who: t("proof.grade.without"), grade: grade(without), rise: Math.round(without) },
    ];
    const withIt = rise(m.idle_ms, m.with_ms);
    if (withIt !== null) {
      out.push({ key: "with", who: t("proof.grade.with"), grade: grade(withIt), rise: Math.round(withIt) });
    }
    return out;
  });
  const calm = $derived(grades.length > 0 && grades[0]?.grade === "A");
</script>

<div class="wrap">
  <div class="now">
    <header class="hero">
      <svg class="shell" viewBox="0 0 512 512" aria-hidden="true">
        <circle class="rim" cx="224" cy="250" r="150" pathLength="1" />
        <path
          class="coil"
          pathLength="1"
          d="M224 250m-22 0a22 22 0 1 1 44 0a50 50 0 1 1-100 0a78 78 0 1 1 156 0a106 106 0 1 1-212 0"
        />
      </svg>
      <h1 aria-live="off">{glance ? glance.title : t("now.heading")}</h1>
      {#if glance?.detail}
        <p class="glance" aria-live="off">{glance.detail}</p>
      {/if}
      <FixOffer {canEdit} />
    </header>

    <div class="main arrive">
      <Timeline {bands} {colors} {lit} {lineBps} totalBps={ledger ? ledger.total : null} />
      <WhoCard {ledger} {canEdit} {colors} {lit} onlight={(key) => (lit = key)} />
    </div>

    <aside class="side arrive">
      <section class="card ping" aria-labelledby="ping-title">
        {#if proof}
          <h2 id="ping-title">{t("proof.title.live")}</h2>
          <ProofRow {proof} />
        {:else}
          <h2 id="ping-title">{t("proof.title.grades")}</h2>
          {#if grades.length > 0 && measured}
            <dl class="grades">
              {#each grades as g (g.key)}
                <div class="grade">
                  <dt class="visually-hidden">{g.who}</dt>
                  <dd>
                    <span class="lt {g.grade}" aria-label="{g.grade} {t('proof.grade.word')}">{g.grade}</span>
                    <span class="rise num">{t("proof.grade.rise", { rise: g.rise })}</span>
                    <span class="who" aria-hidden="true">{g.who}</span>
                  </dd>
                </div>
              {/each}
            </dl>
            <p class="note">{calm ? t("proof.grade.calm") : t("proof.quiet")}</p>
            <p class="dated">{t("proof.from", { date: shortDate(measured.at) })}</p>
          {:else}
            <p class="note">{t("proof.none")}</p>
            <button type="button" class="btn" disabled={!canEdit || !canMeasure()} onclick={() => void measureLine()}>
              <Glyph name="gauge" size={18} />{t("measure.button")}
            </button>
          {/if}
        {/if}
      </section>

      <SplitSummary {ledger} />
    </aside>
  </div>
</div>

<style>
  /* The page measures itself, not the window: the side rail and gutters
     come off first. Wide windows fit the page to the window's height —
     the app list (WhoCard) is the only thing that scrolls; narrow ones
     stack and scroll here instead. */
  .wrap {
    container-type: inline-size;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  .now {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 18px;
    max-width: 1180px;
    isolation: isolate;
  }
  @container (min-width: 860px) {
    .now {
      grid-template-columns: minmax(0, 1fr) 316px;
      grid-template-rows: auto minmax(0, 1fr);
      column-gap: 24px;
      height: 100%;
    }
    .hero {
      grid-column: 1 / -1;
    }
  }
  .hero {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 0 4px;
  }
  /* The shell, behind the heading, drawn once. */
  .shell {
    position: absolute;
    z-index: -1;
    top: -78px;
    right: 40px;
    width: 300px;
    height: 300px;
    pointer-events: none;
    fill: none;
    stroke: var(--shell);
    stroke-linecap: round;
  }
  .rim {
    stroke-width: 2;
    stroke-opacity: 0.09;
    stroke-dasharray: 1;
    animation: draw 1400ms var(--ease-out) backwards;
  }
  .coil {
    stroke-width: 9;
    stroke-opacity: 0.08;
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
    max-width: 56ch;
    font-size: 17px;
    line-height: 1.45;
    color: var(--text-secondary);
  }
  .main {
    display: flex;
    flex-direction: column;
    gap: 18px;
    min-width: 0;
    min-height: 0;
    /* Last resort on very short windows: the column scrolls, never the
       window. */
    overflow-y: auto;
  }
  .arrive {
    animation: rise-in 520ms 160ms var(--ease-out) backwards;
  }
  /* The side column: two panels, the ping and the mode's split. */
  .side {
    display: flex;
    flex-direction: column;
    gap: 18px;
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
  }
  .side.arrive {
    animation-delay: 240ms;
  }
  .ping {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    padding: 16px 20px 18px;
  }
  h2 {
    margin: 0 0 12px;
    font-size: 16px;
    line-height: 22px;
    font-weight: 700;
  }
  .note {
    margin: 0;
    font-size: 13.5px;
    line-height: 1.45;
    color: var(--text-secondary);
  }
  .dated {
    margin: 12px 0 0;
    font-size: 12.5px;
    color: var(--text-tertiary);
  }
  .grades {
    display: grid;
    grid-template-columns: 1fr 1fr;
    margin: 2px 0 12px;
  }
  .grade dd {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding-right: 14px;
  }
  .grade + .grade dd {
    padding: 0 0 0 14px;
    border-left: 1px solid var(--stroke-card);
  }
  .lt {
    font-size: 30px;
    line-height: 1.1;
    font-weight: 800;
    color: var(--caution);
  }
  .lt.A,
  .lt.B {
    color: var(--shell-bright);
  }
  .lt.D,
  .lt.F {
    color: var(--critical);
  }
  .rise {
    font-size: 13px;
    font-weight: 700;
  }
  .who {
    font-size: 12.5px;
    color: var(--text-secondary);
  }
  .side .btn {
    align-self: flex-start;
    margin-top: 12px;
  }
  @media (forced-colors: active) {
    .lt {
      color: CanvasText;
    }
  }
</style>
