<script lang="ts">
  // A Windows-style InfoBar: severity icon, bold title, message, and an
  // optional action button. Used for the service and connection states of
  // PLAN.md §14.8.
  let {
    severity,
    title,
    message,
    action,
  }: {
    severity: "info" | "caution" | "critical";
    title: string;
    message: string;
    action?: { label: string; onclick: () => void };
  } = $props();

  const glyph = { info: "", caution: "", critical: "" };
</script>

<div class="infobar {severity}">
  <i class="icon" aria-hidden="true">{glyph[severity]}</i>
  <p role={severity === "info" ? "status" : "alert"}><strong>{title}</strong> {message}</p>
  {#if action}
    <button type="button" class="btn" onclick={action.onclick}>{action.label}</button>
  {/if}
</div>

<style>
  .infobar {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 14px 16px;
    border: 1px solid var(--stroke-card);
    border-radius: var(--radius-card);
    background: var(--surface-card);
  }
  .infobar.caution {
    background: var(--caution-tint);
  }
  .infobar.critical {
    background: var(--critical-tint);
  }
  .icon {
    font-size: 16px;
    margin-top: 2px;
  }
  .info .icon {
    color: var(--accent);
  }
  .caution .icon {
    color: var(--caution);
  }
  .critical .icon {
    color: var(--critical);
  }
  p {
    flex: 1;
    margin: 0;
    max-width: 75ch;
    color: var(--text-primary);
  }
  strong {
    font-weight: 600;
    margin-right: 8px;
  }
  .btn {
    flex: none;
    align-self: center;
    margin: -6px 0;
  }
</style>
