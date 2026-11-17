<script lang="ts">
  // Who is using the line right now (PLAN.md §14.2): one strip as long as
  // the line, a part per slice (Everything else per app) in its ink, and
  // the bit low-latency mode keeps free hatched at the end. Below it the
  // biggest users by name, and what all of it adds up to. Download or
  // upload: the switch on the right.
  import { mbps } from "../format";
  import { t } from "../i18n";
  import { parts, type Ledger } from "../ledger";
  import { ui, type Dir } from "../ui.svelte";
  import AppIcon from "./AppIcon.svelte";
  import Segmented from "./Segmented.svelte";

  let { ledger, lineBps }: { ledger: Ledger; lineBps: number | null } = $props();

  const list = $derived(parts(ledger));
  /** The strip's length: the line, else what SnailNet shares out. */
  const scale = $derived(Math.max(lineBps ?? ledger.shaped ?? 0, ledger.total, 1));
  const headroom = $derived(
    lineBps !== null && ledger.shaped !== null ? Math.max(0, lineBps - ledger.shaped) : 0,
  );
  const top = $derived([...list].sort((a, b) => b.bps - a.bps).filter((p) => p.target !== null).slice(0, 3));
  const rest = $derived(ledger.total - top.reduce((s, p) => s + p.bps, 0));
  const used = $derived(
    lineBps !== null
      ? t("strip.used", { used: mbps(ledger.total), line: mbps(lineBps) })
      : t("strip.usedOnly", { used: mbps(ledger.total) }),
  );
  const kept = $derived(headroom > 0 ? t("strip.free", { mbps: mbps(headroom) }) : null);
  // The strip's description carries the "kept free" part too: narrow
  // windows drop it from the labels below.
  const summary = $derived(
    t("strip.summary", {
      dir: t(ui.dir === "down" ? "dir.down" : "dir.up"),
      parts: list.map((p) => t("strip.part", { name: p.name, mbps: mbps(p.bps) })).join(", ") || "–",
      used: kept ? t("strip.usedKept", { used, kept }) : used,
    }),
  );
  const dirs: { value: Dir; label: string }[] = [
    { value: "down", label: t("dir.down") },
    { value: "up", label: t("dir.up") },
  ];

  const pct = (bps: number) => `${(bps / scale) * 100}%`;
  const ink = (ink: number, shade: number) => (ink >= 0 ? `var(--ink-${ink})` : `var(--stone-${shade})`);
</script>

<section class="strip" aria-label={t(ui.dir === "down" ? "strip.label.down" : "strip.label.up")}>
  <div class="bar" role="img" aria-label={summary}>
    {#each list as p (p.key)}
      <i style:width={pct(p.bps)} style:background={ink(p.ink, p.shade)}></i>
    {/each}
    {#if headroom > 0}
      <i class="free" style:width={pct(headroom)}></i>
    {/if}
  </div>
  <div class="labels">
    {#each top as p (p.key)}
      <span class="who">
        <AppIcon target={p.target} name={p.name} size={16} /><span class="nm">{p.name}</span><b>{mbps(p.bps)}</b>
      </span>
    {/each}
    {#if top.length > 0 && rest >= 50_000}
      <span class="who"><span class="nm">{t("strip.others")}</span><b>{mbps(rest)}</b></span>
    {/if}
    <span class="sum">
      <b>{used}</b>{#if kept}<span class="kept"><span class="dot" aria-hidden="true">·</span>{kept}</span>{/if}
    </span>
    <Segmented name="dir" label={t("dir.label")} options={dirs} value={ui.dir} onchange={(d) => (ui.dir = d)} small />
  </div>
</section>

<style>
  .strip {
    margin: 20px 0 0;
  }
  .bar {
    display: flex;
    gap: 3px;
    height: 12px;
    border-radius: 4px;
    background: var(--surface-1);
    overflow: hidden;
  }
  .bar i {
    display: block;
    flex: 0 1 auto;
    min-width: 2px;
    height: 100%;
    border-radius: 3px;
    transition: width 400ms var(--ease-out);
  }
  .bar i.free {
    margin-left: auto;
    background: var(--free-hatch);
  }
  .labels {
    display: flex;
    align-items: center;
    gap: 20px;
    margin-top: 10px;
    font-size: 14px;
    color: var(--text-secondary);
    white-space: nowrap;
  }
  /* The names give way first: they shrink and end in "…", while the
     figures, the total and the switch keep their size. */
  .who {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
  }
  .nm {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  b {
    flex: none;
    color: var(--text-primary);
    font-weight: 600;
  }
  .sum {
    flex: none;
    margin-left: auto;
  }
  .dot {
    margin: 0 6px;
  }
  @media (max-width: 860px) {
    .who:nth-child(3),
    .who:nth-child(4),
    .kept {
      display: none;
    }
  }
  @media (forced-colors: active) {
    .bar i {
      forced-color-adjust: none;
      outline: 1px solid CanvasText;
    }
  }
</style>
