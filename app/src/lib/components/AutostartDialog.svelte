<script lang="ts">
  // "Start SnailNet with Windows?" (PLAN.md §14.5): asked once on a fresh
  // install, after first run, in the main window (autostart.rs decides
  // whether to ask and remembers the answer). A native <dialog> like
  // ConfirmDialog: focus stays inside; Escape answers "Not now". The main
  // button has the focus first: starting with Windows is what most people
  // want from a tray app, and Settings changes it any time.
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";
  import { t } from "../i18n";

  let { open, ondone }: { open: boolean; ondone: () => void } = $props();

  const uid = $props.id();
  let el = $state<HTMLDialogElement | null>(null);
  let yesBtn = $state<HTMLButtonElement | null>(null);
  let busy = $state(false);

  $effect(() => {
    if (!el) return;
    if (open && !el.open) {
      el.showModal();
      void tick().then(() => yesBtn?.focus());
    } else if (!open && el.open) {
      el.close();
    }
  });

  async function answer(on: boolean) {
    if (busy) return;
    busy = true;
    try {
      await invoke<boolean>("autostart_answer", { on });
    } catch {
      // Asked again next launch; Settings still has the switch.
    } finally {
      busy = false;
      ondone();
    }
  }
</script>

<dialog
  bind:this={el}
  aria-labelledby="{uid}-t"
  aria-describedby="{uid}-b"
  oncancel={(e) => {
    e.preventDefault();
    void answer(false);
  }}
>
  <div class="in">
    <h2 id="{uid}-t">{t("autostart.ask.title")}</h2>
    <p id="{uid}-b">{t("autostart.ask.body")}</p>
    <div class="row">
      <button type="button" class="btn btn-quiet" disabled={busy} onclick={() => void answer(false)}
        >{t("autostart.ask.no")}</button
      >
      <button bind:this={yesBtn} type="button" class="btn btn-accent" disabled={busy} onclick={() => void answer(true)}
        >{t("autostart.ask.yes")}</button
      >
    </div>
  </div>
</dialog>

<style>
  .in {
    padding: 24px;
    max-width: 420px;
  }
  h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 800;
    letter-spacing: -0.01em;
  }
  p {
    margin: 8px 0 0;
    line-height: 1.5;
    color: var(--text-secondary);
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 24px;
  }
  .btn-accent {
    font-weight: 700;
  }
</style>
