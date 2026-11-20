<script lang="ts">
  // One share of the line (PLAN.md §14.2): icon, name, what it moves now
  // in a few words, and − % + to change its share in 5% steps (no bar of
  // its own: the drag bar above is the one picture of the split), with
  // what it was before while the change isn't saved (draft.ts). The row's
  // priority tints it: High a breath of the shell's green, Low a shade
  // darker. A rule's row opens a short panel in place, one control per
  // line (priority, only spare speed, a speed limit, remove); what each
  // does is its tooltip and description, not a paragraph on screen.
  // Everything else opens to its apps; click one to give it a share of
  // its own. Every edit goes into the page's draft.
  import { mbps } from "../format";
  import { t } from "../i18n";
  import { wasShare } from "../draft";
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
  import { flash } from "../flash";
  import { ui } from "../ui.svelte";
  import type { Limit, Priority } from "../types";
  import AppIcon from "./AppIcon.svelte";
  import Glyph from "./Glyph.svelte";
  import Segmented from "./Segmented.svelte";
  import Stepper from "./Stepper.svelte";

  let { slice, canEdit }: { slice: Slice; canEdit: boolean } = $props();

  const MEMBERS_SHOWN = 4;
  const uid = $props.id();

  const rules = $derived(ui.rules);
  const rule = $derived(slice.rule >= 0 ? (rules[slice.rule] ?? null) : null);
  const isOther = $derived(slice.target === null);
  const open = $derived((rule !== null || isOther) && ui.open === slice.key);
  const prio = $derived(rule && rule.enabled ? rule.priority : null);
  const pct = $derived(slice.share === null ? null : Math.round(slice.share / 100));

  const canLess = $derived(canEdit && canStep(rules, slice.rule, -1));
  const canMore = $derived(canEdit && canStep(rules, slice.rule, 1));
  const was = $derived.by(() => {
    const d = ui.draft;
    if (!d || !slice.enabled || slice.share === null) return null;
    const bp = wasShare(d, slice.rule);
    return bp === null ? null : Math.round(bp / 100);
  });

  const words = $derived(
    slice.status.kind === "quiet" || slice.status.kind === "off" || slice.bps < 50_000
      ? statusText(slice.status)
      : t("row.moving", { mbps: mbps(slice.bps), status: statusText(slice.status) }),
  );


  // Everything else's apps.
  let all = $state(false);
  const members = $derived(all ? slice.members : slice.members.slice(0, MEMBERS_SHOWN));

  function give(m: Member) {
    const i = rules.findIndex((r) => sameTarget(r.target, m.target));
    ui.edit(i >= 0 ? setEnabled(rules, i, true) : addRule(rules, m.target));
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
    ui.edit(setCeiling(rules, slice.rule, dir, limit));
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

<li
  class="slice"
  class:open
  class:hot={ui.hot.includes(slice.key)}
  class:off={!slice.enabled}
  class:prio-high={prio === "high"}
  class:prio-low={prio === "low"}
  use:flash={`${pct}|${rule?.priority}|${slice.enabled}|${JSON.stringify(rule?.ceiling ?? null)}`}
>
  <div class="row">
    {#if (rule || (isOther && slice.members.length > 0)) && canEdit}
      <button
        type="button"
        class="main"
        aria-expanded={open}
        aria-controls="{uid}-ed"
        aria-label={t("row.edit", { name: slice.name, status: words })}
        onclick={() => ui.toggle(slice.key)}
      >
        {@render who(true)}
      </button>
    {:else}
      <div class="main">{@render who(false)}</div>
    {/if}

    <div class="pct">
      {#if !slice.enabled && rule}
        <button type="button" class="btn" disabled={!canEdit} onclick={() => ui.edit(setEnabled(rules, slice.rule, true))}>
          {t("row.turnOn")}
        </button>
      {:else if slice.share === null && rule}
        <span class="spare">{t("row.free")}</span>
      {:else}
        <Stepper
          name={slice.name}
          {pct}
          {was}
          {canLess}
          {canMore}
          onstep={(dir) => ui.edit(stepShare(rules, slice.rule, dir))}
        />
      {/if}
    </div>
  </div>

  {#if open && isOther}
    <div class="ed" id="{uid}-ed">
      <p class="cap">{t("row.members")}</p>
      <div class="members">
        {#each members as m (m.key)}
          <button
            type="button"
            class="member"
            disabled={m.fixed || !canEdit}
            title={m.fixed ? t("list.unknown") : undefined}
            aria-label={m.fixed ? undefined : t("row.give", { name: m.name })}
            onclick={() => give(m)}
          >
            <AppIcon target={m.fixed ? null : m.target} name={m.name} size={18} />
            <span class="mn">{m.fixed ? t("row.unknown") : m.name}</span>
            <span class="mb num">{mbps(m.bps)}</span>
            {#if !m.fixed}<span class="plus"><Glyph name="plus" size={14} /></span>{/if}
          </button>
        {/each}
        {#if !all && slice.members.length > MEMBERS_SHOWN}
          <button type="button" class="member more" onclick={() => (all = true)}>
            {t("row.more.apps", { count: slice.members.length - MEMBERS_SHOWN })}
          </button>
        {/if}
      </div>
    </div>
  {/if}

  {#if open && rule}
    <div class="ed" id="{uid}-ed">
      <div class="panel">
        <div class="opt">
          <span class="k" title={t("edit.priority.hint")}><Glyph name="boost" size={18} />{t("edit.priority")}</span>
          <Segmented
            name="{uid}-prio"
            label={t("edit.priority")}
            options={priorities}
            value={rule.priority}
            disabled={!rule.enabled}
            onchange={(p) => ui.edit(setPriority(rules, slice.rule, p))}
          />
        </div>
        <label class="opt" title={t("edit.free.hint")}>
          <span class="k"><Glyph name="free" size={18} />{t("edit.free")}</span>
          <span class="visually-hidden" id="{uid}-free">{t("edit.free.hint")}</span>
          <input
            type="checkbox"
            role="switch"
            class="switch"
            checked={rule.share === 0}
            disabled={!rule.enabled}
            aria-describedby="{uid}-free"
            onchange={(e) => ui.edit(setRuleShare(rules, slice.rule, e.currentTarget.checked ? 0 : 2000))}
          />
        </label>
        <div class="opt">
          <span class="k" title={t("edit.limit.hint")}><Glyph name="gauge" size={18} />{t("edit.limit")}</span>
          {#if showLimits}
            <div class="limits">
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
          {:else}
            <button type="button" class="btn" disabled={!rule.enabled} onclick={() => (limitsAsked = true)}>
              <Glyph name="plus" size={16} />{t("edit.limit.add")}
            </button>
          {/if}
        </div>
      </div>
      <div class="foot">
        {#if slice.target?.kind === "site"}
          <p class="note">{t("edit.site")}</p>
        {/if}
        <button
          type="button"
          class="btn btn-danger"
          title={t("edit.remove.hint")}
          onclick={() => {
            ui.edit(removeRule(rules, slice.rule));
            ui.open = null;
          }}><Glyph name="trash" size={16} />{t("edit.remove")}</button
        >
      </div>
    </div>
  {/if}
</li>

{#snippet who(opens: boolean)}
  <AppIcon target={slice.target} name={slice.name} size={24} boxed />
  <span class="text">
    <span class="line1">
      <b>{slice.name}</b>
      {#if prio === "high"}
        <span class="chip high"><Glyph name="boost" size={13} />{t("priority.high.chip")}</span>
      {:else if prio === "low"}
        <span class="chip low">{t("priority.low.chip")}</span>
      {/if}
    </span>
    <span class="st">{words}</span>
  </span>
  {#if opens}
    <span class="chev" class:open aria-hidden="true"><Glyph name="down" size={18} /></span>
  {/if}
{/snippet}

<style>
  /* Rows sit flat inside the list's card; hover lifts one step. */
  .slice {
    list-style: none;
    transition: background-color 120ms linear;
  }
  .slice:hover {
    background-color: var(--hover-soft);
  }
  .slice.open {
    background-color: rgb(0 0 0 / 0.1);
  }
  /* The bar above points at this row (a hovered segment, a held handle). */
  .slice.hot {
    background-color: var(--hover-strong);
  }
  /* Priority, felt before it's read: High a breath of the shell's green
     from the left, Low a shade darker. */
  .slice.prio-high {
    background-image: linear-gradient(90deg, rgb(47 168 142 / 0.16), rgb(47 168 142 / 0.04) 45%, transparent 80%);
  }
  .slice.prio-low {
    background-image: linear-gradient(90deg, rgb(0 0 0 / 0.2), transparent 70%);
  }
  .prio-low b {
    color: var(--text-secondary);
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 16px;
    padding: 0 16px 0 0;
  }
  .main {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
    min-height: 68px;
    padding: 12px 0 12px 16px;
    border: 0;
    background: transparent;
    text-align: left;
  }
  button.main:focus-visible {
    outline-offset: -2px;
  }
  .text {
    display: flex;
    flex-direction: column;
    flex: 1;
    gap: 2px;
    min-width: 0;
  }
  .line1 {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 16px;
    font-weight: 700;
  }
  .st {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-secondary);
    font-size: 13.5px;
  }
  .chip {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 3px;
    height: 20px;
    padding: 0 7px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 700;
  }
  .chip.high {
    padding-left: 4px;
    background: var(--shell-soft);
    color: var(--shell-bright);
  }
  .chip.low {
    background: rgb(0 0 0 / 0.2);
    color: var(--text-secondary);
  }
  .chev {
    display: flex;
    flex: none;
    color: var(--text-tertiary);
    transition:
      transform 200ms var(--ease-out),
      color 120ms linear;
  }
  .slice:hover .chev,
  .chev.open {
    color: var(--text-primary);
  }
  .chev.open {
    transform: rotate(180deg);
  }
  .off b,
  .off :global(.wrap) {
    opacity: 0.6;
  }
  .pct {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    min-width: 168px;
  }
  .spare {
    width: 168px;
    text-align: center;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  /* Opened: a short panel under the name, one control per line. */
  .ed {
    padding: 0 16px 16px 66px;
  }
  .panel {
    border-radius: 12px;
    background: rgb(0 0 0 / 0.16);
  }
  .opt {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 24px;
    min-height: 52px;
    padding: 8px 14px;
  }
  .opt + .opt {
    border-top: 1px solid rgb(255 255 255 / 0.05);
  }
  .k {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 14.5px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .k :global(.glyph) {
    color: var(--text-tertiary);
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-top: 10px;
  }
  .foot .btn {
    margin-left: auto;
  }
  .note {
    margin: 0;
    font-size: 13px;
    color: var(--text-secondary);
  }
  .limits {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 20px;
  }
  .limit {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .limit label {
    font-size: 14px;
    color: var(--text-secondary);
  }
  .limit .field {
    width: 72px;
    text-align: right;
  }

  /* Everything else, opened: its apps; one click gives one a share. */
  .cap {
    margin: 0 0 10px;
    font-size: 13px;
    color: var(--text-secondary);
  }
  .members {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .member {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radius-control);
    background: var(--surface-2);
    font-size: 14px;
    transition: background-color 120ms linear;
  }
  .member .mb {
    color: var(--text-secondary);
    font-size: 13px;
  }
  .member .plus {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    background: rgb(0 0 0 / 0.18);
    color: var(--text-secondary);
    transition:
      background-color 120ms linear,
      color 120ms linear;
  }
  .member:hover:not(:disabled) {
    background: var(--surface-3);
  }
  .member:hover:not(:disabled) .plus {
    background: var(--accent);
    color: var(--on-accent);
  }
  .member:disabled {
    cursor: default;
  }
  .member.more {
    padding: 0 12px;
    color: var(--text-secondary);
  }
  @media (forced-colors: active) {
    .member {
      forced-color-adjust: none;
      border: 1px solid ButtonText;
    }
    .slice.hot {
      outline: 2px solid Highlight;
      outline-offset: -2px;
    }
  }
</style>
