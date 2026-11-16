<script lang="ts">
  // The top of the main window (PLAN.md §14.2): one sentence that answers
  // "who is using my internet, and is it a problem?", or, when something
  // needs attention (§14.8), that instead, in the same place. On the
  // right, the line as the user set it; it opens the speed dialog.
  import { t } from "../i18n";
  import type { Glance } from "../ledger";
  import { lineDialog } from "../lineDialog.svelte";
  import { service } from "../service.svelte";
  import type { Notice } from "../ui.svelte";

  let { glance, notice }: { glance: Glance | null; notice: Notice | null } = $props();

  const line = $derived(service.snapshot?.config.line ?? {});
  const canSet = $derived(service.link.kind === "connected" && service.snapshot !== null);
  const fig = (v: number | undefined) => (v === undefined ? "–" : String(Math.round(v * 10) / 10));
  const hasLine = $derived(line.down_mbps !== undefined || line.up_mbps !== undefined);

  const glyph = { info: "", caution: "", critical: "" };
</script>

<header class="head">
  <div class="words">
    {#if notice}
      <h1 class="notice {notice.severity}">
        <i class="icon" aria-hidden="true">{glyph[notice.severity]}</i>{notice.title}
      </h1>
      <p role={notice.severity === "info" ? "status" : "alert"}>{notice.message}</p>
      {#if notice.action}
        <button type="button" class="btn btn-accent action" onclick={notice.action.onclick}>
          {notice.action.label}
        </button>
      {/if}
    {:else if glance}
      <h1>{glance.title}</h1>
      <p>{glance.detail}</p>
    {:else}
      <div class="loading" aria-hidden="true"><span></span></div>
    {/if}
  </div>

  {#if canSet}
    <button
      type="button"
      class="line"
      onclick={() => lineDialog.show()}
      aria-label={hasLine
        ? t("header.line.label", { down: fig(line.down_mbps), up: fig(line.up_mbps) })
        : t("header.line.set")}
    >
      {#if hasLine}
        <span class="k">{t("header.line")}</span>
        <span class="v">{fig(line.down_mbps)}<small>↓</small> {fig(line.up_mbps)}<small>↑ Mbps</small></span>
      {:else}
        <span class="v">{t("header.line.set")}</span>
      {/if}
      <i class="icon pen" aria-hidden="true">&#xE70F;</i>
    </button>
  {/if}
</header>

<style>
  .head {
    display: flex;
    align-items: flex-start;
    gap: 32px;
    padding: 20px var(--gutter) 0;
  }
  .words {
    flex: 1;
    min-width: 0;
  }
  h1 {
    margin: 0;
    font-size: 28px;
    line-height: 34px;
    font-weight: 700;
    letter-spacing: -0.015em;
    text-wrap: balance;
  }
  h1 .icon {
    font-size: 22px;
    margin-right: 12px;
    vertical-align: -1px;
  }
  .caution .icon {
    color: var(--caution);
  }
  .critical .icon {
    color: var(--critical);
  }
  p {
    margin: 6px 0 0;
    max-width: 68ch;
    color: var(--text-secondary);
    font-size: 16px;
    line-height: 1.45;
  }
  .action {
    margin-top: 14px;
  }
  .loading span {
    display: block;
    width: 60%;
    height: 22px;
    margin: 6px 0;
    border-radius: 6px;
    background: var(--skeleton);
  }
  .line {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: none;
    height: 40px;
    padding: 0 14px;
    border: 0;
    border-radius: var(--radius-control);
    background: var(--surface-1);
    transition: background-color 120ms linear;
  }
  .line:hover {
    background: var(--surface-2);
  }
  .k {
    font-size: 13px;
    color: var(--text-secondary);
  }
  .v {
    font-size: 16px;
    font-weight: 600;
  }
  small {
    font-size: 13px;
    font-weight: 400;
    color: var(--text-secondary);
    margin-left: 1px;
  }
  .pen {
    font-size: 12px;
    color: var(--text-secondary);
  }
  @media (forced-colors: active) {
    .line {
      border: 1px solid ButtonText;
    }
  }
</style>
