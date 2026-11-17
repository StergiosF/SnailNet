<script lang="ts">
  // One slice of the line (PLAN.md §14.2): icon, name, what it moves now
  // and what that means in words, a thin bar (its slice, and its use on
  // the same scale), and − % + to change its share in 5% steps. A rule's
  // row opens its settings in place: who goes first, "only when the line
  // is free", a speed limit, remove. Everything else lists its apps; pick
  // one to give it a slice of its own.
  import { mbps } from "../format";
  import { t } from "../i18n";
  import { statusText, type Member, type Slice } from "../ledger";
  import {
    addRule,
    canStep,
    clampLimit,
    parseLimit,
    removeRule,
    sameTarget,
    setCeiling,
    setEnabled,
    setPriority,
    setRuleShare,
    stepShare,
  } from "../rules";
  import { ui } from "../ui.svelte";
  import type { Limit, Priority } from "../types";
  import AppIcon from "./AppIcon.svelte";
  import Segmented from "./Segmented.svelte";
  import Stepper from "./Stepper.svelte";

  let { slice, shaped, canEdit }: { slice: Slice; shaped: number | null; canEdit: boolean } = $props();

  const MEMBERS_SHOWN = 4;
  const uid = $props.id();

  const rules = $derived(ui.rules);
  const rule = $derived(slice.rule >= 0 ? (rules[slice.rule] ?? null) : null);
  const isOther = $derived(slice.target === null);
  const open = $derived(rule !== null && ui.open === slice.key);
  const pct = $derived(slice.share === null ? null : Math.round(slice.share / 100));

  const canLess = $derived(canEdit && canStep(rules, slice.rule, -1));
  const canMore = $derived(canEdit && canStep(rules, slice.rule, 1));

  const words = $derived(
    slice.status.kind === "quiet" || slice.status.kind === "off" || slice.bps < 50_000
      ? statusText(slice.status)
      : t("row.moving", { mbps: mbps(slice.bps), status: statusText(slice.status) }),
  );

  // The bar: the slice, and its use on the same scale (both as % of what
  // SnailNet shares out); use beyond the slice is hatched.
  const track = $derived.by(() => {
    if (slice.share === null) return null;
    const share = slice.share / 100;
    if (shaped === null || slice.kept === null) return { share, use: 0, over: 0 };
    const use = Math.min(100, (slice.bps / shaped) * 100);
    return { share, use: Math.min(use, share), over: Math.max(0, use - share) };
  });
  const ink = $derived(slice.ink >= 0 ? `var(--ink-${slice.ink})` : "var(--stone-0)");

  // Everything else's apps.
  let all = $state(false);
  const members = $derived(all ? slice.members : slice.members.slice(0, MEMBERS_SHOWN));
  const picked = $derived(slice.members.find((m) => m.key === ui.member && !m.fixed) ?? null);

  function give(m: Member) {
    const i = rules.findIndex((r) => sameTarget(r.target, m.target));
    ui.change(i >= 0 ? setEnabled(rules, i, true) : addRule(rules, m.target));
    ui.member = null;
    ui.open = `${m.target.kind}:${m.target.id.toLowerCase()}`;
  }

  // Speed limits: shown once one is set or asked for.
  let limitsAsked = $state(false);
  const limits = $derived({
    down: parseLimit(rule?.ceiling?.down),
    up: parseLimit(rule?.ceiling?.up),
  });
  const showLimits = $derived(limitsAsked || limits.down !== null || limits.up !== null);
  let units = $state<Record<"down" | "up", Limit["unit"]>>({ down: "mbps", up: "mbps" });

  function setLimit(dir: "down" | "up", text: string, unit: Limit["unit"]) {
    const v = Number(text);
    const limit = text.trim() !== "" && Number.isFinite(v) && v > 0 ? clampLimit({ unit, value: v }) : null;
    ui.change(setCeiling(rules, slice.rule, dir, limit));
  }

  const priorities: { value: Priority; label: string }[] = [
    { value: "high", label: t("priority.high") },
    { value: "normal", label: t("priority.normal") },
    { value: "low", label: t("priority.low") },
  ];
  const unitOptions: { value: Limit["unit"]; label: string }[] = [
    { value: "mbps", label: t("editor.ceiling.mbps") },
    { value: "percent", label: t("editor.ceiling.percent") },
  ];
</script>

