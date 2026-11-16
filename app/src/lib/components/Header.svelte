<script lang="ts">
  // Header (PLAN.md §14.2): the active profile as the page title, live
  // totals against the line, and the Low-latency indicator. Profile
  // switching, ping and Pause arrive with Phases 4–5.
  import { mbps, totalBps } from "../format";
  import { t } from "../i18n";
  import { lineDialog } from "../lineDialog.svelte";
  import { service } from "../service.svelte";

  const down = $derived(service.stats?.down ?? null);
  const up = $derived(service.stats?.up ?? null);
  const line = $derived(service.snapshot?.config.line ?? {});
  const lowLatency = $derived(service.snapshot?.config.settings.low_latency ?? false);
  const live = $derived(service.link.kind === "connected" && service.stats !== null);
  const canSet = $derived(service.link.kind === "connected" && service.snapshot !== null);

  /** "of 25.0 Mbps", or "not shared out" without a speed. */
  const of = (lineMbps: number | undefined) =>
    lineMbps !== undefined ? t("header.ofLine", { line: mbps(lineMbps * 1e6) }) : t("header.notShaped");

  const rates = $derived([
    { key: "down" as const, glyph: "", label: t("header.down"), now: down, of: of(line.down_mbps) },
    { key: "up" as const, glyph: "", label: t("header.up"), now: up, of: of(line.up_mbps) },
  ]);
</script>

<div class="header">
  <h1>{service.activeProfile?.name ?? t("app.name")}</h1>

  <dl class="rates" aria-live="off">
    {#each rates as d (d.key)}
      <div class="rate">
        <dt>
          <i class="icon" aria-hidden="true">{d.glyph}</i>
          <span class="visually-hidden">{d.label}</span>
        </dt>
        <dd>
          <span class="value num" class:idle={!live}>{live ? mbps(totalBps(d.now)) : "–"}</span>
          {#if canSet}
            <button
              type="button"
              class="of num"
              aria-label={t("line.change", {
                dir: d.label,
                value: d.of,
              })}
              onclick={() => lineDialog.show(d.key)}
            >
              {d.of}<i class="icon edit" aria-hidden="true">&#xE70F;</i>
            </button>
          {:else}
            <span class="of num">{d.of}</span>
          {/if}
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
  /* The line figure doubles as the way to change it: a quiet text button
     (Fluent subtle button) with a pencil so it reads as editable. */
  button.of {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-height: 24px;
    margin: -4px -6px;
    padding: 4px 6px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    font-family: inherit;
  }
  button.of:hover {
    background: var(--fill-subtle-hover);
    color: var(--text-primary);
  }
  button.of:active {
    background: var(--fill-subtle-pressed);
  }
  .edit {
    font-size: 10px;
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
