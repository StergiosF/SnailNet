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


  let { disabled }: { disabled: boolean } = $props();

  const modes = $derived(availableModes(service.snapshot?.config.profiles ?? []));
  const active = $derived(service.snapshot?.config.active_profile ?? "");
  const picked = $derived(modes.find((m) => m.id === active));
  const uid = $props.id();
</script>

<!-- The modes' own marks, bold and filled as on the first mode tiles (the
     user liked those better): an equals sign, a play arrow, a pad, an
     arrow down to a tray; the user's own modes three sliders. -->
{#snippet mark(id: string)}
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    {#if id === "balanced"}
      <path d="M5 9h14M5 15h14" class="st" />
    {:else if id === "streaming"}
      <path d="M8 5.5v13l10.5-6.5z" class="fl" />
    {:else if id === "gaming"}
      <path
        d="M7 8h10a4 4 0 0 1 3.9 4.9l-.8 3.3a2.3 2.3 0 0 1-4 .9L14.5 15h-5l-1.6 2.1a2.3 2.3 0 0 1-4-.9l-.8-3.3A4 4 0 0 1 7 8z"
        class="fl"
      />
    {:else if id === "quiet-downloads"}
      <path d="M12 4v10m0 0-4-4m4 4 4-4M5 19.5h14" class="st" />
    {:else}
      <path d="M4 6h9m4 0h3M4 12h3m4 0h9M4 18h11m4 0h1M15 4v4M9 10v4M17 16v4" class="st" />
    {/if}
  </svg>
{/snippet}

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
        <span class="fill" aria-hidden="true"></span>
        <span class="gl">{@render mark(m.id)}</span>
        <span class="nm">{m.name}</span>
        {#if m.id === "gaming" && service.playing}
          <span class="playing">{t("mode.playing")}</span>
        {/if}
      </label>
    {/each}
  </div>
  <!-- Outside the labels, so each mode's name stays just its name. -->
  {#each modes as m (m.id)}
    <span class="visually-hidden" id="{uid}-{m.id}">{m.body}</span>
  {/each}
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
    padding: 4px;
    overflow-x: auto;
    scrollbar-width: none;
    border-radius: 14px;
    background: rgb(0 0 0 / 0.22);
    box-shadow: inset 0 0 0 1px var(--stroke-card);
  }
  .seg {
    position: relative;
    isolation: isolate;
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 10px;
    height: 44px;
    padding: 0 16px 0 7px;
    overflow: hidden;
    border-radius: 11px;
    color: var(--text-secondary);
    font-size: 15px;
    font-weight: 700;
    white-space: nowrap;
    transition:
      background-color 160ms linear,
      color 240ms linear,
      transform 120ms var(--ease-out);
  }
  .seg input {
    position: absolute;
    inset: 0;
    z-index: 1;
    margin: 0;
    opacity: 0;
    cursor: inherit;
  }
  /* The cream of the mode that's on, spreading out from its mark. */
  .fill {
    position: absolute;
    inset: 0;
    z-index: -1;
    background: linear-gradient(160deg, #f6f0df, var(--accent) 60%, #e6dcc4);
    clip-path: circle(0 at 22px 50%);
    transition: clip-path 480ms var(--ease-out);
  }
  .seg.on .fill {
    clip-path: circle(150% at 22px 50%);
  }
  .gl {
    display: grid;
    flex: none;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    background: rgb(255 255 255 / 0.06);
    color: var(--text-secondary);
    transition:
      background-color 240ms linear,
      color 240ms linear;
  }
  .gl .st {
    fill: none;
    stroke: currentColor;
    stroke-width: 2.4;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .gl .fl {
    fill: currentColor;
  }
  .seg:hover:not(.on, .disabled) {
    background: var(--hover-soft);
    color: var(--text-primary);
  }
  .seg:hover:not(.on, .disabled) .gl {
    color: var(--text-primary);
  }
  .seg:active:not(.on, .disabled) {
    transform: scale(0.97);
  }
  .seg.on {
    color: var(--on-accent);
    box-shadow: 0 6px 16px -8px rgb(0 0 0 / 0.6);
  }
  .seg.on .gl {
    background: var(--shell);
    color: var(--on-shell);
    animation: pop 420ms var(--ease-out);
  }
  @keyframes pop {
    from {
      transform: scale(0.8) rotate(-8deg);
    }
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
  /* A narrow window: the modes that aren't on show their glyph only (the
     name stays for screen readers and in the tooltip). */
  @media (max-width: 1000px) {
    .k {
      display: none;
    }
    .seg:not(.on) .nm {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
      white-space: nowrap;
    }
    .seg:not(.on) {
      padding: 0 7px;
    }
  }
  @media (forced-colors: active) {
    .fill {
      display: none;
    }
    .seg.on {
      forced-color-adjust: none;
      background: Highlight;
      color: HighlightText;
    }
  }
</style>
