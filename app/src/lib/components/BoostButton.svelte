<script lang="ts">
  // Boost one app or site (PLAN.md §11.5): a button that opens a small menu
  // (for 1 hour, 4 hours, until I stop it; and for an app, block its
  // internet, and how it's grouped: split a program out, put it back, or
  // count it as part of another app, §11.6), or "Stop boost" on the one
  // that's boosted. The menu is a native popover, so Escape and a click
  // outside close it.
  import { block, boost, mergeInto, putBack, setFirstChoice, splitOut, stopBoost } from "../actions";
  import AppIcon from "./AppIcon.svelte";
  import Glyph, { type GlyphName } from "./Glyph.svelte";
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

  const choices: { minutes: number | null; label: string; glyph: GlyphName }[] = [
    { minutes: 60, label: t("boost.1h"), glyph: "clock" },
    { minutes: 240, label: t("boost.4h"), glyph: "clock" },
    { minutes: null, label: t("boost.forever"), glyph: "infinity" },
  ];

  function place(e: ToggleEvent) {
    if (e.newState === "open" && btn && menu) placeMenu(btn, menu);
    if (e.newState === "closed") merging = false;
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
  // "Count it as part of": one row that opens the list of apps right in
  // the menu; picking one does it (no separate dropdown and Move button).
  let merging = $state(false);
  function merge(into: { id: string; name: string }) {
    menu?.hidePopover();
    merging = false;
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
      <button type="button" role="menuitem" onclick={() => pick(c.minutes)}
        ><Glyph name={c.glyph} size={18} />{c.label}</button
      >
    {/each}
    {#if target.kind === "family"}
      <hr />
      {#each [["game", t("first.game"), "gaming"], ["never", t("first.never"), "ban"]] as const as [choice, label, glyph] (choice)}
        <button
          type="button"
          role="menuitemcheckbox"
          aria-checked={first === choice}
          onclick={() => choose(choice)}
        >
          <Glyph name={glyph} size={18} />
          <span class="lab">{label}</span>
          {#if first === choice}<span class="tick"><Glyph name="check" size={16} /></span>{/if}
        </button>
      {/each}
      <hr />
      {#if own}
        <button type="button" role="menuitem" onclick={back}><Glyph name="undo" size={18} />{t("group.back")}</button>
      {:else if programs.length > 1}
        {#each programs as exe (exe)}
          <button type="button" role="menuitem" onclick={() => split(exe)}>
            <Glyph name="unlink" size={18} /><span class="exe">{t("group.split", { exe })}</span>
          </button>
        {/each}
      {/if}
      {#if others.length > 0}
        <button
          type="button"
          role="menuitem"
          aria-expanded={merging}
          aria-controls="{uid}-merge"
          onclick={() => (merging = !merging)}
        >
          <Glyph name="merge" size={18} /><span class="lab">{t("group.merge")}</span><span
            class="chev"
            class:open={merging}><Glyph name="down" size={16} /></span
          >
        </button>
        {#if merging}
          <div class="apps" id="{uid}-merge" role="group" aria-label={t("group.merge")}>
            {#each others as o (o.id)}
              <button type="button" role="menuitem" onclick={() => merge(o)}>
                <AppIcon target={{ kind: "family", id: o.id }} name={o.name} size={18} /><span class="exe">{o.name}</span>
              </button>
            {/each}
          </div>
        {/if}
      {/if}
      <hr />
      <button type="button" role="menuitem" class="danger" onclick={blockIt}><Glyph name="ban" size={18} />{t("block")}</button>
    {/if}
  </div>
{/if}

<style>
  .b {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 12px 0 10px;
    border: 0;
    border-radius: 9px;
    background: var(--shell-soft);
    color: var(--shell-bright);
    font-size: 13px;
    font-weight: 700;
    white-space: nowrap;
    transition:
      background-color 120ms linear,
      color 120ms linear,
      transform 120ms var(--ease-out);
  }
  .b :global(.glyph) {
    stroke-width: 2.25;
  }
  .b:hover:not(:disabled) {
    background: var(--shell-soft-hover);
    color: var(--text-primary);
  }
  .b:active:not(:disabled) {
    transform: scale(0.96);
  }
  .b.live {
    background: var(--shell);
    color: var(--on-shell);
  }
  .b.live:hover:not(:disabled) {
    background: var(--shell-hover);
    color: var(--on-shell);
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
    display: flex;
    color: var(--shell-bright);
  }
  .chev {
    display: flex;
    color: var(--text-tertiary);
    transition: transform 200ms var(--ease-out);
  }
  .chev.open {
    transform: rotate(180deg);
  }
  .exe {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* The apps to count it as part of, tucked under their row. */
  .apps {
    display: flex;
    flex-direction: column;
    margin: 2px 0 2px 18px;
    padding-left: 8px;
    border-left: 1px solid var(--stroke-card);
  }
  .apps button {
    height: 34px;
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
