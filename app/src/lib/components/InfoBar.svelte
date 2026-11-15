<script lang="ts">
  // A Windows-style InfoBar: severity icon, bold title, message. Used for
  // the service and connection states of PLAN.md §14.8.
  let {
    severity,
    title,
    message,
  }: { severity: "info" | "caution" | "critical"; title: string; message: string } = $props();

  const glyph = { info: "", caution: "", critical: "" };
</script>

<div class="infobar {severity}" role={severity === "info" ? "status" : "alert"}>
  <i class="icon" aria-hidden="true">{glyph[severity]}</i>
  <p><strong>{title}</strong> {message}</p>
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
    margin: 0;
    max-width: 75ch;
    color: var(--text-primary);
  }
  strong {
    font-weight: 600;
    margin-right: 8px;
  }
</style>
