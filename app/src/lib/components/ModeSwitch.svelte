<script lang="ts">
  // The mode switch (DESIGN.md): the app's state, so it sits in the
  // window's top strip on every page rather than inside one. A compact
  // segmented choice, every mode always named (a narrow window drops the
  // glyphs, never the names); the mode that's on is ivory. Native radios
  // underneath, so the arrow keys and screen readers work as with any
  // choice of one; each mode's one line is its tooltip and its
  // description. While a game runs, Gaming stands in for the pick (§11.5):
  // the pick stays checked (it's what comes back) and a quiet note beside
  // the switch says Playing.
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

<div class="switch" data-tauri-drag-region>
  <div class="track" role="radiogroup" aria-label={t("mode.label")}>
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
        <span class="gl"><Glyph name={modeGlyph(m.id)} size={16} /></span>
        <span class="nm">{m.name}</span>
      </label>
    {/each}
  </div>
  <!-- Outside the labels, so each mode's name stays just its name. -->
  {#each modes as m (m.id)}
    <span class="visually-hidden" id="{uid}-{m.id}">{m.body}</span>
  {/each}
  <p class="note" role="status" data-tauri-drag-region>
    {#if service.playing && picked}
      <span class="dot" aria-hidden="true"></span>
      <span title={t("mode.playing.note", { mode: picked.name })}>{t("mode.playing")}</span>
      <span class="visually-hidden">{t("mode.playing.note", { mode: picked.name })}</span>
    {/if}
  </p>
</div>

<style>
  .switch {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .track {
    display: flex;
    gap: 2px;
    min-width: 0;
    padding: 3px;
    overflow-x: auto;
    scrollbar-width: none;
    border-radius: 11px;
    background: var(--surface-side);
    box-shadow: inset 0 0 0 1px var(--stroke-card);
  }
  .seg {
    position: relative;
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 7px;
    height: 28px;
    padding: 0 11px;
    border-radius: 8px;
    color: var(--text-secondary);
    font-size: 13.5px;
    font-weight: 650;
    white-space: nowrap;
    transition:
      background-color 200ms var(--ease-out),
      color 200ms var(--ease-out);
  }
  .seg input {
    position: absolute;
    inset: 0;
    z-index: 1;
    margin: 0;
    opacity: 0;
    cursor: inherit;
  }
  .gl {
    display: flex;
  }
  .seg:hover:not(.on, .disabled) {
    background: var(--surface-1);
    color: var(--text-primary);
  }
  .seg.on {
    background: var(--accent);
    color: var(--on-accent);
  }
  /* The snail's jade, deep enough to read on ivory (5.1:1). */
  .seg.on .gl {
    color: #13785f;
  }
  .seg.disabled {
    opacity: 0.55;
  }
  .seg:has(input:focus-visible) {
    outline: 2px solid var(--focus-outer);
    outline-offset: 2px;
  }
  .note {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: 13px;
    color: var(--text-secondary);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    width: 7px;
    height: 7px;
    flex: none;
    border-radius: 50%;
    background: var(--shell-bright);
  }
  /* A narrow window: names stay, the glyphs step aside. */
  @media (max-width: 1000px) {
    .gl {
      display: none;
    }
    .seg {
      padding: 0 9px;
    }
  }
  @media (forced-colors: active) {
    .track {
      border: 1px solid CanvasText;
    }
    .seg.on {
      forced-color-adjust: none;
      background: Highlight;
      color: HighlightText;
    }
    .seg.on .gl {
      color: HighlightText;
    }
  }
</style>
