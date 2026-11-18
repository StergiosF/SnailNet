<script lang="ts">
  // Ask before something that can't be undone (Reset SnailNet). A native
  // <dialog>: focus stays inside, Escape cancels, and focus goes back to
  // the button that opened it. Cancel has the focus first.
  import { quietFocus } from "../focusRing";
  import { tick } from "svelte";
  import { t } from "../i18n";

  let {
    open,
    title,
    body,
    confirm,
    busy = false,
    onconfirm,
    oncancel,
  }: {
    open: boolean;
    title: string;
    body: string;
    confirm: string;
    busy?: boolean;
    onconfirm: () => void;
    oncancel: () => void;
  } = $props();

  const uid = $props.id();
  let el = $state<HTMLDialogElement | null>(null);
  let cancelBtn = $state<HTMLButtonElement | null>(null);
  let opener: HTMLElement | null = null;

  $effect(() => {
    if (!el) return;
    if (open && !el.open) {
      opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      el.showModal();
      void tick().then(() => cancelBtn?.focus());
    } else if (!open && el.open) {
      el.close();
      if (opener) {
        quietFocus(opener);
        opener.focus();
      }
    }
  });
</script>

<dialog
  bind:this={el}
  aria-labelledby="{uid}-t"
  aria-describedby="{uid}-b"
  oncancel={(e) => {
    e.preventDefault();
    oncancel();
  }}
>
  <div class="in">
    <h2 id="{uid}-t">{title}</h2>
    <p id="{uid}-b">{body}</p>
    <div class="row">
      <button bind:this={cancelBtn} type="button" class="btn" onclick={oncancel}>{t("cancel")}</button>
      <button type="button" class="btn danger" disabled={busy} onclick={onconfirm}>{confirm}</button>
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
    font-weight: 700;
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
  .danger {
    background: var(--critical);
    color: #2a0d07;
    font-weight: 700;
  }
  .danger:hover:not(:disabled) {
    background: #f6a595;
  }
</style>
