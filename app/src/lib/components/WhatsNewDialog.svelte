<script lang="ts">
  // Post-update "What's new" modal: shown once after SnailNet installs an
  // update (and reopenable anytime from Settings → About). Built on a native
  // <dialog> with SnailNet's Obsidian Jade surfaces, icon-led highlights,
  // full keyboard focus handling, and reduced-motion respect.
  import { quietFocus } from "../focusRing";
  import { tick } from "svelte";
  import { t } from "../i18n";
  import type { UpdateModalData } from "../whatsNew";
  import Glyph from "./Glyph.svelte";
  import Snail from "./Snail.svelte";

  let {
    data,
    open,
    onclose,
  }: {
    data: UpdateModalData | null;
    open: boolean;
    onclose: () => void;
  } = $props();

  const uid = $props.id();
  let el = $state<HTMLDialogElement | null>(null);
  let okBtn = $state<HTMLButtonElement | null>(null);
  let opener: HTMLElement | null = null;

  $effect(() => {
    if (!el) return;
    if (open && data && !el.open) {
      opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      el.showModal();
      void tick().then(() => okBtn?.focus());
    } else if ((!open || !data) && el.open) {
      el.close();
      if (opener?.isConnected) {
        quietFocus(opener);
        opener.focus();
      }
      opener = null;
    }
  });
</script>

<dialog
  bind:this={el}
  aria-labelledby="{uid}-title"
  aria-describedby="{uid}-sub"
  oncancel={(e) => {
    e.preventDefault();
    onclose();
  }}
>
  {#if data}
    <div class="shell">
      <div class="head">
        <div class="emblem" aria-hidden="true">
          <Snail size={28} light />
        </div>
        <div class="titles">
          <div class="top-row">
            <span class="pill num">{t("whatsNew.version", { version: data.version })}</span>
          </div>
          <h2 id="{uid}-title">{t("whatsNew.title")}</h2>
          <p class="sub" id="{uid}-sub">{t("whatsNew.subtitle")}</p>
        </div>
        <button type="button" class="x" aria-label={t("editor.close")} onclick={onclose}>
          <Glyph name="close" size={18} />
        </button>
      </div>

      <ul class="notes" aria-label={t("whatsNew.list")}>
        {#each data.items as item, i (`${i}:${item.title}`)}
          <li class="note" style:animation-delay="{60 + i * 45}ms">
            <span class="badge {item.tone ?? 'jade'}" aria-hidden="true">
              <Glyph name={item.icon} size={19} />
            </span>
            <div class="copy">
              <b>{item.title}</b>
              {#if item.body}
                <p>{item.body}</p>
              {/if}
            </div>
          </li>
        {/each}
      </ul>

      <div class="foot">
        <button bind:this={okBtn} type="button" class="btn btn-accent done" onclick={onclose}>
          {t("whatsNew.done")}
        </button>
      </div>
    </div>
  {/if}
</dialog>

<style>
  dialog {
    width: min(490px, calc(100vw - 40px));
    max-height: min(620px, calc(100vh - 48px));
    overflow: hidden;
    background:
      radial-gradient(420px 160px at 20% -10%, rgb(45 184 148 / 0.16), transparent 72%),
      var(--surface-1);
    box-shadow:
      0 1px 0 rgb(255 255 255 / 0.06) inset,
      0 0 0 1px var(--stroke-strong),
      0 28px 72px rgb(0 0 0 / 0.62);
  }
  dialog[open] {
    display: flex;
    flex-direction: column;
  }
  .shell {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    padding: 24px 24px 20px;
  }
  .head {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    margin-bottom: 18px;
  }
  .emblem {
    display: grid;
    place-items: center;
    flex: none;
    width: 44px;
    height: 44px;
    border-radius: 13px;
    background: var(--shell-soft);
    box-shadow: inset 0 0 0 1px rgb(94 224 189 / 0.2);
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  .top-row {
    display: flex;
    align-items: center;
    margin-bottom: 4px;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    height: 21px;
    padding: 0 8px;
    border-radius: 999px;
    background: var(--shell-soft);
    color: var(--shell-bright);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.01em;
  }
  h2 {
    margin: 0;
    font-size: 21px;
    line-height: 26px;
    font-weight: 800;
    letter-spacing: -0.02em;
  }
  .sub {
    margin: 4px 0 0;
    font-size: 14px;
    line-height: 1.45;
    color: var(--text-secondary);
  }
  .x {
    display: grid;
    place-items: center;
    flex: none;
    width: 32px;
    height: 32px;
    margin: -4px -6px 0 0;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: var(--text-tertiary);
    transition:
      background-color 120ms linear,
      color 120ms linear;
  }
  .x:hover {
    background: var(--hover-soft);
    color: var(--text-primary);
  }

  .notes {
    display: flex;
    flex: 0 1 auto;
    flex-direction: column;
    min-height: 0;
    margin: 0;
    padding: 4px;
    list-style: none;
    overflow-y: auto;
    border-radius: 14px;
    background: var(--surface-side);
    box-shadow: inset 0 0 0 1px var(--stroke-card);
  }
  .note {
    position: relative;
    display: flex;
    align-items: flex-start;
    gap: 14px;
    padding: 13px 14px;
    animation: rise-in 320ms var(--ease-out) backwards;
  }
  .note + .note::before {
    content: "";
    position: absolute;
    top: 0;
    left: 64px;
    right: 14px;
    height: 1px;
    background: var(--stroke-card);
  }
  .badge {
    display: grid;
    place-items: center;
    flex: none;
    width: 36px;
    height: 36px;
    margin-top: 1px;
    border-radius: 10px;
    background: var(--shell-soft);
    color: var(--shell-bright);
  }
  .badge.warm {
    background: color-mix(in srgb, var(--ink-1) 14%, transparent);
    color: var(--ink-1);
  }
  .badge.sky {
    background: color-mix(in srgb, var(--ink-2) 14%, transparent);
    color: var(--ink-2);
  }
  .badge.neutral {
    background: var(--surface-2);
    color: var(--text-secondary);
  }
  .copy {
    flex: 1;
    min-width: 0;
  }
  .copy b {
    display: block;
    font-size: 14.5px;
    line-height: 1.35;
    font-weight: 700;
    color: var(--text-primary);
  }
  .copy p {
    margin: 3px 0 0;
    font-size: 13.5px;
    line-height: 1.45;
    color: var(--text-secondary);
  }

  .foot {
    display: flex;
    justify-content: flex-end;
    margin-top: 18px;
  }
  .done {
    min-width: 104px;
    font-weight: 700;
  }

  @media (forced-colors: active) {
    .emblem,
    .pill,
    .badge,
    .notes {
      border: 1px solid CanvasText;
    }
    .note + .note::before {
      background: CanvasText;
    }
  }
</style>
