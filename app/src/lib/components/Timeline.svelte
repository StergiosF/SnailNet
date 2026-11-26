<script lang="ts">
  // "Last 10 minutes" (DESIGN.md, Now; history.ts): what used your internet
  // over time, one stacked band per app in the same colour as its row in
  // the list, your speed as a dashed line, and marks where something
  // happened. Hover reads any moment; hovering a row lights its band. The
  // Download / Upload choice here drives the list too. Nothing invented: it
  // starts empty when the app opens.
  import { mbps } from "../format";
  import { REST, SPAN_MS, STEP_MS, type Band } from "../history";
  import { t } from "../i18n";
  import { timeline } from "../timeline.svelte";
  import { ui, type Dir } from "../ui.svelte";
  import Glyph from "./Glyph.svelte";

  let {
    bands,
    colors,
    lit = null,
    lineBps,
    totalBps,
  }: {
    bands: Band[];
    colors: Map<string, string>;
    lit?: string | null;
    lineBps: number | null;
    totalBps: number | null;
  } = $props();

  const dirs: { id: Dir; glyph: "download" | "upload" }[] = [
    { id: "down", glyph: "download" },
    { id: "up", glyph: "upload" },
  ];

  let width = $state(0);
  const H = $derived(width < 560 ? 104 : 128);
  const TOP = 18;
  const AXIS = 20;

  const steps = $derived.by(() => {
    void timeline.rev;
    return timeline.history.steps.map((s) => s.t);
  });
  const marks = $derived.by(() => {
    void timeline.rev;
    return [...timeline.history.marks];
  });
  // The chart's right edge: the end of the newest step.
  const end = $derived(steps.length > 0 ? (steps[steps.length - 1] as number) + STEP_MS : 0);
  const x = (tMs: number) => ((tMs - (end - SPAN_MS)) / SPAN_MS) * width;
  const stackTop = $derived(
    steps.reduce((m, _, i) => Math.max(m, bands.reduce((sum, b) => sum + (b.values[i] ?? 0), 0)), 0),
  );
  const max = $derived(Math.max(lineBps ?? 0, stackTop, 1) * 1.06);
  const y = (bps: number) => TOP + H - (bps / max) * H;
  const enough = $derived(steps.length >= 2);

  const nameOf = (b: Band) => (b.key === REST ? t("history.other") : b.name);

  // Stacked from the floor, the busiest band lowest.
  const paths = $derived.by(() => {
    if (!enough || width === 0) return [];
    const cum = steps.map(() => 0);
    const xs = steps.map((s) => x(s + STEP_MS));
    return bands.map((b) => {
      const lo = cum.slice();
      const hi = b.values.map((v, i) => (cum[i] = (cum[i] ?? 0) + v));
      const upper = hi.map((v, i) => `${(xs[i] ?? 0).toFixed(1)},${y(v).toFixed(1)}`);
      const lower = lo.map((v, i) => `${(xs[i] ?? 0).toFixed(1)},${y(v).toFixed(1)}`).reverse();
      return {
        key: b.key,
        color: colors.get(b.key) ?? "var(--stone-1)",
        fill: `M${upper.join("L")}L${lower.join("L")}Z`,
        edge: `M${upper.join("L")}`,
      };
    });
  });

  // Marks, newest first; a label that would overlap a newer one is left
  // off (its line stays).
  const shownMarks = $derived.by(() => {
    if (!enough || width === 0) return [];
    let edge = Infinity;
    return marks
      .filter((m) => m.t >= end - SPAN_MS)
      .sort((a, b) => b.t - a.t)
      .map((m) => {
        const mx = x(m.t);
        const w = m.text.length * 6.4 + 12;
        const right = mx + 4 + w > width;
        const from = right ? mx - w - 4 : mx + 4;
        const label = from + w < edge - 6;
        if (label) edge = from;
        return { ...m, mx, w, from, label };
      });
  });

  const top = $derived(bands.filter((b) => b.key !== REST).sort((a, b) => b.peak - a.peak)[0]);
  const summary = $derived(
    !enough
      ? ""
      : top && top.peak > 0
        ? t("history.peak", { name: top.name, mbps: mbps(top.peak) })
        : t("history.quiet"),
  );

  // Reading a moment: O(1) bucket index against the cached plot rect.
  let at = $state<number | null>(null);
  let plotBox: { left: number; width: number } | null = null;
  $effect(() => {
    void width;
    plotBox = null;
  });
  function cacheBox(el: SVGSVGElement) {
    const r = el.getBoundingClientRect();
    plotBox = { left: r.left, width: r.width };
  }
  function enter(e: PointerEvent & { currentTarget: SVGSVGElement }) {
    cacheBox(e.currentTarget);
  }
  function point(e: PointerEvent & { currentTarget: SVGSVGElement }) {
    if (!enough || width <= 0) {
      at = null;
      return;
    }
    if (!plotBox) cacheBox(e.currentTarget);
    if (!plotBox || plotBox.width <= 0) return;
    const px = ((e.clientX - plotBox.left) / plotBox.width) * width;
    const tMs = (px / width) * SPAN_MS + (end - SPAN_MS);
    const first = (steps[0] as number) + STEP_MS;
    const best = Math.max(0, Math.min(steps.length - 1, Math.round((tMs - first) / STEP_MS)));
    if (at !== best) at = best;
  }
  function leave() {
    at = null;
    plotBox = null;
  }
  function ago(i: number): string {
    const s = Math.round((end - ((steps[i] ?? 0) + STEP_MS)) / 1000);
    if (s < 3) return t("history.at.now");
    if (s < 60) return t("history.at.s", { s });
    const m = Math.floor(s / 60);
    return s % 60 === 0 ? t("history.at.m", { m }) : t("history.at.ms", { m, s: s % 60 });
  }
  const reading = $derived.by(() => {
    if (at === null || at >= steps.length) return null;
    const i = at;
    const rows = bands
      .map((b) => ({ key: b.key, name: nameOf(b), v: b.values[i] ?? 0 }))
      .filter((r) => r.v >= 50_000)
      .sort((a, b) => b.v - a.v);
    return { i, when: ago(i), rows, total: rows.reduce((s, r) => s + r.v, 0), px: x((steps[i] ?? 0) + STEP_MS) };
  });
