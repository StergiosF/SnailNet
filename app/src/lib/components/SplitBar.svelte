<script lang="ts">
  // The Split page's bar (PLAN.md §14.3), like the website's demo: one
  // segment per slice in its ink, Everything else last in stone, and a
  // cream handle between each pair of neighbours. Dragging a handle trades
  // only between its two neighbours (splitDrag.ts); the bar follows the
  // pointer and settles on a step of 5% on release. Every move goes into
  // the page's draft, never straight to the line.
  //
  // What made the old bar hard is answered here: the handle catches a
  // press 32 px wide over the bar's full height; a segment too thin for its
  // name shows only its %, thinner still nothing (its row says it); and
  // while dragging, a readout under the handle says what each side gets in
  // Mbps. Each handle is a slider for the keyboard. Hovering a segment, or
  // holding a handle, lights up the rows it means.
  //
  // Live use rides inside each segment (DESIGN.md, Split): a thin line
  // along its foot fills as its share is used, so the page has one picture
  // of the split, not a second strip. The pointer is an open hand over a
  // handle and a closed one while dragging.
  import { mbps } from "../format";
  import { t } from "../i18n";
  import type { Ledger, Slice } from "../ledger";
  import { barShares } from "../rules";
  import { FLOOR, MIN_RULE } from "../split";
  import {
    dragBetween,
    edgeBetween,
    fitLabel,
    handlePositions,
    nudgeBetween,
    pickHandle,
    savedTicks,
  } from "../splitDrag";
  import type { Rule } from "../types";
  import { ui } from "../ui.svelte";

  let { ledger, canEdit }: { ledger: Ledger; canEdit: boolean } = $props();

  /** The bar's segments: the slices with a share, Everything else last. */
  const segs = $derived(ledger.slices.filter((s) => s.share !== null && s.enabled));
  /** How full each segment's slice is now, 0–1; null while not shaping. */
  const fill = (s: Slice) => (s.kept === null || s.kept <= 0 ? null : Math.min(1, s.bps / s.kept));
  const shares = $derived(barShares(ui.rules));
  const pos = $derived(handlePositions(shares));
  const ticks = $derived(ui.draft ? savedTicks(ui.draft.base, ui.draft.rules) : null);

  let bar = $state<HTMLDivElement | null>(null);
  let width = $state(0);

  const pct = (bp: number | null) => Math.round((bp ?? 0) / 100);
  const ink = (s: Slice) => (s.ink >= 0 ? `var(--ink-${s.ink})` : "var(--stone-0)");
  const startOf = (k: number) => shares.slice(0, k).reduce((a, b) => a + b, 0) / 100;

  // Dragging. `from` is the rules when the press began, so every move is
  // measured from there and nothing drifts.
  type Drag = { id: number; x0: number; from: Rule[]; handle: number | null; at: number; moved: boolean };
  let drag = $state<Drag | null>(null);
  /** The handle whose readout shows: the one dragged, or one the keyboard is on. */
  let kbd = $state<number | null>(null);
  const showing = $derived(drag?.handle ?? kbd);
  // Leaving the page mid-hover leaves no row lit.
  $effect(() => () => (ui.hot = []));

  function pctAt(x: number): number {
    const r = bar?.getBoundingClientRect();
    return r && r.width > 0 ? ((x - r.left) / r.width) * 100 : 0;
  }

  function light(i: number | null) {
    ui.hot = i === null ? [] : [segs[i]?.key, segs[i + 1]?.key].filter((k): k is string => k !== undefined);
  }

  function down(e: PointerEvent) {
    if (!canEdit || e.button !== 0 || !bar) return;
    const xs = pos.map((p) => (p / 100) * width);
    const x = e.clientX - bar.getBoundingClientRect().left;
    // null with a handle in reach means two overlap: the first move picks.
    const handle = pickHandle(xs, x);
    if (handle === null && pickHandle(xs, x, 1) === null) return;
    e.preventDefault();
    bar.setPointerCapture(e.pointerId);
    drag = { id: e.pointerId, x0: e.clientX, from: ui.rules, handle, at: pctAt(e.clientX), moved: false };
    kbd = null;
    light(handle);
  }

  function move(e: PointerEvent) {
    const d = drag;
    if (!d || e.pointerId !== d.id || !bar) return;
    const dx = e.clientX - d.x0;
    let handle = d.handle;
    if (handle === null) {
      if (dx === 0) return;
      const left = bar.getBoundingClientRect().left;
      handle = pickHandle(handlePositions(barShares(d.from)).map((p) => (p / 100) * width), d.x0 - left, dx);
      if (handle === null) return;
      light(handle);
    }
    const at = pctAt(e.clientX);
    drag = { ...d, handle, at, moved: true };
    ui.edit(dragBetween(d.from, handle, at));
  }

  function up(e: PointerEvent) {
    const d = drag;
    if (!d || e.pointerId !== d.id) return;
    drag = null;
    light(null);
    if (d.moved && d.handle !== null) ui.edit(dragBetween(d.from, d.handle, d.at, true));
  }

  function key(e: KeyboardEvent, i: number) {
    const steps: Record<string, number> = {
      ArrowLeft: -1,
      ArrowDown: -1,
      ArrowRight: 1,
      ArrowUp: 1,
      PageDown: -2,
      PageUp: 2,
    };
    const rules = ui.rules;
    if (e.key in steps) ui.edit(nudgeBetween(rules, i, steps[e.key] ?? 0));
    else if (e.key === "Home") ui.edit(edgeBetween(rules, i, "min"));
    else if (e.key === "End") ui.edit(edgeBetween(rules, i, "max"));
    else return;
    e.preventDefault();
    kbd = i;
  }

  /** The readout's words for handle `i`: each side's share, and in Mbps
   *  what it keeps when the line is busy. */
  function readout(i: number): [string, string] {
    const a = segs[i];
    const b = segs[i + 1];
    if (!a || !b) return ["", ""];
    const kept = (s: Slice) => (ledger.shaped === null ? null : (ledger.shaped * (s.share ?? 0)) / 10_000);
    const ka = kept(a);
    const kb = kept(b);
    return [
      ka === null
        ? t("bar.plain", { name: a.name, pct: pct(a.share) })
        : t("bar.first", { name: a.name, pct: pct(a.share), kept: mbps(ka) }),
      kb === null
        ? t("bar.plain", { name: b.name, pct: pct(b.share) })
        : t("bar.second", { name: b.name, pct: pct(b.share), kept: mbps(kb) }),
    ];
  }

  /** The slider's range for the left neighbour of handle `i`. */
  function range(i: number): { min: number; max: number } {
    const a = shares[i] ?? 0;
    const b = shares[i + 1] ?? 0;
    const minB = i + 1 === shares.length - 1 ? FLOOR : MIN_RULE;
    return { min: pct(MIN_RULE), max: pct(a + b - minB) };
  }
