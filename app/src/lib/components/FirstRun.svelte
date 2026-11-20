<script lang="ts">
  // First run (PLAN.md §14.5): welcome and the privacy line, then the
  // connection speed, measured (the service's speed test and its check
  // with SnailNet, ~30 s) or typed, then done; after a measure, the done
  // step shows ping with and without SnailNet (the proof, §11.10). (The "what matters to you" cards are Phase 5.) It takes the place of the main content until finished; each
  // step's heading takes focus so screen readers hear where they are.
  import { onMount, tick } from "svelte";
  import { measureLine } from "../actions";
  import { t } from "../i18n";
  import { CLEAR_MS } from "../proof";
  import { parseSpeed } from "../line";
  import { service } from "../service.svelte";
  import type { Measured, SaveError } from "../types";
  import ProofBars from "./ProofBars.svelte";
  import Snail from "./Snail.svelte";
  import SpeedFields from "./SpeedFields.svelte";

  let { onfinish }: { onfinish: () => void } = $props();

  type Step = "welcome" | "speed" | "done";
  let step = $state<Step>("welcome");
  /** Whether a speed was saved (the done step says what happens next). */
  let saved = $state(false);
  let down = $state("");
  let up = $state("");
  let submitted = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let headingEl = $state<HTMLHeadingElement | null>(null);
  let fields = $state<SpeedFields | null>(null);

  onMount(() => headingEl?.focus());

  async function go(next: Step) {
    step = next;
    error = null;
    await tick();
    if (next === "speed") fields?.focus("down");
    else headingEl?.focus();
  }

  async function next(e: SubmitEvent) {
    e.preventDefault();
    const d = parseSpeed(down);
    const u = parseSpeed(up);
    if (!d.ok || !u.ok) {
      submitted = true;
      fields?.focus();
      return;
    }
    if (d.mbps === null && u.mbps === null) {
      saved = false;
      return go("done");
    }
    error = null;
    saving = true;
    try {
      await service.saveLine(d.mbps, u.mbps);
      saved = true;
      void go("done");
    } catch (err) {
      const se = err as SaveError;
      error = se.kind === "conflict" ? t("line.conflict") : t("line.failed", { message: se.message });
    } finally {
      saving = false;
    }
  }

  /** Measure instead of typing: the service tests the line (~30 s) and
   *  saves what it finds. */
  let measuring = $state(false);
  /** The test's result, for the done step's before and after. */
  let result = $state<Measured | null>(null);
  /** Ping with and without SnailNet, when both were measured. */
  const pings = $derived(
    result?.loaded_ms !== undefined && result.with_ms !== undefined
      ? { with: result.with_ms, without: result.loaded_ms, at: result.at }
      : null,
  );
  async function measure() {
    error = null;
    measuring = true;
    const m = await measureLine();
    measuring = false;
    result = m;
    if (m) {
      saved = true;
      void go("done");
    } else {
      error = t("measure.none");
    }
  }

  function skip() {
    saved = false;
    void go("done");
  }
</script>

<main class="first-run" aria-label={t("firstrun.label")} aria-busy={saving}>
  {#key step}
  <div class="step">
    {#if step === "welcome"}
      <Snail size={48} />
      <h1 tabindex="-1" bind:this={headingEl}>{t("firstrun.welcome.title")}</h1>
      <p class="lead">{t("firstrun.welcome.body")}</p>
      <p class="privacy">
        <i class="icon" aria-hidden="true">&#xE72E;</i>{t("firstrun.welcome.privacy")}
      </p>
      <div class="actions">
        <button type="button" class="btn btn-accent" onclick={() => void go("speed")}>
          {t("firstrun.start")}
        </button>
      </div>
    {:else if step === "speed"}
      <form onsubmit={next} novalidate aria-labelledby="fr-speed-title" aria-describedby="fr-speed-body">
        <h1 id="fr-speed-title" tabindex="-1" bind:this={headingEl}>{t("firstrun.speed.title")}</h1>
        <p class="lead" id="fr-speed-body">{t("firstrun.speed.body")}</p>
        <button type="button" class="btn btn-accent measure" disabled={saving || measuring} onclick={() => void measure()}>
          <i class="icon" aria-hidden="true">&#xEC4A;</i>{measuring ? t("firstrun.measuring") : t("firstrun.measure")}
        </button>
        <p class="or">{t("firstrun.or")}</p>
        <SpeedFields bind:this={fields} bind:down bind:up {submitted} disabled={saving || measuring} />
        {#if error}
          <p class="error" role="alert">{error}</p>
        {/if}
        <div class="actions">
          <button type="button" class="btn back" disabled={saving || measuring} onclick={() => void go("welcome")}>
            {t("firstrun.back")}
          </button>
          <button type="button" class="btn" disabled={saving || measuring} onclick={skip}>{t("firstrun.skip")}</button>
          <button type="submit" class="btn" disabled={saving || measuring}>{t("firstrun.next")}</button>
        </div>
      </form>
    {:else}
      <h1 tabindex="-1" bind:this={headingEl}>
        {saved ? t("firstrun.done.title") : t("firstrun.later.title")}
      </h1>
      <p class="lead">{saved ? t("firstrun.done.body") : t("firstrun.later.body")}</p>
      {#if saved && pings}
        {#if pings.without - pings.with >= CLEAR_MS}
          <div class="proof">
            <p class="visually-hidden">
              {t("proof.sr", { now: Math.round(pings.with), without: Math.round(pings.without) })}
            </p>
            <ProofBars withMs={pings.with} withoutMs={pings.without} at={pings.at} />
          </div>
        {:else}
          <p class="calm">{t("proof.grade.calm")}</p>
        {/if}
      {/if}
      <div class="actions">
        <button type="button" class="btn btn-accent" onclick={onfinish}>{t("firstrun.finish")}</button>
      </div>
    {/if}
  </div>
  {/key}
</main>

<style>
  .first-run {
    flex: 1;
    display: grid;
    place-items: center;
    overflow-y: auto;
    padding: 32px var(--gutter);
  }
  .step,
  form {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
    width: min(480px, 100%);
    animation: step-in 250ms var(--ease-out);
  }
  form {
    width: 100%;
    animation: none;
  }
  @keyframes step-in {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  h1 {
    margin: 8px 0 0;
    font-size: 30px;
    line-height: 36px;
    font-weight: 700;
    letter-spacing: -0.015em;
    text-wrap: balance;
  }
  .lead {
    margin: 0;
    max-width: 60ch;
    font-size: 16px;
    color: var(--text-secondary);
  }
  .privacy {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 4px 0 0;
    font-size: 13px;
    line-height: 18px;
    color: var(--text-secondary);
  }
  .privacy .icon {
    margin-top: 2px;
  }
  .proof {
    align-self: stretch;
    margin-top: 12px;
  }
  .calm {
    margin: 4px 0 0;
    font-size: 14px;
    color: var(--text-secondary);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-self: stretch;
    margin-top: 20px;
  }
  .back {
    margin-right: auto;
  }
  .measure {
    gap: 8px;
    margin-top: 8px;
  }
  .measure .icon {
    font-size: 14px;
  }
  .or {
    margin: 4px 0 0;
    font-size: 14px;
    color: var(--text-secondary);
  }
</style>
