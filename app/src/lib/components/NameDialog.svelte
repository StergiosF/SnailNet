<script lang="ts">
  // Rename one of your own modes (§11.2). A native <dialog> like
  // ConfirmDialog: focus stays inside, Escape cancels, focus goes back to
  // the button that opened it. The name field has the focus, all selected.
  import { tick } from "svelte";
  import { quietFocus } from "../focusRing";
  import { t } from "../i18n";

  let {
    open,
    name,
    busy = false,
    onsave,
    oncancel,
  }: {
    open: boolean;
    name: string;
    busy?: boolean;
    onsave: (name: string) => void;
    oncancel: () => void;
  } = $props();

  const uid = $props.id();
  let el = $state<HTMLDialogElement | null>(null);
  let input = $state<HTMLInputElement | null>(null);
  let value = $state("");
  let opener: HTMLElement | null = null;
  const ok = $derived(value.trim().length > 0 && value.trim().length <= 40);

  $effect(() => {
    if (!el) return;
    if (open && !el.open) {
      opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      value = name;
      el.showModal();
      void tick().then(() => input?.select());
    } else if (!open && el.open) {
      el.close();
      if (opener) {
        quietFocus(opener);
        opener.focus();
      }
    }
  });

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (ok && !busy) onsave(value.trim());
  }
</script>

<dialog
  bind:this={el}
  aria-labelledby="{uid}-t"
  oncancel={(e) => {
    e.preventDefault();
    oncancel();
  }}
>
  <form class="in" onsubmit={submit}>
    <h2 id="{uid}-t">{t("mode.rename.title")}</h2>
    <label for="{uid}-n">{t("mode.rename.label")}</label>
    <input id="{uid}-n" class="field" bind:this={input} bind:value maxlength="40" autocomplete="off" spellcheck="false" />
    <div class="row">
      <button type="button" class="btn btn-quiet" onclick={oncancel}>{t("cancel")}</button>
      <button type="submit" class="btn btn-accent" disabled={!ok || busy}>{t("mode.rename.save")}</button>
    </div>
  </form>
</dialog>

<style>
  .in {
    display: flex;
    flex-direction: column;
    padding: 24px;
    min-width: 340px;
  }
  h2 {
    margin: 0 0 16px;
    font-size: 20px;
    font-weight: 800;
    letter-spacing: -0.01em;
  }
  label {
    margin-bottom: 6px;
    font-size: 13px;
    font-weight: 700;
    color: var(--text-secondary);
  }
  /* The look is app.css .field, the same as every text box. */
  input {
    font-size: 15px;
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 24px;
  }
</style>
