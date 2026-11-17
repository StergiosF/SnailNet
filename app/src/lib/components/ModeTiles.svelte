<script lang="ts">
  // The four modes (PLAN.md §11.2) as big tiles: one click switches the
  // whole split. Native radio buttons underneath, so the arrow keys and
  // screen readers work as with any choice of one. The mode that's on is
  // cream, with its glyph on the shell's green.
  import { switchMode } from "../actions";
  import { t } from "../i18n";
  import { availableModes, type ModeId } from "../modes";
  import { service } from "../service.svelte";

  let { disabled }: { disabled: boolean } = $props();

  const modes = $derived(availableModes(service.snapshot?.config.profiles ?? []));
  const active = $derived(service.snapshot?.config.active_profile ?? "");
  const uid = $props.id();
</script>

{#snippet glyph(id: ModeId)}
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
    {:else}
      <path
        d="M12 4v11m0 0-4.5-4.5M12 15l4.5-4.5M5 20h14"
        stroke="currentColor"
        stroke-width="2.2"
        stroke-linecap="round"
        stroke-linejoin="round"
        fill="none"
      />
    {/if}
  </svg>
{/snippet}

<div class="tiles" role="radiogroup" aria-label={t("mode.label")}>
  {#each modes as m (m.id)}
    <label class="tile" class:on={m.id === active} class:disabled>
      <input
        type="radio"
        name="{uid}-mode"
        value={m.id}
        checked={m.id === active}
        {disabled}
        onchange={() => void switchMode(m.id)}
      />
      <span class="gl">{@render glyph(m.id)}</span>
      <b>{t(m.name)}</b>
      <span class="body">{t(m.body)}</span>
      {#if m.id === active}<span class="flag" aria-hidden="true">{t("mode.on")}</span>{/if}
    </label>
  {/each}
</div>

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 12px;
  }
  .tile {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-height: 124px;
    padding: 14px 16px 16px;
    border-radius: var(--radius-card);
    background: var(--surface-1);
    transition:
      background-color 160ms linear,
      color 160ms linear,
      transform 160ms var(--ease-out);
  }
  .tile:hover:not(.on, .disabled) {
    background: var(--surface-2);
  }
  .tile:active:not(.on, .disabled) {
    transform: translateY(1px);
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
    background: var(--accent);
    color: var(--on-accent);
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
