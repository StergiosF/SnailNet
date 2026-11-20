<script lang="ts">
  // The proof on the Now page (feature 1, §11.10), under the glance
  // sentence while the line is full: the live ping with SnailNet, and the
  // ping the last speed test on this network found without it. Shown and
  // held by ProofWatch (proof.ts); fades when it lets go. With no test to
  // compare against, a quiet "Measure to compare", only while the line is
  // full, so it never nags. Screen readers get one sentence, read on
  // request (aria-live off: live numbers aren't announced, §14.9).
  import { canMeasure, measureLine } from "../actions";
  import { t } from "../i18n";
  import { ms } from "../motion";
  import type { Proof } from "../proof";
  import { fade } from "svelte/transition";
  import ProofBars from "./ProofBars.svelte";

  let { proof }: { proof: Proof } = $props();

  const now = $derived(Math.round(proof.now));
  const live = $derived(t("proof.live", { now, name: proof.name }));
  const sr = $derived(
    proof.without === null ? `${live}.` : t("proof.sr", { now, without: Math.round(proof.without) }),
  );
</script>

<div class="proof" aria-live="off" in:fade={{ duration: ms(240) }} out:fade={{ duration: ms(480) }}>
  <p class="visually-hidden">{sr}</p>
  <p class="live num" aria-hidden="true">{live}</p>
  {#if proof.without !== null && proof.at !== null}
    <ProofBars withMs={proof.now} withoutMs={proof.without} at={proof.at} />
  {:else if proof.missing}
    <button type="button" class="btn btn-quiet compare" disabled={!canMeasure()} onclick={() => void measureLine()}>
      <i class="icon" aria-hidden="true">&#xEC4A;</i>{t("proof.measure")}
    </button>
  {/if}
</div>

<style>
  .proof {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    margin-top: 4px;
  }
  .live {
    margin: 0;
    font-size: 14px;
    line-height: 20px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .compare {
    gap: 8px;
    margin-left: -10px;
    padding: 0 10px;
    font-size: 13px;
  }
  .compare .icon {
    font-size: 13px;
  }
</style>
