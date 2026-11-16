<script lang="ts" generics="T extends string">
  // A segmented control: native radio buttons, so the arrow keys and
  // screen readers work as they would anywhere in Windows. The chosen
  // option sits on a lighter surface inside a sunken track.
  let {
    name,
    label,
    options,
    value,
    onchange,
    small = false,
    disabled = false,
  }: {
    name: string;
    label: string;
    options: { value: T; label: string }[];
    value: T;
    onchange: (value: T) => void;
    small?: boolean;
    disabled?: boolean;
  } = $props();
</script>

<div class="segmented" class:small role="radiogroup" aria-label={label} aria-disabled={disabled}>
  {#each options as o (o.value)}
    <label class:checked={o.value === value}>
      <input
        type="radio"
        {name}
        {disabled}
        value={o.value}
        checked={o.value === value}
        onchange={() => onchange(o.value)}
      />
      <span>{o.label}</span>
    </label>
  {/each}
</div>

<style>
  .segmented {
    display: inline-flex;
    flex: none;
    padding: 3px;
    gap: 2px;
    border-radius: var(--radius-control);
    background: var(--surface-ground);
    box-shadow: inset 0 0 0 1px var(--stroke-card);
  }
  label {
    position: relative;
    display: flex;
    align-items: center;
    min-height: 30px;
    padding: 0 14px;
    border-radius: 8px;
    color: var(--text-secondary);
    font-size: 14px;
    transition: background-color 120ms linear;
  }
  .small label {
    min-height: 26px;
    padding: 0 10px;
    font-size: 13px;
  }
  label:hover {
    color: var(--text-primary);
  }
  label.checked {
    background: var(--surface-3);
    color: var(--text-primary);
    font-weight: 600;
  }
  [aria-disabled="true"] label {
    opacity: 0.5;
  }
  input {
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
  }
  label:has(input:focus-visible) {
    outline: 2px solid var(--focus-outer);
    outline-offset: 1px;
  }
  @media (forced-colors: active) {
    label.checked {
      outline: 2px solid Highlight;
    }
  }
</style>
