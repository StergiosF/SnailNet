<script lang="ts">
  // Settings (PLAN.md §14.6), one scrolling page of groups: the connection
  // speed, starting with Windows, the two resets (this mode's split; all
  // of SnailNet, which shows setup again), and what version is running.
  // More arrives with the features behind it (low-ping strength, pause,
  // notifications, hotkeys).
  import { getVersion } from "@tauri-apps/api/app";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { resetAll, resetSplit } from "../actions";
  import { t } from "../i18n";
  import { lineDialog } from "../lineDialog.svelte";
  import { modeName } from "../modes";
  import { service } from "../service.svelte";
  import { ui } from "../ui.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";

  let { canEdit }: { canEdit: boolean } = $props();

  let version = $state("");
  let autostart = $state<boolean | null>(null);
  let confirming = $state(false);
  let resetting = $state(false);

  onMount(() => {
    getVersion()
      .then((v) => (version = v))
      .catch(() => {});
    invoke<boolean>("autostart_enabled")
      .then((on) => (autostart = on))
      .catch(() => (autostart = null));
  });

  async function setAutostart(on: boolean) {
    try {
      autostart = await invoke<boolean>("set_autostart", { on });
    } catch {
      autostart = !on;
    }
  }

  const line = $derived(service.snapshot?.config.line ?? {});
  const fig = (v: number | undefined) => (v === undefined ? "–" : String(Math.round(v * 10) / 10));
  const hasLine = $derived(line.down_mbps !== undefined || line.up_mbps !== undefined);
  const mode = $derived(modeName(service.activeProfile));
  const serviceVersion = $derived(service.link.kind === "connected" ? service.link.service_version : null);

  async function doResetAll() {
    resetting = true;
    const ok = await resetAll();
    resetting = false;
    confirming = false;
    if (ok) ui.restartFirstRun();
  }
</script>

<h1>{t("settings.title")}</h1>

<section aria-labelledby="set-conn">
  <h2 class="eyebrow" id="set-conn">{t("settings.connection")}</h2>
  <div class="card group">
    <div class="item">
      <div class="words">
        <b>{t("settings.line")}</b>
        <span
          >{hasLine
            ? t("settings.line.value", { down: fig(line.down_mbps), up: fig(line.up_mbps) })
            : t("sidebar.line.none")}</span
        >
      </div>
      <button type="button" class="btn" disabled={!canEdit} onclick={() => lineDialog.show()}>
        {t("settings.line.change")}
      </button>
    </div>
  </div>
</section>

<section aria-labelledby="set-win">
  <h2 class="eyebrow" id="set-win">{t("settings.windows")}</h2>
  <div class="card group">
    <label class="item">
      <div class="words">
        <b>{t("settings.autostart")}</b>
        <span>{t("settings.autostart.body")}</span>
      </div>
      <input
        type="checkbox"
        role="switch"
        class="switch"
        checked={autostart === true}
        disabled={autostart === null}
        onchange={(e) => void setAutostart(e.currentTarget.checked)}
      />
    </label>
  </div>
</section>

<section aria-labelledby="set-reset">
  <h2 class="eyebrow" id="set-reset">{t("settings.reset")}</h2>
  <div class="card group">
    <div class="item">
      <div class="words">
        <b>{t("settings.resetMode", { mode })}</b>
        <span>{t("settings.resetMode.body", { mode })}</span>
      </div>
      <button type="button" class="btn" disabled={!canEdit} onclick={() => void resetSplit()}>
        {t("split.reset")}
      </button>
    </div>
    <div class="item">
      <div class="words">
        <b>{t("settings.resetAll")}</b>
        <span>{t("settings.resetAll.body")}</span>
      </div>
      <button type="button" class="btn" disabled={!canEdit} onclick={() => (confirming = true)}>
        {t("settings.resetAll.button")}
      </button>
    </div>
  </div>
</section>

<section aria-labelledby="set-about">
  <h2 class="eyebrow" id="set-about">{t("settings.about")}</h2>
  <div class="card group">
    <div class="item">
      <div class="words">
        <b>{t("settings.about.version", { app: version })}</b>
        {#if serviceVersion}<span>{t("settings.about.service", { version: serviceVersion })}</span>{/if}
      </div>
    </div>
  </div>
</section>

<ConfirmDialog
  open={confirming}
  title={t("settings.resetAll.title")}
  body={t("settings.resetAll.confirm")}
  confirm={t("settings.resetAll.ok")}
  busy={resetting}
  onconfirm={() => void doResetAll()}
  oncancel={() => (confirming = false)}
/>

<style>
  h1 {
    margin: 0 0 8px;
    font-size: 26px;
    line-height: 32px;
    font-weight: 800;
    letter-spacing: -0.015em;
  }
  section {
    margin-top: 20px;
  }
  .eyebrow {
    margin-bottom: 8px;
  }
  .group {
    display: flex;
    flex-direction: column;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 16px;
    min-height: 68px;
    padding: 12px 18px;
  }
  .item + .item {
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
  .switch {
    appearance: none;
    position: relative;
    flex: none;
    width: 44px;
    height: 24px;
    margin: 0;
    border-radius: 12px;
    background: var(--surface-3);
    transition: background-color 160ms linear;
  }
  .switch::after {
    content: "";
    position: absolute;
    top: 4px;
    left: 4px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--text-secondary);
    transition:
      transform 160ms var(--ease-out),
      background-color 160ms linear;
  }
  .switch:checked {
    background: var(--shell);
  }
  .switch:checked::after {
    transform: translateX(20px);
    background: var(--on-shell);
  }
  .switch:disabled {
    opacity: 0.5;
  }
  @media (forced-colors: active) {
    .item + .item {
      border-top-color: CanvasText;
    }
    .switch {
      border: 1px solid ButtonText;
    }
    .switch::after {
      forced-color-adjust: none;
      background: ButtonText;
    }
    .switch:checked::after {
      background: Highlight;
    }
  }
</style>
