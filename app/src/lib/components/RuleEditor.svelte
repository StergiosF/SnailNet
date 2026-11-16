<script lang="ts">
  // The rule editor (PLAN.md §11.1, §14.2): a side panel over the right of
  // the window. In "pick" mode it lists the apps and websites a new slice
  // can go to; in "edit" mode it shows one target's rule (or offers to
  // give it a slice): on/off, slice size or "only when the line is free",
  // priority, speed limits per direction, remove. Every change saves at
  // once, the way Windows Settings does; the panel then shows whatever the
  // service now holds.
  import { tick } from "svelte";
  import { editor } from "../editor.svelte";
  import { targetLabel } from "../format";
  import { t } from "../i18n";
  import {
    addRule,
    clampLimit,
    parseLimit,
    pickable,
    removeRule,
    sameTarget,
    setCeiling,
    setEnabled,
    setPriority,
    setRuleShare,
    shownShare,
  } from "../rules";
  import { service } from "../service.svelte";
  import type { Limit, Priority, Rule, SaveError, Target } from "../types";
  import Segmented from "./Segmented.svelte";
  import ToggleSwitch from "./ToggleSwitch.svelte";

  const rules = $derived(service.activeProfile?.rules ?? []);
  const sites = $derived(service.snapshot?.sites ?? []);
  const label = (target: Target) =>
    targetLabel(target, service.snapshot?.apps ?? [], service.siteNames);

  const panel = $derived(editor.panel);
  const target = $derived(panel?.mode === "edit" ? panel.target : null);
  const index = $derived(target ? rules.findIndex((r) => sameTarget(r.target, target)) : -1);
  const rule = $derived<Rule | null>(rules[index] ?? null);
  const name = $derived(target ? label(target) : "");
  const shown = $derived(rule ? shownShare(rules, index) : null);

  let query = $state("");
  let error = $state<string | null>(null);
  let saving = $state(false);
  let headingEl = $state<HTMLHeadingElement | null>(null);

  const choices = $derived(
    panel?.mode === "pick" ? pickable(sites, service.snapshot?.apps ?? [], rules, query) : [],
  );

  // A fresh panel starts clean and takes focus, so keyboard and screen
  // reader users land in it.
  $effect(() => {
    if (!panel) return;
    void (panel.mode === "edit" ? panel.target.id : "pick");
    error = null;
    query = "";
    void tick().then(() => headingEl?.focus());
  });

  /** Save; returns false if the service refused. */
  async function save(next: Rule[]): Promise<boolean> {
    error = null;
    saving = true;
    try {
      await service.saveRules(next);
      return true;
    } catch (e) {
      const err = e as SaveError;
      error = err.kind === "conflict" ? t("editor.conflict") : t("editor.failed", { message: err.message });
      return false;
    } finally {
      saving = false;
    }
  }

  /** Commit a number field: show the value that will be saved (clamped),
   *  and the saved one again if the service refuses or the text is bad. */
  async function commit(input: HTMLInputElement, before: string, value: number | null, next: () => Rule[]) {
    if (value === null) {
      input.value = before;
      return;
    }
    input.value = String(value);
    if (!(await save(next()))) input.value = before;
  }

  function onShare(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const v = Number(input.value);
    const ok = input.value.trim() !== "" && Number.isFinite(v) && v >= 1;
    const pct = ok ? Math.min(95, Math.round(v)) : null;
    const before = String(Math.round((shown ?? rule?.share ?? 0) / 100));
    void commit(input, before, pct, () => setRuleShare(rules, index, (pct ?? 0) * 100));
  }

  type Unit = "none" | Limit["unit"];
  const DEFAULT_LIMIT: Record<Limit["unit"], Limit> = {
    percent: { unit: "percent", value: 50 },
    mbps: { unit: "mbps", value: 5 },
  };

  function limitOf(dir: "down" | "up"): Limit | null {
    return parseLimit(rule?.ceiling?.[dir]);
  }

  function onUnit(dir: "down" | "up", unit: Unit) {
    void save(setCeiling(rules, index, dir, unit === "none" ? null : DEFAULT_LIMIT[unit]));
  }

  function onLimit(dir: "down" | "up", unit: Limit["unit"], e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const v = Number(input.value);
    const limit = input.value.trim() !== "" && Number.isFinite(v) && v > 0 ? clampLimit({ unit, value: v }) : null;
    const before = String(limitOf(dir)?.value ?? "");
    void commit(input, before, limit?.value ?? null, () => setCeiling(rules, index, dir, limit));
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      editor.close();
    }
  }

  const priorities: { value: Priority; label: string }[] = [
    { value: "high", label: t("priority.high") },
    { value: "normal", label: t("priority.normal") },
    { value: "low", label: t("priority.low") },
  ];
  const units: { value: Unit; label: string }[] = [
    { value: "none", label: t("editor.ceiling.none") },
    { value: "percent", label: t("editor.ceiling.percent") },
    { value: "mbps", label: t("editor.ceiling.mbps") },
  ];
