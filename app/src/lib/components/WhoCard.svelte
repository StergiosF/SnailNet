<script lang="ts">
  // "Using your internet" (PLAN.md §14.2): who is moving data right now,
  // busiest first, each with what's happening to it and a Boost. The
  // boosted one is its own thing: lit by the shell's green, its icon
  // ringed, a "Boosted" mark, the time it has left in words and as a bar
  // along its foot that shortens as the boost runs out. The green sweeps
  // in once when a boost starts; nothing loops. When the order changes
  // the rows glide to their new places, so the eye can follow who
  // overtook whom.
  import { mbps, rowMbps, targetLabel } from "../format";
  import { t } from "../i18n";
  import { unnamed, type Ledger } from "../ledger";
  import { flip } from "svelte/animate";
  import { fade } from "svelte/transition";
  import { unblock } from "../actions";
  import { glide, ms } from "../motion";
  import { boostLeft, boostShare, gameWhy, inCall, liveBoost, users } from "../modes";
  import { service } from "../service.svelte";
  import type { Target } from "../types";
  import { ui } from "../ui.svelte";
  import AppIcon from "./AppIcon.svelte";
  import BoostButton from "./BoostButton.svelte";
  import Snail from "./Snail.svelte";

  let { ledger, canEdit }: { ledger: Ledger | null; canEdit: boolean } = $props();

  const label = (target: Target) => targetLabel(target, service.snapshot?.apps ?? [], service.siteNames);
  const live = $derived(liveBoost(service.snapshot?.config.boost, ui.now));
  const focusBoost = $derived(service.snapshot?.config.settings.focus_boost !== false);
  // Missing from older services: on.
  const callsFirst = $derived(service.snapshot?.config.settings.calls_first !== false);
  const calls = $derived(callsFirst ? (service.stats?.calls ?? []) : []);
  const list = $derived(ledger ? users(ledger, live, label, 5, calls) : []);
  /** What's left of the boost, 0–1, for its bar; null: until stopped. */
  const left = $derived(live ? boostShare(live, ui.now) : null);
  // The row's second line: a boost's time left, else why a game goes
  // first, else that it's in a call, else what its slice is doing.
  function sub(u: (typeof list)[number]): string {
    if (u.boosted && live) return t("who.boost.left", { left: boostLeft(live, ui.now) });
    const why = gameWhy(u.target, service.stats?.games, focusBoost);
    if (why) return t(`who.game.${why}`);
    return inCall(u.target, service.stats?.calls, callsFirst) ? t("who.call") : u.status;
  }
  // Website traffic whose name is hidden (ECH): said, never hidden.
  const hidden = $derived(ledger ? unnamed(service.stats?.down) : null);
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
  <h2 class="card-title" id="who-title">{t("who.title")}</h2>
  {#if !ledger}
    <ul aria-hidden="true">
      {#each [0.5, 0.4, 0.55] as w, i (i)}
        <li class="skeleton"><span class="ic"></span><span class="bar" style:width="{w * 100}%"></span></li>
      {/each}
    </ul>
  {:else if list.length === 0}
    <div class="empty">
      <Snail size={40} light />
      <p>{t("who.empty")}</p>
    </div>
  {:else}
    <ul>
      {#each list as u (u.key)}
        <li class:boosted={u.boosted} animate:flip={glide()} in:fade={{ duration: ms(240) }}>
          <span class="ic"><AppIcon target={u.target} name={u.name} size={22} boxed /></span>
          <span class="nm">
            <span class="line1">
              <b>{u.name}</b>
              {#if u.boosted}
                <span class="badge"><i class="icon" aria-hidden="true">&#xE945;</i>{t("who.boost.badge")}</span>
              {/if}
            </span>
            <small>{sub(u)}</small>
          </span>
          <span class="mb num">{u.bps > 0 ? rowMbps(u.bps) : ""}<small>{u.bps > 0 ? ` ${t("line.unit")}` : ""}</small></span>
          <BoostButton target={u.target} name={u.name} boosted={u.boosted} disabled={!canEdit} />
          {#if u.boosted && left !== null}
            <span class="left" aria-hidden="true"><i style:width="{left * 100}%"></i></span>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if hidden}
    <p class="unnamed">
      <i class="icon" aria-hidden="true">&#xE72E;</i>
      <span
        ><b class="num">{t("unnamed.speed", { speed: mbps(hidden.bps) })}</b>
        {t("unnamed.body", { apps: hiddenApps })}</span
      >
    </p>
  {/if}
  {#if blocked.length > 0}
    <div class="blocked" role="group" aria-labelledby="who-blocked">
      <span class="k" id="who-blocked"><i class="icon" aria-hidden="true">&#xE8F8;</i>{t("block.title")}</span>
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
    gap: 8px;
    padding: 16px 12px 10px;
    min-width: 0;
  }
  h2 {
    padding: 0 6px 4px;
  }
  ul {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    position: relative;
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 12px;
    min-height: 52px;
    padding: 6px 6px;
    border-radius: 12px;
    overflow: hidden;
    background: linear-gradient(90deg, rgb(47 168 142 / 0.26), rgb(47 168 142 / 0.08) 75%) no-repeat;
    background-size: 0% 100%;
    transition:
      background-size 600ms var(--ease-out),
      box-shadow 400ms var(--ease-out);
  }
  li:hover {
    background-color: rgb(255 255 255 / 0.025);
  }
  /* A boost: the shell's green sweeps in from the left (once), the row
     takes a hairline of it and sits a little forward. */
  li.boosted {
    min-height: 60px;
    margin: 4px 0;
    background-size: 100% 100%;
    box-shadow:
      inset 0 0 0 1px rgb(47 168 142 / 0.4),
      0 10px 24px -14px rgb(0 0 0 / 0.6);
  }
  .ic {
    position: relative;
    display: grid;
  }
  /* Its icon ringed in the shell's green. */
  li.boosted .ic::after {
    content: "";
    position: absolute;
    inset: -3px;
    border-radius: 13px;
    box-shadow: 0 0 0 2px var(--shell);
    animation: ring-in 520ms 120ms var(--ease-out) backwards;
  }
  @keyframes ring-in {
    from {
      opacity: 0;
      transform: scale(1.25);
    }
  }
  .line1 {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .badge {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 4px;
    height: 20px;
    padding: 0 7px 0 5px;
    border-radius: 6px;
    background: var(--shell);
    color: var(--on-shell);
    font-size: 12px;
    font-weight: 800;
    animation: badge-in 480ms 200ms var(--ease-out) backwards;
  }
  .badge .icon {
    font-size: 11px;
  }
  @keyframes badge-in {
    from {
      opacity: 0;
      transform: translateX(-6px) scale(0.9);
    }
  }
  /* The time it has left, shortening along the row's foot. */
  .left {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 3px;
    background: rgb(47 168 142 / 0.18);
  }
  .left i {
    display: block;
    height: 100%;
    background: var(--shell);
    border-radius: 0 2px 2px 0;
    transition: width 1200ms linear;
    animation: drain-in 900ms 240ms var(--ease-out) backwards;
    transform-origin: left;
  }
  @keyframes drain-in {
    from {
      transform: scaleX(0);
    }
  }
  .nm {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 15px;
    font-weight: 700;
  }
  .nm small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    color: var(--text-secondary);
  }
  li.boosted .nm small {
    color: var(--text-primary);
  }
  .mb {
    font-size: 15px;
    font-weight: 700;
    text-align: right;
    white-space: nowrap;
  }
  .mb small {
    font-size: 12px;
    font-weight: 400;
    color: var(--text-secondary);
  }
  .unnamed {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 4px 6px 2px;
    padding-top: 10px;
    border-top: 1px solid var(--stroke-card);
    font-size: 13px;
    line-height: 1.45;
    color: var(--text-secondary);
  }
  .unnamed .icon {
    flex: none;
    font-size: 12px;
    transform: translateY(1px);
  }
  .unnamed b {
    font-weight: 700;
    color: var(--text-primary);
  }
  .blocked {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin: 4px 6px 6px;
    padding-top: 10px;
    border-top: 1px solid var(--stroke-card);
  }
  .k {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-right: 4px;
    font-size: 13px;
    font-weight: 700;
    color: var(--critical);
  }
  .k .icon {
    font-size: 12px;
  }
  .chip {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 4px 0 8px;
    border-radius: 9px;
    background: var(--surface-2);
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
  .empty {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 6px 12px;
  }
  .empty p {
    margin: 0;
    color: var(--text-secondary);
    line-height: 1.45;
  }
  .skeleton {
    display: flex;
    gap: 12px;
  }
  .skeleton .ic {
    width: 34px;
    height: 34px;
    border-radius: var(--radius-control);
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
    .badge,
    .left i {
      forced-color-adjust: none;
      background: Highlight;
      color: HighlightText;
    }
  }
</style>
