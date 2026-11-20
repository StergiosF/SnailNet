<script lang="ts">
  // The mode switch (DESIGN.md, Now and Split): the app's state, so it sits
  // in the window's top strip above both pages rather than in either one.
  // One segment per mode with its drawn glyph; the mode that's on is cream.
  // Native radios underneath, so the arrow keys and screen readers work as
  // with any choice of one; each mode's one line is its tooltip and its
  // description. While a game runs, Gaming stands in for the pick (§11.5):
  // the pick stays checked (it's what comes back) and Gaming says Playing.
  import { switchMode } from "../actions";
  import { t } from "../i18n";
  import { availableModes } from "../modes";
  import { service } from "../service.svelte";
  import Glyph, { modeGlyph } from "./Glyph.svelte";

  let { disabled }: { disabled: boolean } = $props();

  const modes = $derived(availableModes(service.snapshot?.config.profiles ?? []));
  const active = $derived(service.snapshot?.config.active_profile ?? "");
  const picked = $derived(modes.find((m) => m.id === active));
  const uid = $props.id();
</script>

<div class="switch" role="radiogroup" aria-label={t("mode.label")} data-tauri-drag-region>
  <span class="k" data-tauri-drag-region>{t("mode.label")}</span>
  <div class="track">
    {#each modes as m (m.id)}
      <label class="seg" class:on={m.id === active} class:disabled title={m.body}>
        <input
          type="radio"
          name="{uid}-mode"
          value={m.id}
          checked={m.id === active}
          {disabled}
          aria-describedby="{uid}-{m.id}"
          onchange={() => void switchMode(m.id)}
        />
        <Glyph name={modeGlyph(m.id)} size={17} />
        <span class="nm">{m.name}</span>
        {#if m.id === "gaming" && service.playing}
          <span class="playing">{t("mode.playing")}</span>
        {/if}
        <span class="visually-hidden" id="{uid}-{m.id}">{m.body}</span>
      </label>
    {/each}
  </div>
  <p class="note" role="status" data-tauri-drag-region>
    {#if service.playing && picked}{t("mode.playing.note", { mode: picked.name })}{/if}
  </p>
</div>

<style>
  .switch {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .k {
    font-size: 13px;
    font-weight: 700;
    color: var(--text-secondary);
  }
  .track {
    display: flex;
    gap: 2px;
    min-width: 0;
    padding: 3px;
    overflow-x: auto;
    scrollbar-width: none;
    border-radius: 12px;
    background: rgb(0 0 0 / 0.22);
    box-shadow: inset 0 0 0 1px var(--stroke-card);
  }
  .seg {
    position: relative;
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 7px;
    height: 34px;
    padding: 0 13px 0 11px;
    border-radius: 9px;
    color: var(--text-secondary);
    font-size: 14px;
    font-weight: 700;
    white-space: nowrap;
    transition:
      background-color 220ms var(--ease-out),
      color 220ms linear,
      transform 120ms var(--ease-out);
  }
  .seg input {
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
    cursor: inherit;
  }
  .seg:hover:not(.on, .disabled) {
    background: rgb(255 255 255 / 0.06);
    color: var(--text-primary);
  }
  .seg:active:not(.on, .disabled) {
    transform: scale(0.97);
  }
  .seg.on {
    background: var(--accent);
    color: var(--on-accent);
    box-shadow: 0 6px 16px -8px rgb(0 0 0 / 0.6);
  }
  .seg.disabled {
    opacity: 0.55;
  }
  .seg:has(input:focus-visible) {
    outline: 2px solid var(--focus-outer);
    outline-offset: 2px;
  }
  .playing {
    padding: 1px 6px;
    border-radius: 5px;
    background: var(--shell);
    color: var(--on-shell);
    font-size: 11.5px;
    font-weight: 800;
  }
  .note {
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: 13px;
    color: var(--text-secondary);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  @media (forced-colors: active) {
    .seg.on {
      forced-color-adjust: none;
      background: Highlight;
      color: HighlightText;
    }
  }
</style>