</script>

{#if panel}
  <!-- A non-modal side sheet: the list stays usable beside it, so picking
       another row just switches what the panel shows. -->
  <div
    class="panel"
    role="dialog"
    aria-modal="false"
    aria-labelledby="editor-heading"
    aria-busy={saving}
    tabindex="-1"
    onkeydown={onKey}
  >
    <header>
      <div class="titles">
        {#if target}
          <p class="kind">{t(`editor.kind.${target.kind}`)}</p>
        {/if}
        <h2 id="editor-heading" tabindex="-1" bind:this={headingEl}>
          {target ? name : t("picker.title")}
        </h2>
      </div>
      <button type="button" class="icon-button" onclick={() => editor.close()} aria-label={t("editor.close")}>
        <i class="icon" aria-hidden="true">&#xE711;</i>
      </button>
    </header>

    <div class="body">
      {#if panel.mode === "pick"}
        <input
          class="search"
          type="search"
          placeholder={t("picker.search")}
          aria-label={t("picker.search")}
          bind:value={query}
        />
        {#if choices.length === 0}
          <p class="hint">
            {query.trim() ? t("picker.empty", { query: query.trim() }) : t("picker.hint")}
          </p>
        {:else}
          <ul class="choices">
            {#each choices as c (c.target.kind + ":" + c.target.id)}
              <li>
                <button type="button" class="choice" onclick={() => editor.open({ mode: "edit", target: c.target })}>
                  <i class="icon" aria-hidden="true">{c.target.kind === "site" ? "" : ""}</i>
                  <span class="choice-name">{c.name}</span>
                  <span class="choice-kind">{t(`editor.kind.${c.target.kind}`)}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      {:else if !rule}
        <p>{t("editor.none.body", { name })}</p>
        <button
          type="button"
          class="accent-button"
          disabled={saving || !target}
          onclick={() => target && void save(addRule(rules, target))}
        >
          {t("editor.give")}
        </button>
      {:else}
        <div class="card">
          <ToggleSwitch
            label={t("editor.on")}
            checked={rule.enabled}
            onchange={(on) => void save(setEnabled(rules, index, on))}
          />
          {#if !rule.enabled}
            <p class="hint">{t("editor.off.hint", { name })}</p>
          {/if}
        </div>

        <fieldset class="card" disabled={!rule.enabled}>
          <legend>{t("editor.share")}</legend>
          <p class="hint">{t("editor.share.hint", { name })}</p>
          {#if rule.share > 0}
            <label class="number">
              <input
                type="number"
                class="num"
                min="1"
                max="95"
                step="1"
                value={Math.round((shown ?? rule.share) / 100)}
                onchange={onShare}
                aria-describedby="share-unit"
              />
              <span id="share-unit">{t("editor.share.unit")}</span>
            </label>
          {/if}
          <label class="check">
            <input
              type="checkbox"
              checked={rule.share === 0}
              onchange={(e) =>
                void save(setRuleShare(rules, index, (e.currentTarget as HTMLInputElement).checked ? 0 : 2000))}
            />
            <span>
              {t("editor.free")}
              <span class="hint">{t("editor.free.hint", { name })}</span>
            </span>
          </label>
        </fieldset>

        <fieldset class="card" disabled={!rule.enabled}>
          <legend>{t("editor.priority")}</legend>
          <p class="hint">{t("editor.priority.hint")}</p>
          <Segmented
            name="priority"
            label={t("editor.priority")}
            options={priorities}
            value={rule.priority}
            onchange={(p) => void save(setPriority(rules, index, p))}
          />
        </fieldset>

        <fieldset class="card" disabled={!rule.enabled}>
          <legend>{t("editor.ceiling")}</legend>
          <p class="hint">{t("editor.ceiling.hint")}</p>
          {#each ["down", "up"] as const as dir (dir)}
            {@const limit = limitOf(dir)}
            {@const dirLabel = t(dir === "down" ? "editor.ceiling.down" : "editor.ceiling.up")}
            <div class="limit">
              <span class="limit-label">{dirLabel}</span>
              <Segmented
                name="ceiling-{dir}"
                label={dirLabel}
                options={units}
                value={limit?.unit ?? "none"}
                onchange={(u) => onUnit(dir, u)}
              />
              {#if limit}
                <input
                  type="number"
                  class="num limit-value"
                  min={limit.unit === "percent" ? 1 : 0.1}
                  max={limit.unit === "percent" ? 100 : 10000}
                  step={limit.unit === "percent" ? 1 : 0.1}
                  value={limit.value}
                  aria-label={t("editor.ceiling.value", { dir: dirLabel })}
                  onchange={(e) => onLimit(dir, limit.unit, e)}
                />
              {/if}
            </div>
          {/each}
        </fieldset>

        <button type="button" class="remove" disabled={saving} onclick={() => void save(removeRule(rules, index))}>
          <i class="icon" aria-hidden="true">&#xE74D;</i>{t("editor.remove")}
        </button>
      {/if}

      {#if error}
        <p class="error" role="alert">{error}</p>
      {/if}
    </div>
  </div>
{/if}

<style>
  .panel {
    position: fixed;
    top: var(--titlebar-h);
    right: 0;
    bottom: 0;
    z-index: 10;
    display: flex;
    flex-direction: column;
    width: min(380px, 100vw - 48px);
    border-left: 1px solid var(--stroke-card);
    border-top-left-radius: var(--radius-card);
    background: var(--surface-ground);
    box-shadow: -8px 0 24px rgb(0 0 0 / 0.14);
    animation: slide-in 200ms var(--ease-out);
  }
  @keyframes slide-in {
    from {
      transform: translateX(24px);
      opacity: 0;
    }
  }
  header {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 20px 16px 12px 24px;
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  .kind {
    margin: 0;
    font-size: 12px;
    line-height: 16px;
    color: var(--text-secondary);
  }
  h2 {
    margin: 0;
    font: 600 20px/28px var(--font-display);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .body {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 4px 24px 24px;
  }
  .body > p {
    margin: 0 0 8px;
  }
  .card {
    margin: 0;
    padding: 12px 16px 16px;
    border: 1px solid var(--stroke-card);
    border-radius: var(--radius-card);
    background: var(--surface-card);
    min-width: 0;
  }
  fieldset.card {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  fieldset:disabled {
    opacity: 0.55;
  }
  legend {
    float: left;
    padding: 0;
    font-weight: 600;
  }
  legend + * {
    clear: left;
  }
  .hint {
    margin: 0;
    font-size: 12px;
    line-height: 16px;
    color: var(--text-secondary);
  }
  .card > .hint:first-of-type {
    margin-top: 2px;
  }
  .number {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  input[type="number"],
  .search {
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--stroke-card);
    border-bottom-color: var(--text-tertiary);
    border-radius: var(--radius-control);
    background: var(--surface-card);
    color: var(--text-primary);
    font: inherit;
    user-select: text;
  }
  input[type="number"] {
    width: 80px;
  }
  input[type="number"]:focus,
  .search:focus {
    border-bottom: 2px solid var(--accent);
    outline: none;
  }
  .search {
    width: 100%;
    margin-bottom: 8px;
  }
  .check {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  .check input {
    width: 18px;
    height: 18px;
    margin: 1px 0 0;
    accent-color: var(--accent);
  }
  .check .hint {
    display: block;
  }
  .limit {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
  }
  .limit-label {
    width: 76px;
  }
  .limit-value {
    width: 88px;
  }
  .choices {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .choice {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 40px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-primary);
    font: inherit;
    text-align: left;
  }
  .choice:hover {
    background: var(--fill-subtle-hover);
  }
  .choice .icon {
    font-size: 16px;
    color: var(--text-secondary);
  }
  .choice-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .choice-kind {
    font-size: 12px;
    color: var(--text-secondary);
  }
  .icon-button {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-primary);
  }
  .icon-button:hover {
    background: var(--fill-subtle-hover);
  }
  .accent-button {
    align-self: flex-start;
    min-height: 32px;
    padding: 0 16px;
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: var(--accent);
    color: var(--on-accent);
    font: inherit;
    font-weight: 600;
  }
  .accent-button:hover:not(:disabled) {
    filter: brightness(1.08);
  }
  .remove {
    align-self: flex-start;
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    margin-top: 8px;
    padding: 0 12px;
    border: 1px solid var(--stroke-card);
    border-radius: var(--radius-control);
    background: var(--surface-card);
    color: var(--critical);
    font: inherit;
  }
  .remove:hover:not(:disabled) {
    background: var(--critical-tint);
  }
  button:disabled {
    opacity: 0.55;
  }
  .error {
    margin: 8px 0 0;
    font-size: 12px;
    line-height: 16px;
    color: var(--critical);
  }
  @media (forced-colors: active) {
    .panel {
      border-left-color: CanvasText;
    }
  }
</style>
