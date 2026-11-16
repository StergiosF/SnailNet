<script lang="ts">
  // The connection-speed dialog (PLAN.md §14.6 Connection, by hand until
  // the speed test arrives in Phase 4): a Windows ContentDialog with the
  // two speed boxes, filled with what the service holds now. A native
  // <dialog> keeps focus inside and closes on Escape; focus goes back to
  // whatever opened it.
  import { tick, untrack } from "svelte";
  import { t } from "../i18n";
  import { parseSpeed, speedText } from "../line";
  import { lineDialog } from "../lineDialog.svelte";
  import { service } from "../service.svelte";
  import type { SaveError } from "../types";
  import SpeedFields from "./SpeedFields.svelte";

  let dialogEl = $state<HTMLDialogElement | null>(null);
  let fields = $state<SpeedFields | null>(null);
  let down = $state("");
  let up = $state("");
  let submitted = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);

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
        {#if error}
          <p class="error" role="alert">{error}</p>
        {/if}
      </div>
      <div class="footer">
        <button type="submit" class="btn btn-accent" disabled={saving}>{t("line.save")}</button>
        <button type="button" class="btn" disabled={saving} onclick={() => lineDialog.close()}>
          {t("line.cancel")}
        </button>
      </div>
    </form>
  {/if}
</dialog>

<style>
  /* Fluent ContentDialog: a layer card over a smoke backdrop, the actions
     in a darker footer band. */
  dialog {
    width: min(448px, calc(100vw - 48px));
    max-height: calc(100vh - 48px);
    padding: 0;
    overflow: auto;
    border: 1px solid var(--stroke-card);
    border-radius: var(--radius-card);
    background: var(--surface-ground);
    color: var(--text-primary);
    box-shadow: 0 32px 64px rgb(0 0 0 / 0.19), 0 2px 21px rgb(0 0 0 / 0.15);
  }
  dialog[open] {
    animation: dialog-in 167ms var(--ease-out);
  }
  dialog::backdrop {
    /* Fluent's smoke fill, the same in both themes. */
    background: rgb(0 0 0 / 0.3);
  }
  @keyframes dialog-in {
    from {
      opacity: 0;
      transform: scale(1.05);
    }
  }
  .content {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 24px;
    background: var(--surface-layer);
  }
  h2 {
    margin: 0;
    font: 600 20px/28px var(--font-display);
  }
  .content > p {
    margin: 0 0 4px;
    max-width: 60ch;
  }
  .error {
    margin: 0;
    font-size: 12px;
    line-height: 16px;
    color: var(--critical);
  }
  .footer {
    display: flex;
    gap: 8px;
    padding: 24px;
    border-top: 1px solid var(--stroke-divider);
  }
  .footer .btn {
    flex: 1;
  }
  @media (forced-colors: active) {
    dialog {
      border-color: CanvasText;
    }
  }
</style>
