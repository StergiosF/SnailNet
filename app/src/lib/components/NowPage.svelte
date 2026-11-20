<script lang="ts">
  // The Now page (DESIGN.md, Now; Obsidian Jade): "what's happening, and
  // is it OK?".
  //   1. The hero, full width: the sentence on how things are going as the
  //      page's heading, and the one-tap fix under it.
  //   2. Who is using the internet (WhoCard), busiest first, in the main
  //      column.
  //   3. A side column: the ping, and this mode in one paragraph with the
  //      way to its split. While the line is full the ping is live, with
  //      and without SnailNet (ProofRow); at rest it's the last speed
  //      test's grades, dated. Never an idle ping the service didn't give.
  // Wide windows put 2 and 3 side by side; narrow ones stack them.
  //
  // Its one authored moment: the snail's shell (the same spiral as the
  // icon) draws itself once, large and faint, behind the heading as the
  // page opens.
  import { canMeasure, measureLine } from "../actions";
  import { shortDate } from "../format";
  import { t } from "../i18n";
  import type { Glance, Ledger } from "../ledger";
  import { MODES, modeName } from "../modes";
  import { grade, rise, type Grade, type Proof } from "../proof";
  import { service } from "../service.svelte";
  import { ui } from "../ui.svelte";
  import FixOffer from "./FixOffer.svelte";
  import Glyph, { modeGlyph } from "./Glyph.svelte";
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

  const profile = $derived(service.activeProfile);
  const mode = $derived(modeName(profile));
  const body = $derived(
    profile ? t(MODES.find((m) => m.id === profile.id)?.body ?? "mode.own.body") : "",
  );
  // Shares of the mode's own: rules that are on and hold a share; and the
  // ones that only use spare speed.
  const rules = $derived((profile?.rules ?? []).filter((r) => r.enabled));
  const slices = $derived(rules.filter((r) => r.share > 0).length);
  const spare = $derived(rules.filter((r) => r.share === 0).length);
  const summary = $derived(
    slices > 0
      ? t(slices === 1 ? "now.slices.one" : "now.slices.many", { mode, count: slices })
      : spare === 0
        ? t("now.noslices", { mode })
        : "",
  );
  const spareText = $derived(
    spare === 0 ? "" : spare === 1 ? t("now.spare.one") : t("now.spare.many", { count: spare }),
  );

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

    <div class="main arrive"><WhoCard {ledger} {canEdit} {lineBps} /></div>

    <aside class="side arrive">
      <section aria-labelledby="ping-title">
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

      {#if profile && mode}
        <section class="mode" aria-labelledby="mode-title">
          <h2 id="mode-title">{t("now.mode")}</h2>
          <p class="nm"><Glyph name={modeGlyph(profile.id)} size={18} />{mode}</p>
          <p class="note">{body} {summary} {spareText}</p>
          <button type="button" class="btn go" onclick={() => ui.go("split")}>
            {slices + spare === 0 ? t("now.noslices.go") : t("now.slices.go")}<Glyph name="chevron" size={16} />
          </button>
        </section>
      {/if}
    </aside>
  </div>
</div>

<style>
  /* The page measures itself, not the window: the side rail and gutters
     come off first. */
  .wrap {
    container-type: inline-size;
  }
  .now {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 26px;
    max-width: 1080px;
    isolation: isolate;
  }
  @container (min-width: 860px) {
    .now {
      grid-template-columns: minmax(0, 1fr) 300px;
      column-gap: 44px;
    }
    .hero {
      grid-column: 1 / -1;
    }
  }
  .hero {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 22px;
  }
  /* The shell, behind the heading, drawn once. */
  .shell {
    position: absolute;
    z-index: -1;
    top: -70px;
    right: -10px;
    width: 300px;
    height: 300px;
    pointer-events: none;
    fill: none;
    stroke: var(--shell);
    stroke-linecap: round;
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
    min-width: 0;
  }
  .arrive {
    animation: rise-in 520ms 160ms var(--ease-out) backwards;
  }
  /* The side column: plain sections parted by a hairline, no cards. */
  .side {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .side section {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    padding: 2px 0 22px;
  }
  .side section + section {
    padding-top: 20px;
    border-top: 1px solid var(--stroke-card);
  }
  h2 {
    margin: 0 0 10px;
    font-size: 14px;
    line-height: 20px;
    font-weight: 700;
    color: var(--text-secondary);
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
  .mode .nm {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 4px;
    font-size: 15px;
    font-weight: 700;
  }
  .mode .nm :global(.glyph) {
    color: var(--shell-bright);
  }
  .go {
    gap: 4px;
    padding-right: 10px;
  }
  @media (forced-colors: active) {
    .lt {
      color: CanvasText;
    }
  }
</style>
