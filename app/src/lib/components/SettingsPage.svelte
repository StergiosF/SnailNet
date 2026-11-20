<script lang="ts">
  // Settings (DESIGN.md, Settings): grouped by what people come for, with
  // an index beside the groups that follows the scroll: your line (speed,
  // Measure, the last test with its grades, following the line, other
  // networks); what goes first (calls, the app you're using, Gaming while
  // you play, the fix, the apps you marked); apps (how they are grouped);
  // startup and keys; about; and last, quietly, starting over. One line per
  // setting with a short "what it does"; lists you need once fold.
  import { getVersion } from "@tauri-apps/api/app";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { canMeasure, measureLine, putBack, resetAll, resetSplit, separate, setFirstChoice } from "../actions";
  import { groupings } from "../families";
  import { shortDate, stateKind } from "../format";
  import { fixer } from "../fixer.svelte";
  import { t } from "../i18n";
  import { lineDialog } from "../lineDialog.svelte";
  import { modeName } from "../modes";
  import { grade, rise, type Grade } from "../proof";
  import { service } from "../service.svelte";
  import type { Settings } from "../types";
  import { ui } from "../ui.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Dropdown from "./Dropdown.svelte";
  import Glyph, { type GlyphName } from "./Glyph.svelte";
  import UpdateRow from "./UpdateRow.svelte";

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
    invoke<HotkeyInfo>("hotkey_info")
      .then((info) => (hotkey = info))
      .catch(() => (hotkey = null));
  });

  // A hotkey switches mode (§11.2), Ctrl+Alt+S unless another is picked.
  // Off when turned off, or when another program holds those keys; the
  // switch says which by refusing to stay on. Picking keys another program
  // holds keeps the old ones and says so.
  type HotkeyInfo = { on: boolean; keys: string; choices: { keys: string; label: string }[] };
  let hotkey = $state<HotkeyInfo | null>(null);
  // Why the last change didn't take: the switch's keys, or the picked ones.
  let hotkeyTaken = $state<{ keys: string; kept?: string } | null>(null);
  const keysLabel = (keys: string) => hotkey?.choices.find((c) => c.keys === keys)?.label ?? keys;
  async function setHotkey(on: boolean) {
    if (!hotkey) return;
    const keys = keysLabel(hotkey.keys);
    try {
      const now = await invoke<boolean>("set_hotkey", { on });
      hotkey = { ...hotkey, on: now };
      hotkeyTaken = on && !now ? { keys } : null;
    } catch {
      hotkey = { ...hotkey, on: !on };
      hotkeyTaken = on ? { keys } : null;
    }
  }
  async function setHotkeyKeys(keys: string) {
    if (!hotkey) return;
    const kept = keysLabel(hotkey.keys);
    try {
      hotkey = await invoke<HotkeyInfo>("set_hotkey_keys", { keys });
      hotkeyTaken = null;
    } catch {
      // The dropdown still shows the keys in use: `hotkey` didn't change.
      hotkeyTaken = { keys: keysLabel(keys), kept };
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
  const measuredOn = $derived(measured ? shortDate(measured.at) : "");
  // The last test's grades (§11.10): how much ping rose while downloading,
  // without SnailNet and, after the check, with it. None without pings.
  type GradeRow = { key: string; who: string; grade: Grade; rise: number };
  const grades = $derived.by((): GradeRow[] => {
    const m = measured;
    if (!m) return [];
    const out: GradeRow[] = [];
    const without = rise(m.idle_ms, m.loaded_ms);
    if (without === null) return out;
    out.push({ key: "without", who: t("proof.grade.without"), grade: grade(without), rise: Math.round(without) });
    const withIt = rise(m.idle_ms, m.with_ms);
    if (withIt !== null) {
      out.push({ key: "with", who: t("proof.grade.with"), grade: grade(withIt), rise: Math.round(withIt) });
    }
    return out;
  });
  let pinning = $state(false);
  let focusing = $state(false);
  const focusBoost = $derived(service.snapshot?.config.settings.focus_boost !== false);
  const gamingWhilePlaying = $derived(service.snapshot?.config.settings.gaming_while_playing === true);
  // Missing from older services: on.
  const callsFirst = $derived(service.snapshot?.config.settings.calls_first !== false);
  // Missing from older services: on.
  const packUpdates = $derived(service.snapshot?.config.settings.pack_updates !== false);
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

  // Programs split out and apps merged (§11.6), each with its undo.
  const grouped = $derived(groupings(service.snapshot?.config.families, service.snapshot?.apps ?? []));

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

  // The switches of "What goes first", in order.
  const firstSwitches = $derived([
    {
      glyph: "phone" as GlyphName,
      title: t("calls.title"),
      body: t("calls.body"),
      on: callsFirst,
      disabled: !canEdit || focusing,
      set: (on: boolean) => void setSetting({ calls_first: on }),
    },
    {
      glyph: "window" as GlyphName,
      title: t("focus.title"),
      body: t("focus.body"),
      on: focusBoost,
      disabled: !canEdit || focusing,
      set: (on: boolean) => void setSetting({ focus_boost: on }),
    },
    {
      glyph: "gaming" as GlyphName,
      title: t("gaming.title"),
      body: t("gaming.body"),
      on: gamingWhilePlaying,
      disabled: !canEdit || focusing,
      set: (on: boolean) => void setSetting({ gaming_while_playing: on }),
    },
    {
      glyph: "bulb" as GlyphName,
      title: t("fix.setting.title"),
      body: t("fix.setting.body"),
      on: fixer.prefs.on,
      disabled: false,
      set: (on: boolean) => fixer.setOn(on),
    },
  ]);

  // The index: one entry per group shown, lit for the group in view.
  // Words only: the rows carry the glyphs, once (DESIGN.md, Settings).
  const index = $derived<{ id: string; label: string }[]>([
    { id: "set-line", label: t("settings.connection") },
    { id: "set-first", label: t("settings.first") },
    ...(grouped.length > 0 ? [{ id: "set-apps", label: t("settings.apps") }] : []),
    { id: "set-start", label: t("settings.windows") },
    { id: "set-about", label: t("settings.about") },
    { id: "set-reset", label: t("settings.reset") },
  ]);
  let current = $state("set-line");
  let groupsEl = $state<HTMLDivElement | null>(null);
  // A group picked in the index stays lit while the page scrolls to it:
  // the last groups can't reach the top third (the page ends first), so
  // the observer would otherwise light the one above.
  let picked: string | null = null;
  $effect(() => {
    const el = groupsEl;
    if (!el || typeof IntersectionObserver !== "function") return;
    void index.length;
    // A group is "in view" once its top passes the upper third of the page.
    const io = new IntersectionObserver(
      (entries) => {
        const seen = entries
          .filter((e) => e.isIntersecting)
          .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
        if (picked) return;
        const main = el.closest("main");
        // Scrolled to the very end: the last group is the one in view.
        if (main && main.scrollTop + main.clientHeight >= main.scrollHeight - 2) {
          current = index.at(-1)?.id ?? current;
        } else if (seen[0]) current = seen[0].target.id;
      },
      { root: el.closest("main"), rootMargin: "0px 0px -66% 0px" },
    );
    for (const sec of el.querySelectorAll("section[id]")) io.observe(sec);
    return () => io.disconnect();
  });
  function go(id: string) {
    const reduce = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
    picked = id;
    current = id;
    const main = groupsEl?.closest("main");
    const done = () => {
      picked = null;
      clearTimeout(timer);
    };
    // scrollend ends the pin; a group already in place never scrolls, so a
    // timer ends it too.
    const timer = setTimeout(done, reduce ? 100 : 1500);
    main?.addEventListener("scrollend", done, { once: true });
    document.getElementById(id)?.scrollIntoView({ behavior: reduce ? "auto" : "smooth", block: "start" });
  }

  async function doResetAll() {
    resetting = true;
    const ok = await resetAll();
    resetting = false;
    confirming = false;
    if (ok) ui.restartFirstRun();
  }
</script>

<div class="settings">
  <h1>{t("settings.title")}</h1>
  <nav class="index" aria-label={t("settings.index")}>
    {#each index as g (g.id)}
      <button
        type="button"
        class:on={current === g.id}
        aria-current={current === g.id ? "true" : undefined}
        onclick={() => go(g.id)}
      >
        {g.label}
      </button>
    {/each}
  </nav>
  <div class="groups" bind:this={groupsEl}>
    <section id="set-line" aria-labelledby="set-line-h">
      <h2 class="group-title" id="set-line-h">{t("settings.connection")}</h2>
      <div class="card group">
        <div class="item">
          <span class="ic"><Glyph name="line" size={20} /></span>
          <div class="words">
            <b>{t("settings.line")}</b>
            <span class="num"
              >{hasLine
                ? t("settings.line.value", { down: fig(line.down_mbps), up: fig(line.up_mbps) })
                : t("sidebar.line.none")}{#if network}
                · {t("settings.line.network", { network: network.name })}{/if}</span
            >
            {#if metered}
              <span class="metered">{t("settings.metered")}</span>
            {/if}
          </div>
          <div class="pair">
            <button type="button" class="btn" disabled={!canEdit || !canMeasure()} onclick={() => void measureLine()}>
              {measuring || ui.measuring ? t("measure.running") : t("measure.button")}
            </button>
            <button type="button" class="btn btn-quiet" disabled={!canEdit} onclick={() => lineDialog.show()}>
              {t("settings.line.change")}
            </button>
          </div>
        </div>
        {#if measured}
          <div class="item test">
            <span class="ic"><Glyph name="gauge" size={20} /></span>
            <div class="words">
              <b>{t("measure.last", { date: measuredOn })}</b>
              <span class="num">{t("settings.line.value", { down: fig(measured.down_mbps), up: fig(measured.up_mbps) })}</span>
              {#if grades.length > 0}
                <ul class="grades">
                  {#each grades as g (g.key)}
                    <li class:ours={g.key === "with"}>
                      <span class="who">{g.who}</span>
                      <span class="score">
                        <span class="letter g-{g.grade}"
                          ><span class="visually-hidden">{t("proof.grade.word")} </span>{g.grade}</span
                        >
                        <span class="rise num">{t("proof.grade.rise", { rise: g.rise })}</span>
                      </span>
                    </li>
                  {/each}
                </ul>
                <span class="note">{grades[0]?.grade === "A" ? t("proof.grade.calm") : t("proof.grade.what")}</span>
              {/if}
            </div>
          </div>
        {/if}
        <label class="item">
          <span class="ic"><Glyph name="auto" size={20} /></span>
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
        {#if others.length > 0}
          <details class="item fold">
            <summary>
              <span class="ic"><Glyph name="window" size={20} /></span>
              <span class="words"><b>{t("settings.networks")}</b><span>{t("settings.networks.body")}</span></span>
              <span class="count">{t("settings.networks.count", { count: others.length })}<Glyph name="chevron" size={16} /></span>
            </summary>
            <ul class="nets">
              {#each others as n (n.id)}
                <li>
                  <span class="net">{n.name}</span>
                  <span class="num">{t("settings.line.value", { down: fig(n.line.down_mbps), up: fig(n.line.up_mbps) })}</span>
                </li>
              {/each}
            </ul>
          </details>
        {/if}
      </div>
    </section>

    <section id="set-first" aria-labelledby="set-first-h">
      <h2 class="group-title" id="set-first-h">{t("settings.first")}</h2>
      <div class="card group">
        {#each firstSwitches as sw (sw.title)}
          <label class="item">
            <span class="ic"><Glyph name={sw.glyph} size={20} /></span>
            <div class="words">
              <b>{sw.title}</b>
              <span>{sw.body}</span>
            </div>
            <input
              type="checkbox"
              role="switch"
              class="switch"
              checked={sw.on}
              disabled={sw.disabled}
              onchange={(e) => sw.set(e.currentTarget.checked)}
            />
          </label>
        {/each}
        {#if choices.length > 0}
          <details class="item fold">
            <summary>
              <span class="ic"><Glyph name="flag" size={20} /></span>
              <span class="words"><b>{t("first.list.title")}</b><span>{t("first.list.body")}</span></span>
              <span class="count">{t("settings.count", { count: choices.length })}<Glyph name="chevron" size={16} /></span>
            </summary>
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
          </details>
        {/if}
      </div>
    </section>

    {#if grouped.length > 0}
      <section id="set-apps" aria-labelledby="set-apps-h">
        <h2 class="group-title" id="set-apps-h">{t("settings.apps")}</h2>
        <div class="card group">
          <details class="item fold">
            <summary>
              <span class="ic"><Glyph name="apps" size={20} /></span>
              <span class="words"><b>{t("group.list.title")}</b><span>{t("group.list.body")}</span></span>
              <span class="count">{t("settings.count", { count: grouped.length })}<Glyph name="chevron" size={16} /></span>
            </summary>
            <ul class="nets choices">
              {#each grouped as g (g.key)}
                <li>
                  <span class="net">{g.name}</span>
                  {#if g.kind === "split"}
                    <span>{t("group.list.split")}</span>
                    <button
                      type="button"
                      class="btn btn-quiet"
                      disabled={!canEdit}
                      aria-label={t("group.list.back.label", { name: g.name })}
                      onclick={() => void putBack(g.exe)}>{t("group.list.back")}</button
                    >
                  {:else}
                    <span>{t("group.list.merge", { into: g.into })}</span>
                    <button
                      type="button"
                      class="btn btn-quiet"
                      disabled={!canEdit}
                      aria-label={t("group.list.separate.label", { name: g.name })}
                      onclick={() => void separate(g.from, g.name)}>{t("group.list.separate")}</button
                    >
                  {/if}
                </li>
              {/each}
            </ul>
          </details>
        </div>
      </section>
    {/if}

    <section id="set-start" aria-labelledby="set-start-h">
      <h2 class="group-title" id="set-start-h">{t("settings.windows")}</h2>
      <div class="card group">
        <label class="item">
          <span class="ic"><Glyph name="power" size={20} /></span>
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
        <div class="item">
          <span class="ic"><Glyph name="keyboard" size={20} /></span>
          <div class="words">
            <b id="set-hotkey">{t("settings.hotkey", { keys: keysLabel(hotkey?.keys ?? "ctrl+alt+s") })}</b>
            <span role="status"
              >{hotkeyTaken?.kept
                ? t("settings.hotkey.kept", hotkeyTaken)
                : hotkeyTaken
                  ? t("settings.hotkey.taken", hotkeyTaken)
                  : t("settings.hotkey.body")}</span
            >
          </div>
          <div class="hotkey">
            {#if hotkey?.on}
              <Dropdown
                label={t("settings.hotkey.pick")}
                options={hotkey.choices.map((c) => ({ value: c.keys, label: c.label }))}
                value={hotkey.keys}
                onchange={(k) => void setHotkeyKeys(k)}
              />
            {/if}
            <input
              type="checkbox"
              role="switch"
              class="switch"
              aria-labelledby="set-hotkey"
              checked={hotkey?.on === true}
              disabled={hotkey === null}
              onchange={(e) => void setHotkey(e.currentTarget.checked)}
            />
          </div>
        </div>
      </div>
    </section>

    <section id="set-about" aria-labelledby="set-about-h">
      <h2 class="group-title" id="set-about-h">{t("settings.about")}</h2>
      <div class="card group">
        <div class="item">
          <span class="ic"><Glyph name="info" size={20} /></span>
          <div class="words">
            <b>{t("settings.about.version", { app: version })}</b>
            {#if serviceVersion}<span>{t("settings.about.service", { version: serviceVersion })}</span>{/if}
          </div>
        </div>
        <UpdateRow />
        <label class="item">
          <span class="ic"><Glyph name="list" size={20} /></span>
          <div class="words">
            <b>{t("packs.title")}</b>
            <span>{t("packs.body")}</span>
          </div>
          <input
            type="checkbox"
            role="switch"
            class="switch"
            checked={packUpdates}
            disabled={!canEdit || focusing}
            onchange={(e) => void setSetting({ pack_updates: e.currentTarget.checked })}
          />
        </label>
      </div>
    </section>

    <section id="set-reset" aria-labelledby="set-reset-h">
      <h2 class="group-title" id="set-reset-h">{t("settings.reset")}</h2>
      <div class="card group">
        <div class="item">
          <span class="ic"><Glyph name="reset" size={20} /></span>
          <div class="words">
            <b>{t("settings.resetMode", { mode })}</b>
            <span>{t("settings.resetMode.body", { mode })}</span>
          </div>
          <button type="button" class="btn" disabled={!canEdit} onclick={() => void resetSplit()}>
            {t("split.reset")}
          </button>
        </div>
        <div class="item">
          <span class="ic"><Glyph name="trash" size={20} /></span>
          <div class="words">
            <b>{t("settings.resetAll")}</b>
            <span>{t("settings.resetAll.body")}</span>
          </div>
          <button type="button" class="btn btn-danger" disabled={!canEdit} onclick={() => (confirming = true)}>
            {t("settings.resetAll.button")}
          </button>
        </div>
      </div>
    </section>
  </div>
</div>

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
  /* An index beside the groups (a map of the page), the groups in one
     column at a reading width. */
  .settings {
    display: grid;
    grid-template-columns: 168px minmax(0, 680px);
    column-gap: 40px;
    align-items: start;
  }
  h1 {
    grid-column: 1 / -1;
    margin: 0 0 18px;
    font-size: 28px;
    line-height: 34px;
    font-weight: 800;
    letter-spacing: -0.02em;
  }
  .index {
    position: sticky;
    top: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .index button {
    display: flex;
    align-items: center;
    height: 34px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font-size: 14px;
    font-weight: 600;
    text-align: left;
    transition:
      background-color 140ms linear,
      color 140ms linear;
  }
  .index button:hover {
    background: var(--fill-subtle-hover);
    color: var(--text-primary);
  }
  /* The group in view: a soft fill and bright words, as the side rail
     marks the open page. Never a stripe down one side. */
  .index button.on {
    background: var(--surface-1);
    color: var(--text-primary);
  }
  .groups {
    display: flex;
    flex-direction: column;
    gap: 30px;
    padding-bottom: 40px;
  }
  section {
    scroll-margin-top: 8px;
  }
  /* A list needed once: one row that opens. */
  .item.fold {
    display: block;
  }
  .fold summary {
    display: flex;
    align-items: center;
    gap: 14px;
    list-style: none;
    cursor: pointer;
  }
  .fold summary::-webkit-details-marker {
    display: none;
  }
  .count {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 6px;
    font-size: 14px;
    color: var(--text-secondary);
  }
  .count :global(.glyph) {
    transition: transform 200ms var(--ease-out);
  }
  .fold[open] .count :global(.glyph) {
    transform: rotate(90deg);
  }
  .fold summary:hover .count {
    color: var(--text-primary);
  }
  .fold .nets {
    margin: 12px 0 2px 38px;
  }
  @media (max-width: 980px) {
    .settings {
      grid-template-columns: minmax(0, 1fr);
    }
    .index {
      display: none;
    }
  }
  .group-title {
    margin: 0 0 10px 2px;
    font-size: 16px;
    line-height: 22px;
    font-weight: 700;
    color: var(--text-primary);
  }
  .group {
    display: flex;
    flex-direction: column;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 18px;
    min-height: 64px;
    padding: 12px 18px;
  }
  /* Hairlines between rows, light enough to see on the dark panel. */
  .item + .item {
    border-top: 1px solid var(--stroke-card);
  }
  /* A row you click anywhere on says so under the pointer. */
  label.item,
  .fold summary {
    transition: background-color 120ms linear;
  }
  label.item:hover:not(:has(input:disabled)) {
    background: var(--hover-soft);
  }
  .group > :first-child {
    border-radius: var(--radius-card) var(--radius-card) 0 0;
  }
  .group > :last-child {
    border-radius: 0 0 var(--radius-card) var(--radius-card);
  }
  .group > :only-child {
    border-radius: var(--radius-card);
  }
  .pair {
    display: flex;
    flex: none;
    gap: 8px;
  }
  .hotkey {
    display: flex;
    flex: none;
    align-items: center;
    gap: 16px;
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
  /* The last test's grades: without and with SnailNet side by side, each
     its letter and how far ping rose. The letter says it; its colour only
     echoes it (§14.9). */
  /* Side by side, parted by a hairline: a big plain letter, how far ping
     rose, and whose it is. No boxes. */
  .grades {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    max-width: 420px;
    margin: 14px 0 10px;
    padding: 0;
    list-style: none;
  }
  .grades li {
    display: flex;
    flex-direction: column-reverse;
    gap: 0;
    padding-right: 14px;
  }
  .grades li + li {
    padding: 0 0 0 16px;
    border-left: 1px solid var(--stroke-card);
  }
  .words .who {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .score {
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  .words .rise {
    font-size: 13.5px;
    font-weight: 700;
    color: var(--text-primary);
  }
  .words .letter {
    font-size: 30px;
    font-weight: 800;
    line-height: 1.15;
    color: var(--text-primary);
  }
  .words .g-A,
  .words .g-B {
    color: var(--shell-bright);
  }
  .words .g-C {
    color: var(--caution);
  }
  .words .g-D,
  .words .g-F {
    color: var(--critical);
  }
  .words .note {
    max-width: 64ch;
    font-size: 13px;
  }
  .words .metered {
    margin-top: 4px;
    color: var(--caution);
  }
  /* Every setting leads with one quiet grey glyph of what it's about:
     plain, never on a tile, and nowhere else on the page. */
  .ic {
    display: flex;
    flex: none;
    width: 20px;
    color: var(--text-tertiary);
    transition: color 120ms linear;
  }
  .item:hover .ic,
  .fold summary:hover .ic {
    color: var(--text-secondary);
  }
  .item.test .ic {
    align-self: flex-start;
    margin-top: 2px;
  }
  .choices li {
    align-items: center;
  }
  .choices .net {
    flex: 1;
    min-width: 0;
  }
  @media (forced-colors: active) {
    .item + .item {
      border-top-color: CanvasText;
    }
  }
</style>
