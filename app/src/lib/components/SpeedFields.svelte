<script lang="ts">
  // The two speed boxes (download, upload, in Mbps) shared by the
  // connection-speed dialog and first run. A box says what's wrong once
  // the person leaves it or tries to save; the parent parses the same text
  // with parseSpeed when it saves.
  import { parseSpeed } from "../line";
  import { t } from "../i18n";
  import type { Direction } from "../lineDialog.svelte";
  import Glyph from "./Glyph.svelte";

  let {
    down = $bindable(""),
    up = $bindable(""),
    submitted = false,
    disabled = false,
  }: { down?: string; up?: string; submitted?: boolean; disabled?: boolean } = $props();

  const uid = $props.id();
  const inputs: Partial<Record<Direction, HTMLInputElement>> = {};
  let left = $state<Record<Direction, boolean>>({ down: false, up: false });

  const invalid = $derived({ down: !parseSpeed(down).ok, up: !parseSpeed(up).ok });
  const shown = $derived({
    down: invalid.down && (submitted || left.down),
    up: invalid.up && (submitted || left.up),
  });

  /** Focus a box: the first invalid one if any, else `dir`. */
  export function focus(dir: Direction = "down") {
    const target = invalid.down ? "down" : invalid.up ? "up" : dir;
    inputs[target]?.focus();
    inputs[target]?.select();
  }

  const dirs: { dir: Direction; glyph: "download" | "upload"; label: string }[] = [
    { dir: "down", glyph: "download", label: t("line.down") },
    { dir: "up", glyph: "upload", label: t("line.up") },
  ];
</script>

<div class="fields">
  {#each dirs as d (d.dir)}
    <div class="speed">
      <label for="{uid}-{d.dir}">
        <Glyph name={d.glyph} size={16} />{d.label}
      </label>
      <div class="box" class:invalid={shown[d.dir]}>
        <input
          id="{uid}-{d.dir}"
          bind:this={inputs[d.dir]}
          type="text"
          inputmode="decimal"
          autocomplete="off"
          spellcheck="false"
          class="num"
          placeholder={t("line.notSet")}
          {disabled}
          value={d.dir === "down" ? down : up}
          oninput={(e) => {
            const v = e.currentTarget.value;
            if (d.dir === "down") down = v;
            else up = v;
          }}
          onblur={() => (left[d.dir] = true)}
          aria-invalid={shown[d.dir]}
          aria-describedby="{uid}-{d.dir}-unit {shown[d.dir] ? `${uid}-${d.dir}-error` : `${uid}-empty`}"
        />
        <span class="unit" id="{uid}-{d.dir}-unit">{t("line.unit")}</span>
      </div>
      {#if shown[d.dir]}
        <p class="error" id="{uid}-{d.dir}-error">{t("line.invalid")}</p>
      {/if}
    </div>
  {/each}
</div>
<p class="hint" id="{uid}-empty">{t("line.empty")}</p>

<style>
  .fields {
    display: flex;
    flex-wrap: wrap;
    gap: 12px 16px;
  }
  /* One speed: its name, then one box with the number and its unit. */
  .speed {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex: 1 1 160px;
    min-width: 0;
  }
  label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 700;
    color: var(--text-secondary);
  }
  label :global(.glyph) {
    color: var(--text-tertiary);
  }
  /* The app's one text box look (app.css .field), a little taller for a
     big number. */
  .box {
    display: flex;
    align-items: center;
    height: 46px;
    padding: 0 12px;
    border-radius: var(--radius-control);
    background: var(--surface-side);
    box-shadow: inset 0 0 0 1px var(--stroke-strong);
    transition: box-shadow 120ms linear;
  }
  .box:hover {
    box-shadow: inset 0 0 0 1px rgb(236 234 226 / 0.2);
  }
  .box:focus-within {
    box-shadow: inset 0 0 0 2px var(--focus-outer);
  }
  .box.invalid {
    box-shadow: inset 0 0 0 2px var(--critical);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text-primary);
    font: inherit;
    font-size: 18px;
    font-weight: 700;
    letter-spacing: -0.01em;
    user-select: text;
    cursor: text;
  }
  input::placeholder {
    font-size: 15px;
    font-weight: 500;
    color: var(--text-tertiary);
  }
  input:focus-visible {
    outline: none;
    box-shadow: none;
  }
  input:disabled {
    color: var(--text-tertiary);
  }
  .unit {
    padding-left: 6px;
    font-size: 13.5px;
    font-weight: 600;
    color: var(--text-tertiary);
  }
  .hint {
    margin: 10px 0 0;
    font-size: 13px;
    line-height: 18px;
    color: var(--text-tertiary);
  }
  @media (forced-colors: active) {
    .box {
      border: 1px solid CanvasText;
    }
    /* The red ring is a box-shadow, which high contrast drops. */
    .box.invalid {
      border: 2px dashed CanvasText;
    }
  }
</style>
