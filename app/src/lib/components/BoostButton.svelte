<script lang="ts">
  // Boost one app or site (PLAN.md §11.5): a button that opens a small menu
  // (for 1 hour, 4 hours, until I stop it), or "Stop boost" on the one
  // that's boosted. The menu is a native popover, so Escape and a click
  // outside close it; it opens under the button, kept inside the window.
  import { tick } from "svelte";
  import { boost, stopBoost } from "../actions";
  import { t } from "../i18n";
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

  /** Place the menu under the button, right edges lined up, and focus
   *  its first choice. */
  async function place(e: ToggleEvent) {
    if (e.newState !== "open" || !btn || !menu) return;
    const r = btn.getBoundingClientRect();
    const w = menu.offsetWidth;
    const h = menu.offsetHeight;
    const left = Math.max(8, Math.min(r.right - w, window.innerWidth - w - 8));
    const below = r.bottom + 6;
    const top = below + h > window.innerHeight - 8 ? Math.max(8, r.top - h - 6) : below;
    menu.style.left = `${left}px`;
    menu.style.top = `${top}px`;
    await tick();
    menu.querySelector<HTMLButtonElement>("button")?.focus();
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
  <div bind:this={menu} id="{uid}-menu" class="menu" popover="auto" role="menu" ontoggle={place}>
    <p class="title">{t("boost.menu.title", { name })}</p>
    <p class="body">{t("boost.menu.body")}</p>
    {#each choices as c (c.label)}
      <button type="button" role="menuitem" onclick={() => pick(c.minutes)}>{c.label}</button>
    {/each}
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
    transition: background-color 120ms linear;
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
  .menu {
    position: fixed;
    inset: auto;
    width: 260px;
    margin: 0;
    padding: 12px 6px 6px;
    border: 0;
    border-radius: 14px;
    background: var(--surface-2);
    color: var(--text-primary);
    box-shadow:
      0 0 0 1px var(--stroke-card),
      0 16px 40px rgb(0 0 0 / 0.45);
  }
  .menu:popover-open {
    animation: rise 140ms var(--ease-out);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }
  .title {
    margin: 0 10px;
    font-weight: 700;
  }
  .body {
    margin: 4px 10px 8px;
    font-size: 13px;
    line-height: 1.4;
    color: var(--text-secondary);
  }
  .menu button {
    display: block;
    width: 100%;
    height: 38px;
    padding: 0 10px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    text-align: left;
    font-size: 14px;
  }
  .menu button:hover,
  .menu button:focus-visible {
    background: var(--surface-3);
    outline-offset: -2px;
  }
  @media (forced-colors: active) {
    .b,
    .menu {
      border: 1px solid ButtonText;
    }
    .b.live {
      forced-color-adjust: none;
      background: Highlight;
      color: HighlightText;
    }
  }
</style>
