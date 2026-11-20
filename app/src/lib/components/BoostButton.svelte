<script lang="ts">
  // Boost one app or site (PLAN.md §11.5): a button that opens a small menu
  // (for 1 hour, 4 hours, until I stop it; and for an app, block its
  // internet, and how it's grouped: split a program out, put it back, or
  // count it as part of another app, §11.6), or "Stop boost" on the one
  // that's boosted. The menu is a native popover, so Escape and a click
  // outside close it.
  import { block, boost, mergeInto, putBack, setFirstChoice, splitOut, stopBoost } from "../actions";
  import Glyph from "./Glyph.svelte";
  import { members, splitExe } from "../families";
  import { firstChoiceOf, type FirstChoice } from "../firstChoice";
  import { t } from "../i18n";
  import { service } from "../service.svelte";
  import { placeMenu } from "../popover";
  import type { Target } from "../types";

  let { target, name, boosted, disabled }: { target: Target; name: string; boosted: boolean; disabled: boolean } =
    $props();

  const uid = $props.id();
  let btn = $state<HTMLButtonElement | null>(null);
  let menu = $state<HTMLDivElement | null>(null);

  const choices: { minutes: number | null; label: string }[] = [
    { minutes: 60, label: t("boost.1h") },
    { minutes: 240, label: t("boost.4h") },
    { minutes: null, label: t("boost.forever") },
  ];

  function place(e: ToggleEvent) {
    if (e.newState === "open" && btn && menu) placeMenu(btn, menu);
  }

  function pick(minutes: number | null) {
    menu?.hidePopover();
    void boost(target, minutes);
  }

  function blockIt() {
    menu?.hidePopover();
    void block(target.id, name);
  }

  // Whether this app goes first (§11.5): a game, never, or SnailNet's call.
  // Picking the ticked one again goes back to SnailNet's call.
  const first = $derived(firstChoiceOf(service.snapshot?.config.settings, target.id));

  // Grouping (§11.6). Only an app with more than one program can lose one.
  const apps = $derived(service.snapshot?.apps ?? []);
  const own = $derived(splitExe(target.id));
  const programs = $derived(own ? [] : members(apps, target.id).slice(0, 4));
  const others = $derived.by(() => {
    const list: { id: string; name: string }[] = [];
    for (const a of apps) {
      const id = a.family.toLowerCase();
      const skip = id === target.id.toLowerCase() || id === "system" || id === "unknown";
      if (!skip && !list.some((o) => o.id === id)) list.push({ id, name: a.name });
    }
    return list.sort((a, b) => a.name.localeCompare(b.name));
  });
  function split(exe: string) {
    menu?.hidePopover();
    void splitOut(exe, name);
  }
  function back() {
    menu?.hidePopover();
    if (own) void putBack(own);
  }
  // The select only picks; the button does it. Chromium on Windows fires
  // `change` on every arrow key in a closed select, so acting on change
  // would merge into whichever app the first key press landed on.
  let mergeTo = $state("");
  function merge() {
    const into = others.find((o) => o.id === mergeTo);
    if (!into) return;
    menu?.hidePopover();
    mergeTo = "";
    void mergeInto({ id: target.id, name }, into);
  }
  function choose(choice: FirstChoice) {
    menu?.hidePopover();
    void setFirstChoice(target.id, name, first === choice ? "auto" : choice);
  }
</script>

