<script lang="ts" module>
  /** The glyphs SnailNet draws itself (DESIGN.md, Icons). */
  export type GlyphName =
    | "now"
    | "split"
    | "settings"
    | "pause"
    | "play"
    | "boost"
    | "balanced"
    | "streaming"
    | "gaming"
    | "quiet-downloads"
    | "own-mode"
    | "plus"
    | "minus"
    | "chevron"
    | "down"
    | "free"
    | "ping"
    | "copy"
    | "reset"
    | "line"
    | "apps"
    | "power"
    | "info"
    | "clock"
    | "infinity"
    | "ban"
    | "unlink"
    | "undo"
    | "merge"
    | "gauge"
    | "phone"
    | "window"
    | "bulb"
    | "flag"
    | "keyboard"
    | "download"
    | "upload"
    | "list"
    | "trash"
    | "check"
    | "pen"
    | "auto"
    | "more"
    | "search"
    | "close";

  /** A mode's glyph: the four built-in modes have their own, the user's
   *  own modes share one. */
  export function modeGlyph(id: string): GlyphName {
    return id === "balanced" || id === "streaming" || id === "gaming" || id === "quiet-downloads" ? id : "own-mode";
  }
</script>

<script lang="ts">
  // SnailNet's own icon family (DESIGN.md, Icons): drawn on a 24 box with
  // one stroke weight and round ends, in the current text colour. The
  // shell's spiral is Now, a bar cut in three is Split, two chevrons are
  // Boost ("goes first"). Decorative: the control around it carries the
  // name.
  let { name, size = 20 }: { name: GlyphName; size?: number } = $props();
</script>

