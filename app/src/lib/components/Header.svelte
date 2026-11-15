<script lang="ts">
  // Header (PLAN.md §14.2): the active profile as the page title, live
  // totals against the line, and the Low-latency indicator. Profile
  // switching, ping and Pause arrive with Phases 4–5.
  import { mbps, totalBps } from "../format";
  import { t } from "../i18n";
  import { service } from "../service.svelte";

  const down = $derived(service.stats?.down ?? null);
  const up = $derived(service.stats?.up ?? null);
  const line = $derived(service.snapshot?.config.line ?? {});
  const lowLatency = $derived(service.snapshot?.config.settings.low_latency ?? false);
  const live = $derived(service.link.kind === "connected" && service.stats !== null);
</script>

<div class="header">
  <h1>{service.activeProfile?.name ?? t("app.name")}</h1>

  <dl class="rates" aria-live="off">
    {#each [{ key: "down", glyph: "", now: down, mbpsLine: line.down_mbps }, { key: "up", glyph: "", now: up, mbpsLine: line.up_mbps }] as d (d.key)}
      <div class="rate">
        <dt>
          <i class="icon" aria-hidden="true">{d.glyph}</i>
          <span class="visually-hidden">{d.key === "down" ? t("header.down") : t("header.up")}</span>
        </dt>
        <dd>
          <span class="value num" class:idle={!live}>{live ? mbps(totalBps(d.now)) : "–"}</span>
          <span class="of num">
            {d.mbpsLine !== undefined
              ? t("header.ofLine", { line: mbps(d.mbpsLine * 1e6) })
              : t("header.notShaped")}
          </span>
        </dd>
      </div>
    {/each}
    {#if lowLatency}
      <div class="pill">
        <span class="dot" aria-hidden="true"></span>{t("header.lowLatency")}
      </div>
    {/if}
  </dl>
</div>

<style>
  .header {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 24px;
    flex-wrap: wrap;
    padding: 12px var(--gutter) 20px;
  }
  h1 {
    margin: 0;
    font: 600 28px/36px var(--font-display);
  }
  .rates {
    display: flex;
    align-items: center;
    gap: 24px;
    margin: 0;
  }
  .rate {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  dt .icon {
    font-size: 14px;
    color: var(--text-secondary);
  }
  dd {
    margin: 0;
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .value {
    font: 600 20px/28px var(--font-display);
    font-variant-numeric: tabular-nums;
    min-width: 3.2ch;
    text-align: right;
  }
  .value.idle {
    color: var(--text-tertiary);
  }
  .of {
    font-size: 12px;
    line-height: 16px;
    color: var(--text-secondary);
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 10px;
    border-radius: 12px;
    background: var(--accent-tint);
    color: var(--accent-text);
    font-size: 12px;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
</style>