<li class="slice" class:open class:off={!slice.enabled}>
  <div class="row">
    {#if rule && canEdit}
      <button
        type="button"
        class="main"
        aria-expanded={open}
        aria-controls="{uid}-ed"
        aria-label={t("row.edit", { name: slice.name, status: words })}
        onclick={() => ui.toggle(slice.key)}
      >
        {@render who()}
      </button>
    {:else}
      <div class="main">{@render who()}</div>
    {/if}

    <div class="pct">
      {#if !slice.enabled && rule}
        <button type="button" class="btn" disabled={!canEdit} onclick={() => ui.change(setEnabled(rules, slice.rule, true))}>
          {t("row.turnOn")}
        </button>
      {:else}
        <Stepper
          name={slice.name}
          {pct}
          {canLess}
          {canMore}
          onstep={(dir) => ui.change(stepShare(rules, slice.rule, dir))}
        />
      {/if}
    </div>
  </div>

  {#if isOther && slice.members.length > 0}
    <div class="members" role="group" aria-label={t("row.members")}>
      {#each members as m (m.key)}
        <button
          type="button"
          class="member"
          class:picked={ui.member === m.key}
          disabled={m.fixed || !canEdit}
          aria-pressed={ui.member === m.key}
          title={m.fixed ? t("list.unknown") : undefined}
          onclick={() => (ui.member = ui.member === m.key ? null : m.key)}
        >
          <AppIcon target={m.fixed ? null : m.target} name={m.name} size={18} />
          {m.fixed ? t("row.unknown") : m.name}
          <span>{mbps(m.bps)}</span>
        </button>
      {/each}
      {#if !all && slice.members.length > MEMBERS_SHOWN}
        <button type="button" class="member more" onclick={() => (all = true)}>
          {t("row.more.apps", { count: slice.members.length - MEMBERS_SHOWN })}
        </button>
      {/if}
      {#if picked}
        <button type="button" class="give" onclick={() => give(picked)}>
          <span aria-hidden="true">+</span>{t("row.give", { name: picked.name })}
        </button>
      {/if}
    </div>
  {/if}

  {#if open && rule}
    <div class="ed" id="{uid}-ed">
      {#if slice.target?.kind === "site"}
        <p class="note">{t("edit.site")}</p>
      {/if}
      <div class="controls">
        <div class="grp" title={t("edit.priority.hint")}>
          <span class="k" id="{uid}-prio">{t("edit.priority")}</span>
          <Segmented
            name="{uid}-prio"
            label={t("edit.priority")}
            options={priorities}
            value={rule.priority}
            disabled={!rule.enabled}
            onchange={(p) => ui.change(setPriority(rules, slice.rule, p))}
          />
        </div>
        <label class="check">
          <input
            type="checkbox"
            checked={rule.share === 0}
            disabled={!rule.enabled}
            onchange={(e) => ui.change(setRuleShare(rules, slice.rule, e.currentTarget.checked ? 0 : 2000))}
          />
          <span class="box" aria-hidden="true"></span>{t("edit.free")}
        </label>
        {#if !showLimits}
          <button type="button" class="btn" disabled={!rule.enabled} onclick={() => (limitsAsked = true)}>
            {t("edit.limit.add")}
          </button>
        {/if}
        <button
          type="button"
          class="btn btn-quiet end"
          onclick={() => {
            ui.change(removeRule(rules, slice.rule));
            ui.open = null;
          }}>{t("edit.remove")}</button
        >
      </div>
      {#if showLimits}
        <div class="limits">
          <span class="k">{t("edit.limit")}</span>
          {#each ["down", "up"] as const as dir (dir)}
            {@const lim = limits[dir]}
            {@const unit = lim?.unit ?? units[dir]}
            {@const dirLabel = t(dir === "down" ? "editor.ceiling.down" : "editor.ceiling.up")}
            <div class="limit">
              <label for="{uid}-lim-{dir}">{dirLabel}</label>
              <input
                id="{uid}-lim-{dir}"
                class="field num"
                type="text"
                inputmode="decimal"
                autocomplete="off"
                placeholder="–"
                value={lim?.value ?? ""}
                disabled={!rule.enabled}
                aria-label={t("editor.ceiling.value", { dir: dirLabel })}
                onchange={(e) => setLimit(dir, e.currentTarget.value, unit)}
              />
              <Segmented
                name="{uid}-unit-{dir}"
                label={dirLabel}
                options={unitOptions}
                value={unit}
                small
                disabled={!rule.enabled}
                onchange={(u) => {
                  units[dir] = u;
                  if (lim) setLimit(dir, String(lim.value), u);
                }}
              />
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</li>

{#snippet who()}
  <AppIcon target={slice.target} name={slice.name} size={24} boxed />
  <span class="text">
    <span class="line1"><b>{slice.name}</b><span class="st">{words}</span></span>
    {#if track}
      <span class="trk" aria-hidden="true">
        <i style:width="{track.share}%" style:background="color-mix(in srgb, {ink} 28%, transparent)"></i>
        <i style:width="{track.use}%" style:background={ink}></i>
        {#if track.over > 0}
          <i class="over" style:left="{track.share}%" style:width="{track.over}%" style:color={ink}></i>
        {/if}
      </span>
    {/if}
  </span>
{/snippet}

<style>
  .slice {
    list-style: none;
    border-radius: var(--radius-card);
    transition: background-color 120ms linear;
  }
  .slice:hover,
  .slice.open {
    background: var(--surface-1);
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 16px;
    padding: 0 12px 0 0;
  }
  .main {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
    padding: 12px 0 12px 12px;
    border: 0;
    border-radius: var(--radius-card);
    background: transparent;
    text-align: left;
  }
  button.main:focus-visible {
    outline-offset: -2px;
  }
  .text {
    display: block;
    flex: 1;
    min-width: 0;
  }
  .line1 {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
    white-space: nowrap;
  }
  b {
    font-size: 17px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .st {
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-secondary);
    font-size: 14px;
  }
  .off b,
  .off :global(.wrap) {
    opacity: 0.6;
  }
  .trk {
    position: relative;
    display: block;
    max-width: 420px;
    height: 6px;
    margin-top: 8px;
    border-radius: 3px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .trk i {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    border-radius: 3px;
    transition: width 400ms var(--ease-out);
  }
  .trk i.over {
    background: repeating-linear-gradient(135deg, currentColor 0 2px, transparent 2px 5px);
  }
  .pct {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    min-width: 168px;
  }
  .members {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 0 12px 14px 66px;
    margin-top: -4px;
  }
  .member {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 12px 0 8px;
    border: 0;
    border-radius: 9px;
    background: var(--surface-2);
    font-size: 14px;
    transition: background-color 120ms linear;
  }
  .member span {
    color: var(--text-secondary);
  }
  .member:hover:not(:disabled),
  .member.picked {
    background: var(--surface-3);
  }
  .member.picked {
    box-shadow: inset 0 0 0 1px var(--text-secondary);
  }
  .member:disabled {
    cursor: default;
  }
  .member.more {
    padding: 0 12px;
    color: var(--text-secondary);
  }
  .give {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 10px;
    border: 0;
    border-radius: 9px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 14px;
    font-weight: 600;
  }
  .give span {
    font-size: 17px;
  }

  .ed {
    padding: 0 12px 16px 66px;
    font-size: 14px;
  }
  .note {
    margin: -2px 0 12px;
    color: var(--text-secondary);
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px 24px;
  }
  .grp {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .k {
    color: var(--text-secondary);
  }
  .end {
    margin-left: auto;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 10px;
    position: relative;
  }
  .check input {
    position: absolute;
    opacity: 0;
    width: 20px;
    height: 20px;
    margin: 0;
  }
  .box {
    width: 20px;
    height: 20px;
    border-radius: 6px;
    background: var(--surface-ground);
    box-shadow: inset 0 0 0 1.5px var(--text-tertiary);
  }
  .check input:checked + .box {
    background: var(--accent);
    box-shadow: none;
  }
  .check input:checked + .box::after {
    content: "";
    display: block;
    width: 10px;
    height: 5px;
    margin: 6px 0 0 5px;
    border: solid var(--on-accent);
    border-width: 0 0 2px 2px;
    transform: rotate(-45deg);
  }
  .check input:focus-visible + .box {
    outline: 2px solid var(--focus-outer);
    outline-offset: 2px;
  }
  .check:has(input:disabled) {
    opacity: 0.5;
  }
  .limits {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px 24px;
    margin-top: 14px;
  }
  .limit {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .limit label {
    color: var(--text-secondary);
  }
  .limit .field {
    width: 72px;
    text-align: right;
  }
  /* High contrast drops box-shadows, so the rings drawn with them come
     back as outlines and borders in system colours. */
  @media (forced-colors: active) {
    .trk i,
    .member {
      forced-color-adjust: none;
      border: 1px solid ButtonText;
    }
    .member.picked {
      outline: 2px solid Highlight;
      outline-offset: 1px;
    }
    .box {
      outline: 1.5px solid CanvasText;
      outline-offset: -1.5px;
    }
    .check input:checked + .box {
      forced-color-adjust: none;
      background: Highlight;
      outline-color: Highlight;
    }
    .check input:checked + .box::after {
      border-color: HighlightText;
    }
  }
</style>