{#if boosted}
  <button
    type="button"
    class="b live"
    {disabled}
    aria-label={t("boost.stop.label", { name })}
    onclick={() => void stopBoost()}>{t("boost.stop")}</button
  >
{:else}
  <button
    bind:this={btn}
    type="button"
    class="b"
    {disabled}
    popovertarget="{uid}-menu"
    aria-haspopup="menu"
    aria-label={t("boost.label", { name })}><Glyph name="boost" size={16} />{t("boost")}</button
  >
  <div bind:this={menu} id="{uid}-menu" class="popmenu" popover="auto" role="menu" ontoggle={place}>
    <p class="title">{t("boost.menu.title", { name })}</p>
    <p class="body">{t("boost.menu.body")}</p>
    {#each choices as c (c.label)}
      <button type="button" role="menuitem" onclick={() => pick(c.minutes)}>{c.label}</button>
    {/each}
    {#if target.kind === "family"}
      <hr />
      {#each [["game", t("first.game")], ["never", t("first.never")]] as const as [choice, label] (choice)}
        <button
          type="button"
          role="menuitemcheckbox"
          aria-checked={first === choice}
          onclick={() => choose(choice)}
        >
          <span class="lab">{label}</span>
          {#if first === choice}<i class="icon tick" aria-hidden="true">&#xE73E;</i>{/if}
        </button>
      {/each}
      <hr />
      {#if own}
        <button type="button" role="menuitem" onclick={back}>
          <i class="icon" aria-hidden="true">&#xE7A7;</i>{t("group.back")}
        </button>
      {:else if programs.length > 1}
        {#each programs as exe (exe)}
          <button type="button" role="menuitem" onclick={() => split(exe)}>
            <i class="icon" aria-hidden="true">&#xE8C6;</i><span class="exe">{t("group.split", { exe })}</span>
          </button>
        {/each}
      {/if}
      {#if others.length > 0}
        <div class="merge" role="group" aria-labelledby="{uid}-merge">
          <span id="{uid}-merge">{t("group.merge")}</span>
          <div class="row">
            <select bind:value={mergeTo} aria-labelledby="{uid}-merge">
              <option value="">{t("group.merge.pick")}</option>
              {#each others as o (o.id)}
                <option value={o.id}>{o.name}</option>
              {/each}
            </select>
            <button type="button" class="go" disabled={mergeTo === ""} onclick={merge}>{t("group.merge.go")}</button>
          </div>
        </div>
      {/if}
      <hr />
      <button type="button" role="menuitem" class="danger" onclick={blockIt}>
        <i class="icon" aria-hidden="true">&#xE8F8;</i>{t("block")}
      </button>
    {/if}
  </div>
{/if}

<style>
  .b {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 12px;
    border: 0;
    border-radius: 9px;
    background: var(--surface-2);
    color: var(--text-primary);
    font-size: 13px;
    font-weight: 700;
    white-space: nowrap;
    transition:
      background-color 120ms linear,
      transform 120ms var(--ease-out);
  }
  .b:active:not(:disabled) {
    transform: scale(0.96);
  }
  .b:hover:not(:disabled) {
    background: var(--surface-3);
  }
  .b.live {
    background: var(--shell);
    color: var(--on-shell);
  }
  .b :global(.glyph) {
    margin-left: -2px;
    color: var(--shell);
  }
  .b.live:hover:not(:disabled) {
    background: var(--shell-hover);
  }
  .b:disabled {
    opacity: 0.5;
  }
  /* Ticked choices carry the mark at the end, so every label in the menu
     starts on the same line. */
  .lab {
    flex: 1;
    min-width: 0;
  }
  .tick {
    color: var(--shell-hover);
  }
  .exe {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .merge {
    display: grid;
    gap: 6px;
    padding: 6px 10px 4px;
    font-size: 13px;
    color: var(--text-secondary);
  }
  .merge .row {
    display: flex;
    gap: 6px;
  }
  .merge select {
    flex: 1;
    min-width: 0;
    height: 34px;
    padding: 0 8px;
    border: 0;
    border-radius: 9px;
    background: var(--surface-3);
    color: var(--text-primary);
    font-size: 14px;
  }
  .merge .go {
    flex: none;
    width: auto;
    height: 34px;
    padding: 0 12px;
    background: var(--surface-3);
    color: var(--text-primary);
    font-weight: 700;
  }
  .merge .go:hover:not(:disabled) {
    background: var(--accent);
    color: var(--on-accent);
  }
  .merge .go:disabled {
    opacity: 0.5;
  }
  @media (forced-colors: active) {
    .b {
      border: 1px solid ButtonText;
    }
    .b.live {
      forced-color-adjust: none;
      background: Highlight;
      color: HighlightText;
    }
  }
</style>