<svg class="glyph" viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" focusable="false">
  {#if name === "now"}
    <path d="M10 12a2 2 0 1 1 4 0a4 4 0 1 1-8 0a6 6 0 1 1 12 0a8 8 0 0 1-8 8" />
  {:else if name === "split"}
    <rect x="3" y="7" width="18" height="10" rx="3" /><path d="M10 7v10M15 7v10" />
  {:else if name === "settings"}
    <path d="M4 8h5M13 8h7M4 16h9M17 16h3" /><circle cx="11" cy="8" r="2" /><circle cx="15" cy="16" r="2" />
  {:else if name === "pause"}
    <path d="M9 6v12M15 6v12" />
  {:else if name === "play"}
    <path d="M8 6.5v11l9-5.5z" />
  {:else if name === "boost"}
    <path d="M7 12.5 12 7.5l5 5M7 17.5l5-5 5 5" />
  {:else if name === "balanced"}
    <circle cx="12" cy="12" r="8.5" /><path d="M12 12V3.5M12 12l7.4 4.25M12 12l-7.4 4.25" />
  {:else if name === "streaming"}
    <rect x="3" y="5" width="18" height="13" rx="3" /><path d="M10.5 9.25v5.5l4.25-2.75z" /><path d="M9 21h6" />
  {:else if name === "gaming"}
    <path
      d="M7.5 8h9a4 4 0 0 1 3.9 3.1l.8 3.7a2.2 2.2 0 0 1-3.8 1.9L15.6 15H8.4l-1.8 1.7a2.2 2.2 0 0 1-3.8-1.9l.8-3.7A4 4 0 0 1 7.5 8z"
    /><path d="M8.5 10.5v3M7 12h3" /><circle class="dot" cx="15.5" cy="11" r="1" /><circle
      class="dot"
      cx="17.25"
      cy="13"
      r="1"
    />
  {:else if name === "quiet-downloads"}
    <path d="M12 4v9" stroke-dasharray="1.5 2.6" /><path d="M8.5 10.5 12 14l3.5-3.5" /><path
      d="M4.5 15.5v2a2.5 2.5 0 0 0 2.5 2.5h10a2.5 2.5 0 0 0 2.5-2.5v-2"
    />
  {:else if name === "own-mode"}
    <path d="M4 6h9M17 6h3M4 12h3M11 12h9M4 18h11M19 18h1" /><circle cx="15" cy="6" r="2" /><circle
      cx="9"
      cy="12"
      r="2"
    /><circle cx="17" cy="18" r="2" />
  {:else if name === "plus"}
    <path d="M12 6v12M6 12h12" />
  {:else if name === "minus"}
    <path d="M6 12h12" />
  {:else if name === "chevron"}
    <path d="m9 6 6 6-6 6" />
  {:else if name === "down"}
    <path d="m6 9 6 6 6-6" />
  {:else if name === "free"}
    <path d="M4 12h3M10 12h4M17 12h3" /><path d="m15 8 4 4-4 4" />
  {:else if name === "ping"}
    <path d="M3 12h4l2-5 4 10 2-5h6" />
  {:else if name === "copy"}
    <rect x="8" y="8" width="12" height="12" rx="2.5" /><path
      d="M16 8V6.5A2.5 2.5 0 0 0 13.5 4h-7A2.5 2.5 0 0 0 4 6.5v7A2.5 2.5 0 0 0 6.5 16H8"
    />
  {:else if name === "reset"}
    <path d="M4.5 12a7.5 7.5 0 1 0 2.2-5.3M4.5 4.5v3.2h3.2" />
  {:else if name === "line"}
    <path d="M3.5 9.5a12 12 0 0 1 17 0" /><path d="M6.5 12.8a7.5 7.5 0 0 1 11 0" /><path d="M9.5 16a3.2 3.2 0 0 1 5 0" /><circle class="dot" cx="12" cy="19" r="1.3" />
  {:else if name === "apps"}
    <rect x="4" y="4" width="7" height="7" rx="2" /><rect x="13" y="4" width="7" height="7" rx="2" /><rect
      x="4"
      y="13"
      width="7"
      height="7"
      rx="2"
    /><rect x="13" y="13" width="7" height="7" rx="2" />
  {:else if name === "power"}
    <path d="M12 3.5v8" /><path d="M7 6.5a7 7 0 1 0 10 0" />
  {:else if name === "info"}
    <circle cx="12" cy="12" r="8.5" /><path d="M12 11v5.5" /><circle class="dot" cx="12" cy="7.8" r="1.1" />
  {:else if name === "clock"}
    <circle cx="12" cy="12" r="8.5" /><path d="M12 7.5V12l3 2" />
  {:else if name === "infinity"}
    <path d="M12 12c-2-2.6-3.6-4-5.5-4a4 4 0 0 0 0 8c1.9 0 3.5-1.4 5.5-4s3.6-4 5.5-4a4 4 0 0 1 0 8c-1.9 0-3.5-1.4-5.5-4z" />
  {:else if name === "ban"}
    <circle cx="12" cy="12" r="8.5" /><path d="m6 6 12 12" />
  {:else if name === "unlink"}
    <path d="M9.5 14.5 7.7 16.3a3.2 3.2 0 0 1-4.5-4.5L5 10M14.5 9.5l1.8-1.8a3.2 3.2 0 0 1 4.5 4.5L19 14M8 4l1 2.5M4 8l2.5 1M16 20l-1-2.5M20 16l-2.5-1" />
  {:else if name === "undo"}
    <path d="M9 7 5 11l4 4" /><path d="M5 11h9a5 5 0 0 1 0 10h-2" />
  {:else if name === "merge"}
    <path d="M6 4v4a4 4 0 0 0 4 4h8" /><path d="M6 20v-4a4 4 0 0 1 4-4" /><path d="m15 9 3 3-3 3" />
  {:else if name === "gauge"}
    <path d="M4.5 17a8.5 8.5 0 1 1 15 0" /><path d="m12 13 3.5-4" /><circle class="dot" cx="12" cy="13" r="1.4" />
  {:else if name === "phone"}
    <path
      d="M6.5 4h2.6l1.4 3.6-1.8 1.2a9.5 9.5 0 0 0 4.5 4.5l1.2-1.8 3.6 1.4v2.6A2.5 2.5 0 0 1 15.5 18 11.5 11.5 0 0 1 4 6.5 2.5 2.5 0 0 1 6.5 4z"
    />
  {:else if name === "window"}
    <rect x="3.5" y="5" width="17" height="14" rx="3" /><path d="M3.5 9.5h17" /><circle class="dot" cx="7" cy="7.25" r=".9" />
  {:else if name === "bulb"}
    <path d="M9 17.5h6M10 20.5h4" /><path d="M8.6 14.5A6 6 0 1 1 15.4 14.5c-.6.5-.9 1.2-.9 1.9v1.1h-5v-1.1c0-.7-.3-1.4-.9-1.9z" />
  {:else if name === "flag"}
    <path d="M5.5 21V4.5" /><path d="M5.5 5h11.5l-2.5 4 2.5 4H5.5" />
  {:else if name === "keyboard"}
    <rect x="2.5" y="6" width="19" height="12" rx="3" /><path d="M6.5 10h.01M10 10h.01M14 10h.01M17.5 10h.01M8 14h8" />
  {:else if name === "download"}
    <path d="M12 4v11" /><path d="m7.5 10.5 4.5 4.5 4.5-4.5" /><path d="M5 20h14" />
  {:else if name === "upload"}
    <path d="M12 20V9" /><path d="m7.5 13.5 4.5-4.5 4.5 4.5" /><path d="M5 4h14" />
  {:else if name === "list"}
    <path d="M9 6.5h11M9 12h11M9 17.5h11" /><circle class="dot" cx="4.75" cy="6.5" r="1.1" /><circle
      class="dot"
      cx="4.75"
      cy="12"
      r="1.1"
    /><circle class="dot" cx="4.75" cy="17.5" r="1.1" />
  {:else if name === "trash"}
    <path d="M4.5 7h15M10 4h4M6.5 7l.8 11.2A2 2 0 0 0 9.3 20h5.4a2 2 0 0 0 2-1.8L17.5 7M10 11v5M14 11v5" />
  {:else if name === "check"}
    <path d="m5 12.5 4.5 4.5L19 7.5" />
  {:else if name === "pen"}
    <path d="M14.5 5.5 18.5 9.5M4.5 19.5l1-4.5L15.8 4.7a1.9 1.9 0 0 1 2.7 0l.8.8a1.9 1.9 0 0 1 0 2.7L9 18.5z" />
  {:else if name === "more"}
    <circle class="dot" cx="6" cy="12" r="1.6" /><circle class="dot" cx="12" cy="12" r="1.6" /><circle class="dot" cx="18" cy="12" r="1.6" />
  {:else if name === "search"}
    <circle cx="11" cy="11" r="6.5" /><path d="m16 16 4 4" />
  {:else if name === "close"}
    <path d="m7 7 10 10M17 7 7 17" />
  {:else if name === "auto"}
    <path d="M19.5 12a7.5 7.5 0 0 1-13.1 5M4.5 12a7.5 7.5 0 0 1 13.1-5" /><path d="M17.8 3.5v3.7h-3.7M6.2 20.5v-3.7h3.7" />
  {/if}
</svg>

<style>
  .glyph {
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.75;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .dot {
    fill: currentColor;
    stroke: none;
  }
</style>
