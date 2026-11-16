<script lang="ts" generics="T extends string">
  // A Fluent segmented control (PLAN.md §15): native radio buttons, so the
  // arrow keys and screen readers work as they would anywhere in Windows.
  let {
    name,
    label,
    options,
    value,
    onchange,
  }: {
    name: string;
    label: string;
    options: { value: T; label: string }[];
    value: T;
    onchange: (value: T) => void;
  } = $props();
</script>

<div class="segmented" role="radiogroup" aria-label={label}>
  {#each options as o (o.value)}
    <label class:checked={o.value === value}>
      <input
        type="radio"
        {name}
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
    padding: 2px;
    gap: 2px;
    border: 1px solid var(--stroke-card);
    border-radius: var(--radius-control);
    background: var(--fill-subtle-pressed);
  }
  label {
    position: relative;
    display: flex;
    align-items: center;
    min-height: 28px;
    padding: 0 12px;
    border-radius: 3px;
    color: var(--text-secondary);
  }
  label:hover {
    background: var(--fill-subtle-hover);
    color: var(--text-primary);
  }
  label.checked {
    background: var(--surface-fallback);
    color: var(--text-primary);
    font-weight: 600;
    box-shadow: 0 0 0 1px var(--stroke-card);
  }
  label.checked::after {
    content: "";
    position: absolute;
    left: 50%;
    bottom: 2px;
    width: 16px;
    height: 3px;
    border-radius: 2px;
    background: var(--accent);
    transform: translateX(-50%);
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
