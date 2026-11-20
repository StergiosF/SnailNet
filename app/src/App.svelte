<script lang="ts">
  // Main window (PLAN.md §14.2, the Garden): the green side rail with the
  // pages (Now, Split, Settings) and the line; beside it the title bar's
  // caption buttons, a state that needs attention if there is one, and
  // the open page. First run takes the whole window until it's done.
  import { onMount } from "svelte";
  import AddSlice from "./lib/components/AddSlice.svelte";
  import FirstRun from "./lib/components/FirstRun.svelte";
  import LeaveDialog from "./lib/components/LeaveDialog.svelte";
  import LineDialog from "./lib/components/LineDialog.svelte";
  import NoticeBar from "./lib/components/NoticeBar.svelte";
  import NowPage from "./lib/components/NowPage.svelte";
  import SettingsPage from "./lib/components/SettingsPage.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import SplitPage from "./lib/components/SplitPage.svelte";
  import TitleBar from "./lib/components/TitleBar.svelte";
  import Toast from "./lib/components/Toast.svelte";
  import { targetLabel } from "./lib/format";
  import { t } from "./lib/i18n";
  import { glance, ledger } from "./lib/ledger";
  import { ProofWatch, proofOf } from "./lib/proof";
  import { needsFirstRun, newNetworkName } from "./lib/line";
  import { lineDialog } from "./lib/lineDialog.svelte";
  import { resume, startService } from "./lib/actions";
  import { noticeOf } from "./lib/notice";
  import { service } from "./lib/service.svelte";
  import { ui } from "./lib/ui.svelte";
  import { updates } from "./lib/update.svelte";
  import type { Target } from "./lib/types";

  onMount(() => {
    service.start().catch((e: unknown) => {
      service.link = { kind: "lost", detail: String(e) };
    });
    const stopTick = ui.tick();
    const stopUpdates = updates.watch();
    return () => {
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

  const connected = $derived(service.link.kind === "connected" && service.snapshot !== null);
  const label = (target: Target) => targetLabel(target, service.snapshot?.apps ?? [], service.siteNames);
  const dirStats = $derived(ui.dir === "down" ? service.stats?.down : service.stats?.up);
  const book = $derived(connected ? ledger(ui.rules, dirStats, label) : null);
  // The Now page always speaks of downloads: that's where the sharing is felt.
  const downBook = $derived(connected ? ledger(ui.rules, service.stats?.down, label) : null);
  const upBook = $derived(connected ? ledger(ui.rules, service.stats?.up, label) : null);
  // The proof (§11.10): ping with and without SnailNet while the line is
  // full, steadied by ProofWatch.
  const proofWatch = new ProofWatch();
  const proof = $derived.by(() => {
    const s = service.stats;
    if (!connected || !s) return proofWatch.step(null, 0, true);
    const now = proofOf(s, downBook, upBook, service.snapshot?.config.line.measured);
    return proofWatch.step(now, s.t_ms, s.state !== "Shaping");
  });
  const lineMbps = $derived(
    ui.dir === "down" ? service.snapshot?.config.line.down_mbps : service.snapshot?.config.line.up_mbps,
  );
  const lineBps = $derived(lineMbps === undefined ? null : lineMbps * 1e6);
  const downLine = $derived(service.snapshot?.config.line.down_mbps);
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

<div class="window">
  {#if firstRun}
    <div class="first">
      <TitleBar brand />
      <FirstRun onfinish={finishFirstRun} />
    </div>
  {:else}
    <Sidebar />
    <div class="col">
      <TitleBar />
      <main>
        {#if service.link.kind === "connecting"}
          <p class="visually-hidden" role="status">{t("link.connecting")}</p>
        {/if}
        {#if notice}
          <div class="notice"><NoticeBar {notice} /></div>
        {/if}
        {#key ui.page}
          <div class="page">
            {#if ui.page === "now"}
              <NowPage ledger={loading ? null : downBook} glance={sentence} {proof} canEdit={connected} />
            {:else if ui.page === "split"}
              {#if service.link.kind === "connecting" || connected}
                <SplitPage
                  ledger={book}
                  {lineBps}
                  {loading}
                  canEdit={connected}
                  showStrip={book !== null && service.stats !== null}
                />
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
    flex: 1;
    overflow-y: auto;
    padding: 0 var(--gutter) 28px;
  }
  .notice {
    margin-bottom: 16px;
    animation: page-in 320ms var(--ease-out) backwards;
  }
  .page {
    animation: page-in 380ms var(--ease-out) backwards;
  }
</style>
