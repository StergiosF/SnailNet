<script lang="ts">
  // The SnailNet glyph (same drawing as src-tauri/icons/snail.svg). Brand
  // rule: title bar, tray and empty states only.
  let { size = 16 }: { size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 512 512" aria-hidden="true">
  <path
    d="M64 404c0-18 14-30 34-30h300c34 0 58-26 62-62l4-34c2-16 26-14 26 2v52c0 54-42 98-96 98H98c-20 0-34-10-34-26z"
    fill="#1F6F64"
  />
  <path
    d="M430 284l-22-96M452 280l12-92"
    stroke="#1F6F64"
    stroke-width="16"
    stroke-linecap="round"
    fill="none"
  />
  <circle cx="405" cy="178" r="17" fill="#1F6F64" />
  <circle cx="466" cy="178" r="17" fill="#1F6F64" />
  <circle cx="224" cy="250" r="150" fill="#2FA88E" />
  <path
    d="M224 250m-22 0a22 22 0 1 1 44 0a50 50 0 1 1-100 0a78 78 0 1 1 156 0a106 106 0 1 1-212 0"
    stroke="#0E3B35"
    stroke-width="20"
    stroke-linecap="round"
    fill="none"
  />
</svg>
