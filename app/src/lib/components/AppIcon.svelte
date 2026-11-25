<script lang="ts">
  // What an app, a website or Everything else looks like in the list: the
  // program's own icon (icon.rs); else the brand's real logo (brands/,
  // Simple Icons) for sites and known apps; else a neutral app tile for
  // apps (a random letter read as junk, 2026-11-25) or a letter tile in
  // the site's colour for sites. Windows' own traffic gets a PC glyph,
  // Everything else four squares. Decorative: the name is always written
  // beside it.
  import { brandOf } from "../brands";
  import { icons } from "../icons.svelte";
  import { service } from "../service.svelte";
  import type { Target } from "../types";
  import Glyph from "./Glyph.svelte";

  let {
    target,
    name,
    size = 24,
    boxed = false,
  }: { target: Target | null; name: string; size?: number; boxed?: boolean } = $props();

  /** Website tiles: letters and colours. Unknown sites get stone. */
  const SITES: Record<string, { text: string; bg: string; fg: string }> = {
    youtube: { text: "▶", bg: "#ff0033", fg: "#ffffff" },
    twitch: { text: "T", bg: "#9146ff", fg: "#ffffff" },
    netflix: { text: "N", bg: "#141414", fg: "#e50914" },
    "disney-plus": { text: "D+", bg: "#113ccf", fg: "#ffffff" },
    "prime-video": { text: "P", bg: "#1f2e3d", fg: "#1a98ff" },
    spotify: { text: "S", bg: "#1ed760", fg: "#121212" },
    kick: { text: "K", bg: "#53fc18", fg: "#0b0e0f" },
    zoom: { text: "Z", bg: "#0b5cff", fg: "#ffffff" },
    "google-meet": { text: "M", bg: "#00832d", fg: "#ffffff" },
    teams: { text: "T", bg: "#5059c9", fg: "#ffffff" },
  };

  const url = $derived(target ? icons.forTarget(target, service.snapshot?.apps ?? []) : null);
  const brand = $derived(target ? brandOf(target) : null);
  /** Windows' own traffic: the system, its services, Windows Update
   *  (snailnet-core classify/family.rs). */
  const WINDOWS = new Set(["system", "windows-services", "windows-update"]);
  const system = $derived(target?.kind === "family" && WINDOWS.has(target.id.toLowerCase()));
  const site = $derived(
    target?.kind === "site"
      ? (SITES[target.id.toLowerCase()] ?? { text: name.charAt(0).toUpperCase(), bg: "var(--surface-3)", fg: "var(--text-secondary)" })
      : null,
  );
</script>

<span class="wrap" class:boxed style:--s="{size}px" aria-hidden="true">
  {#if target === null}
    <svg class="glyph" viewBox="0 0 20 20" width={size * 0.8} height={size * 0.8}>
      <g fill="none" stroke="currentColor" stroke-width="1.5">
        <rect x="2.5" y="2.5" width="6" height="6" rx="1.5" />
        <rect x="11.5" y="2.5" width="6" height="6" rx="1.5" />
        <rect x="2.5" y="11.5" width="6" height="6" rx="1.5" />
        <rect x="11.5" y="11.5" width="6" height="6" rx="1.5" />
      </g>
    </svg>
  {:else if url}
    <img src={url} alt="" width={size} height={size} />
  {:else if brand && boxed}
    <span class="solid" style:background={brand.bg}>
      <svg class="logo" viewBox="0 0 24 24" width={size * 0.78} height={size * 0.78}>
        <path d={brand.path} fill={brand.mark} />
      </svg>
    </span>
  {:else if brand}
    <svg class="logo" viewBox="0 0 24 24" width={size * 0.86} height={size * 0.86}>
      <path d={brand.path} fill={brand.ink} />
    </svg>
  {:else if system && boxed}
    <span class="solid win"><Glyph name="window" size={Math.round(size * 0.7)} /></span>
  {:else if system}
    <span class="sys"><Glyph name="window" size={Math.round(size * 0.8)} /></span>
  {:else if site}
    <span class="tile" style:background={site.bg} style:color={site.fg} class:wide={site.text.length > 1}>{site.text}</span>
  {:else}
    <span class="tile plain"><Glyph name="window" size={Math.round(size * 0.62)} /></span>
  {/if}
</span>

<style>
  .wrap {
    display: inline-grid;
    place-items: center;
    flex: none;
    width: var(--s);
    height: var(--s);
    color: var(--text-secondary);
  }
  .wrap.boxed {
    width: calc(var(--s) + 16px);
    height: calc(var(--s) + 16px);
    border-radius: var(--radius-control);
    background: var(--surface-2);
  }
  img,
  .logo {
    display: block;
  }
  /* A brand's own app-icon tile: it fills the box, so the box's soft
     surface doesn't show round it as a ghost square. */
  .solid {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    border-radius: inherit;
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.12);
  }
  .solid.win {
    background: #0067c0;
    color: #ffffff;
  }
  .sys {
    display: flex;
    color: var(--text-secondary);
  }
  .tile {
    display: grid;
    place-items: center;
    width: var(--s);
    height: var(--s);
    border-radius: calc(var(--s) * 0.24);
    font-size: calc(var(--s) * 0.55);
    font-weight: 700;
    line-height: 1;
  }
  .boxed .tile {
    width: 100%;
    height: 100%;
    border-radius: inherit;
  }
  .tile.wide {
    font-size: calc(var(--s) * 0.42);
    letter-spacing: -0.04em;
  }
  .tile.plain {
    background: var(--surface-3);
    color: var(--text-secondary);
  }
  @media (forced-colors: active) {
    .logo path {
      fill: CanvasText;
    }
    .tile {
      forced-color-adjust: none;
    }
    .solid {
      background: Canvas !important;
      outline: 1px solid CanvasText;
    }
    .solid .logo path {
      fill: CanvasText;
    }
  }
</style>
