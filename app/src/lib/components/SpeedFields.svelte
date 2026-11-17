<script lang="ts">
  // The two speed boxes (download, upload, in Mbps) shared by the
  // connection-speed dialog and first run. A box says what's wrong once
  // the person leaves it or tries to save; the parent parses the same text
  // with parseSpeed when it saves.
  import { parseSpeed } from "../line";
  import { t } from "../i18n";
  import type { Direction } from "../lineDialog.svelte";

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

  const dirs: { dir: Direction; glyph: string; label: string }[] = [
    { dir: "down", glyph: "", label: t("line.down") },
    { dir: "up", glyph: "", label: t("line.up") },
  ];
</script>

<div class="fields">
  {#each dirs as d (d.dir)}
    <div class="speed">
      <label for="{uid}-{d.dir}">
        <i class="icon" aria-hidden="true">{d.glyph}</i>{d.label}
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
    gap: 8px;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  label .icon {
    font-size: 13px;
    color: var(--shell);
  }
  .box {
    display: flex;
    align-items: center;
    height: 52px;
    padding: 0 14px;
    border-radius: 12px;
    background: var(--surface-ground);
    box-shadow:
      inset 0 0 0 1px rgb(255 255 255 / 0.08),
      inset 0 2px 6px rgb(0 0 0 / 0.25);
    transition: box-shadow 160ms linear;
  }
  .box:hover {
    box-shadow:
      inset 0 0 0 1px rgb(255 255 255 / 0.16),
      inset 0 2px 6px rgb(0 0 0 / 0.25);
  }
  .box:focus-within {
    box-shadow:
      inset 0 0 0 2px var(--shell),
      inset 0 2px 6px rgb(0 0 0 / 0.25);
  }
  .box.invalid {
    box-shadow:
      inset 0 0 0 2px var(--critical),
      inset 0 2px 6px rgb(0 0 0 / 0.25);
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
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
    user-select: text;
    cursor: text;
  }
  input::placeholder {
    font-size: 16px;
    font-weight: 500;
    color: var(--text-secondary);
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
    font-size: 14px;
    color: var(--text-secondary);
  }
  .hint {
    margin: 10px 0 0;
    font-size: 13px;
    line-height: 18px;
    color: var(--text-secondary);
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
