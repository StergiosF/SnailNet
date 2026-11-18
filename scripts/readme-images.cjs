// Draws the README's pictures of SnailNet (docs/images/*.svg) from the
// app's own components and tokens (app/src/lib/components, app/src/app.css),
// with the brand icons from site/assets/icons embedded. Run it again when
// the app's look changes:   node scripts/readme-images.cjs
// Real screenshots can replace these files under the same names.
const fs = require("fs");
const path = require("path");
const root = path.resolve(__dirname, "..");
const out = path.join(root, "docs", "images");
fs.mkdirSync(out, { recursive: true });
const icon = (n) =>
  "data:image/svg+xml;base64," + fs.readFileSync(path.join(root, "site/assets/icons", n + ".svg")).toString("base64");
const T = {
  side: "#0a211d", ground: "#0e2b26", s1: "#143831", s2: "#1b443c",
  text: "#f3eee2", sub: "#b8cbc4", cream: "#efe8d6", onCream: "#0a211d", shell: "#2fa88e", onShell: "#062520",
  mint: "#8fd3bf", ink0: "#e8836a", ink1: "#eab04e", stone: "#a6b5ad", onInk: "#0f2420", tileSub: "#2f4a43",
};
const FONT = `font-family="Atkinson Hyperlegible Next, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"`;
const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;");
const text = (x, y, s, size, fill, weight = 400, extra = "") =>
  `<text x="${x}" y="${y}" font-size="${size}" font-weight="${weight}" fill="${fill}" ${extra}>${esc(s)}</text>`;
const END = `text-anchor="end"`;
const snail = (x, y, size, body = T.mint) => `<g transform="translate(${x} ${y}) scale(${size / 512})">
  <path d="M64 404c0-18 14-30 34-30h300c34 0 58-26 62-62l4-34c2-16 26-14 26 2v52c0 54-42 98-96 98H98c-20 0-34-10-34-26z" fill="${body}"/>
  <path d="M470 276 442 192M484 276l4-86" stroke="${body}" stroke-width="16" stroke-linecap="round" fill="none"/>
  <circle cx="440" cy="182" r="17" fill="${body}"/><circle cx="488" cy="182" r="17" fill="${body}"/>
  <circle cx="224" cy="250" r="150" fill="#2FA88E"/>
  <path d="M224 250m-22 0a22 22 0 1 1 44 0a50 50 0 1 1-100 0a78 78 0 1 1 156 0a106 106 0 1 1-212 0" stroke="#0E3B35" stroke-width="20" stroke-linecap="round" fill="none"/></g>`;
const appIcon = (x, y, name, s = 32) =>
  `<rect x="${x}" y="${y}" width="${s}" height="${s}" rx="8" fill="${T.text}"/><image href="${icon(name)}" x="${x + s * 0.2}" y="${y + s * 0.2}" width="${s * 0.6}" height="${s * 0.6}"/>`;
const glyphs = {
  balanced: `<path d="M4 9h16M4 15h16" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" fill="none"/>`,
  streaming: `<path d="M8 5.5v13l10.5-6.5z" fill="currentColor"/>`,
  gaming: `<path d="M7 8h10a4 4 0 0 1 3.9 4.9l-.8 3.3a2.3 2.3 0 0 1-4 .9L14.5 15h-5l-1.6 2.1a2.3 2.3 0 0 1-4-.9l-.8-3.3A4 4 0 0 1 7 8z" fill="currentColor"/>`,
  quiet: `<path d="M12 4v11m0 0-4.5-4.5M12 15l4.5-4.5M5 20h14" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>`,
  now: `<path d="M4 11 12 4l8 7v8.5a.5.5 0 0 1-.5.5H15v-6H9v6H4.5a.5.5 0 0 1-.5-.5z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/>`,
  split: `<path d="M4 6h9m4 0h3M4 12h3m4 0h9M4 18h11m4 0h1M15 4v4M9 10v4M17 16v4" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>`,
  gear: `<g fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="12" cy="12" r="3.2"/><path d="M12 3v2.5M12 18.5V21M3 12h2.5M18.5 12H21M5.6 5.6l1.8 1.8M16.6 16.6l1.8 1.8M5.6 18.4l1.8-1.8M16.6 7.4l1.8-1.8"/></g>`,
  boost: `<path d="M13.5 3 5 13.5h6L10 21l9-11h-6z" fill="currentColor"/>`,
  pause: `<path d="M8 5v14M16 5v14" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"/>`,
};
const glyph = (x, y, name, size, color) =>
  `<g transform="translate(${x} ${y}) scale(${size / 24})" color="${color}">${glyphs[name]}</g>`;

