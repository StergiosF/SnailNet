<script lang="ts">
  // The split bar (PLAN.md §14.3): one slice per enabled rule of the active
  // profile, then Everything else. Drag a divider to trade share between
  // its neighbours (1% steps, Shift 5%, Alt 0.1%), or focus it and use the
  // arrow keys; click a slice to type its percentage. Each slice's inner
  // fill shows what it's using right now against what it's guaranteed.
  // Changes save when the drag ends (keys: after a short pause).
  import { className, mbps, percent, targetLabel } from "../format";
  import { t } from "../i18n";
  import { service } from "../service.svelte";
  import { TOTAL, moveDivider, normalize, setShare } from "../split";
  import type { SaveError } from "../types";

  const KEY_SAVE_DELAY_MS = 600;

  const rules = $derived(service.activeProfile?.rules.filter((r) => r.enabled) ?? []);
  const saved = $derived(normalize(rules.map((r) => r.share)));

  let draft = $state<number[] | null>(null);
  let pendingRev = $state<number | null>(null);
  let dragging = $state<number | null>(null);
  let editing = $state<number | null>(null);
  let error = $state<string | null>(null);
  let barEl = $state<HTMLDivElement | null>(null);
  let keyTimer: ReturnType<typeof setTimeout> | undefined;

  const shares = $derived(draft ?? saved);

  // Drop the draft once the service's snapshot has caught up with it.
  $effect(() => {
    const rev = service.snapshot?.rev;
    if (pendingRev !== null && rev !== undefined && rev >= pendingRev) {
      draft = null;
      pendingRev = null;
    }
  });

  const slices = $derived.by(() => {
    const down = service.stats?.down;
    const capacity = down?.shaped_bps ?? 0;
    const classes = down?.classes ?? [];
    const apps = service.snapshot?.apps ?? [];
    return shares.map((share, i) => {
      const rule = rules[i];
      const isOther = rule === undefined;
      const cls = isOther ? classes[classes.length - 1] : classes.find((c) => c.name === className(rule.target));
      const guaranteed = (capacity * share) / TOTAL;
      return {
        key: isOther ? "other" : className(rule.target),
        label: isOther ? t("split.everything") : targetLabel(rule.target, apps, service.siteNames),
        share,
        isOther,
        hue: isOther ? "other" : String((i % 6) + 1),
        guaranteedBps: guaranteed,
        fill: guaranteed > 0 && cls ? Math.min(1, cls.bps / guaranteed) : 0,
      };
    });
  });

  /** Left edge of divider `i` (between slice i and i+1), in percent. */
  function dividerAt(i: number): number {
    return shares.slice(0, i + 1).reduce((a, b) => a + b, 0) / 100;
  }

  function stepFor(e: { shiftKey: boolean; altKey: boolean }): number {
    return e.altKey ? 10 : e.shiftKey ? 500 : 100;
  }

  async function save(next: number[]) {
    draft = next;
    error = null;
    try {
      pendingRev = await service.saveShares(next);
    } catch (e) {
      const err = e as SaveError;
      error = err.kind === "conflict" ? t("split.conflict") : t("split.failed", { message: err.message });
      draft = null;
      pendingRev = null;
    }
  }

  // Divider drag.
  let start: { x: number; shares: number[] } | null = null;

  function onPointerDown(e: PointerEvent, i: number) {
    if (e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    start = { x: e.clientX, shares: [...shares] };
    dragging = i;
  }

  function onPointerMove(e: PointerEvent, i: number) {
    if (!start || dragging !== i || !barEl) return;
    const step = stepFor(e);
    const raw = ((e.clientX - start.x) / barEl.clientWidth) * TOTAL;
    draft = moveDivider(start.shares, i, Math.round(raw / step) * step);
  }

  function onPointerUp(i: number) {
    if (dragging !== i) return;
    dragging = null;
    const moved = start && draft && draft.some((s, k) => s !== start?.shares[k]);
    start = null;
    if (moved && draft) void save(draft);
    else draft = null;
  }

  function onDividerKey(e: KeyboardEvent, i: number) {
    const dir = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (dir === 0) return;
    e.preventDefault();
    const next = moveDivider(shares, i, dir * stepFor(e));
    draft = next;
    clearTimeout(keyTimer);
    keyTimer = setTimeout(() => void save(next), KEY_SAVE_DELAY_MS);
  }

  function onEditKey(e: KeyboardEvent, i: number) {
    if (e.key === "Escape") {
      editing = null;
    } else if (e.key === "Enter") {
      const value = Number((e.currentTarget as HTMLInputElement).value);
      editing = null;
      if (Number.isFinite(value)) void save(setShare(shares, i, value * 100));
    }
  }

  function focusOnMount(el: HTMLInputElement) {
    el.focus();
    el.select();
  }
</script>

<section class="split" aria-label={t("split.label")}>
  <div class="track">
    <div class="bar" bind:this={barEl}>
      {#each slices as s, i (s.key)}
        <div
          class="slice"
          data-hue={s.hue}
          style:flex-basis="{s.share / 100}%"
          class:dragging={dragging !== null}
        >
          <span class="fill" style:width="{s.fill * 100}%" aria-hidden="true"></span>
          {#if editing === i}
            <input
              type="number"
              min="1"
              max="94"
              value={Math.round(s.share / 100)}
              aria-label={t("split.edit", { name: s.label })}
              onkeydown={(e) => onEditKey(e, i)}
              onblur={() => (editing = null)}
              use:focusOnMount
            />
          {:else if s.isOther}
            <span class="content">
              <span class="name">{s.label}</span>
              <span class="pct num">{percent(s.share)}</span>
            </span>
          {:else}
            <button type="button" class="content" onclick={() => (editing = i)}>
              <span class="name">{s.label}</span>
              <span class="pct num">{percent(s.share)}</span>
            </button>
          {/if}
        </div>
      {/each}
    </div>

    {#each slices.slice(0, -1) as s, i (s.key)}
      {@const next = slices[i + 1]}
      {#if next}
        <!-- A focusable separator with a value is the WAI-ARIA window
             splitter pattern, an interactive widget; Svelte's rule only
             knows the static separator. -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
        <div
          class="divider"
          class:active={dragging === i}
          style:left="{dividerAt(i)}%"
          role="separator"
          tabindex="0"
          aria-orientation="vertical"
          aria-valuemin="1"
          aria-valuemax="94"
          aria-valuenow={Math.round(s.share / 100)}
          aria-label={t("split.divider", {
            a: s.label,
            pa: Math.round(s.share / 100),
            b: next.label,
            pb: Math.round(next.share / 100),
          })}
          onpointerdown={(e) => onPointerDown(e, i)}
          onpointermove={(e) => onPointerMove(e, i)}
          onpointerup={() => onPointerUp(i)}
          onpointercancel={() => onPointerUp(i)}
          onkeydown={(e) => onDividerKey(e, i)}
        >
          <span class="grip" aria-hidden="true"></span>
          {#if dragging === i}
            <span class="hint num" aria-hidden="true">
              {t("split.hint", { a: mbps(s.guaranteedBps), b: mbps(next.guaranteedBps) })}
            </span>
          {/if}
        </div>
      {/if}
    {/each}
  </div>

  {#if rules.length === 0}
    <p class="note">{t("split.empty")}</p>
  {/if}
  {#if error}
    <p class="note error" role="alert">{error}</p>
  {/if}
</section>

<style>
  .split {
    padding: 0 var(--gutter) 24px;
  }
  .track {
    position: relative;
  }
  .bar {
    display: flex;
    height: 44px;
    border-radius: var(--radius-card);
    overflow: hidden;
    border: 1px solid var(--stroke-card);
  }
  .slice {
    position: relative;
    min-width: 0;
    container-type: inline-size;
    transition: flex-basis 200ms var(--ease-out);
  }
  .slice.dragging {
    transition: none;
  }
  .slice + .slice {
    border-left: 1px solid var(--surface-card);
  }
  .slice[data-hue="1"] { background: var(--slice-1-bg); color: var(--slice-1-fg); }
  .slice[data-hue="2"] { background: var(--slice-2-bg); color: var(--slice-2-fg); }
  .slice[data-hue="3"] { background: var(--slice-3-bg); color: var(--slice-3-fg); }
  .slice[data-hue="4"] { background: var(--slice-4-bg); color: var(--slice-4-fg); }
  .slice[data-hue="5"] { background: var(--slice-5-bg); color: var(--slice-5-fg); }
  .slice[data-hue="6"] { background: var(--slice-6-bg); color: var(--slice-6-fg); }
  .slice[data-hue="other"] { background: var(--slice-other-bg); color: var(--slice-other-fg); }

  /* What the slice uses now, against what it's guaranteed. */
  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    background: currentColor;
    opacity: 0.1;
    transition: width 300ms var(--ease-out);
    pointer-events: none;
  }
  .content {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 100%;
    padding: 0 12px;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
  }
  button.content:hover {
    background: var(--fill-subtle-hover);
  }
  button.content:focus-visible {
    outline-offset: -3px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .pct {
    margin-left: auto;
  }
  @container (max-width: 80px) {
    .name {
      display: none;
    }
    .pct {
      margin: 0 auto;
    }
  }
  @container (max-width: 36px) {
    .pct {
      display: none;
    }
  }
  input {
    position: relative;
    width: calc(100% - 12px);
    height: 32px;
    margin: 5px 6px;
    padding: 0 8px;
    border: 1px solid var(--stroke-card);
    border-bottom: 2px solid var(--accent);
    border-radius: var(--radius-control);
    background: var(--surface-fallback);
    color: var(--text-primary);
    font: inherit;
    font-variant-numeric: tabular-nums;
  }

  .divider {
    position: absolute;
    top: -4px;
    bottom: -4px;
    width: 16px;
    transform: translateX(-50%);
    cursor: ew-resize;
    touch-action: none;
    display: flex;
    justify-content: center;
    border-radius: var(--radius-control);
  }
  .grip {
    width: 4px;
    height: 100%;
    border-radius: 2px;
    background: transparent;
    transition: background-color 120ms linear;
  }
  .divider:hover .grip,
  .divider.active .grip,
  .divider:focus-visible .grip {
    background: var(--accent);
  }
  .hint {
    position: absolute;
    top: calc(100% + 6px);
    padding: 4px 8px;
    border: 1px solid var(--stroke-card);
    border-radius: var(--radius-control);
    background: var(--surface-fallback);
    color: var(--text-primary);
    font-size: 12px;
    line-height: 16px;
    white-space: nowrap;
    box-shadow: 0 4px 8px rgb(0 0 0 / 0.14);
    pointer-events: none;
  }
  .note {
    margin: 8px 0 0;
    font-size: 12px;
    line-height: 16px;
    color: var(--text-secondary);
  }
  .note.error {
    color: var(--critical);
  }
</style>
