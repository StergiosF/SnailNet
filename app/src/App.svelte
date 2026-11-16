<script lang="ts">
  // Main window (PLAN.md §14.2, the Ledger): title bar; a sentence on who
  // is using the line (or the state that needs attention); the live strip;
  // the slices, each with − % + and its settings in place; dialogs.
  import { onMount } from "svelte";
  import AddSlice from "./lib/components/AddSlice.svelte";
  import FirstRun from "./lib/components/FirstRun.svelte";
  import Header from "./lib/components/Header.svelte";
  import LineDialog from "./lib/components/LineDialog.svelte";
  import SliceList from "./lib/components/SliceList.svelte";
  import TitleBar from "./lib/components/TitleBar.svelte";
  import UsageStrip from "./lib/components/UsageStrip.svelte";
  import { targetLabel } from "./lib/format";
  import { t } from "./lib/i18n";
  import { glance, ledger } from "./lib/ledger";
  import { dismissFirstRun, firstRunDismissed, needsFirstRun } from "./lib/line";
  import { lineDialog } from "./lib/lineDialog.svelte";
  import { service } from "./lib/service.svelte";
  import { noticeOf } from "./lib/notice";
  import { ui } from "./lib/ui.svelte";
  import type { Target } from "./lib/types";

  onMount(() => {
    service.start().catch((e: unknown) => {
      service.link = { kind: "lost", detail: String(e) };
    });
  });

  // First run (§14.5): once the service answers with no speed set; it
  // then stays until finished, even after a speed is saved.
  let firstRunDone = $state(firstRunDismissed());
  let firstRun = $state(false);
  $effect(() => {
    if (needsFirstRun(service.link, service.snapshot, firstRunDone)) firstRun = true;
  });
  function finishFirstRun() {
    dismissFirstRun();
    firstRunDone = true;
    firstRun = false;
  }

  const connected = $derived(service.link.kind === "connected" && service.snapshot !== null);
  const label = (target: Target) => targetLabel(target, service.snapshot?.apps ?? [], service.siteNames);
  const dirStats = $derived(ui.dir === "down" ? service.stats?.down : service.stats?.up);
  const book = $derived(connected ? ledger(ui.rules, dirStats, label) : null);
  const lineMbps = $derived(
    ui.dir === "down" ? service.snapshot?.config.line.down_mbps : service.snapshot?.config.line.up_mbps,
  );
  const lineBps = $derived(lineMbps === undefined ? null : lineMbps * 1e6);
  const loading = $derived(
    service.link.kind === "connecting" || (service.link.kind === "connected" && service.stats === null),
  );
  const sentence = $derived(book && service.stats ? glance(book, lineBps) : null);

  const notice = $derived(noticeOf(service.link, service.state, () => lineDialog.show()));
</script>

<div class="window">
  <TitleBar />
  {#if firstRun}
    <FirstRun onfinish={finishFirstRun} />
  {:else}
    <main>
      <Header glance={sentence} {notice} />
      {#if service.link.kind === "connecting"}
        <p class="visually-hidden" role="status">{t("link.connecting")}</p>
      {/if}
      {#if book && service.stats}
        <UsageStrip ledger={book} {lineBps} />
      {/if}
      {#if service.link.kind === "connecting" || connected}
        <SliceList ledger={book} {loading} canEdit={connected} />
      {/if}
    </main>
  {/if}
  <AddSlice />
  <LineDialog />
</div>

<style>
  .window {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  main {
    flex: 1;
    overflow-y: auto;
  }
</style>