</script>

<section class="drag" aria-label={t("bar.label")}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="bar"
    class:dragging={drag !== null}
    class:locked={!canEdit}
    bind:this={bar}
    bind:clientWidth={width}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    onlostpointercapture={up}
  >
    {#each segs as s, k (s.key)}
      {@const first = k === 0}
      {@const last = k === segs.length - 1}
      {@const fit = fitLabel(((s.share ?? 0) / 10_000) * width - 4)}
      <div
        class="seg"
        class:first
        class:last
        class:hot={ui.hot.includes(s.key)}
        class:other={s.rule < 0}
        style:left="calc({startOf(k)}% + {first ? 0 : 2}px)"
        style:width="max(0px, calc({(s.share ?? 0) / 100}% - {(first ? 0 : 2) + (last ? 0 : 2)}px))"
        style:background={ink(s)}
        title={fit === "full" ? undefined : t("bar.plain", { name: s.name, pct: pct(s.share) })}
        aria-hidden="true"
        onpointerenter={() => drag === null && (ui.hot = [s.key])}
        onpointerleave={() => drag === null && (ui.hot = [])}
      >
        {#if fit === "full"}
          <span class="name">{s.name}</span>
        {/if}
        {#if fit !== "none"}
          <b>{pct(s.share)}<small>%</small></b>
        {/if}
        {#if ledger.shaped !== null && fill(s) !== null}
          <span class="use"><i style:width="{(fill(s) ?? 0) * 100}%"></i></span>
        {/if}
      </div>
    {/each}

    {#if canEdit}
      {#each pos as p, i (segs[i]?.key ?? i)}
        {@const a = segs[i]}
        {@const b = segs[i + 1]}
        {#if a && b}
          {@const r = range(i)}
          <div
            class="handle"
            class:on={showing === i}
            style:left="{p}%"
            role="slider"
            tabindex="0"
            aria-label={t("bar.handle", { a: a.name, b: b.name })}
            aria-valuemin={r.min}
            aria-valuemax={r.max}
            aria-valuenow={pct(a.share)}
            aria-valuetext={t("bar.value", { a: a.name, pa: pct(a.share), b: b.name, pb: pct(b.share) })}
            onkeydown={(e) => key(e, i)}
            onfocus={(e) => {
              if (e.currentTarget.matches(":focus-visible")) {
                kbd = i;
                light(i);
              }
            }}
            onblur={() => {
              kbd = null;
              if (drag === null) light(null);
            }}
          ></div>
        {/if}
      {/each}
    {/if}

    {#if showing !== null && pos[showing] !== undefined}
      {@const at = pos[showing] ?? 50}
      {@const [left, right] = readout(showing)}
      <div class="readout" class:start={at < 30} class:end={at > 70} style:left="{at}%" aria-hidden="true">
        <span>{left}</span><i></i><span>{right}</span>
      </div>
    {/if}
  </div>

  {#if ticks}
    <div class="ticks" aria-hidden="true">
      {#each ticks as p, i (i)}
        <i style:left="{p}%"></i>
      {/each}
    </div>
  {/if}

</section>

<style>
  .drag {
    margin-top: 24px;
  }
  .bar {
    position: relative;
    height: 56px;
    /* Sideways drags move the split; an up-or-down swipe still scrolls. */
    touch-action: pan-y;
    user-select: none;
  }
  .seg {
    position: absolute;
    top: 0;
    bottom: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 14px;
    overflow: hidden;
    border-radius: 6px;
    color: var(--on-accent);
    white-space: nowrap;
    transition:
      left 300ms var(--ease-out),
      width 300ms var(--ease-out),
      filter 120ms linear;
  }
  .seg.first {
    border-radius: 14px 6px 6px 14px;
  }
  .seg.last {
    justify-content: flex-end;
    border-radius: 6px 14px 14px 6px;
  }
  .seg.first.last {
    border-radius: 14px;
  }
  .dragging .seg {
    transition: filter 120ms linear;
  }
  .seg.hot {
    filter: brightness(1.08);
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 15px;
    font-weight: 600;
  }
  .seg b {
    flex: none;
    font-size: 17px;
    font-weight: 800;
    font-variant-numeric: tabular-nums;
  }
  .seg b small {
    font-size: 13px;
    font-weight: 700;
  }

  /* The handle: a 32 px catch over the bar's full height, a narrow cream
     grip in the middle of it. */
  .handle {
    position: absolute;
    top: -6px;
    bottom: -6px;
    width: 32px;
    margin-left: -16px;
    border-radius: 10px;
    cursor: grab;
    transition: left 300ms var(--ease-out);
  }
  .dragging .handle {
    transition: none;
  }
  /* Holding a handle: a closed hand wherever the pointer goes. */
  .bar.dragging,
  .bar.dragging * {
    cursor: grabbing;
  }
  .handle::before {
    content: "";
    position: absolute;
    inset: 0 12px;
    border-radius: 4px;
    background: var(--accent);
    box-shadow: 0 2px 10px rgb(0 0 0 / 0.45);
    transition: transform 160ms var(--ease-out);
  }
  .handle:hover::before,
  .handle.on::before {
    transform: scaleX(1.4);
  }
  .handle:focus-visible {
    outline: 2px solid var(--focus-outer);
    outline-offset: 0;
  }
  .locked {
    cursor: default;
  }

  /* While dragging (or on the keyboard): each side, in % and Mbps. */
  .readout {
    position: absolute;
    /* Under the bar: over it sits the page's sentence. */
    top: calc(100% + 14px);
    z-index: 3;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 12px;
    border-radius: var(--radius-control);
    background: var(--surface-3);
    box-shadow: var(--shadow-raised);
    color: var(--text-primary);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    transform: translateX(-50%);
    pointer-events: none;
  }
  .readout.start {
    transform: translateX(-16px);
  }
  .readout.end {
    transform: translateX(calc(-100% + 16px));
  }
  .readout i {
    width: 1px;
    align-self: stretch;
    background: var(--text-tertiary);
  }

  /* Where the handles stood before the unsaved changes. */
  .ticks {
    position: relative;
    height: 8px;
    margin-top: 4px;
  }
  .ticks i {
    position: absolute;
    top: 0;
    width: 2px;
    height: 8px;
    margin-left: -1px;
    border-radius: 1px;
    background: var(--text-tertiary);
    opacity: 0.8;
  }
  /* Live use, inside the segment's foot: how much of its share is used. */
  .use {
    position: absolute;
    left: 10px;
    right: 10px;
    bottom: 7px;
    height: 3px;
    border-radius: 2px;
    background: rgb(10 33 29 / 0.14);
    overflow: hidden;
  }
  .use i {
    display: block;
    height: 100%;
    border-radius: 2px;
    background: rgb(10 33 29 / 0.5);
    transition: width 900ms var(--ease-out);
  }
  @media (forced-colors: active) {
    .seg {
      forced-color-adjust: none;
      background: Canvas !important;
      color: CanvasText;
      border: 1px solid CanvasText;
    }
    .seg.hot {
      outline: 2px solid Highlight;
      outline-offset: -2px;
    }
    .handle::before {
      forced-color-adjust: none;
      background: Highlight;
    }
    .readout {
      border: 1px solid CanvasText;
    }
    .ticks i {
      forced-color-adjust: none;
      background: CanvasText;
    }
  }
</style>
