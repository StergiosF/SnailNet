// The states that need attention (PLAN.md §14.8), as the header says them:
// the link to the service first, then the service's own state. Null while
// all is well (or still connecting, which the list shows on its own).

import { stateKind } from "./format";
import { t } from "./i18n";
import { timeLeft } from "./modes";
import type { Link, ServiceState } from "./types";
import type { Notice } from "./ui.svelte";

/** What a notice's button does. */
export interface NoticeActions {
  setSpeed: () => void;
  resume: () => void;
}

export function noticeOf(
  link: Link,
  s: ServiceState | null,
  actions: NoticeActions,
  now: number,
): Notice | null {
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
  if (s === null) return null;
  switch (stateKind(s)) {
    case "unshaped":
      return {
        severity: "info",
        title: t("state.unshaped.title"),
        message: t("state.unshaped.body"),
        action: { label: t("state.unshaped.action"), onclick: actions.setSpeed },
      };
    case "paused": {
      const until = typeof s === "object" && "Paused" in s ? s.Paused.until : null;
      return {
        severity: "info",
        title: t("state.paused.title"),
        message: t("state.paused.body", { left: timeLeft(until ?? undefined, now) }),
        action: { label: t("pause.resume"), onclick: actions.resume },
      };
    }
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
}
