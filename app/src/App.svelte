<script lang="ts">
  // Main window (PLAN.md §14.2, Obsidian Jade): the side rail with the
  // pages (Now, Split, Settings) and the line; beside it the title bar's
  // caption buttons, a state that needs attention if there is one, and
  // the open page. First run takes the whole window until it's done.
  import { onMount, untrack } from "svelte";
  import { timeline } from "./lib/timeline.svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { invoke } from "@tauri-apps/api/core";
  import AddSlice from "./lib/components/AddSlice.svelte";
  import AutostartDialog from "./lib/components/AutostartDialog.svelte";
  import FirstRun from "./lib/components/FirstRun.svelte";
  import LeaveDialog from "./lib/components/LeaveDialog.svelte";
  import LineDialog from "./lib/components/LineDialog.svelte";
  import NoticeBar from "./lib/components/NoticeBar.svelte";
  import NowPage from "./lib/components/NowPage.svelte";
  import SettingsPage from "./lib/components/SettingsPage.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import SplitPage from "./lib/components/SplitPage.svelte";
  import ModeSwitch from "./lib/components/ModeSwitch.svelte";
  import TitleBar from "./lib/components/TitleBar.svelte";
  import Toast from "./lib/components/Toast.svelte";
  import WhatsNewDialog from "./lib/components/WhatsNewDialog.svelte";
  import { targetLabel } from "./lib/format";
  import { t } from "./lib/i18n";
  import { glance, ledger } from "./lib/ledger";
  import { ProofWatch, proofOf, type Proof } from "./lib/proof";
  import { needsFirstRun, newNetworkName } from "./lib/line";
  import { lineDialog } from "./lib/lineDialog.svelte";
  import { resume, startService } from "./lib/actions";
  import { fixer } from "./lib/fixer.svelte";
  import { noticeOf } from "./lib/notice";
  import { service } from "./lib/service.svelte";
  import { ui } from "./lib/ui.svelte";
  import { updates } from "./lib/update.svelte";
  import { askAutostartNow } from "./lib/autostart";
  import type { Target } from "./lib/types";

  onMount(() => {
    const stopCatchup = service.onCatchup((ticks) => {
      if (!service.snapshot) return;
      const rules = service.activeProfile?.rules ?? [];
      for (const tick of ticks) {
        timeline.feed(tick.t_ms, ledger(rules, tick.down, label), ledger(rules, tick.up, label));
      }
    });
    service.start().catch((e: unknown) => {
      service.link = { kind: "lost", detail: String(e) };
    });
    invoke<boolean>("autostart_ask")
      .then((ask) => (autostartPending = ask))
      .catch(() => {});
    getVersion()
      .then((v) => updates.initVersion(v))
      .catch(() => {});
    const stopTick = ui.tick();
    const stopUpdates = updates.watch();
    return () => {
      stopCatchup();
      stopTick();
      stopUpdates();
    };
  });

  // First run (§14.5): once the service answers with no speed set; it
  // then stays until finished, even after a speed is saved.
  let firstRun = $state(false);
  $effect(() => {
    if (needsFirstRun(service.link, service.snapshot, ui.firstRunDone)) firstRun = true;
  });
  function finishFirstRun() {
    ui.finishFirstRun();
    firstRun = false;
  }

  // A fresh install asks once whether to start with Windows, after first
  // run (autostart.rs remembers the answer).
  let autostartPending = $state(false);
  const askAutostart = $derived(askAutostartNow(autostartPending, firstRun, service.link, service.snapshot));

  const connected = $derived(service.link.kind === "connected" && service.snapshot !== null);
  const label = (target: Target) => targetLabel(target, service.snapshot?.apps ?? [], service.siteNames);
  // The Now page's sentence always speaks of downloads (that's where the
  // sharing is felt); its list follows the Download / Upload choice.
  const downBook = $derived(connected ? ledger(ui.rules, service.stats?.down, label, service.waiting.down) : null);
  const upBook = $derived(connected ? ledger(ui.rules, service.stats?.up, label, service.waiting.up) : null);
  const book = $derived(ui.dir === "down" ? downBook : upBook);
  // "Last 10 minutes" (timeline.svelte.ts), ping proof (§11.10), and the
  // one-tap fix (fixer.svelte.ts): driven strictly by stats ticks and link
  // state, reusing the single down/up ledgers so Split-bar drags (`ui.rules`)
  // never trigger side effects.
  const proofWatch = new ProofWatch();
  let proof = $state<Proof | null>(null);
  $effect(() => {
    const s = service.stats;
    const ok = connected;
    untrack(() => {
      if (!ok || !s) {
        proof = proofWatch.step(null, 0, true);
        fixer.sync(null, null);
        return;
      }
      const down = downBook;
      const up = upBook;
      timeline.feed(s.t_ms, down, up);
      const now = proofOf(s, down, up, service.snapshot?.config.line.measured);
      proof = proofWatch.step(now, s.t_ms, s.state !== "Shaping");
      fixer.sync(down, up);
    });
  });
  $effect(() => {
    const c = service.snapshot?.config;
    if (c) untrack(() => timeline.config(c, label));
  });
  const downLine = $derived(service.snapshot?.config.line.down_mbps);
  const dirLine = $derived(ui.dir === "down" ? downLine : service.snapshot?.config.line.up_mbps);
  const loading = $derived(
    service.link.kind === "connecting" || (service.link.kind === "connected" && service.stats === null),
  );
  const sentence = $derived(
    downBook && service.stats ? glance(downBook, downLine === undefined ? null : downLine * 1e6) : null,
  );
  const notice = $derived(
    noticeOf(
      service.link,
      service.state,
      { setSpeed: () => lineDialog.show(), resume: () => void resume(), startService: () => void startService() },
      ui.now,
      ui.online,
      newNetworkName(service.snapshot?.config),
    ),
  );
