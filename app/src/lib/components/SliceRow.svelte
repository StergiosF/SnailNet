<script lang="ts">
  // One share of your internet (PLAN.md §14.2, DESIGN.md Split): icon,
  // name with its ink dot (the colour of its segment in the drag bar),
  // what it moves now in a few words, and − % + in 5% steps (no bar of its
  // own: the drag bar is the one picture of the split), with what it was
  // while the change isn't saved (draft.ts). Priority is said in words at
  // the start of the status ("High priority ·"), never a tint or a chip.
  // Opened, the options sit flat inside the row, one per line with its
  // explanation under its name (priority, only spare speed, a speed
  // limit), and Remove last. Everything else opens to its apps; one click
  // gives one a share of its own. Every edit goes into the page's draft.
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

  const inkOf = (s: Slice) =>
    s.rule < 0 || s.share === null ? "var(--stone-1)" : `var(--ink-${Math.max(0, s.ink)})`;
  const opens = $derived((rule !== null || (isOther && slice.members.length > 0)) && canEdit);

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
  style:--ink={inkOf(slice)}
  use:flash={`${pct}|${rule?.priority}|${slice.enabled}|${JSON.stringify(rule?.ceiling ?? null)}`}
>
  <div class="row">
    {#if opens}
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
        <button type="button" class="btn" disabled={!canEdit} onclick={() => ui.edit(setEnabled(rules, slice.rule, true))}>
          {t("row.turnOn")}
        </button>
      {:else if slice.share === null && rule}
        <span class="spare">{t("row.free")}</span>
      {:else if rule}
        <Stepper
          name={slice.name}
          {pct}
          {was}
          {canLess}
          {canMore}
          onstep={(dir) => ui.edit(stepShare(rules, slice.rule, dir))}
        />
      {:else if pct !== null}
        <span class="spare other num">{pct}%</span>
      {/if}
    </div>
    {#if opens}
      <!-- The same toggle as the row's name, for the pointer at the row's
           end; the name's button is the one for the keyboard. -->
      <button type="button" class="chev" class:open tabindex="-1" aria-hidden="true" onclick={() => ui.toggle(slice.key)}
        ><Glyph name="chevron" size={18} /></button
      >
    {:else}
      <span class="chev-gap" aria-hidden="true"></span>
    {/if}
  </div>

  {#if open && isOther}
    <div class="ed members-ed" id="{uid}-ed">
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
      <div class="opts">
        <div class="opt">
          <span class="k"><b>{t("edit.priority")}</b><small>{t("edit.priority.hint")}</small></span>
          <Segmented
            name="{uid}-prio"
            label={t("edit.priority")}
            options={priorities}
            value={rule.priority}
            disabled={!rule.enabled}
            onchange={(p) => ui.edit(setPriority(rules, slice.rule, p))}
          />
        </div>
        <label class="opt">
          <span class="k"><b>{t("edit.free")}</b><small id="{uid}-free">{t("edit.free.hint")}</small></span>
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
          <span class="k"><b>{t("edit.limit")}</b><small>{t("edit.limit.hint")}</small></span>
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
      <div class="opt foot">
        <p class="note">{slice.target?.kind === "site" ? t("edit.site") : t("edit.remove.hint")}</p>
        <button
          type="button"
          class="btn btn-quiet remove"
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

{#snippet who()}
  <AppIcon target={slice.target} name={slice.name} size={22} boxed />
  <span class="text">
    <span class="line1">
      <b>{slice.name}</b>
      {#if slice.enabled && slice.rule >= 0}<span class="dotk" aria-hidden="true"></span>{/if}
    </span>
    <span class="st"
      >{#if prio === "high"}<span class="pr">{t("priority.high.chip")}</span>
      {:else if prio === "low"}<span class="pr low">{t("priority.low.chip")}</span>
      {/if}{isOther && !open ? t("split.other.hint") : words}</span
    >
  </span>
{/snippet}

<style>
  /* Rows sit flat inside the list's panel; hover lifts one step. */
  .slice {
    list-style: none;
    border-radius: 12px;
    transition: background-color 120ms linear;
  }
  .slice:hover {
    background-color: var(--hover-soft);
  }
  /* The bar above points at this row (a hovered segment, a held handle). */
  .slice.hot {
    background-color: var(--hover-strong);
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto 32px;
    align-items: center;
    gap: 12px;
    padding: 0 8px 0 0;
  }
  .main {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
    min-height: 62px;
    padding: 8px 0 8px 14px;
    border: 0;
    border-radius: 12px;
    background: transparent;
    text-align: left;
  }
  .dotk {
    flex: none;
    width: 9px;
    height: 9px;
    border-radius: 3px;
    background: var(--ink);
  }
  .pr {
    color: var(--shell-bright);
    font-weight: 700;
  }
  .pr.low {
    color: var(--text-tertiary);
  }
  /* The separator before the status words (Svelte trims a literal one). */
  .pr::after {
    content: " · ";
    color: var(--text-secondary);
    font-weight: 400;
    white-space: pre;
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
  .line1 b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 15px;
    font-weight: 700;
  }
  .st {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-secondary);
    font-size: 13.5px;
  }
  .chev {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: var(--text-tertiary);
    transition: color 120ms linear, background-color 120ms linear;
  }
  .chev :global(.glyph) {
    transition: transform 240ms var(--ease-out);
  }
  .chev:hover {
    background: var(--surface-3);
    color: var(--text-primary);
  }
  .slice:hover .chev,
  .chev.open {
    color: var(--text-primary);
  }
  .chev.open :global(.glyph) {
    transform: rotate(90deg);
  }
  .off b,
  .off :global(.wrap) {
    opacity: 0.6;
  }
  .pct {
    display: flex;
    align-items: center;
    justify-content: flex-end;
  }
  /* Everything else's share: what's left, not stepped here. */
  .spare.other {
    min-width: 46px;
    color: var(--text-primary);
    font-size: 15px;
    font-weight: 800;
  }
  .spare {
    padding: 0 10px;
    text-align: center;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  /* Opened: the options flat inside the row, under the name, one per line
     with a hairline between, its explanation under its name. */
  .ed {
    margin: 0 14px 0 60px;
    border-top: 1px solid var(--stroke-card);
  }
  .members-ed {
    padding: 12px 0 14px;
  }
  .opt {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 20px;
    min-height: 54px;
    padding: 8px 0;
  }
  .opt + .opt {
    border-top: 1px solid var(--stroke-card);
  }
  .k {
    display: flex;
    flex-direction: column;
    flex: 1 1 260px;
    min-width: 0;
  }
  .k b {
    font-size: 14.5px;
    font-weight: 650;
  }
  .k small {
    font-size: 13px;
    color: var(--text-tertiary);
  }
  .note {
    flex: 1 1 260px;
    margin: 0;
    font-size: 13px;
    color: var(--text-tertiary);
  }
  .remove {
    color: var(--critical);
  }
  .remove:hover:not(:disabled) {
    background: var(--critical-soft);
    color: var(--critical);
  }
  .remove :global(.glyph),
  .remove:hover :global(.glyph) {
    color: var(--critical);
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
    display: flex;
    color: var(--text-tertiary);
    transition: color 120ms linear;
  }
  .member:hover:not(:disabled) {
    background: var(--surface-3);
  }
  .member:hover:not(:disabled) .plus {
    color: var(--shell-bright);
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
