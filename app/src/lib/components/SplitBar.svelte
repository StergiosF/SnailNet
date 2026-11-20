<script lang="ts">
  // The Split page's bar (PLAN.md §14.3), like the website's demo: one
  // segment per slice in its ink, Everything else last in stone, and a
  // cream handle between each pair of neighbours. Dragging a handle trades
  // only between its two neighbours (splitDrag.ts); the bar follows the
  // pointer and settles on a step of 5% on release. Every move goes into
  // the page's draft, never straight to the line.
  //
  // Each segment is its ink laid thin over the panel (Obsidian Jade), with
  // its name over "40% · ≥ 10 Mbps": what it keeps when your internet is
  // busy, right where it's being changed, so no readout line wraps under
  // the bar. A segment too thin for its name shows only its %, thinner
  // still nothing (its row says it). Everything else is hatched stone. The
  // handle catches a press 32 px wide over the bar's full height, its grip
  // a narrow ivory line; each handle is a slider for the keyboard, and a
  // screen reader hears each side's share as it moves. Hovering a segment,
  // or holding a handle, lights up the rows it means.
  //
  // Live use rides inside each segment (DESIGN.md, Split): a thin line in
  // its ink along its foot fills as its share is used, so the page has one
  // picture of the split, not a second strip. The pointer is an open hand
  // over a handle and a closed one while dragging.
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
  const ink = (s: Slice) => (s.ink >= 0 && s.rule >= 0 ? `var(--ink-${s.ink})` : "var(--stone-1)");
  /** What a segment keeps when your internet is busy, in bps; null when
   *  SnailNet isn't sharing it out (no speed set). */
  const keptOf = (s: Slice) => (ledger.shaped === null ? null : (ledger.shaped * (s.share ?? 0)) / 10_000);
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
    const ka = keptOf(a);
    const kb = keptOf(b);
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
        style:--ink={ink(s)}
        title={fit === "full" ? undefined : t("bar.plain", { name: s.name, pct: pct(s.share) })}
        aria-hidden="true"
        onpointerenter={() => drag === null && (ui.hot = [s.key])}
        onpointerleave={() => drag === null && (ui.hot = [])}
      >
        {#if fit === "full"}
          {@const kept = s.rule >= 0 ? keptOf(s) : null}
          <span class="name">{s.name}</span>
          <span class="fig num"
            >{pct(s.share)}%{#if kept !== null}<em>&nbsp;· ≥ {mbps(kept)} Mbps</em>{/if}</span
          >
        {:else if fit !== "none"}
          <span class="fig num">{pct(s.share)}%</span>
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

  </div>
  <!-- What each side of the moving handle gets, for screen readers; on
       screen the segments say it themselves. -->
  <p class="visually-hidden" aria-live="polite">
    {#if showing !== null}{readout(showing).join(", ")}{/if}
  </p>

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
    margin-top: 0;
  }
  .bar {
    position: relative;
    height: 52px;
    border-radius: 12px;
    background: var(--surface-side);
    box-shadow: inset 0 0 0 1px var(--stroke-card);
    /* Sideways drags move the split; an up-or-down swipe still scrolls. */
    touch-action: pan-y;
    user-select: none;
  }
  .seg {
    position: absolute;
    top: 0;
    bottom: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: flex-start;
    gap: 1px;
    padding: 0 12px;
    overflow: hidden;
    border-radius: 4px;
    background: color-mix(in srgb, var(--ink) 27%, var(--surface-1));
    color: var(--text-primary);
    white-space: nowrap;
    transition:
      left 300ms var(--ease-out),
      width 300ms var(--ease-out),
      filter 120ms linear;
  }
  /* Everything else: hatched, never colour alone. */
  .seg.other {
    background: var(--free-hatch), var(--surface-1);
  }
  .seg.first {
    border-radius: 12px 4px 4px 12px;
  }
  .seg.last {
    border-radius: 4px 12px 12px 4px;
  }
  .seg.first.last {
    border-radius: 12px;
  }
  .dragging .seg {
    transition: filter 120ms linear;
  }
  .seg.hot {
    filter: brightness(1.08);
  }
  .name {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 13px;
    font-weight: 700;
  }
  .fig {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 13px;
    font-weight: 650;
    color: var(--text-secondary);
  }
  .fig em {
    font-style: normal;
    font-weight: 600;
    color: var(--text-tertiary);
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
  /* The grip: a narrow ivory line, ringed in the ground so it reads on
     any ink. */
  .handle::before {
    content: "";
    position: absolute;
    top: 6px;
    bottom: 6px;
    left: 14px;
    width: 4px;
    border-radius: 2px;
    background: var(--accent);
    box-shadow:
      0 0 0 3px var(--surface-ground),
      0 2px 8px rgb(0 0 0 / 0.5);
    transition: transform 160ms var(--ease-out);
  }
  .handle:hover::before,
  .handle.on::before {
    transform: scaleX(1.5);
  }
  .handle:focus-visible {
    outline: 2px solid var(--focus-outer);
    outline-offset: 0;
  }
  .locked {
    cursor: default;
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
  /* Live use, along the segment's foot in its ink: how much of its share
     is used. */
  .use {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 3px;
    overflow: hidden;
  }
  .use i {
    display: block;
    height: 100%;
    background: var(--ink);
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
    .ticks i {
      forced-color-adjust: none;
      background: CanvasText;
    }
  }
</style>
