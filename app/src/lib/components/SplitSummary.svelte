<script lang="ts">
  // "How {mode} shares it" (DESIGN.md, Now's side column; back from the
  // Garden's split card): the mode's shares as one bar in their inks, then
  // a line each with its priority, what it's sure of when your internet is
  // busy, and what it uses now. Apps that only use spare speed are counted
  // in a sentence. Changing any of it is the Split page's job.
  import { mbps } from "../format";
  import { t } from "../i18n";
  import { names, QUIET_BPS, type Ledger, type Slice } from "../ledger";
  import { modeName } from "../modes";
  import { service } from "../service.svelte";
  import { ui } from "../ui.svelte";
  import Glyph, { modeGlyph } from "./Glyph.svelte";

  let { ledger }: { ledger: Ledger | null } = $props();

  const profile = $derived(service.activeProfile);
  const mode = $derived(modeName(profile));
  const shares = $derived((ledger?.slices ?? []).filter((s) => s.share !== null && s.share > 0));
  const own = $derived(shares.filter((s) => s.target !== null));
  const spare = $derived((ledger?.slices ?? []).filter((s) => s.status.kind === "free").map((s) => s.name));
  const spareText = $derived(
    spare.length === 0
      ? ""
      : spare.length === 1
        ? t("now.spare.one")
        : spare.length <= 3
          ? t("share.spare.few", { names: names(spare) })
          : t("now.spare.many", { count: spare.length }),
  );
  const pct = (s: Slice) => Math.round((s.share ?? 0) / 100);
  const color = (s: Slice) => (s.target === null ? "var(--surface-3)" : `var(--ink-${s.ink})`);
  const prio = (s: Slice) =>
    s.target !== null && s.priority === "high"
      ? `${t("priority.high.chip")} · `
      : s.target !== null && s.priority === "low"
        ? `${t("priority.low.chip")} · `
        : "";
  const sub = (s: Slice) =>
    s.target === null
      ? t("share.else")
      : `${prio(s)}${s.kept !== null ? t("share.atleast", { mbps: mbps(s.kept) }) : ""}`;
</script>

{#if profile && mode}
  <section class="card" aria-labelledby="share-title">
    <h2 class="card-title" id="share-title">{t("share.title", { mode })}</h2>
    <div class="body">
      {#if own.length === 0}
        <p class="nm"><Glyph name={modeGlyph(profile.id)} size={18} />{mode}</p>
      {/if}
      <div class="bar" aria-hidden="true">
        {#each shares as s (s.key)}
          <span class:else={s.target === null} style:flex-grow={s.share} style:background={color(s)}
            >{#if pct(s) >= (s.target === null ? 18 : 15)}{t("share.bar", { name: s.name, pct: pct(s) })}{/if}</span
          >
        {/each}
      </div>
      {#if own.length > 0}
        <ul>
          {#each shares as s (s.key)}
            <li>
              <span class="dot" style:background={s.target === null ? "var(--stone-1)" : color(s)}></span>
              <span class="n">{t("share.row", { name: s.name, pct: pct(s) })}<small>{sub(s)}</small></span>
              {#if s.bps >= QUIET_BPS}
                <span class="r num" aria-live="off">{mbps(s.bps)}<small>{t("share.now")}</small></span>
              {:else}
                <span class="r idle">{t("share.idle")}</span>
              {/if}
            </li>
          {/each}
        </ul>
      {:else}
        <p class="note">{t("share.fair")}</p>
      {/if}
      {#if spareText}
        <p class="note">{spareText}</p>
      {/if}
      <button type="button" class="btn go" onclick={() => ui.go("split")}>
        {own.length + spare.length === 0 ? t("now.noslices.go") : t("now.slices.go")}<Glyph name="chevron" size={16} />
      </button>
    </div>
  </section>
{/if}

<style>
  .card-title {
    padding: 16px 20px 0;
  }
  .body {
    display: flex;
    flex-direction: column;
    padding: 12px 20px 18px;
  }
  .nm {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 8px;
    font-size: 15px;
    font-weight: 700;
  }
  .nm :global(.glyph) {
    color: var(--shell-bright);
  }
  .bar {
    display: flex;
    gap: 2px;
    height: 30px;
    margin-bottom: 10px;
    border-radius: 9px;
    overflow: hidden;
  }
  .bar span {
    display: flex;
    flex-basis: 0;
    align-items: center;
    min-width: 0;
    padding: 0 8px;
    overflow: hidden;
    color: #10201b;
    font-size: 12px;
    font-weight: 700;
    white-space: nowrap;
    transition: flex-grow 600ms var(--ease-out);
  }
  .bar span.else {
    color: var(--text-secondary);
    font-weight: 400;
  }
  ul {
    margin: 0 0 10px;
    padding: 0;
    list-style: none;
  }
  li {
    display: grid;
    grid-template-columns: 10px minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
    padding: 7px 0;
    font-size: 14px;
  }
  li + li {
    border-top: 1px solid var(--stroke-card);
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 3px;
  }
  .n {
    min-width: 0;
  }
  .n small,
  .r small {
    display: block;
    font-size: 12.5px;
    font-weight: 400;
    color: var(--text-tertiary);
  }
  .r {
    text-align: right;
    font-weight: 700;
    white-space: nowrap;
  }
  .r.idle {
    font-weight: 400;
    color: var(--text-tertiary);
  }
  .note {
    margin: 0 0 10px;
    font-size: 13.5px;
    line-height: 1.45;
    color: var(--text-secondary);
  }
  .go {
    align-self: flex-start;
    gap: 4px;
    margin-top: 2px;
    padding-right: 10px;
  }
  @media (forced-colors: active) {
    .bar span,
    .dot {
      forced-color-adjust: none;
      border: 1px solid CanvasText;
    }
  }
</style>
