<script lang="ts">
  // Pause (PLAN.md §11.8), at the foot of the side rail: a quiet item that
  // opens a menu (15 minutes, 1 hour, until I resume). While paused it
  // becomes Resume, with the time left under it.
  import { pause, resume } from "../actions";
  import Glyph from "./Glyph.svelte";
  import { t } from "../i18n";
  import { timeLeft } from "../modes";
  import { placeMenu } from "../popover";
  import { service } from "../service.svelte";
  import { ui } from "../ui.svelte";

  let { disabled }: { disabled: boolean } = $props();

  const uid = $props.id();
  let btn = $state<HTMLButtonElement | null>(null);
  let menu = $state<HTMLDivElement | null>(null);

  const paused = $derived.by(() => {
    const p = service.snapshot?.config.pause;
    if (!p) return null;
    return p.until === undefined || ui.now < p.until ? p : null;
  });

  const choices: { minutes: number | null; label: string; glyph: "clock" | "infinity" }[] = [
    { minutes: 15, label: t("pause.15m"), glyph: "clock" },
    { minutes: 60, label: t("pause.1h"), glyph: "clock" },
    { minutes: null, label: t("pause.forever"), glyph: "infinity" },
  ];

  function place(e: ToggleEvent) {
    if (e.newState === "open" && btn && menu) placeMenu(btn, menu, "above", "start");
  }

  function pick(minutes: number | null) {
    menu?.hidePopover();
    void pause(minutes);
  }
</script>

{#if paused}
  <button type="button" class="ctl resume" {disabled} onclick={() => void resume()}>
    <Glyph name="play" size={18} />
    <span class="txt">
      <b>{t("pause.resume")}</b>
      <small>{timeLeft(paused.until, ui.now)}</small>
    </span>
  </button>
{:else}
  <button
    bind:this={btn}
    type="button"
    class="ctl"
    {disabled}
    popovertarget="{uid}-menu"
    aria-haspopup="menu"
    aria-label={t("pause.label")}
  >
    <Glyph name="pause" size={18} />
    <span class="txt"><b>{t("pause")}</b></span>
  </button>
  <div bind:this={menu} id="{uid}-menu" class="popmenu" popover="auto" role="menu" ontoggle={place}>
    <p class="title">{t("pause.label")}</p>
    <p class="body">{t("pause.menu.body")}</p>
    {#each choices as c (c.label)}
      <button type="button" role="menuitem" onclick={() => pick(c.minutes)}
        ><Glyph name={c.glyph} size={18} />{c.label}</button
      >
    {/each}
  </div>
{/if}

<style>
  /* A quiet rail item like the pages above it; Resume says it in amber. */
  .ctl {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 40px;
    padding: 4px 10px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    text-align: left;
    transition:
      background-color 140ms linear,
      color 140ms linear;
  }
  .ctl:hover:not(:disabled) {
    background: var(--surface-1);
    color: var(--text-primary);
  }
  .ctl:disabled {
    opacity: 0.5;
  }
  .txt {
    display: flex;
    flex-direction: column;
  }
  b {
    font-size: 15px;
    font-weight: 600;
  }
  small {
    font-size: 12.5px;
    color: var(--text-secondary);
  }
  .resume {
    background: color-mix(in srgb, var(--caution) 12%, transparent);
    color: var(--caution);
  }
  .resume:hover:not(:disabled) {
    background: color-mix(in srgb, var(--caution) 20%, transparent);
    color: var(--caution);
  }
  @media (forced-colors: active) {
    .ctl {
      border: 1px solid ButtonText;
    }
  }
</style>
