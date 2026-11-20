<script lang="ts" generics="V extends string">
  // SnailNet's own dropdown: a button that shows the choice, opening a list
  // in the menus' look (a native popover, so Escape and a click outside
  // close it). Arrow keys, Home and End move through the list; Enter or a
  // click picks. Nothing happens until a choice is picked, unlike a native
  // select, which acts on every arrow key while closed.
  import { placeMenu } from "../popover";
  import Glyph from "./Glyph.svelte";

  let {
    options,
    value,
    label,
    disabled = false,
    onchange,
  }: {
    options: { value: V; label: string }[];
    value: V;
    /** What the choice is, for screen readers. */
    label: string;
    disabled?: boolean;
    onchange: (value: V) => void;
  } = $props();

  const uid = $props.id();
  let btn = $state<HTMLButtonElement | null>(null);
  let list = $state<HTMLDivElement | null>(null);
  let open = $state(false);
  const current = $derived(options.find((o) => o.value === value));

  function toggled(e: ToggleEvent) {
    open = e.newState === "open";
    if (!open || !btn || !list) return;
    list.style.minWidth = `${btn.offsetWidth}px`;
    placeMenu(btn, list, "below", "start");
    list.querySelector<HTMLButtonElement>('[aria-selected="true"]')?.focus({ preventScroll: true });
  }

  function pick(v: V) {
    list?.hidePopover();
    btn?.focus();
    if (v !== value) onchange(v);
  }

  function keys(e: KeyboardEvent) {
    const items = [...(list?.querySelectorAll<HTMLButtonElement>("[role=option]") ?? [])];
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    const to =
      e.key === "ArrowDown"
        ? Math.min(items.length - 1, at + 1)
        : e.key === "ArrowUp"
          ? Math.max(0, at - 1)
          : e.key === "Home"
            ? 0
            : e.key === "End"
              ? items.length - 1
              : null;
    if (to === null) return;
    e.preventDefault();
    items[to]?.focus();
  }
</script>

<button
  bind:this={btn}
  type="button"
  class="dd"
  class:open
  {disabled}
  popovertarget="{uid}-list"
  aria-haspopup="listbox"
  aria-expanded={open}
  aria-label="{label}: {current?.label ?? ''}"
>
  <span class="cur">{current?.label ?? ""}</span>
  <span class="chev"><Glyph name="down" size={16} /></span>
</button>
<!-- svelte-ignore a11y_interactive_supports_focus -->
<div
  bind:this={list}
  id="{uid}-list"
  class="popmenu dd-list"
  popover="auto"
  role="listbox"
  aria-label={label}
  ontoggle={toggled}
  onkeydown={keys}
>
  {#each options as o (o.value)}
    <button type="button" role="option" aria-selected={o.value === value} onclick={() => pick(o.value)}>
      <span class="lab">{o.label}</span>
      {#if o.value === value}<span class="tick"><Glyph name="check" size={16} /></span>{/if}
    </button>
  {/each}
</div>

<style>
  .dd {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-width: 140px;
    height: 36px;
    padding: 0 10px 0 12px;
    border: 0;
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--text-primary);
    font-size: 14px;
    font-weight: 600;
    transition: background-color 120ms linear;
  }
  .dd:hover:not(:disabled),
  .dd.open {
    background: var(--surface-3);
  }
  .dd:disabled {
    opacity: 0.5;
  }
  .cur {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chev {
    display: flex;
    color: var(--text-secondary);
    transition: transform 200ms var(--ease-out);
  }
  .dd.open .chev {
    transform: rotate(180deg);
  }
  .dd-list {
    width: auto;
    padding: 6px;
  }
  .lab {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
  }
  .tick {
    display: flex;
    color: var(--shell-bright);
  }
  [aria-selected="true"] {
    font-weight: 700;
  }
  @media (forced-colors: active) {
    .dd {
      border: 1px solid ButtonText;
    }
  }
</style>
