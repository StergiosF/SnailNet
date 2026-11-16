<script lang="ts">
  // A Fluent toggle switch: a native checkbox with the switch role, the
  // state word ("On"/"Off") beside it as Windows Settings shows it.
  import { t } from "../i18n";

  let {
    label,
    checked,
    onchange,
  }: { label: string; checked: boolean; onchange: (on: boolean) => void } = $props();
</script>

<label class="toggle">
  <span class="text">{label}</span>
  <span class="state" aria-hidden="true">{checked ? t("toggle.on") : t("toggle.off")}</span>
  <input
    type="checkbox"
    role="switch"
    {checked}
    onchange={(e) => onchange((e.currentTarget as HTMLInputElement).checked)}
  />
  <span class="track" aria-hidden="true"><span class="knob"></span></span>
</label>

<style>
  .toggle {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 32px;
  }
  .text {
    flex: 1;
  }
  .state {
    color: var(--text-secondary);
  }
  input {
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
  }
  .track {
    position: relative;
    width: 40px;
    height: 20px;
    border: 1px solid var(--text-secondary);
    border-radius: 10px;
    transition: background-color 150ms linear;
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 6px;
    background: var(--text-secondary);
    transition: transform 150ms var(--ease-out);
  }
  input:checked + .track {
    border-color: var(--accent);
    background: var(--accent);
  }
  input:checked + .track .knob {
    background: var(--on-accent);
    transform: translateX(20px);
  }
  input:focus-visible + .track {
    outline: 2px solid var(--focus-outer);
    outline-offset: 2px;
  }
</style>
