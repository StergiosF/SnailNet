<script lang="ts">
  // The SnailNet glyph (same drawing as src-tauri/icons/snail.svg). On the
  // dark side rail its body is drawn light (`light`) so it doesn't sink
  // into the green.
  let { size = 16, light = false }: { size?: number; light?: boolean } = $props();
  const body = $derived(light ? "#8fd3bf" : "#1F6F64");
</script>

<svg width={size} height={size} viewBox="0 0 512 512" aria-hidden="true">
  <path
    d="M64 404c0-18 14-30 34-30h300c34 0 58-26 62-62l4-34c2-16 26-14 26 2v52c0 54-42 98-96 98H98c-20 0-34-10-34-26z"
    fill={body}
  />
  <path
    d="M470 276 442 192M484 276l4-86"
    stroke={body}
    stroke-width="16"
    stroke-linecap="round"
    fill="none"
  />
  <circle cx="440" cy="182" r="17" fill={body} />
  <circle cx="488" cy="182" r="17" fill={body} />
  <circle cx="224" cy="250" r="150" fill="#2FA88E" />
  <path
    d="M224 250m-22 0a22 22 0 1 1 44 0a50 50 0 1 1-100 0a78 78 0 1 1 156 0a106 106 0 1 1-212 0"
    stroke="#0E3B35"
    stroke-width="20"
    stroke-linecap="round"
    fill="none"
  />
</svg>