</script>

{#snippet modeSwitch()}
  <ModeSwitch disabled={!connected} />
{/snippet}

<div class="window">
  {#if firstRun}
    <div class="first">
      <TitleBar brand />
      <FirstRun onfinish={finishFirstRun} />
    </div>
  {:else}
    <Sidebar />
    <div class="col">
      <!-- The mode switch on every page: the strip never changes height. -->
      <TitleBar lead={modeSwitch} />
      <main class:now={ui.page === "now"}>
        {#if service.link.kind === "connecting"}
          <p class="visually-hidden" role="status">{t("link.connecting")}</p>
        {/if}
        {#if notice}
          <div class="notice"><NoticeBar {notice} /></div>
        {/if}
        {#key ui.page}
          <div class="page">
            {#if ui.page === "now"}
              <NowPage
                ledger={loading ? null : book}
                glance={sentence}
                {proof}
                canEdit={connected}
                lineBps={dirLine === undefined ? null : dirLine * 1e6}
              />
            {:else if ui.page === "split"}
              {#if service.link.kind === "connecting" || connected}
                <SplitPage ledger={book} {loading} canEdit={connected} />
              {/if}
            {:else}
              <SettingsPage canEdit={connected} />
            {/if}
          </div>
        {/key}
      </main>
    </div>
    <Toast />
  {/if}
  <AddSlice />
  <LineDialog />
  <LeaveDialog />
  <AutostartDialog open={askAutostart} ondone={() => (autostartPending = false)} />
  <WhatsNewDialog
    data={updates.justUpdated}
    open={!firstRun && !askAutostart && updates.justUpdated !== null}
    onclose={() => updates.dismissWhatsNew()}
  />
</div>

<style>
  .window {
    display: flex;
    height: 100%;
  }
  .first {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .col {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  main {
    display: flex;
    flex-direction: column;
    flex: 1;
    overflow-y: auto;
    padding: 14px var(--gutter) 28px;
  }
  /* Now fits the window: the page itself never scrolls, only the app
     list inside it (WhoCard). Narrow windows still stack and scroll. */
  main.now {
    overflow: hidden;
  }
  @media (max-width: 900px) {
    main {
      --gutter: 20px;
    }
  }
  .notice {
    flex: none;
    margin-bottom: 16px;
    animation: page-in 320ms var(--ease-out) backwards;
  }
  .page {
    flex: none;
    animation: page-in 380ms var(--ease-out) backwards;
  }
  main.now .page {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
</style>