// ---------- 1. The main window, Now page ----------
{
  const W = 1280, H = 760;
  const wx = 40, wy = 40, ww = W - 80, wh = H - 80, sideW = 212;
  const mx = wx + sideW + 32, mw = ww - sideW - 64;
  let s = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" ${FONT}>
<title>SnailNet's main window: the Now page in Balanced mode, with YouTube, a Steam download and Discord sharing the line</title>
<defs>
<filter id="sh" x="-20%" y="-20%" width="140%" height="160%"><feDropShadow dx="0" dy="12" stdDeviation="14" flood-color="#000" flood-opacity=".45"/></filter>
<radialGradient id="glowA" cx="20%" cy="0%" r="90%"><stop offset="0" stop-color="${T.shell}" stop-opacity=".34"/><stop offset="1" stop-color="${T.shell}" stop-opacity="0"/></radialGradient>
<radialGradient id="glowB" cx="100%" cy="100%" r="70%"><stop offset="0" stop-color="${T.ink1}" stop-opacity=".2"/><stop offset="1" stop-color="${T.ink1}" stop-opacity="0"/></radialGradient>
<radialGradient id="room" cx="62%" cy="-12%" r="70%"><stop offset="0" stop-color="${T.shell}" stop-opacity=".17"/><stop offset="1" stop-color="${T.shell}" stop-opacity="0"/></radialGradient>
<linearGradient id="cream" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#f6f0df"/><stop offset=".6" stop-color="${T.cream}"/><stop offset="1" stop-color="#e6dcc4"/></linearGradient>
<clipPath id="win"><rect x="${wx}" y="${wy}" width="${ww}" height="${wh}" rx="12"/></clipPath>
</defs>
<rect width="${W}" height="${H}" rx="24" fill="#0b2420"/>
<rect width="${W}" height="${H}" rx="24" fill="url(#glowA)"/><rect width="${W}" height="${H}" rx="24" fill="url(#glowB)"/>
<g filter="url(#sh)"><rect x="${wx}" y="${wy}" width="${ww}" height="${wh}" rx="12" fill="${T.ground}"/></g>
<g clip-path="url(#win)">
<rect x="${wx}" y="${wy}" width="${ww}" height="${wh}" fill="url(#room)"/>
<rect x="${wx}" y="${wy}" width="${sideW}" height="${wh}" fill="${T.side}"/>
${snail(wx + 18, wy + 18, 34)}${text(wx + 62, wy + 43, "SnailNet", 19, T.text, 800)}
<rect x="${wx + 12}" y="${wy + 76}" width="${sideW - 24}" height="44" rx="10" fill="${T.shell}" fill-opacity=".16"/>
${glyph(wx + 26, wy + 88, "now", 20, T.shell)}${text(wx + 58, wy + 104, "Now", 16, T.text, 700)}
${glyph(wx + 26, wy + 132, "split", 20, T.sub)}${text(wx + 58, wy + 148, "Split", 16, T.sub)}
${glyph(wx + 26, wy + 176, "gear", 20, T.sub)}${text(wx + 58, wy + 192, "Settings", 16, T.sub)}
<rect x="${wx + 12}" y="${wy + wh - 166}" width="${sideW - 24}" height="40" rx="10" fill="${T.s2}"/>
${glyph(wx + 26, wy + wh - 156, "pause", 18, T.text)}${text(wx + 54, wy + wh - 140, "Pause", 15, T.text, 700)}
${text(wx + 24, wy + wh - 96, "Your line", 13, T.sub, 600)}
${text(wx + 24, wy + wh - 68, "Download", 13, T.sub)}${text(wx + sideW - 24, wy + wh - 68, "25 Mbps", 16, T.text, 700, END)}
${text(wx + 24, wy + wh - 42, "Upload", 13, T.sub)}${text(wx + sideW - 24, wy + wh - 42, "5 Mbps", 16, T.text, 700, END)}
<g fill="none" stroke="${T.shell}" stroke-linecap="round" transform="translate(${wx + ww - 330} ${wy - 70}) scale(.82)"><circle cx="224" cy="250" r="150" stroke-width="2" stroke-opacity=".1"/><path d="M224 250m-22 0a22 22 0 1 1 44 0a50 50 0 1 1-100 0a78 78 0 1 1 156 0a106 106 0 1 1-212 0" stroke-width="10" stroke-opacity=".09"/></g>
<g stroke="${T.sub}" stroke-width="1.2" fill="none"><path d="M${wx + ww - 120} ${wy + 16}h10"/><rect x="${wx + ww - 74}" y="${wy + 11}" width="10" height="10" rx="1"/><path d="M${wx + ww - 28} ${wy + 11}l10 10M${wx + ww - 18} ${wy + 11}l-10 10"/></g>
${text(mx, wy + 86, "What matters right now?", 32, T.text, 800, `letter-spacing="-0.8"`)}`;
  const modes = [
    ["balanced", "Balanced", ["Everyone gets a fair", "share."], true],
    ["streaming", "Streaming", ["Video and calls first,", "downloads in the spare room."]],
    ["gaming", "Gaming", ["Voice chat first. Video and", "downloads step back."]],
    ["quiet", "Quiet downloads", ["Downloads only use what", "nobody else wants."]],
  ];
  const tw = (mw - 36) / 4, ty = wy + 112, th = 136;
  modes.forEach(([id, name, body, on], i) => {
    const tx = mx + i * (tw + 12);
    s += `<rect x="${tx}" y="${ty}" width="${tw}" height="${th}" rx="16" fill="${on ? "url(#cream)" : T.s1}"/>`;
    s += `<rect x="${tx + 16}" y="${ty + 16}" width="36" height="36" rx="10" fill="${on ? T.shell : T.s2}"/>`;
    s += glyph(tx + 24, ty + 24, id, 20, on ? T.onShell : T.text);
    s += text(tx + 16, ty + 80, name, 16, on ? T.onCream : T.text, 700);
    body.forEach((l, k) => (s += text(tx + 16, ty + 102 + k * 18, l, 13, on ? T.tileSub : T.sub)));
    if (on)
      s += `<rect x="${tx + tw - 48}" y="${ty + 14}" width="34" height="22" rx="6" fill="${T.onCream}"/>` +
        text(tx + tw - 31, ty + 30, "On", 12, T.cream, 800, `text-anchor="middle"`);
  });
  const cy = ty + th + 22, ch = wh - (cy - wy) - 24;
  const whoW = Math.round((mw - 14) * 0.565), spX = mx + whoW + 14, spW = mw - whoW - 14;
  s += `<rect x="${mx}" y="${cy}" width="${whoW}" height="${ch}" rx="16" fill="${T.s1}"/>` + text(mx + 18, cy + 34, "Using your internet", 16, T.text, 700);
  const who = [
    ["youtube", "YouTube", "gets all it asks for", "12.4"],
    ["steam", "Steam", "0.7 Mbps of it is spare room", "10.2"],
    ["discord", "Discord", "in Everything else", "0.3"],
  ];
  who.forEach(([ic, name, sub, mb], i) => {
    const ry = cy + 56 + i * 62;
    s += appIcon(mx + 18, ry, ic, 36);
    s += text(mx + 68, ry + 15, name, 15, T.text, 700) + text(mx + 68, ry + 34, sub, 13, T.sub);
    s += text(mx + whoW - 140, ry + 24, mb, 15, T.text, 700, END) + text(mx + whoW - 136, ry + 24, "Mbps", 12, T.sub);
    s += `<rect x="${mx + whoW - 92}" y="${ry + 3}" width="74" height="30" rx="8" fill="${T.s2}"/>` +
      glyph(mx + whoW - 82, ry + 11, "boost", 14, T.text) + text(mx + whoW - 62, ry + 23, "Boost", 13, T.text, 700);
  });
  s += `<rect x="${spX}" y="${cy}" width="${spW}" height="${ch}" rx="16" fill="${T.s1}"/>` + text(spX + 18, cy + 34, "The split", 16, T.text, 700);
  const bw = spW - 36, bx = spX + 18, by = cy + 52;
  const parts = [[0.55, T.ink0, "55%"], [0.4, T.ink1, "40%"], [0.05, T.stone, ""]];
  let px = bx;
  s += `<clipPath id="bar"><rect x="${bx}" y="${by}" width="${bw}" height="30" rx="8"/></clipPath><g clip-path="url(#bar)">`;
  parts.forEach(([f, c, l]) => {
    const w = f * (bw - 4);
    s += `<rect x="${px}" y="${by}" width="${w}" height="30" fill="${c}"/>` + (l ? text(px + 9, by + 20, l, 12, T.onInk, 800) : "");
    px += w + 2;
  });
  s += `</g>`;
  [["YouTube", "12.4 / 13.1 Mbps", "55%", T.ink0], ["Steam", "10.2 / 9.5 Mbps", "40%", T.ink1], ["Everything else", "0.6 / 1.2 Mbps", "5%", T.stone]].forEach(([n, u, p, c], i) => {
    const ry = by + 58 + i * 30;
    s += `<rect x="${bx}" y="${ry - 10}" width="10" height="10" rx="3" fill="${c}"/>` + text(bx + 20, ry, n, 14, T.text) +
      text(bx + bw - 44, ry, u, 13, T.sub, 400, END) + text(bx + bw, ry, p, 14, T.text, 700, END);
  });
  s += `<rect x="${bx}" y="${cy + ch - 56}" width="104" height="36" rx="10" fill="${T.s2}"/>` +
    text(bx + 16, cy + ch - 33, "Fine-tune", 14, T.text, 700) + text(bx + 124, cy + ch - 33, "Reset split", 14, T.sub, 700);
  s += `</g></svg>\n`;
  fs.writeFileSync(path.join(out, "now-page.svg"), s);
}

