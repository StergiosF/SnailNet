<script lang="ts">
  // The live proof in the Now page's side column (feature 1, §11.10),
  // while the line is full: the ping with SnailNet as the section's one
  // big figure, who is filling the internet, and the bars against the
  // ping the last speed test on this network found without it. Shown and
  // held by ProofWatch (proof.ts); fades when it lets go. With no test to
  // compare against, a quiet "Measure to compare", only while it's busy,
  // so it never nags. Screen readers get one sentence, read on request
  // (aria-live off: live numbers aren't announced, §14.9).
  import { canMeasure, measureLine } from "../actions";
  import { t } from "../i18n";
  import { ms } from "../motion";
  import type { Proof } from "../proof";
  import { fade } from "svelte/transition";
  import Glyph from "./Glyph.svelte";
  import ProofBars from "./ProofBars.svelte";

  let { proof }: { proof: Proof } = $props();

  const now = $derived(Math.round(proof.now));
  const sr = $derived(
    proof.without === null
      ? `${t("proof.live", { now, name: proof.name })}.`
      : t("proof.sr", { now, without: Math.round(proof.without) }),
  );
</script>

<div class="proof" aria-live="off" in:fade={{ duration: ms(240) }}>
  <p class="visually-hidden">{sr}</p>
  <p class="big num" aria-hidden="true"><b>{now}</b><span>{t("proof.with.ms")}</span></p>
  <p class="note" aria-hidden="true">{t("proof.note", { name: proof.name, now })}</p>
  {#if proof.without !== null && proof.at !== null}
    <ProofBars withMs={proof.now} withoutMs={proof.without} at={proof.at} />
  {:else if proof.missing}
    <button type="button" class="btn compare" disabled={!canMeasure()} onclick={() => void measureLine()}>
      <Glyph name="gauge" size={18} />{t("proof.measure")}
    </button>
  {/if}
</div>

<style>
  .proof {
    display: flex;
    flex-direction: column;
    align-items: stretch;
  }
  .big {
    display: flex;
    align-items: baseline;
    gap: 6px;
    margin: 0;
  }
  .big b {
    font-size: 34px;
    line-height: 1;
    font-weight: 800;
    letter-spacing: -0.02em;
  }
  .big span {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .note {
    margin: 8px 0 14px;
    font-size: 13.5px;
    line-height: 1.45;
    color: var(--text-secondary);
  }
  .compare {
    align-self: flex-start;
  }
</style>
