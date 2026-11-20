<script lang="ts">
  // "Using your internet" (PLAN.md §14.2, Obsidian Jade): who is moving
  // data right now, busiest first, in one flat panel with hairlines between
  // the rows. Each row: the app's icon, its name and what's happening to
  // it, a thin bar of its part of your speed in its band's colour from
  // "Last 10 minutes" (Timeline, which also holds the Download / Upload
  // choice and the total), its Mbps, then Boost and its own ••• menu
  // (AppMenu), both there whether or not it's boosted. Hovering a row
  // lights its band in the chart. A
  // boosted row takes a jade wash, a "Boosted" mark, and the time it has
  // left in words and as a bar along its foot. When the order changes the
  // rows glide, so the eye can follow who overtook whom. While nothing is
  // using the internet, apps that are only open (steady.ts holds them at a
  // trickle) are said once, as idle, never listed as users (modes.idle).
  import { mbps, rowMbps, targetLabel } from "../format";
  import { t } from "../i18n";
  import { names, unnamed, type Ledger } from "../ledger";
  import { flip } from "svelte/animate";
  import { fade } from "svelte/transition";
  import { unblock } from "../actions";
  import { flash } from "../flash";
  import { glide, ms } from "../motion";
  import { boostLeft, boostShare, gameWhy, idle, inCall, inFront, liveBoost, users } from "../modes";
  import { service } from "../service.svelte";
  import type { Target } from "../types";
  import { ui } from "../ui.svelte";
  import AppIcon from "./AppIcon.svelte";
  import AppMenu from "./AppMenu.svelte";
  import BoostButton from "./BoostButton.svelte";
  import Glyph from "./Glyph.svelte";

  let {
    ledger,
    canEdit,
    colors = new Map(),
    lit = null,
    onlight,
  }: {
    ledger: Ledger | null;
    canEdit: boolean;
    /** Each row's colour, as its band in "Last 10 minutes" has it. */
    colors?: Map<string, string>;
    /** The row whose band is lit. */
    lit?: string | null;
    onlight?: (key: string | null) => void;
  } = $props();
  const ink = (key: string, n: number) => colors.get(key) ?? (n >= 0 ? `var(--ink-${n})` : "var(--stone-1)");

  const label = (target: Target) => targetLabel(target, service.snapshot?.apps ?? [], service.siteNames);
  const live = $derived(liveBoost(service.snapshot?.config.boost, ui.now));
  const focusBoost = $derived(service.snapshot?.config.settings.focus_boost !== false);
  // Missing from older services: on.
  const callsFirst = $derived(service.snapshot?.config.settings.calls_first !== false);
  const calls = $derived(callsFirst ? (service.stats?.calls ?? []) : []);
  const list = $derived(ledger ? users(ledger, live, label, 5, calls) : []);
  const resting = $derived(ledger ? idle(ledger).slice(0, 3) : []);
  /** What's left of the boost, 0–1, for its bar; null: until stopped. */
  const left = $derived(live ? boostShare(live, ui.now) : null);
  // The row's second line: a boost's time left, else why a game goes
  // first, else that it's in a call, else that it's in front, else what its
  // share is doing.
  function sub(u: (typeof list)[number]): string {
    if (u.boosted && live) return t("who.boost.left", { left: boostLeft(live, ui.now) });
    const why = gameWhy(u.target, service.stats?.games, focusBoost);
    if (why) return t(`who.game.${why}`);
    if (inCall(u.target, service.stats?.calls, callsFirst)) return t("who.call");
    return inFront(u.target, service.stats?.front, focusBoost) ? t("who.front") : u.status;
  }
  const first = (u: (typeof list)[number]) =>
    u.boosted ||
    gameWhy(u.target, service.stats?.games, focusBoost) !== null ||
    inCall(u.target, service.stats?.calls, callsFirst) ||
    inFront(u.target, service.stats?.front, focusBoost);
  // Website traffic whose name is hidden (ECH): said, never hidden.
  const hidden = $derived(ledger ? unnamed(ui.dir === "down" ? service.stats?.down : service.stats?.up) : null);
  const hiddenApps = $derived(
    hidden
      ? hidden.apps.length > 3
        ? t("unnamed.apps.more", { apps: hidden.apps.slice(0, 3).join(", "), count: hidden.apps.length - 3 })
        : hidden.apps.join(", ")
      : "",
  );
  const blocked = $derived(
    (service.snapshot?.config.blocked ?? []).map((id) => {
      const target: Target = { kind: "family", id };
      return { id, target, name: label(target) };
    }),
  );
