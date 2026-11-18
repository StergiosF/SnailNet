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
  import { measureLine, resetAll, resetSplit, setFirstChoice } from "../actions";
  import { stateKind } from "../format";
  import { t } from "../i18n";
  import { lineDialog } from "../lineDialog.svelte";
  import { modeName } from "../modes";
  import { service } from "../service.svelte";
  import type { Settings } from "../types";
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
    invoke<boolean>("hotkey_enabled")
      .then((on) => (hotkey = on))
      .catch(() => (hotkey = null));
  });

  // Ctrl+Alt+S switches mode (§11.2). Off when turned off, or when another
  // program holds those keys; the switch says which by refusing to stay on.
  let hotkey = $state<boolean | null>(null);
  let hotkeyTaken = $state(false);
  async function setHotkey(on: boolean) {
    try {
      hotkey = await invoke<boolean>("set_hotkey", { on });
      hotkeyTaken = on && !hotkey;
    } catch {
      hotkey = !on;
      hotkeyTaken = on;
    }
  }

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
  // Windows' metered flag (§10.3), read again whenever the network changes.
  let metered = $state(false);
  $effect(() => {
    void network?.id;
    void ui.online;
    let live = true;
    invoke<boolean>("network_metered")
      .then((m) => live && (metered = m))
      .catch(() => live && (metered = false));
    return () => (live = false);
  });
  const measuring = $derived(service.state !== null && stateKind(service.state) === "measuring");
  const measuredOn = $derived(
    measured ? new Date(measured.at * 1000).toLocaleDateString(undefined, { day: "numeric", month: "short" }) : "",
  );
  let pinning = $state(false);
  let focusing = $state(false);
  const focusBoost = $derived(service.snapshot?.config.settings.focus_boost !== false);
  const gamingWhilePlaying = $derived(service.snapshot?.config.settings.gaming_while_playing === true);
  // The apps the user marked from their Boost menu (§11.5 overrides).
  const choices = $derived.by(() => {
    const s = service.snapshot?.config.settings;
    const name = (id: string) =>
      service.snapshot?.apps.find((a) => a.family.toLowerCase() === id.toLowerCase())?.name ?? id;
    return [
      ...(s?.games ?? []).map((id) => ({ id, name: name(id), kind: t("first.list.game") })),
      ...(s?.never_first ?? []).map((id) => ({ id, name: name(id), kind: t("first.list.never") })),
    ];
  });

  async function setSetting(change: Partial<Settings>) {
    focusing = true;
    try {
      await service.setSettings(change);
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
        {#if metered}
          <span class="metered"><i class="icon" aria-hidden="true">&#xE7BA;</i>{t("settings.metered")}</span>
        {/if}
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
        onchange={(e) => void setSetting({ focus_boost: e.currentTarget.checked })}
      />
    </label>
    <label class="item">
      <div class="words">
        <b>{t("gaming.title")}</b>
        <span>{t("gaming.body")}</span>
      </div>
      <input
        type="checkbox"
        role="switch"
        class="switch"
        checked={gamingWhilePlaying}
        disabled={!canEdit || focusing}
        onchange={(e) => void setSetting({ gaming_while_playing: e.currentTarget.checked })}
      />
    </label>
    {#if choices.length > 0}
      <div class="item">
        <div class="words">
          <b>{t("first.list.title")}</b>
          <span>{t("first.list.body")}</span>
          <ul class="nets choices">
            {#each choices as c (c.id)}
              <li>
                <span class="net">{c.name}</span>
                <span>{c.kind}</span>
                <button
                  type="button"
                  class="btn btn-quiet"
                  disabled={!canEdit}
                  aria-label={t("first.list.remove.label", { name: c.name })}
                  onclick={() => void setFirstChoice(c.id, c.name, "auto")}>{t("first.list.remove")}</button
                >
              </li>
            {/each}
          </ul>
        </div>
      </div>
    {/if}
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
    <label class="item">
      <div class="words">
        <b>{t("settings.hotkey")}</b>
        <span>{hotkeyTaken ? t("settings.hotkey.taken") : t("settings.hotkey.body")}</span>
      </div>
      <input
        type="checkbox"
        role="switch"
        class="switch"
        checked={hotkey === true}
        disabled={hotkey === null}
        onchange={(e) => void setHotkey(e.currentTarget.checked)}
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
  .metered {
    display: flex;
    gap: 6px;
    align-items: baseline;
    margin-top: 4px;
  }
  .metered .icon {
    flex: none;
    font-size: 12px;
    color: var(--caution);
  }
  .choices li {
    align-items: center;
  }
  .choices .net {
    flex: 1;
    min-width: 0;
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
