<script lang="ts">
  // Settings (PLAN.md §14.6), one scrolling page of groups: the connection
  // (speed, Measure, the last test and its ping, following the line
  // automatically), starting with Windows, the two resets (this mode's split; all
  // of SnailNet, which shows setup again), and what version is running.
  // More arrives with the features behind it (low-ping strength, pause,
  // notifications, hotkeys).
  import { getVersion } from "@tauri-apps/api/app";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { measureLine, resetAll, resetSplit } from "../actions";
  import { stateKind } from "../format";
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
  const measured = $derived(line.measured);
  const network = $derived(service.snapshot?.config.network);
  // Other networks with a speed, most recently left first.
  const others = $derived(
    (service.snapshot?.config.networks ?? [])
      .filter((n) => n.line.down_mbps !== undefined || n.line.up_mbps !== undefined)
      .sort((a, b) => b.left - a.left),
  );
  const measuring = $derived(service.state !== null && stateKind(service.state) === "measuring");
  const measuredOn = $derived(
    measured ? new Date(measured.at * 1000).toLocaleDateString(undefined, { day: "numeric", month: "short" }) : "",
  );
  let pinning = $state(false);
  let focusing = $state(false);
  const focusBoost = $derived(service.snapshot?.config.settings.focus_boost !== false);

  async function setFocusBoost(on: boolean) {
    focusing = true;
    try {
      await service.setFocusBoost(on);
    } catch {
      // The switch shows the saved value again from the next snapshot.
    } finally {
      focusing = false;
    }
  }

  async function setAdapt(adapt: boolean) {
    pinning = true;
    try {
      await service.setPinned(!adapt);
    } catch {
      // The switch shows the saved value again from the next snapshot.
    } finally {
      pinning = false;
    }
  }
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
  <h2 class="group-title" id="set-conn">{t("settings.connection")}</h2>
  <div class="card group">
    <div class="item">
      <div class="words">
        <b>{t("settings.line")}</b>
        {#if network}
          <span class="net">{t("settings.line.network", { network: network.name })}</span>
        {/if}
        <span
          >{hasLine
            ? t("settings.line.value", { down: fig(line.down_mbps), up: fig(line.up_mbps) })
            : t("sidebar.line.none")}</span
        >
      </div>
      <div class="pair">
        <button type="button" class="btn" disabled={!canEdit || measuring} onclick={() => void measureLine()}>
          {measuring ? t("measure.running") : t("measure.button")}
        </button>
        <button type="button" class="btn btn-quiet" disabled={!canEdit} onclick={() => lineDialog.show()}>
          {t("settings.line.change")}
        </button>
      </div>
    </div>
    {#if measured}
      <div class="item">
        <div class="words">
          <b>{t("measure.last", { date: measuredOn })}</b>
          <span class="num">{t("settings.line.value", { down: fig(measured.down_mbps), up: fig(measured.up_mbps) })}</span>
        </div>
      </div>
      {#if measured.idle_ms !== undefined && measured.loaded_ms !== undefined}
        <div class="item">
          <div class="words">
            <b>{t("ping.title")}</b>
            <span class="num"
              >{t("ping.body", { idle: Math.round(measured.idle_ms), loaded: Math.round(measured.loaded_ms) })}</span
            >
          </div>
        </div>
      {/if}
    {/if}
    {#if others.length > 0}
      <div class="item">
        <div class="words">
          <b>{t("settings.networks")}</b>
          <span>{t("settings.networks.body")}</span>
          <ul class="nets">
            {#each others as n (n.id)}
              <li>
                <span class="net">{n.name}</span>
                <span class="num">{t("settings.line.value", { down: fig(n.line.down_mbps), up: fig(n.line.up_mbps) })}</span>
              </li>
            {/each}
          </ul>
        </div>
      </div>
    {/if}
    <label class="item">
      <div class="words">
        <b>{t("adapt.title")}</b>
        <span>{t("adapt.body")}</span>
      </div>
      <input
        type="checkbox"
        role="switch"
        class="switch"
        checked={line.pinned !== true}
        disabled={!canEdit || !hasLine || pinning}
        onchange={(e) => void setAdapt(e.currentTarget.checked)}
      />
    </label>
    <label class="item">
      <div class="words">
        <b>{t("focus.title")}</b>
        <span>{t("focus.body")}</span>
      </div>
      <input
        type="checkbox"
        role="switch"
        class="switch"
        checked={focusBoost}
        disabled={!canEdit || focusing}
        onchange={(e) => void setFocusBoost(e.currentTarget.checked)}
      />
    </label>
  </div>
</section>

<section aria-labelledby="set-win">
  <h2 class="group-title" id="set-win">{t("settings.windows")}</h2>
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
  <h2 class="group-title" id="set-reset">{t("settings.reset")}</h2>
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
  <h2 class="group-title" id="set-about">{t("settings.about")}</h2>
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
  .group-title {
    margin: 0 0 10px 4px;
    font-size: 14px;
    line-height: 20px;
    font-weight: 700;
    color: var(--text-secondary);
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
  .pair {
    display: flex;
    flex: none;
    gap: 8px;
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
  .words .net {
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .nets {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 6px 0 0;
    padding: 0;
    list-style: none;
  }
  .nets li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
  }
  .nets .num {
    flex: none;
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
