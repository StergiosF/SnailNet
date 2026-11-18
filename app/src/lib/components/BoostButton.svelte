<script lang="ts">
  // Boost one app or site (PLAN.md §11.5): a button that opens a small menu
  // (for 1 hour, 4 hours, until I stop it; and for an app, block its
  // internet), or "Stop boost" on the one that's boosted. The menu is a
  // native popover, so Escape and a click outside close it.
  import { block, boost, setFirstChoice, stopBoost } from "../actions";
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
    aria-label={t("boost.label", { name })}>{t("boost")}</button
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
          <i class="icon" aria-hidden="true">{first === choice ? "" : ""}</i>{label}
        </button>
      {/each}
      <hr />
      <button type="button" role="menuitem" class="danger" onclick={blockIt}>
        <i class="icon" aria-hidden="true">&#xE8F8;</i>{t("block")}
      </button>
    {/if}
  </div>
{/if}

<style>
  .b {
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
  .b.live:hover:not(:disabled) {
    background: var(--shell-hover);
  }
  .b:disabled {
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
