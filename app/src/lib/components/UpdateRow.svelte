<script lang="ts">
  // Settings → Updates (PLAN.md §17.2): what the last check found (the app
  // checks by itself too, update.svelte.ts), Check for updates, and Install
  // once a newer version is out. Installing runs the signed installer,
  // which stops the service for a few seconds and restarts the app, so it
  // never happens without that click.
  import { t } from "../i18n";
  import { updates } from "../update.svelte";

  const status = $derived(updates.status);
  const body = $derived.by(() => {
    switch (status.kind) {
      case "idle":
        return t("settings.update.body");
      case "checking":
        return t("settings.update.checking");
      case "up_to_date":
        return t("settings.update.upToDate");
      case "available":
        return t("settings.update.available", { version: status.version });
      case "installing":
        return t("settings.update.installing", { version: status.version });
      case "failed":
        return t("settings.update.failed");
    }
  });
</script>

<div class="item">
  <div class="words">
    <b>{t("settings.update")}</b>
    <span aria-live="polite" title={status.kind === "failed" ? status.message : undefined}>{body}</span>
  </div>
  {#if status.kind === "available"}
    {@const version = status.version}
    <button type="button" class="btn btn-accent" onclick={() => void updates.install()}>
      {t("settings.update.install", { version })}
    </button>
  {:else}
    <button type="button" class="btn" disabled={updates.busy} onclick={() => void updates.check()}>
      {status.kind === "failed" ? t("settings.update.retry") : t("settings.update.check")}
    </button>
  {/if}
</div>

<style>
  .item {
    display: flex;
    align-items: center;
    gap: 16px;
    min-height: 68px;
    padding: 12px 18px;
    border-top: 1px solid var(--surface-ground);
  }
  .words {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  b {
    font-size: 15px;
    font-weight: 700;
  }
  .words span {
    font-size: 14px;
    line-height: 1.4;
    color: var(--text-secondary);
  }
  .btn {
    flex: none;
  }
  @media (forced-colors: active) {
    .item {
      border-top-color: CanvasText;
    }
  }
</style>
