<script lang="ts">
  // Settings → About → Updates (PLAN.md §17.2): check the release feed when
  // asked, then install with one more click. Installing runs the signed
  // installer, which stops the service for a few seconds and restarts the
  // app, so it never happens without that click.
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "../i18n";

  type Check =
    | { kind: "up_to_date" }
    | { kind: "available"; version: string; notes: string | null }
    | { kind: "failed"; message: string };

  type Status =
    | { kind: "idle" }
    | { kind: "checking" }
    | { kind: "installing"; version: string }
    | Check;

  let status = $state<Status>({ kind: "idle" });

  async function check() {
    status = { kind: "checking" };
    try {
      status = await invoke<Check>("check_update");
    } catch (e) {
      status = { kind: "failed", message: String(e) };
    }
  }

  async function install(version: string) {
    status = { kind: "installing", version };
    try {
      // Returns only on failure: on success the installer takes over.
      await invoke("install_update");
    } catch (e) {
      status = { kind: "failed", message: String(e) };
    }
  }

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
  const busy = $derived(status.kind === "checking" || status.kind === "installing");
</script>

<div class="item">
  <div class="words">
    <b>{t("settings.update")}</b>
    <span aria-live="polite" title={status.kind === "failed" ? status.message : undefined}>{body}</span>
  </div>
  {#if status.kind === "available"}
    {@const version = status.version}
    <button type="button" class="btn btn-accent" onclick={() => void install(version)}>
      {t("settings.update.install")}
    </button>
  {:else}
    <button type="button" class="btn" disabled={busy} onclick={() => void check()}>
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
