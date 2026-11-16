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
    <div class="field">
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
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1 1 160px;
    min-width: 0;
  }
  label {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  label .icon {
    font-size: 12px;
    color: var(--text-secondary);
  }
  /* A text box with the unit inside. */
  .box {
    display: flex;
    align-items: center;
    height: 40px;
    border-radius: var(--radius-control);
    background: var(--surface-2);
  }
  .box:hover {
    background: var(--surface-3);
  }
  .box:focus-within {
    outline: 2px solid var(--focus-outer);
  }
  .box.invalid {
    box-shadow: inset 0 0 0 1.5px var(--critical);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0 10px;
    border: 0;
    background: transparent;
    color: var(--text-primary);
    font: inherit;
    user-select: text;
    cursor: text;
  }
  input:focus-visible {
    outline: none;
    box-shadow: none;
  }
  input:disabled {
    color: var(--text-tertiary);
  }
  .unit {
    padding: 0 10px 0 4px;
    color: var(--text-secondary);
  }
  .error {
    margin: 0;
    font-size: 13px;
    line-height: 18px;
    color: var(--critical);
  }
  .hint {
    margin: 8px 0 0;
    font-size: 13px;
    line-height: 18px;
    color: var(--text-secondary);
  }
  @media (forced-colors: active) {
    .box {
      border: 1px solid CanvasText;
    }
  }
</style>
