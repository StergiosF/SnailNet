<script lang="ts">
  // The connection-speed dialog (PLAN.md §14.6 Connection, by hand until
  // the speed test arrives in Phase 4): a Windows ContentDialog with the
  // two speed boxes, filled with what the service holds now; when those
  // speeds came from a speed test, the day it ran, and a way to measure
  // again (it closes the dialog and runs Settings' measure). A native
  // <dialog> keeps focus inside and closes on Escape; focus goes back to
  // whatever opened it.
  import { tick, untrack } from "svelte";
  import { canMeasure, measureLine } from "../actions";
  import { shortDate } from "../format";
  import { t } from "../i18n";
  import { parseSpeed, speedText } from "../line";
  import { lineDialog } from "../lineDialog.svelte";
  import { service } from "../service.svelte";
  import type { SaveError } from "../types";
  import Glyph from "./Glyph.svelte";
  import SpeedFields from "./SpeedFields.svelte";

  let dialogEl = $state<HTMLDialogElement | null>(null);
  let fields = $state<SpeedFields | null>(null);
  let down = $state("");
  let up = $state("");
  let submitted = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);

  const line = $derived(service.snapshot?.config.line);
  const measured = $derived(line?.measured);
  /** The speeds in use are the last test's (not typed over since). */
  const same = (a: number | undefined, b: number) => a !== undefined && Math.abs(a - b) < 0.05;
  const fromTest = $derived(
    measured !== undefined && same(line?.down_mbps, measured.down_mbps) && same(line?.up_mbps, measured.up_mbps),
  );

  function measure() {
    lineDialog.close();
    void measureLine();
  }

  /** Fill the boxes with what the service holds now. */
  function load() {
    const line = service.snapshot?.config.line ?? {};
    down = speedText(line.down_mbps);
    up = speedText(line.up_mbps);
  }

  $effect(() => {
    const el = dialogEl;
    if (!el) return;
    if (lineDialog.open && !el.open) {
      untrack(() => {
        load();
        submitted = false;
        error = null;
        el.showModal();
        void tick().then(() => fields?.focus(lineDialog.focus));
      });
    } else if (!lineDialog.open) {
      if (el.open) el.close();
      lineDialog.returnFocus();
    }
  });

  async function save(e: SubmitEvent) {
    e.preventDefault();
    const d = parseSpeed(down);
    const u = parseSpeed(up);
    if (!d.ok || !u.ok) {
      submitted = true;
      fields?.focus();
      return;
    }
    error = null;
    saving = true;
    try {
      await service.saveLine(d.mbps, u.mbps);
      lineDialog.close();
    } catch (err) {
      const se = err as SaveError;
      if (se.kind === "conflict") {
        load();
        error = t("line.conflict");
      } else {
        error = t("line.failed", { message: se.message });
      }
    } finally {
      saving = false;
    }
  }
</script>

<dialog
  bind:this={dialogEl}
  aria-labelledby="line-title"
  aria-describedby="line-body"
  aria-busy={saving}
  oncancel={(e) => {
    e.preventDefault();
    if (!saving) lineDialog.close();
  }}
  onclose={() => lineDialog.close()}
>
  {#if lineDialog.open}
    <form onsubmit={save} novalidate>
      <div class="content">
        <h2 id="line-title">{t("line.title")}</h2>
        <p id="line-body">{t("line.body")}</p>
        <SpeedFields bind:this={fields} bind:down bind:up {submitted} disabled={saving} />
        {#if fromTest && measured}
          <p class="source">{t("line.measured", { date: shortDate(measured.at) })}</p>
        {/if}
        {#if error}
          <p class="error" role="alert">{error}</p>
        {/if}
      </div>
      <!-- In reading order, so Tab walks it left to right (WCAG 2.4.3):
           Measure on its own at the left, then Cancel, Save. -->
      <div class="footer">
        <button type="button" class="btn measure" disabled={saving || !canMeasure()} onclick={measure}>
          <Glyph name="gauge" size={18} />{measured ? t("line.measure") : t("line.measure.first")}
        </button>
        <button type="button" class="btn btn-quiet" disabled={saving} onclick={() => lineDialog.close()}>
          {t("line.cancel")}
        </button>
        <button type="submit" class="btn btn-accent" disabled={saving}>{t("line.save")}</button>
      </div>
    </form>
  {/if}
</dialog>

<style>
  /* The frame is app.css's dialog. */
  dialog {
    max-height: calc(100vh - 48px);
    overflow: auto;
  }
  .content {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 24px 24px 8px;
  }
  h2 {
    margin: 0;
    font-size: 20px;
    line-height: 28px;
    font-weight: 800;
    letter-spacing: -0.01em;
  }
  .content > p {
    margin: 0 0 4px;
    max-width: 60ch;
    color: var(--text-secondary);
  }
  .footer {
    display: flex;
    gap: 8px;
    padding: 16px 24px 24px;
  }
  .measure {
    margin-right: auto;
  }
  .content > .source {
    margin: 0;
    font-size: 13px;
    color: var(--text-secondary);
  }
</style>
