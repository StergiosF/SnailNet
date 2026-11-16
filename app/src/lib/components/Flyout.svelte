<script lang="ts">
  // The tray flyout (PLAN.md §14.4): a glance while a game launches or a
  // video stalls. Total speeds, what SnailNet is doing, the three busiest
  // apps or sites with their slice, and the way to the main window. It
  // hides on Escape and when it loses focus (tray.rs). Profile pills and
  // Pause come with Phase 5.
  import { getCurrentWindow, Window } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { mbps, stateKind, targetLabel, totalBps } from "../format";
  import { t } from "../i18n";
  import { listRows, ruleChip } from "../rules";
  import { service } from "../service.svelte";
  import { startSystem } from "../system";
  import Snail from "./Snail.svelte";

  const TOP = 3;

  onMount(() => {
    void startSystem();
    service.start().catch((e: unknown) => {
      service.link = { kind: "lost", detail: String(e) };
    });
  });

  const rules = $derived(service.activeProfile?.rules ?? []);
  const rows = $derived(
    listRows(service.stats?.down, service.stats?.up, rules, (target) =>
      targetLabel(target, service.snapshot?.apps ?? [], service.siteNames),
    )
      .filter((r) => !r.idle)
      .slice(0, TOP),
  );
  const down = $derived(totalBps(service.stats?.down));
  const up = $derived(totalBps(service.stats?.up));

  /** One line on what SnailNet is doing, and whether it needs a look. */
  const status = $derived.by((): { text: string; tone: "ok" | "info" | "caution" } => {
    const link = service.link;
    if (link.kind === "connecting") return { text: t("link.connecting"), tone: "info" };
    if (link.kind === "not_running") return { text: t("link.not_running.title"), tone: "caution" };
    if (link.kind === "lost") return { text: t("link.lost.title"), tone: "caution" };
    if (link.kind !== "connected") return { text: t("flyout.cantControl"), tone: "caution" };
    const s = service.state;
    if (s === null) return { text: t("link.connecting"), tone: "info" };
    switch (stateKind(s)) {
      case "starting":
        return { text: t("state.starting"), tone: "info" };
      case "unshaped":
        return { text: t("state.unshaped.title"), tone: "info" };
      case "retrying":
        return { text: t("state.retrying.title"), tone: "caution" };
      case "safe_mode":
        return { text: t("state.safe_mode.title"), tone: "caution" };
      case "shaping": {
        const lowLatency = service.snapshot?.config.settings.low_latency ?? false;
        return { text: t(lowLatency ? "flyout.sharing.lowLatency" : "flyout.sharing"), tone: "ok" };
      }
    }
  });

  async function openMain() {
    const main = await Window.getByLabel("main");
    if (main) {
      await main.show();
      await main.unminimize();
      await main.setFocus();
    }
    await getCurrentWindow().hide();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") void getCurrentWindow().hide();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="flyout">
  <header>
    <h1>{t("app.name")}</h1>
    <p class="totals num">
      <span aria-label="{t('header.down')} {mbps(down)} Mbps"><i class="icon" aria-hidden="true">&#xE896;</i>{mbps(down)}</span>
      <span aria-label="{t('header.up')} {mbps(up)} Mbps"><i class="icon" aria-hidden="true">&#xE898;</i>{mbps(up)}</span>
      <span class="unit" aria-hidden="true">Mbps</span>
    </p>
  </header>
  <p class="status" data-tone={status.tone}>
    <span class="dot" aria-hidden="true"></span>{status.text}
  </p>

  <section aria-labelledby="top-heading">
    <h2 id="top-heading">{t("flyout.top")}</h2>
    {#if rows.length === 0}
      <div class="quiet">
        <Snail size={32} />
        <p>{t("flyout.quiet")}</p>
      </div>
    {:else}
      <ul>
        {#each rows as r (r.key)}
          <li>
            <span class="name">{r.name}</span>
            <span class="speed num" aria-label="{t('list.down')} {mbps(r.down)} Mbps">
              <i class="icon" aria-hidden="true">&#xE896;</i>{mbps(r.down)}
            </span>
            <span class="chip" class:ruled={r.rule >= 0}>{r.fixed ? "" : ruleChip(rules, r.rule)}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <footer>
    <button type="button" class="accent-button" onclick={() => void openMain()}>
      {t("flyout.open")}<i class="icon" aria-hidden="true">&#xE8A7;</i>
    </button>
  </footer>
</div>

<style>
  .flyout {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 16px 16px 0;
    /* Windows 11 rounds the borderless window and draws its edge. */
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }
  h1 {
    margin: 0;
    font: 600 14px/20px var(--font-text);
  }
  .totals {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin: 0;
    font: 600 20px/28px var(--font-display);
  }
  .totals span {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .totals .icon {
    font-size: 12px;
    color: var(--text-secondary);
  }
  .totals .unit {
    font: 12px/16px var(--font-text);
    color: var(--text-secondary);
  }
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 4px 0 16px;
    font-size: 12px;
    line-height: 16px;
    color: var(--text-secondary);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 4px;
    background: var(--text-tertiary);
  }
  .status[data-tone="ok"] .dot {
    background: var(--accent);
  }
  .status[data-tone="caution"] .dot {
    background: var(--caution);
  }
  .status[data-tone="caution"] {
    color: var(--text-primary);
  }
  section {
    flex: 1;
    min-height: 0;
  }
  h2 {
    margin: 0 0 4px;
    font-size: 12px;
    line-height: 16px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    border: 1px solid var(--stroke-card);
    border-radius: var(--radius-card);
    background: var(--surface-card);
  }
  li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 64px 96px;
    align-items: center;
    gap: 12px;
    min-height: 40px;
    padding: 0 12px;
  }
  li + li {
    border-top: 1px solid var(--stroke-divider);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .speed {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 4px;
  }
  .speed .icon {
    font-size: 10px;
    color: var(--text-secondary);
  }
  .chip {
    justify-self: end;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    line-height: 16px;
    color: var(--text-secondary);
  }
  .chip.ruled {
    color: var(--accent-text);
    font-weight: 600;
  }
  .quiet {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    border: 1px solid var(--stroke-card);
    border-radius: var(--radius-card);
    background: var(--surface-card);
  }
  .quiet p {
    margin: 0;
    color: var(--text-secondary);
  }
  footer {
    display: flex;
    justify-content: flex-end;
    margin: 16px -16px 0;
    padding: 12px 16px;
    border-top: 1px solid var(--stroke-card);
    background: var(--surface-layer);
  }
  .accent-button {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    padding: 0 16px;
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: var(--accent);
    color: var(--on-accent);
    font: inherit;
    font-weight: 600;
  }
  .accent-button:hover {
    filter: brightness(1.08);
  }
  .accent-button .icon {
    font-size: 12px;
  }
</style>
