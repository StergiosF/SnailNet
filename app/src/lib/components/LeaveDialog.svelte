<script lang="ts">
  // "Save your changes?" (PLAN.md §14.3): asked when the Split page has
  // unsaved changes and the person opens another page or picks another
  // mode (ui.guard). Save, Don't save, or Keep editing. A native <dialog>:
  // focus stays inside, Escape keeps editing, and Save has the focus
  // first, since keeping work is the safe answer.
  import { tick } from "svelte";
  import { t } from "../i18n";
  import { modeName } from "../modes";
  import { service } from "../service.svelte";
  import { ui } from "../ui.svelte";

  let el = $state<HTMLDialogElement | null>(null);
  let saveBtn = $state<HTMLButtonElement | null>(null);
  const mode = $derived(
    modeName(service.snapshot?.config.profiles.find((p) => p.id === ui.draft?.profile) ?? service.activeProfile),
  );

  $effect(() => {
    if (!el) return;
    if (ui.pending && !el.open) {
      el.showModal();
      void tick().then(() => saveBtn?.focus());
    } else if (!ui.pending && el.open) {
      el.close();
    }
  });
</script>

<dialog
  bind:this={el}
  aria-labelledby="leave-t"
  aria-describedby="leave-b"
  oncancel={(e) => {
    e.preventDefault();
    void ui.answer("stay");
  }}
>
  <div class="in">
    <h2 id="leave-t">{t("leave.title", { mode })}</h2>
    <p id="leave-b">{t("leave.body")}</p>
    <div class="row">
      <button type="button" class="btn btn-quiet" onclick={() => void ui.answer("stay")}>{t("leave.stay")}</button>
      <span class="gap"></span>
      <button type="button" class="btn" disabled={ui.committing} onclick={() => void ui.answer("discard")}
        >{t("leave.discard")}</button
      >
      <button
        bind:this={saveBtn}
        type="button"
        class="btn btn-accent"
        disabled={ui.committing}
        onclick={() => void ui.answer("save")}>{t("leave.save")}</button
      >
    </div>
  </div>
</dialog>

<style>
  .in {
    padding: 24px;
  }
  h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 800;
    letter-spacing: -0.01em;
    text-wrap: balance;
  }
  p {
    margin: 8px 0 0;
    line-height: 1.5;
    color: var(--text-secondary);
  }
  .row {
    display: flex;
    gap: 8px;
    margin-top: 24px;
  }
  .gap {
    flex: 1;
  }
</style>