</script>

<section class="card who" aria-labelledby="who-title">
  <div class="hd">
    <h2 class="card-title" id="who-title">{t("who.title")}</h2>
    {#if list.length > 1}
      <span class="order">{t("who.busiest")}</span>
    {/if}
  </div>
  {#if !ledger}
    <ul aria-hidden="true">
      {#each [0.5, 0.4, 0.55] as w, i (i)}
        <li class="skeleton"><span class="sk-ic"></span><span class="bar" style:width="{w * 100}%"></span></li>
      {/each}
    </ul>
  {:else if list.length === 0}
    <p class="empty">{ui.dir === "down" ? t("who.empty") : t("who.empty.up")}</p>
  {:else}
    <ul>
      {#each list as u (u.key)}
        <li
          class:boosted={u.boosted}
          class:lit={lit === u.key}
          onpointerenter={() => onlight?.(u.key)}
          onpointerleave={() => onlight?.(null)}
          animate:flip={glide()}
          in:fade={{ duration: ms(240) }}
          use:flash={`${u.boosted}|${u.ink}`}
        >
          <AppIcon target={u.target} name={u.name} size={22} boxed />
          <span class="nm">
            <span class="line1">
              <b>{u.name}</b>
              {#if u.boosted}
                <span class="tag">{t("who.boost.badge")}</span>
              {/if}
            </span>
            <small class:wait={u.waiting && !u.boosted} class:first={first(u)}>{sub(u)}</small>
            <span class="use" aria-hidden="true"
              ><i style:width="{Math.max(1.5, u.part * 100)}%" style:background={ink(u.key, u.ink)}></i></span
            >
          </span>
          <span class="mb num" aria-live="off"
            >{#if u.bps > 0}{rowMbps(u.bps)}<small>{t("line.unit")}</small>{/if}</span
          >
          <span class="acts">
            <BoostButton target={u.target} name={u.name} boosted={u.boosted} disabled={!canEdit} />
            <AppMenu target={u.target} name={u.name} disabled={!canEdit} />
          </span>
          {#if u.boosted && left !== null}
            <span class="left" aria-hidden="true"><i style:width="{left * 100}%"></i></span>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if resting.length > 0 && list.length === 0}
    <p class="foot idle">
      <span class="faces" aria-hidden="true">
        {#each resting as r (r.key)}
          <AppIcon target={r.target} name={r.name} size={14} boxed />
        {/each}
      </span>
      <span>{t(resting.length === 1 ? "now.idle.one" : "now.idle.many", { names: names(resting.map((r) => r.name)) })}</span>
    </p>
  {/if}
  {#if hidden}
    <p class="foot">
      <Glyph name="info" size={16} />
      <span
        ><b class="num">{t("unnamed.speed", { speed: mbps(hidden.bps) })}</b>
        {t("unnamed.body", { apps: hiddenApps })}</span
      >
    </p>
  {/if}
  {#if blocked.length > 0}
    <div class="foot blocked" role="group" aria-labelledby="who-blocked">
      <span class="k" id="who-blocked"><Glyph name="ban" size={16} />{t("block.title")}</span>
      {#each blocked as b (b.id)}
        <span class="chip">
          <AppIcon target={b.target} name={b.name} size={16} />
          {b.name}
          <button
            type="button"
            disabled={!canEdit}
            aria-label={t("block.unblock.label", { name: b.name })}
            onclick={() => void unblock(b.id)}>{t("block.unblock")}</button
          >
        </span>
      {/each}
    </div>
  {/if}
</section>

<style>
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding-bottom: 6px;
  }
  .hd {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    padding: 16px 16px 10px 20px;
  }
  .order {
    margin-left: auto;
    font-size: 13.5px;
    color: var(--text-tertiary);
  }
  ul {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0 6px;
    list-style: none;
  }
  li {
    position: relative;
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) 84px auto;
    align-items: center;
    gap: 14px;
    min-height: 62px;
    padding: 8px 8px 8px 14px;
    border-radius: 12px;
    overflow: hidden;
    transition: background-color 150ms var(--ease-out);
  }
  /* Hairlines between rows, from the name's edge, gone around the hovered
     one. */
  li + li::before {
    content: "";
    position: absolute;
    top: 0;
    left: 60px;
    right: 12px;
    height: 1px;
    background: var(--stroke-card);
    transition: opacity 150ms linear;
  }
  li:hover,
  li.lit {
    background-color: var(--hover-soft);
  }
  li:hover::before,
  li:hover + li::before,
  li.lit::before,
  li.lit + li::before {
    opacity: 0;
  }
  /* A boost: a jade wash from the left and the Boosted mark. */
  li.boosted {
    background-image: linear-gradient(90deg, var(--shell-soft), transparent 70%);
  }
  li.boosted::before,
  li.boosted + li::before {
    opacity: 0;
  }
  .nm {
    display: flex;
    flex-direction: column;
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
    font-size: 15px;
    font-weight: 700;
  }
  .tag {
    display: inline-flex;
    flex: none;
    align-items: center;
    height: 20px;
    padding: 0 7px;
    border-radius: 6px;
    background: var(--shell-soft);
    color: var(--shell-bright);
    font-size: 11.5px;
    font-weight: 800;
    animation: tag-in 420ms 160ms var(--ease-out) backwards;
  }
  @keyframes tag-in {
    from {
      opacity: 0;
      transform: translateX(-4px);
    }
  }
  .nm small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13.5px;
    color: var(--text-secondary);
  }
  .nm small.wait {
    color: var(--caution);
  }
  .nm small.first {
    color: var(--shell-bright);
  }
  /* How much of your speed it takes: a thin bar under its name, in its
     share's ink (stone inside Everything else). Its words say it too. */
  .use {
    display: block;
    height: 3px;
    margin-top: 7px;
    border-radius: 2px;
    background: rgb(255 255 255 / 0.05);
    overflow: hidden;
  }
  .use i {
    display: block;
    height: 100%;
    border-radius: 2px;
    transition: width 900ms var(--ease-out);
  }
  .mb {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    font-size: 15px;
    font-weight: 700;
    white-space: nowrap;
  }
  .mb small {
    margin-top: -2px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-tertiary);
  }
  .acts {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  /* The time it has left, shortening along the row's foot. */
  .left {
    position: absolute;
    left: 14px;
    right: 14px;
    bottom: 0;
    height: 2px;
    border-radius: 1px;
    background: rgb(45 184 148 / 0.15);
    overflow: hidden;
  }
  .left i {
    display: block;
    height: 100%;
    background: var(--shell);
    transition: width 1200ms linear;
  }
  .empty {
    margin: 0;
    padding: 2px 20px 16px;
    font-size: 14.5px;
    line-height: 1.45;
    color: var(--text-secondary);
  }
  /* What sits under the list: who's idle, unnamed traffic, blocked apps. */
  .foot {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin: 6px 0 4px;
    padding: 12px 20px 8px;
    border-top: 1px solid var(--stroke-card);
    font-size: 13.5px;
    line-height: 1.45;
    color: var(--text-secondary);
  }
  .foot :global(.glyph) {
    flex: none;
    align-self: center;
    color: var(--text-tertiary);
  }
  .foot b {
    font-size: inherit;
    color: var(--text-primary);
  }
  .idle {
    align-items: center;
    color: var(--text-tertiary);
  }
  .faces {
    display: flex;
    flex: none;
  }
  .faces > :global(*) {
    margin-right: -6px;
    border-radius: 7px;
    box-shadow: 0 0 0 2px var(--surface-1);
  }
  .faces > :global(*:last-child) {
    margin-right: 0;
  }
  .blocked {
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .k {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-right: 4px;
    font-weight: 700;
    color: var(--critical);
  }
  .k :global(.glyph) {
    color: var(--critical);
  }
  .chip {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 4px 0 8px;
    border-radius: 9px;
    background: var(--surface-2);
    color: var(--text-primary);
    font-size: 14px;
  }
  .chip button {
    height: 24px;
    padding: 0 8px;
    border: 0;
    border-radius: 7px;
    background: var(--surface-3);
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 700;
  }
  .chip button:hover:not(:disabled) {
    background: var(--accent);
    color: var(--on-accent);
  }
  .skeleton {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .sk-ic {
    width: 32px;
    height: 32px;
    border-radius: 9px;
    background: var(--skeleton);
  }
  .skeleton .bar {
    height: 14px;
    border-radius: 4px;
    background: var(--skeleton);
  }
  @media (forced-colors: active) {
    li.boosted {
      outline: 2px solid Highlight;
      outline-offset: -2px;
    }
    .tag,
    .left i {
      forced-color-adjust: none;
      background: Highlight;
      color: HighlightText;
    }
  }
</style>