// ---------- 2. A slice is a guarantee, never a cage ----------
{
  const W = 1280, H = 420;
  let s = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" ${FONT}>
<title>On a 20 Mbps line, Steam keeps 40% and YouTube 60% while both are busy; when the video pauses, Steam gets the whole line</title>
<defs><pattern id="lent" width="14" height="14" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><rect width="14" height="14" fill="${T.ink1}"/><rect width="7" height="14" fill="#d99a3a"/></pattern>
<radialGradient id="g1" cx="90%" cy="0%" r="80%"><stop offset="0" stop-color="${T.ink1}" stop-opacity=".12"/><stop offset="1" stop-color="${T.ink1}" stop-opacity="0"/></radialGradient>
<radialGradient id="g2" cx="0%" cy="100%" r="80%"><stop offset="0" stop-color="${T.ink0}" stop-opacity=".1"/><stop offset="1" stop-color="${T.ink0}" stop-opacity="0"/></radialGradient></defs>
<rect width="${W}" height="${H}" rx="24" fill="${T.ground}"/><rect width="${W}" height="${H}" rx="24" fill="url(#g1)"/><rect width="${W}" height="${H}" rx="24" fill="url(#g2)"/>`;
  const panel = (x, title, sub, segs, rows) => {
    const pw = 568, bx = x + 28, bw = pw - 56;
    let p = `<rect x="${x}" y="40" width="${pw}" height="${H - 80}" rx="16" fill="${T.s1}"/>`;
    p += text(x + 28, 86, title, 22, T.text, 800) + text(x + 28, 114, sub, 15, T.sub);
    p += `<clipPath id="c${x}"><rect x="${bx}" y="142" width="${bw}" height="64" rx="14"/></clipPath><g clip-path="url(#c${x})">`;
    let px = bx;
    segs.forEach(([f, fill, label]) => {
      const w = f * (bw - 4);
      p += `<rect x="${px}" y="142" width="${w}" height="64" fill="${fill}"/>` + (label ? text(px + 16, 181, label, 17, T.onInk, 800) : "");
      px += w + 4;
    });
    p += `</g>`;
    rows.forEach(([ic, name, line], i) => {
      const ry = 238 + i * 64;
      p += appIcon(bx, ry, ic, 40) + text(bx + 56, ry + 17, name, 17, T.text, 700) + text(bx + 56, ry + 37, line, 15, T.sub);
    });
    return p;
  };
  s += panel(40, "Both busy", "Each keeps its slice of the 20 Mbps line.",
    [[0.4, T.ink1, "Steam 40%"], [0.6, T.ink0, "YouTube 60%"]],
    [["steam", "Steam", "8.0 Mbps · using its whole slice, wants more"], ["youtube", "YouTube", "12.0 Mbps · using its whole slice"]]);
  s += panel(672, "Video paused", "What YouTube doesn't use, Steam gets.",
    [[0.4, T.ink1, "Steam 40%"], [0.6, "url(#lent)", "YouTube 60%"]],
    [["steam", "Steam", "20.0 Mbps · 12.0 Mbps of it is spare room"], ["youtube", "YouTube", "quiet right now · lending 12.0 Mbps it isn't using"]]);
  s += `</svg>\n`;
  fs.writeFileSync(path.join(out, "split-guarantee.svg"), s);
}
console.log("wrote", fs.readdirSync(out).join(", "));
