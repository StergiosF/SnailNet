<script lang="ts">
  // Boost one app or site (PLAN.md §11.5): a quiet text button that opens a
  // small menu (for 1 hour, 4 hours, until I stop it), or "Stop" on the one
  // that's boosted. Only boosting lives here; everything else about the app
  // (a share of its own, game or never first, grouping, blocking) is in its
  // own ••• menu (AppMenu), so it stays reachable while the app is boosted.
  // The menu is a native popover, so Escape and a click outside close it.
  import { boost, stopBoost } from "../actions";
  import Glyph, { type GlyphName } from "./Glyph.svelte";
  import { t } from "../i18n";
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
  }

  function pick(minutes: number | null) {
    menu?.hidePopover();
    void boost(target, minutes);
  }
</script>

{#if boosted}
  <button
    type="button"
    class="b live"
    {disabled}
    aria-label={t("boost.stop.label", { name })}
    onclick={() => void stopBoost()}><Glyph name="boost" size={17} />{t("boost.stop.short")}</button
  >
{:else}
  <button
    bind:this={btn}
    type="button"
    class="b"
    {disabled}
    popovertarget="{uid}-menu"
    aria-haspopup="menu"
    aria-label={t("boost.label", { name })}><Glyph name="boost" size={17} />{t("boost")}</button
  >
  <div bind:this={menu} id="{uid}-menu" class="popmenu" popover="auto" role="menu" ontoggle={place}>
    <p class="title">{t("boost.menu.title", { name })}</p>
    <p class="body">{t("boost.menu.body")}</p>
    {#each choices as c (c.label)}
      <button type="button" role="menuitem" onclick={() => pick(c.minutes)}
        ><Glyph name={c.glyph} size={18} />{c.label}</button
      >
    {/each}
  </div>
{/if}

<style>
  /* Quiet until pointed at: the boost glyph and a word, the jade only on
     hover and while it's on. A list of apps is never a wall of green. */
  .b {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 11px 0 9px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 13.5px;
    font-weight: 700;
    white-space: nowrap;
    transition:
      background-color 120ms linear,
      color 120ms linear,
      transform 120ms var(--ease-out);
  }
  .b:hover:not(:disabled),
  .b:has(+ .popmenu:popover-open) {
    background: var(--shell-soft);
    color: var(--shell-bright);
  }
  .b:active:not(:disabled) {
    transform: scale(0.96);
  }
  .b.live {
    color: var(--shell-bright);
  }
  .b.live:hover:not(:disabled) {
    background: var(--shell-soft-hover);
    color: var(--text-primary);
  }
  .b:disabled {
    opacity: 0.5;
  }
  @media (forced-colors: active) {
    .b {
      border: 1px solid ButtonText;
    }
  }
</style>
