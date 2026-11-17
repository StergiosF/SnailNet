<script lang="ts">
  // "Add a slice" (PLAN.md §11.1): a dialog listing the websites SnailNet
  // knows and the apps it has seen, without the ones that have a rule,
  // with a search box. Picking one gives it a 20% slice (from Everything
  // else first) and opens its row. A native <dialog>: focus stays inside,
  // Escape closes it, focus goes back to the button that opened it.
  import { tick } from "svelte";
  import { t } from "../i18n";
  import { addRule, pickable } from "../rules";
  import { service } from "../service.svelte";
  import type { Target } from "../types";
  import { ui } from "../ui.svelte";
  import AppIcon from "./AppIcon.svelte";

  let dialogEl = $state<HTMLDialogElement | null>(null);
  let searchEl = $state<HTMLInputElement | null>(null);
  let query = $state("");
  let opener: HTMLElement | null = null;

  const choices = $derived(
    ui.adding ? pickable(service.snapshot?.sites ?? [], service.snapshot?.apps ?? [], ui.rules, query) : [],
  );

  $effect(() => {
    const el = dialogEl;
    if (!el) return;
    if (ui.adding && !el.open) {
      const active = document.activeElement;
      opener = active instanceof HTMLElement ? active : null;
      query = "";
      el.showModal();
      void tick().then(() => searchEl?.focus());
    } else if (!ui.adding && el.open) {
      el.close();
    }
  });

  function closed() {
    ui.adding = false;
    if (opener?.isConnected) opener.focus();
    opener = null;
  }

  function pick(target: Target) {
    ui.change(addRule(ui.rules, target));
    ui.open = `${target.kind}:${target.id.toLowerCase()}`;
    ui.adding = false;
  }
</script>

<dialog bind:this={dialogEl} aria-labelledby="add-title" onclose={closed}>
  {#if ui.adding}
    <div class="top">
      <h2 id="add-title">{t("picker.title")}</h2>
      <button type="button" class="x" aria-label={t("editor.close")} onclick={() => (ui.adding = false)}>
        <i class="icon" aria-hidden="true">&#xE711;</i>
      </button>
    </div>
    <input
      bind:this={searchEl}
      bind:value={query}
      class="field search"
      type="search"
      placeholder={t("picker.search")}
      aria-label={t("picker.search")}
    />
    {#if choices.length === 0}
      <p class="hint">{query.trim() ? t("picker.empty", { query: query.trim() }) : t("picker.hint")}</p>
    {:else}
      <ul>
        {#each choices as c (c.target.kind + ":" + c.target.id)}
          <li>
            <button type="button" class="choice" onclick={() => pick(c.target)}>
              <AppIcon target={c.target} name={c.name} size={22} />
              <span class="name">{c.name}</span>
              <span class="kind">{t(`editor.kind.${c.target.kind}`)}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</dialog>

<style>
  /* The frame is app.css's dialog. */
  dialog {
    max-height: min(520px, calc(100vh - 64px));
    padding: 20px;
    overflow: hidden;
  }
  dialog[open] {
    display: flex;
    flex-direction: column;
  }
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 14px;
  }
  h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 700;
  }
  .x {
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 12px;
  }
  .x:hover {
    background: var(--surface-2);
    color: var(--text-primary);
  }
  .search {
    width: 100%;
    flex: none;
  }
  ul {
    margin: 12px -8px 0;
    padding: 0;
    list-style: none;
    overflow-y: auto;
  }
  .choice {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 44px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    text-align: left;
  }
  .choice:hover {
    background: var(--surface-2);
  }
  .choice:focus-visible {
    outline-offset: -2px;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .kind {
    color: var(--text-secondary);
    font-size: 13px;
  }
  .hint {
    margin: 14px 0 0;
    color: var(--text-secondary);
  }
</style>
