<script lang="ts">
  // "Add an app or website" (PLAN.md §11.1): a dialog listing the apps
  // SnailNet has seen and the websites it knows, in two groups, without
  // the ones that have a rule, with a search box (the one text-box style,
  // a search glyph inside it), each with its real logo where there is one.
  // Picking one gives it a 20% slice (from Everything else first) in the
  // Split page's draft and opens its row. A native <dialog>: focus stays inside,
  // Escape closes it, focus goes back to the button that opened it.
  import { quietFocus } from "../focusRing";
  import { tick } from "svelte";
  import { t } from "../i18n";
  import { addRule, pickable } from "../rules";
  import { service } from "../service.svelte";
  import type { Target } from "../types";
  import { ui } from "../ui.svelte";
  import AppIcon from "./AppIcon.svelte";
  import Glyph from "./Glyph.svelte";

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

  const groups = $derived(
    [
      { label: t("settings.apps"), items: choices.filter((c) => c.target.kind !== "site") },
      { label: t("editor.kind.site.many"), items: choices.filter((c) => c.target.kind === "site") },
    ].filter((g) => g.items.length > 0),
  );

  function closed() {
    ui.adding = false;
    if (opener?.isConnected) {
      quietFocus(opener);
      opener.focus();
    }
    opener = null;
  }

  function pick(target: Target) {
    ui.edit(addRule(ui.rules, target));
    ui.open = `${target.kind}:${target.id.toLowerCase()}`;
    ui.adding = false;
  }
</script>

<dialog bind:this={dialogEl} aria-labelledby="add-title" onclose={closed}>
  {#if ui.adding}
    <div class="top">
      <h2 id="add-title">{t("picker.title")}</h2>
      <button type="button" class="x" aria-label={t("editor.close")} onclick={() => (ui.adding = false)}>
        <Glyph name="close" size={18} />
      </button>
    </div>
    <label class="search">
      <Glyph name="search" size={18} />
      <input
        bind:this={searchEl}
        bind:value={query}
        type="search"
        placeholder={t("picker.search")}
        aria-label={t("picker.search")}
      />
    </label>
    {#if choices.length === 0}
      <p class="hint">{query.trim() ? t("picker.empty", { query: query.trim() }) : t("picker.hint")}</p>
    {:else}
      <div class="lists">
        {#each groups as g (g.label)}
          <h3>{g.label}</h3>
          <ul>
            {#each g.items as c (c.target.kind + ":" + c.target.id)}
              <li>
                <button type="button" class="choice" onclick={() => pick(c.target)}>
                  <AppIcon target={c.target} name={c.name} size={22} boxed />
                  <span class="name">{c.name}</span>
                  <span class="kind">{t(`editor.kind.${c.target.kind}`)}</span>
                  <span class="plus" aria-hidden="true"><Glyph name="plus" size={18} /></span>
                </button>
              </li>
            {/each}
          </ul>
        {/each}
      </div>
    {/if}
  {/if}
</dialog>

<style>
  /* The frame is app.css's dialog. */
  dialog {
    width: min(500px, calc(100vw - 48px));
    max-height: min(560px, calc(100vh - 64px));
    padding: 22px 22px 14px;
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
    font-weight: 800;
    letter-spacing: -0.01em;
  }
  .x {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--text-secondary);
  }
  .x:hover {
    background: var(--surface-2);
    color: var(--text-primary);
  }
  /* The one text-box style (app.css .field), with its glyph inside. */
  .search {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
    height: 42px;
    padding: 0 12px;
    border-radius: var(--radius-control);
    background: var(--surface-side);
    box-shadow: inset 0 0 0 1px var(--stroke-strong);
    color: var(--text-tertiary);
    cursor: text;
    transition: box-shadow 120ms linear;
  }
  .search:hover {
    box-shadow: inset 0 0 0 1px rgb(236 234 226 / 0.2);
  }
  .search:focus-within {
    box-shadow: inset 0 0 0 2px var(--focus-outer);
  }
  .search input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    background: transparent;
    color: var(--text-primary);
    font-size: 15px;
    outline: none;
  }
  .search input::placeholder {
    color: var(--text-tertiary);
  }
  .lists {
    margin: 8px -8px 0;
    padding: 0 0 6px;
    overflow-y: auto;
  }
  h3 {
    margin: 12px 10px 4px;
    font-size: 12.5px;
    font-weight: 700;
    color: var(--text-tertiary);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .choice {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 48px;
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
  .plus {
    display: flex;
    color: var(--text-tertiary);
    transition: color 120ms linear;
  }
  .choice:hover .plus {
    color: var(--shell-bright);
  }
  @media (forced-colors: active) {
    .search {
      border: 1px solid CanvasText;
    }
  }
  .hint {
    margin: 14px 0 0;
    color: var(--text-secondary);
  }
</style>
