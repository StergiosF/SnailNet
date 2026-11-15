<script lang="ts">
  // Main window: title bar, header, the state that needs attention (if
  // any), the live list.
  import { onMount } from "svelte";
  import AppList from "./lib/components/AppList.svelte";
  import Header from "./lib/components/Header.svelte";
  import InfoBar from "./lib/components/InfoBar.svelte";
  import SplitBar from "./lib/components/SplitBar.svelte";
  import TitleBar from "./lib/components/TitleBar.svelte";
  import { stateKind } from "./lib/format";
  import { t } from "./lib/i18n";
  import { service } from "./lib/service.svelte";

  onMount(() => {
    service.start().catch((e: unknown) => {
      service.link = { kind: "lost", detail: String(e) };
    });
  });

  type Bar = { severity: "info" | "caution" | "critical"; title: string; message: string };

  const bar = $derived.by((): Bar | null => {
    const link = service.link;
    switch (link.kind) {
      case "connecting":
      case "connected":
        break;
      case "not_running":
        return { severity: "caution", title: t("link.not_running.title"), message: t("link.not_running.body") };
      case "rejected":
        return { severity: "critical", title: t("link.rejected.title"), message: t("link.rejected.body") };
      case "denied":
        return {
          severity: "critical",
          title: t("link.denied.title"),
          message: t("link.denied.body", { detail: link.detail }),
        };
      case "lost":
        return { severity: "caution", title: t("link.lost.title"), message: t("link.lost.body") };
    }
    const s = service.state;
    if (s === null) return null;
    switch (stateKind(s)) {
      case "unshaped":
        return { severity: "info", title: t("state.unshaped.title"), message: t("state.unshaped.body") };
      case "retrying":
        return {
          severity: "caution",
          title: t("state.retrying.title"),
          message: t("state.retrying.body", {
            reason: typeof s === "object" && "Retrying" in s ? s.Retrying.reason : "",
          }),
        };
      case "safe_mode":
        return {
          severity: "critical",
          title: t("state.safe_mode.title"),
          message: t("state.safe_mode.body", {
            failures: typeof s === "object" && "SafeMode" in s ? s.SafeMode.failures : 0,
          }),
        };
      default:
        return null;
    }
  });

  const loading = $derived(
    service.link.kind === "connecting" ||
      (service.link.kind === "connected" && service.stats === null),
  );
</script>

<div class="window">
  <TitleBar />
  <main>
    <Header />
    {#if bar}
      <div class="bar"><InfoBar {...bar} /></div>
    {/if}
    {#if service.link.kind === "connecting"}
      <p class="visually-hidden" role="status">{t("link.connecting")}</p>
    {/if}
    {#if service.link.kind === "connected" && service.snapshot}
      <SplitBar />
    {/if}
    <AppList {loading} />
  </main>
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
    /* The content layer: Mica stays visible under the title bar only, as
       in Windows Settings. */
    background: var(--surface-layer);
    border-top: 1px solid var(--stroke-card);
    border-top-left-radius: var(--radius-card);
  }
  .bar {
    padding: 0 var(--gutter) 16px;
  }
</style>