</script>

<section class="card tl" aria-labelledby="tl-title">
  <div class="hd">
    <h2 class="card-title" id="tl-title">{t("history.title")}</h2>
    {#if totalBps !== null}
      <span class="total num" aria-live="off"
        >{lineBps
          ? t("history.total", { used: mbps(totalBps), line: mbps(lineBps) })
          : t("history.total.noline", { used: mbps(totalBps) })}</span
      >
    {/if}
    <div class="dir" role="group" aria-label={t("dir.label")}>
      {#each dirs as d (d.id)}
        <button type="button" aria-pressed={ui.dir === d.id} onclick={() => (ui.dir = d.id)}
          ><Glyph name={d.glyph} size={15} />{t(`dir.${d.id}`)}</button
        >
      {/each}
    </div>
  </div>
  <div class="chart" class:lit={lit !== null && bands.some((b) => b.key === lit)} bind:clientWidth={width}>
    {#if width > 0}
      <svg
        viewBox="0 0 {width} {TOP + H + AXIS}"
        height={TOP + H + AXIS}
        role="img"
        aria-label={t("history.label")}
        onpointerenter={enter}
        onpointermove={point}
        onpointerleave={leave}
      >
        <line class="grid" x1="0" x2={width} y1={TOP + H} y2={TOP + H} />
        {#if lineBps}
          <line class="grid" x1="0" x2={width} y1={y(lineBps / 2)} y2={y(lineBps / 2)} />
        {/if}
        {#each paths as p (p.key)}
          <g class="band" class:on={lit === p.key}>
            <path class="fill" fill={p.color} d={p.fill} />
            <path class="edge" stroke={p.color} d={p.edge} />
          </g>
        {/each}
        {#if lineBps}
          <line class="ceil" x1="0" x2={width} y1={y(lineBps)} y2={y(lineBps)} />
          <text class="ceil-t num" x={width} y={y(lineBps) - 5} text-anchor="end"
            >{t("history.speed", { line: mbps(lineBps) })}</text
          >
        {/if}
        {#each shownMarks as m, i (`${m.t}:${i}`)}
          <g class="mark">
            <line x1={m.mx} x2={m.mx} y1={TOP - 4} y2={TOP + H} />
            <circle cx={m.mx} cy={TOP - 4} r="2.5" />
            {#if m.label}
              <rect x={m.from} y={TOP - 13} width={m.w} height="17" rx="5" />
              <text x={m.from + 6} y={TOP}>{m.text}</text>
            {/if}
          </g>
        {/each}
        {#if reading}
          <line class="cross" x1={reading.px} x2={reading.px} y1={TOP - 6} y2={TOP + H} />
        {/if}
        <text class="xl" x="0" y={TOP + H + 15}>{t("history.from")}</text>
        <text class="xl" x={width / 2} y={TOP + H + 15} text-anchor="middle">{t("history.mid")}</text>
        <text class="xl" x={width} y={TOP + H + 15} text-anchor="end">{t("history.to")}</text>
      </svg>
    {/if}
    {#if !enough}
      <p class="empty">{t("history.empty")}</p>
    {/if}
    {#if reading}
      <div
        class="tip"
        aria-hidden="true"
        style:left="{reading.px + 14 + 190 > width ? reading.px - 204 : reading.px + 14}px"
      >
        <div class="when">{reading.when}</div>
        {#if reading.rows.length > 0}
          {#each reading.rows as r (r.key)}
            <div class="r">
              <i style:background={colors.get(r.key) ?? "var(--stone-1)"}></i><span class="n">{r.name}</span><span
                class="v num">{mbps(r.v)}</span
              >
            </div>
          {/each}
          <div class="r tot">
            <span class="n">{t("history.all")}</span><span class="v num">{mbps(reading.total)} {t("line.unit")}</span>
          </div>
        {:else}
          <div class="r">{t("history.nothing")}</div>
        {/if}
      </div>
    {/if}
  </div>
  {#if summary}
    <p class="visually-hidden">{summary}</p>
  {/if}
</section>

<style>
  .tl {
    min-width: 0;
  }
  .hd {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 14px;
    padding: 16px 16px 4px 20px;
  }
  .total {
    margin-left: auto;
    font-size: 14px;
    color: var(--text-secondary);
  }
  /* Download / Upload: a small segmented choice at the header's end. */
  .dir {
    display: flex;
    padding: 2px;
    border-radius: 9px;
    background: var(--surface-side);
    box-shadow: inset 0 0 0 1px var(--stroke-card);
  }
  .dir button {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 9px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 650;
    transition:
      background-color 120ms linear,
      color 120ms linear;
  }
  .dir button:hover {
    color: var(--text-primary);
  }
  .dir button[aria-pressed="true"] {
    background: var(--surface-3);
    color: var(--text-primary);
  }
  .dir button:focus-visible {
    outline-offset: -1px;
  }
  .chart {
    position: relative;
    margin: 6px 20px 14px;
  }
  svg {
    display: block;
    width: 100%;
    overflow: visible;
    touch-action: pan-y;
  }
  .grid {
    stroke: var(--stroke-card);
  }
  .ceil {
    stroke: var(--text-tertiary);
    stroke-dasharray: 3 4;
  }
  .ceil-t,
  .xl {
    fill: var(--text-tertiary);
    font-size: 11.5px;
  }
  .band {
    transition: opacity 200ms var(--ease-out);
  }
  .fill {
    fill-opacity: 0.5;
  }
  .edge {
    fill: none;
    stroke-width: 1.5;
    stroke-linejoin: round;
  }
  .chart.lit .band {
    opacity: 0.18;
  }
  .chart.lit .band.on {
    opacity: 1;
  }
  .mark line {
    stroke: var(--accent);
    stroke-opacity: 0.38;
    stroke-dasharray: 2 3;
  }
  .mark circle {
    fill: var(--accent);
  }
  .mark rect {
    fill: var(--surface-1);
  }
  .mark text {
    fill: var(--text-secondary);
    font-size: 11.5px;
  }
  .cross {
    stroke: var(--accent);
    stroke-opacity: 0.5;
  }
  .empty {
    position: absolute;
    inset: 18px 0 20px;
    display: grid;
    place-items: center;
    margin: 0;
    padding: 0 24px;
    font-size: 13.5px;
    text-align: center;
    color: var(--text-tertiary);
  }
  .tip {
    position: absolute;
    z-index: 3;
    top: 6px;
    width: 190px;
    padding: 9px 11px;
    border-radius: 10px;
    background: var(--surface-3);
    box-shadow: var(--shadow-raised);
    font-size: 12.5px;
    pointer-events: none;
  }
  .when {
    margin-bottom: 4px;
    color: var(--text-secondary);
  }
  .r {
    display: flex;
    align-items: center;
    gap: 7px;
    line-height: 20px;
  }
  .r i {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }
  .n {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .v {
    margin-left: auto;
    padding-left: 10px;
    font-weight: 700;
  }
  .tot {
    margin-top: 4px;
    padding-top: 4px;
    border-top: 1px solid var(--stroke-card);
    color: var(--text-secondary);
  }
  @media (forced-colors: active) {
    .fill {
      fill-opacity: 1;
    }
    .dir button[aria-pressed="true"] {
      outline: 2px solid Highlight;
    }
  }
</style>
