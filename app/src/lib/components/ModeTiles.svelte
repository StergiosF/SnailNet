<script lang="ts">
  // The four modes (PLAN.md Â§11.2) as big tiles: one click switches the
  // whole split. Native radio buttons underneath, so the arrow keys and
  // screen readers work as with any choice of one. The mode that's on is
  // cream, with its glyph on the shell's green; picking one spreads the
  // cream out from its glyph. The tiles arrive one after another.
  import { switchMode } from "../actions";
  import { t } from "../i18n";
  import { availableModes } from "../modes";
  import { service } from "../service.svelte";

  let { disabled }: { disabled: boolean } = $props();

  const modes = $derived(availableModes(service.snapshot?.config.profiles ?? []));
  const active = $derived(service.snapshot?.config.active_profile ?? "");
  const uid = $props.id();
  // While a game runs, Gaming stands in for the pick (Â§11.5); the pick
  // stays checked, since it's what comes back.
  const picked = $derived(modes.find((m) => m.id === active));
</script>

{#snippet glyph(id: string)}
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    {#if id === "balanced"}
      <path d="M4 9h16M4 15h16" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" fill="none" />
    {:else if id === "streaming"}
      <path d="M8 5.5v13l10.5-6.5z" fill="currentColor" />
    {:else if id === "gaming"}
      <path
        d="M7 8h10a4 4 0 0 1 3.9 4.9l-.8 3.3a2.3 2.3 0 0 1-4 .9L14.5 15h-5l-1.6 2.1a2.3 2.3 0 0 1-4-.9l-.8-3.3A4 4 0 0 1 7 8z"
        fill="currentColor"
      />
    {:else if id === "quiet-downloads"}
      <path
        d="M12 4v11m0 0-4.5-4.5M12 15l4.5-4.5M5 20h14"
        stroke="currentColor"
        stroke-width="2.2"
        stroke-linecap="round"
        stroke-linejoin="round"
        fill="none"
      />
    {:else}
      <!-- One of the user's own modes: three sliders. -->
      <path
        d="M4 6h9m4 0h3M4 12h3m4 0h9M4 18h11m4 0h1M15 4v4M9 10v4M17 16v4"
        stroke="currentColor"
        stroke-width="2.2"
        stroke-linecap="round"
        fill="none"
      />
    {/if}
  </svg>
{/snippet}

<div class="tiles" role="radiogroup" aria-label={t("mode.label")}>
  {#each modes as m, i (m.id)}
    <label class="tile" class:on={m.id === active} class:disabled style:--d="{60 + i * 50}ms">
      <input
        type="radio"
        name="{uid}-mode"
        value={m.id}
        checked={m.id === active}
        {disabled}
        onchange={() => void switchMode(m.id)}
      />
      <span class="fill" aria-hidden="true"></span>
      <span class="gl">{@render glyph(m.id)}</span>
      <b>{m.name}</b>
      <span class="body">{m.body}</span>
      {#if m.id === active}<span class="flag" aria-hidden="true">{t("mode.on")}</span>{/if}
      {#if m.id === "gaming" && service.playing}<span class="flag playing" aria-hidden="true"
          >{t("mode.playing")}</span
        >{/if}
    </label>
  {/each}
</div>
<!-- Always mounted, so screen readers hear it appear; empty, it takes no room. -->
<p class="note" class:shown={service.playing && picked} role="status">
  {#if service.playing && picked}{t("mode.playing.note", { mode: picked.name })}{/if}
</p>

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 12px;
  }
  .tile {
    position: relative;
    isolation: isolate;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-height: 132px;
    padding: 16px 16px 18px;
    border-radius: var(--radius-card);
    background: linear-gradient(180deg, rgb(255 255 255 / 0.03), transparent 70%), var(--surface-1);
    box-shadow: var(--shadow-card);
    overflow: hidden;
    transition:
      background-color 160ms linear,
      color 320ms linear,
      transform 260ms var(--ease-out),
      box-shadow 260ms var(--ease-out);
    animation: rise-in 520ms var(--d, 0ms) var(--ease-out) backwards;
  }
  .tile:hover:not(.on, .disabled) {
    background: linear-gradient(180deg, rgb(255 255 255 / 0.05), transparent 70%), var(--surface-2);
    transform: translateY(-2px);
    box-shadow: var(--shadow-raised);
  }
  .tile:active:not(.on, .disabled) {
    transform: translateY(0) scale(0.985);
  }
  /* The cream of the mode that's on, spreading from its glyph. */
  .fill {
    position: absolute;
    inset: 0;
    z-index: -1;
    background: linear-gradient(160deg, #f6f0df, var(--accent) 60%, #e6dcc4);
    clip-path: circle(0 at 34px 34px);
    transition: clip-path 560ms var(--ease-out);
  }
  .tile.on .fill {
    clip-path: circle(160% at 34px 34px);
  }
  .tile.disabled {
    opacity: 0.55;
  }
  input {
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
    cursor: inherit;
  }
  .tile:has(input:focus-visible) {
    outline: 2px solid var(--focus-outer);
    outline-offset: 2px;
  }
  .gl {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    margin-bottom: 6px;
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--text-primary);
    transition:
      background-color 240ms linear,
      color 240ms linear;
  }
  .tile.on .gl {
    animation: pop 480ms var(--ease-out);
  }
  @keyframes pop {
    from {
      transform: scale(0.82) rotate(-8deg);
    }
  }
  b {
    font-size: 16px;
    font-weight: 700;
  }
  .body {
    font-size: 13px;
    line-height: 1.35;
    color: var(--text-secondary);
  }
  .tile.on {
    color: var(--on-accent);
    box-shadow: var(--shadow-raised);
  }
  .tile.on .body {
    color: #2f4a43;
  }
  .tile.on .gl {
    background: var(--shell);
    color: var(--on-shell);
  }
  .flag {
    position: absolute;
    top: 14px;
    right: 14px;
    padding: 2px 8px;
    border-radius: 6px;
    background: var(--on-accent);
    color: var(--accent);
    font-size: 12px;
    font-weight: 800;
  }
  /* Gaming standing in while a game runs: outlined, not the pick's cream. */
  .flag.playing {
    background: transparent;
    color: var(--accent);
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .note {
    margin: 0;
    font-size: 13px;
    color: var(--text-secondary);
  }
  .note.shown {
    margin-top: 10px;
  }
  @media (max-width: 860px) {
    .tiles {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .tile {
      min-height: 0;
    }
  }
  @media (forced-colors: active) {
    .tile {
      border: 1px solid CanvasText;
    }
    .fill {
      display: none;
    }
    .tile.on {
      forced-color-adjust: none;
      background: Highlight;
      color: HighlightText;
    }
    .tile.on .body,
    .tile.on .gl {
      color: HighlightText;
    }
  }
</style>
