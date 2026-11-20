<script lang="ts">
  // An app's ••• menu on the Now page: everything about it that isn't a
  // boost, so it's there whether the app is boosted or not.
  //   - Give it a share of its own (opens Split with it added, unsaved);
  //   - for an app: it's a game / never let it go first (§11.5), how it's
  //     grouped: split a program out, put it back, count it as part of
  //     another app (§11.6), and block its internet.
  // A native popover, so Escape and a click outside close it.
  import { block, mergeInto, putBack, setFirstChoice, splitOut } from "../actions";
  import AppIcon from "./AppIcon.svelte";
  import Glyph from "./Glyph.svelte";
  import { members, splitExe } from "../families";
  import { firstChoiceOf, type FirstChoice } from "../firstChoice";
  import { t } from "../i18n";
  import { placeMenu } from "../popover";
  import { addRule, sameTarget, setEnabled } from "../rules";
  import { service } from "../service.svelte";
  import type { Target } from "../types";
  import { ui } from "../ui.svelte";

  let { target, name, disabled }: { target: Target; name: string; disabled: boolean } = $props();

  const uid = $props.id();
  let btn = $state<HTMLButtonElement | null>(null);
  let menu = $state<HTMLDivElement | null>(null);

  function place(e: ToggleEvent) {
    if (e.newState === "open" && btn && menu) placeMenu(btn, menu);
    if (e.newState === "closed") merging = false;
  }
  const close = () => menu?.hidePopover();

  // A share of its own, unless it already has one that's on.
  const hasShare = $derived(ui.rules.some((r) => r.enabled && r.share > 0 && sameTarget(r.target, target)));
  function give() {
    close();
    const rules = ui.rules;
    const i = rules.findIndex((r) => sameTarget(r.target, target));
    ui.edit(i >= 0 ? setEnabled(rules, i, true) : addRule(rules, target));
    ui.open = `${target.kind}:${target.id.toLowerCase()}`;
    ui.go("split");
  }

  // Whether this app goes first (§11.5): a game, never, or SnailNet's call.
  // Picking the ticked one again goes back to SnailNet's call.
  const first = $derived(firstChoiceOf(service.snapshot?.config.settings, target.id));
  function choose(choice: FirstChoice) {
    close();
    void setFirstChoice(target.id, name, first === choice ? "auto" : choice);
  }

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
  // "Count it as part of": one row that opens the list of apps right in
  // the menu; picking one does it.
  let merging = $state(false);
  function merge(into: { id: string; name: string }) {
    close();
    merging = false;
    void mergeInto({ id: target.id, name }, into);
  }
</script>

<button
  bind:this={btn}
  type="button"
  class="more"
  {disabled}
  popovertarget="{uid}-menu"
  aria-haspopup="menu"
  aria-label={t("more.label", { name })}><Glyph name="more" size={18} /></button
>
<div bind:this={menu} id="{uid}-menu" class="popmenu" popover="auto" role="menu" ontoggle={place}>
  <p class="title">{name}</p>
  {#if !hasShare}
    <hr />
    <button type="button" role="menuitem" onclick={give}><Glyph name="split" size={18} />{t("menu.give")}</button>
  {/if}
  {#if target.kind === "family"}
    <hr />
    {#each [["game", t("first.game"), "gaming"], ["never", t("first.never"), "flag"]] as const as [choice, label, glyph] (choice)}
      <button type="button" role="menuitemcheckbox" aria-checked={first === choice} onclick={() => choose(choice)}>
        <Glyph name={glyph} size={18} />
        <span class="lab">{label}</span>
        {#if first === choice}<span class="tick"><Glyph name="check" size={16} /></span>{/if}
      </button>
    {/each}
    {#if own || programs.length > 1 || others.length > 0}
      <hr />
    {/if}
    {#if own}
      <button type="button" role="menuitem" onclick={() => (close(), void putBack(own))}
        ><Glyph name="undo" size={18} />{t("group.back")}</button
      >
    {:else if programs.length > 1}
      {#each programs as exe (exe)}
        <button type="button" role="menuitem" onclick={() => (close(), void splitOut(exe, name))}>
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
        <Glyph name="merge" size={18} /><span class="lab">{t("group.merge")}</span><span class="chev" class:open={merging}
          ><Glyph name="down" size={16} /></span
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
    <button type="button" role="menuitem" class="danger" onclick={() => (close(), void block(target.id, name))}
      ><Glyph name="ban" size={18} />{t("block")}</button
    >
  {/if}
</div>

<style>
  .more {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: var(--text-tertiary);
    transition:
      background-color 120ms linear,
      color 120ms linear;
  }
  .more:hover:not(:disabled),
  .more:has(+ .popmenu:popover-open) {
    background: var(--surface-3);
    color: var(--text-primary);
  }
  .more:disabled {
    opacity: 0.5;
  }
  .popmenu .title {
    margin-bottom: 6px;
  }
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
    .more {
      border: 1px solid ButtonText;
    }
  }
</style>
