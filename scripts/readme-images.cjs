// Draws the README's pictures of SnailNet (docs/images/*.svg) in the app's
// own look, Obsidian Jade "Blend" (app/src/app.css, DESIGN.md), the same
// Now page as the website's hero (site/index.html). Brand marks are Simple
// Icons in white on the brand's tile, as AppIcon.svelte draws them, so run
// it after the app's packages are installed:
//   node scripts/readme-images.cjs
// Real screenshots can replace these files under the same names.
const fs = require("fs");
const path = require("path");
const root = path.resolve(__dirname, "..");
const si = require(path.join(root, "app", "node_modules", "simple-icons"));
const out = path.join(root, "docs", "images");
fs.mkdirSync(out, { recursive: true });

// Tokens, from app/src/app.css.
const T = {
  side: "#0a1311", ground: "#0f1b18", s1: "#15221e", s2: "#1c2b27", s3: "#253631",
  text: "#efece3", sub: "#abbcb5", ter: "#80948b", hair: "#ecebe2", cream: "#f0ece1", onCream: "#121716",
  shell: "#2db894", bright: "#5ee0bd", deep: "#0f6b54",
  ink0: "#e8836a", ink1: "#eab04e", ink2: "#9cc7e8", ink3: "#d3a0d8", ink4: "#c9c46e", stone0: "#a3aea9", stone1: "#8d9893", onInk: "#10201b",
};
const HAIR = `stroke="${T.hair}" stroke-opacity=".075"`;
const FONT = `font-family="Atkinson Hyperlegible Next, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"`;
const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;");
const text = (x, y, s, size, fill, weight = 400, extra = "") =>
  `<text x="${x}" y="${y}" font-size="${size}" font-weight="${weight}" fill="${fill}" ${extra}>${esc(s)}</text>`;
const END = `text-anchor="end"`;
const snail = (x, y, size) => `<g transform="translate(${x} ${y}) scale(${size / 512})">
  <path d="M64 404c0-18 14-30 34-30h300c34 0 58-26 62-62l4-34c2-16 26-14 26 2v52c0 54-42 98-96 98H98c-20 0-34-10-34-26z" fill="#8fd3bf"/>
  <path d="M470 276 442 192M484 276l4-86" stroke="#8fd3bf" stroke-width="16" stroke-linecap="round" fill="none"/>
  <circle cx="440" cy="182" r="17" fill="#8fd3bf"/><circle cx="488" cy="182" r="17" fill="#8fd3bf"/>
  <circle cx="224" cy="250" r="150" fill="#2FA88E"/>
  <path d="M224 250m-22 0a22 22 0 1 1 44 0a50 50 0 1 1-100 0a78 78 0 1 1 156 0a106 106 0 1 1-212 0" stroke="#0E3B35" stroke-width="20" stroke-linecap="round" fill="none"/></g>`;

// The app's glyphs (Glyph.svelte): a 24 box, 1.75 stroke, round ends.
const DOT = `fill="currentColor" stroke="none"`;
const G = {
  now: `<path d="M10 12a2 2 0 1 1 4 0a4 4 0 1 1-8 0a6 6 0 1 1 12 0a8 8 0 0 1-8 8"/>`,
  split: `<rect x="3" y="7" width="18" height="10" rx="3"/><path d="M10 7v10M15 7v10"/>`,
  settings: `<path d="M4 8h5M13 8h7M4 16h9M17 16h3"/><circle cx="11" cy="8" r="2"/><circle cx="15" cy="16" r="2"/>`,
  pause: `<path d="M9 6v12M15 6v12"/>`,
  boost: `<path d="M7 12.5 12 7.5l5 5M7 17.5l5-5 5 5"/>`,
  down: `<path d="M12 4v11"/><path d="m7.5 10.5 4.5 4.5 4.5-4.5"/><path d="M5 20h14"/>`,
  up: `<path d="M12 20V9"/><path d="m7.5 13.5 4.5-4.5 4.5 4.5"/><path d="M5 4h14"/>`,
  chevron: `<path d="m9 6 6 6-6 6"/>`,
  more: `<circle ${DOT} cx="6" cy="12" r="1.5"/><circle ${DOT} cx="12" cy="12" r="1.5"/><circle ${DOT} cx="18" cy="12" r="1.5"/>`,
  balanced: `<circle cx="12" cy="12" r="8.5"/><path d="M12 12V3.5M12 12l7.4 4.25M12 12l-7.4 4.25"/>`,
  streaming: `<rect x="3" y="5" width="18" height="13" rx="3"/><path d="M10.5 9.25v5.5l4.25-2.75z"/><path d="M9 21h6"/>`,
  gaming: `<path d="M7.5 8h9a4 4 0 0 1 3.9 3.1l.8 3.7a2.2 2.2 0 0 1-3.8 1.9L15.6 15H8.4l-1.8 1.7a2.2 2.2 0 0 1-3.8-1.9l.8-3.7A4 4 0 0 1 7.5 8z"/><path d="M8.5 10.5v3M7 12h3"/><circle ${DOT} cx="15.5" cy="11" r="1"/><circle ${DOT} cx="17.3" cy="13" r="1"/>`,
  quiet: `<path d="M12 4v9" stroke-dasharray="1.5 2.6"/><path d="M8.5 10.5 12 14l3.5-3.5"/><path d="M4.5 15.5v2a2.5 2.5 0 0 0 2.5 2.5h10a2.5 2.5 0 0 0 2.5-2.5v-2"/>`,
};
const glyph = (x, y, name, size, color) =>
  `<g transform="translate(${x} ${y}) scale(${size / 24})" color="${color}" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">${G[name]}</g>`;

// Brand tiles: the mark in white on the brand's colour.
const BRANDS = { steam: [si.siSteam, "#1b2838"], youtube: [si.siYoutube, "#ff0000"], twitch: [si.siTwitch, "#9146ff"], discord: [si.siDiscord, "#5865f2"] };
const tile = (x, y, name, s) => {
  const [icon, bg] = BRANDS[name];
  const m = s * 0.58, o = (s - m) / 2;
  return `<rect x="${x}" y="${y}" width="${s}" height="${s}" rx="${s * 0.27}" fill="${bg}"/>` +
    `<g transform="translate(${x + o} ${y + o}) scale(${m / 24})"><path d="${icon.path}" fill="#fff"/></g>`;
};
const panel = (x, y, w, h) => `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="16" fill="${T.s1}" filter="url(#lift)"/>`;
const DEFS = `<filter id="lift" x="-10%" y="-10%" width="120%" height="140%"><feDropShadow dx="0" dy="14" stdDeviation="12" flood-color="#000" flood-opacity=".45"/></filter>
<radialGradient id="lightA" cx="62%" cy="-14%" r="75%"><stop offset="0" stop-color="${T.shell}" stop-opacity=".11"/><stop offset="1" stop-color="${T.shell}" stop-opacity="0"/></radialGradient>
<radialGradient id="lightB" cx="-6%" cy="110%" r="60%"><stop offset="0" stop-color="#1f6f64" stop-opacity=".09"/><stop offset="1" stop-color="#1f6f64" stop-opacity="0"/></radialGradient>`;

// The last 10 minutes: one stacked band per app, a point every 5 s (the
// same made-up minutes as the website's hero).
function timeline(W) {
  const N = 121, DT = 5;
  const T5 = Array.from({ length: N }, (_, i) => -600 + i * DT);
  const wob = (i, s) => Math.sin(i * 0.37 + s) * 0.5 + Math.sin(i * 0.11 + s * 2.1) * 0.35 + Math.sin(i * 1.3 + s * 3.7) * 0.15;
  const ramp = (t, t0, d = 25) => Math.max(0, Math.min(1, (t - t0) / d));
  const series = [
    [T.stone0, (t, i) => ramp(t, -540) * (t < -240 ? 17.2 : 21.4 - 8.4 - ramp(t, -150, 20) * 3.1) + ramp(t, -540) * wob(i, 1) * 0.9],
    [T.ink2, (t, i) => (t < -240 ? 6.4 - 2.6 * ramp(t, -540) + wob(i, 2) * 1.1 : 8.2 + wob(i, 2) * 0.6)],
    [T.ink3, (t, i) => ramp(t, -150, 20) * (3.1 + wob(i, 8) * 0.3)],
    [T.ink0, (t, i) => 0.6 + wob(i, 5) * 0.08],
  ].map(([c, f]) => ({ c, pts: T5.map((t, i) => Math.max(0, f(t, i))) }));
  const H = 128, top = 18, line = 25, max = line * 1.06;
  const x = (i) => (i / (N - 1)) * W, y = (v) => top + H - (v / max) * H, r = (n) => n.toFixed(1);
  const cum = new Array(N).fill(0);
  let s = `<path d="M0 ${top + H}H${W}M0 ${r(y(line / 2))}H${W}" ${HAIR}/>`;
  series.forEach(({ c, pts }) => {
    const lo = cum.slice(), hi = pts.map((v, i) => (cum[i] += v));
    const upper = hi.map((v, i) => `${r(x(i))},${r(y(v))}`);
    const lower = lo.map((v, i) => `${r(x(i))},${r(y(v))}`).reverse();
    s += `<path d="M${upper.join("L")}L${lower.join("L")}Z" fill="${c}" fill-opacity=".5"/><path d="M${upper.join("L")}" fill="none" stroke="${c}" stroke-width="1.5"/>`;
  });
  s += `<path d="M0 ${r(y(line))}H${W}" stroke="${T.ter}" stroke-dasharray="3 4"/>` + text(W, r(y(line) - 5), "25 Mbps · your speed", 11.5, T.ter, 400, END);
  [[-540, "Steam started"], [-240, "Switched to Streaming"]].forEach(([t, label]) => {
    const ex = x((t + 600) / DT), tw = label.length * 6.3 + 12, rx = ex > W / 2 ? ex - tw - 4 : ex + 4;
    s += `<path d="M${r(ex)} ${top - 4}V${top + H}" stroke="${T.cream}" stroke-opacity=".38" stroke-dasharray="2 3"/><circle cx="${r(ex)}" cy="${top - 4}" r="2.5" fill="${T.cream}"/>` +
      `<rect x="${r(rx)}" y="${top - 13}" width="${r(tw)}" height="17" rx="5" fill="${T.s1}"/>` + text(r(rx + 6), top, label, 11.5, T.sub);
  });
  s += text(0, top + H + 15, "10 min ago", 11.5, T.ter) + text(W / 2, top + H + 15, "5 min ago", 11.5, T.ter, 400, `text-anchor="middle"`) + text(W, top + H + 15, "now", 11.5, T.ter, 400, END);
  return { svg: s, h: top + H + 20 };
}

// ---------- 1. The main window, Now page ----------
{
  const W = 1280, wx = 40, wy = 40, ww = W - 80, sideW = 212;
  const mx = wx + sideW + 32, mw = ww - sideW - 64, sideCol = 316, mainW = mw - sideCol - 24;
  const tl = timeline(mainW - 36);
  const headY = wy + 44 + 14;
  const colY = headY + 104;
  const tlH = 48 + tl.h + 12;
  const whoY = colY + tlH + 18, whoH = 48 + 4 * 58 + 8;
  const wh = whoY + whoH + 26 - wy, H = wh + 80;
  let s = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" ${FONT}>
<title>SnailNet's main window: the Now page in Streaming mode, where a Steam download uses only spare speed while YouTube, Twitch and a Discord call get all they need</title>
<defs>${DEFS}
<clipPath id="win"><rect x="${wx}" y="${wy}" width="${ww}" height="${wh}" rx="12"/></clipPath>
<clipPath id="bar"><rect x="${wx + sideW + 32 + mainW + 24 + 18}" y="${colY + 50}" width="${sideCol - 36}" height="30" rx="9"/></clipPath>
</defs>
<rect width="${W}" height="${H}" rx="24" fill="${T.side}"/><rect width="${W}" height="${H}" rx="24" fill="url(#lightA)"/>
<rect x="${wx}" y="${wy}" width="${ww}" height="${wh}" rx="12" fill="${T.ground}" filter="url(#lift)"/>
<g clip-path="url(#win)">
<rect x="${wx}" y="${wy}" width="${ww}" height="${wh}" fill="url(#lightA)"/><rect x="${wx}" y="${wy}" width="${ww}" height="${wh}" fill="url(#lightB)"/>
<rect x="${wx}" y="${wy}" width="${sideW}" height="${wh}" fill="${T.side}"/>
${snail(wx + 20, wy + 15, 28)}${text(wx + 58, wy + 35, "SnailNet", 16, T.text, 800)}
<rect x="${wx + 12}" y="${wy + 66}" width="${sideW - 24}" height="38" rx="10" fill="${T.s2}"/>
${glyph(wx + 24, wy + 76, "now", 18, T.bright)}${text(wx + 53, wy + 90, "Now", 15, T.text, 700)}
${glyph(wx + 24, wy + 116, "split", 18, T.sub)}${text(wx + 53, wy + 130, "Split", 15, T.sub)}
${glyph(wx + 24, wy + 156, "settings", 18, T.sub)}${text(wx + 53, wy + 170, "Settings", 15, T.sub)}
${glyph(wx + 24, wy + wh - 122, "pause", 18, T.sub)}${text(wx + 52, wy + wh - 108, "Pause SnailNet", 14.5, T.sub)}
<path d="M${wx + 12} ${wy + wh - 78}H${wx + sideW - 12}" ${HAIR}/>
${text(wx + 24, wy + wh - 54, "Your internet speed", 13, T.sub)}
${glyph(wx + 24, wy + wh - 39, "down", 14, T.text)}${text(wx + 41, wy + wh - 27, "25", 13, T.text, 700)}
${glyph(wx + 64, wy + wh - 39, "up", 14, T.text)}${text(wx + 81, wy + wh - 27, "5", 13, T.text, 700)}${text(wx + 92, wy + wh - 27, "Mbps", 12, T.ter)}
${text(wx + sideW - 24, wy + wh - 27, "Change", 13, T.sub, 400, `${END} text-decoration="underline"`)}`;
  // The top strip: the mode switch and the caption buttons.
  const modes = [["balanced", "Balanced", 104], ["streaming", "Streaming", 112], ["gaming", "Gaming", 92], ["quiet", "Quiet downloads", 148]];
  const trackW = modes.reduce((a, m) => a + m[2], 0) + 2 * 3 + 6;
  s += `<rect x="${mx}" y="${wy + 6}" width="${trackW}" height="34" rx="11" fill="${T.side}" stroke="${T.hair}" stroke-opacity=".075"/>`;
  let sx = mx + 3;
  modes.forEach(([id, name, w]) => {
    const on = id === "streaming";
    if (on) s += `<rect x="${sx}" y="${wy + 9}" width="${w}" height="28" rx="8" fill="${T.cream}"/>`;
    s += glyph(sx + 11, wy + 15, id, 16, on ? T.deep : T.sub) + text(sx + 34, wy + 28, name, 13.5, on ? T.onCream : T.sub, on ? 700 : 400);
    sx += w + 2;
  });
  s += `<g stroke="${T.sub}" stroke-width="1.1" fill="none"><path d="M${wx + ww - 120} ${wy + 22}h8"/><rect x="${wx + ww - 73}" y="${wy + 18}" width="7.6" height="7.6" rx="1"/><path d="M${wx + ww - 27} ${wy + 18}l7 7M${wx + ww - 20} ${wy + 18}l-7 7"/></g>`;
  // The sentence over the shell spiral.
  s += `<g fill="none" stroke="${T.shell}" stroke-linecap="round" transform="translate(${mx + mainW - 260} ${headY - 64}) scale(${290 / 512})"><circle cx="224" cy="250" r="150" stroke-width="3.5" stroke-opacity=".09"/><path d="M224 250m-22 0a22 22 0 1 1 44 0a50 50 0 1 1-100 0a78 78 0 1 1 156 0a106 106 0 1 1-212 0" stroke-width="16" stroke-opacity=".08"/></g>`;
  s += text(mx, headY + 36, "Steam is using the most right now.", 32, T.text, 800, `letter-spacing="-0.8"`);
  s += text(mx, headY + 68, "It's using speed nobody else needs. YouTube, Twitch and Discord still get all they need.", 17, T.sub);
  // Last 10 minutes.
  s += panel(mx, colY, mainW, tlH) + text(mx + 18, colY + 34, "Last 10 minutes", 16, T.text, 700);
  const dirX = mx + mainW - 18 - 196;
  s += text(dirX - 12, colY + 34, "of 25 Mbps now", 13.5, T.sub, 400, END) + text(dirX - 112, colY + 34, "22.4", 13.5, T.text, 700, END);
  s += `<rect x="${dirX}" y="${colY + 16}" width="196" height="30" rx="9" fill="${T.side}" stroke="${T.hair}" stroke-opacity=".075"/><rect x="${dirX + 2}" y="${colY + 18}" width="100" height="26" rx="7" fill="${T.s3}"/>`;
  s += glyph(dirX + 11, colY + 24, "down", 14, T.text) + text(dirX + 30, colY + 36, "Download", 13, T.text, 700) + glyph(dirX + 113, colY + 24, "up", 14, T.sub) + text(dirX + 132, colY + 36, "Upload", 13, T.sub);
  s += `<g transform="translate(${mx + 18} ${colY + 50})">${tl.svg}</g>`;
  // Using your internet.
  s += panel(mx, whoY, mainW, whoH) + text(mx + 18, whoY + 34, "Using your internet", 16, T.text, 700) + text(mx + mainW - 18, whoY + 34, "busiest first", 13.5, T.sub, 400, END);
  const rows = [
    ["steam", "Steam", "only uses spare speed", "10.5", 42, T.stone0, T.sub],
    ["youtube", "YouTube", "getting all it needs", "8.2", 32.8, T.ink2, T.sub],
    ["twitch", "Twitch", "getting all it needs", "3.1", 12.4, T.ink3, T.sub],
    ["discord", "Discord", "Goes first · in a call", "0.5", 2, T.ink0, T.bright],
  ];
  const nameW = mainW - 18 - 30 - 12 - 12 - 70 - 12 - 82 - 32 - 18;
  rows.forEach(([ic, name, sub, mb, part, ink, subColor], i) => {
    const ry = whoY + 48 + i * 58, cx = mx + 18;
    if (i) s += `<path d="M${cx + 42} ${ry}H${mx + mainW - 18}" ${HAIR}/>`;
    s += tile(cx, ry + 14, ic, 30);
    s += text(cx + 42, ry + 22, name, 15, T.text, 700) + text(cx + 42, ry + 39, sub, 13, subColor);
    s += `<rect x="${cx + 42}" y="${ry + 46}" width="${nameW}" height="3" rx="1.5" fill="${T.hair}" fill-opacity=".075"/><rect x="${cx + 42}" y="${ry + 46}" width="${(nameW * part) / 100}" height="3" rx="1.5" fill="${ink}"/>`;
    const right = mx + mainW - 18;
    s += glyph(right - 26, ry + 20, "more", 18, T.ter);
    s += glyph(right - 106, ry + 21, "boost", 16, T.sub) + text(right - 86, ry + 34, "Boost", 13.5, T.sub);
    s += text(right - 150, ry + 35, mb, 16, T.text, 700, END) + text(right - 146, ry + 35, "Mbps", 12, T.ter);
  });
  // How Streaming shares it.
  const px = mx + mainW + 24, pw = sideCol, bx = px + 18, bw = pw - 36;
  const shares = [
    ["Discord · 10%", "High priority · at least 2.5 Mbps", "0.5", T.ink0],
    ["YouTube · 30%", "at least 7.5 Mbps", "8.2", T.ink2],
    ["Twitch · 15%", "at least 3.8 Mbps", "3.1", T.ink3],
    ["Everything else · 20%", "every other app, fairly", "10.5", T.stone1],
  ];
  const ph = 50 + 30 + 12 + shares.length * 48 + 60 + 32 + 18;
  s += panel(px, colY, pw, ph) + text(bx, colY + 34, "How Streaming shares it", 16, T.text, 700);
  s += `<g clip-path="url(#bar)">`;
  let segX = bx;
  [[10, T.ink0, ""], [10, T.ink1, ""], [30, T.ink2, "YouTube 30%"], [15, T.ink3, ""], [15, T.ink4, ""], [20, T.s3, ""]].forEach(([pct, c, label], i, all) => {
    const w = ((bw - 2 * (all.length - 1)) * pct) / 100;
    s += `<rect x="${segX}" y="${colY + 50}" width="${w}" height="30" fill="${c}"/>`;
    if (label) s += `<svg x="${segX}" y="${colY + 50}" width="${w}" height="30">${text(8, 19, label, 12, c === T.s3 ? T.sub : T.onInk, c === T.s3 ? 400 : 700)}</svg>`;
    segX += w + 2;
  });
  s += `</g>`;
  shares.forEach(([n, small, now, c], i) => {
    const ry = colY + 92 + i * 48;
    if (i) s += `<path d="M${bx} ${ry}H${bx + bw}" ${HAIR}/>`;
    s += `<rect x="${bx}" y="${ry + 15}" width="9" height="9" rx="3" fill="${c}"/>` + text(bx + 20, ry + 21, n, 14, T.text) + text(bx + 20, ry + 38, small, 12.5, T.ter);
    s += text(bx + bw, ry + 21, now, 14, T.text, 700, END) + text(bx + bw, ry + 37, "Mbps now", 12, T.ter, 400, END);
  });
  const ny = colY + 92 + shares.length * 48 + 24;
  s += text(bx, ny, "Steam, Epic Games and 3 more only use", 13.5, T.sub) + text(bx, ny + 20, "spare speed.", 13.5, T.sub);
  s += `<rect x="${bx}" y="${ny + 36}" width="152" height="32" rx="10" fill="${T.s2}"/>` + text(bx + 11, ny + 57, "Change the split", 13.5, T.text, 700) + glyph(bx + 128, ny + 44, "chevron", 15, T.text);
  s += `</g></svg>\n`;
  fs.writeFileSync(path.join(out, "now-page.svg"), s);
}

// ---------- 2. A share is a guarantee, never a cage ----------
{
  const W = 1280, H = 420;
  let s = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" ${FONT}>
<title>With 20 Mbps of internet, Steam keeps 40% and YouTube 60% while both are busy; when the video pauses, Steam gets all of it</title>
<defs>${DEFS}<pattern id="spare" width="14" height="14" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><rect width="14" height="14" fill="${T.ink1}"/><rect width="7" height="14" fill="#d99a3a"/></pattern></defs>
<rect width="${W}" height="${H}" rx="24" fill="${T.ground}"/><rect width="${W}" height="${H}" rx="24" fill="url(#lightA)"/><rect width="${W}" height="${H}" rx="24" fill="url(#lightB)"/>`;
  const card = (x, title, sub, segs, rows) => {
    const pw = 568, bx = x + 28, bw = pw - 56;
    let p = panel(x, 40, pw, H - 80) + text(x + 28, 86, title, 22, T.text, 800) + text(x + 28, 114, sub, 15, T.sub);
    p += `<clipPath id="c${x}"><rect x="${bx}" y="142" width="${bw}" height="64" rx="14"/></clipPath><g clip-path="url(#c${x})">`;
    let px = bx;
    segs.forEach(([f, fill, label]) => {
      const w = f * (bw - 4);
      p += `<rect x="${px}" y="142" width="${w}" height="64" fill="${fill}"/>` + text(px + 16, 181, label, 17, T.onInk, 800);
      px += w + 4;
    });
    p += `</g>`;
    rows.forEach(([ic, name, line], i) => {
      const ry = 238 + i * 64;
      if (i) p += `<path d="M${bx + 56} ${ry - 12}H${bx + bw}" ${HAIR}/>`;
      p += tile(bx, ry, ic, 40) + text(bx + 56, ry + 17, name, 17, T.text, 700) + text(bx + 56, ry + 37, line, 15, T.sub);
    });
    return p;
  };
  s += card(40, "Both busy", "Each keeps its share of 20 Mbps.",
    [[0.4, T.ink1, "Steam 40%"], [0.6, T.ink0, "YouTube 60%"]],
    [["steam", "Steam", "8.0 Mbps · using all of its share"], ["youtube", "YouTube", "12.0 Mbps · using all of its share"]]);
  s += card(672, "Video paused", "What YouTube doesn't use, Steam gets.",
    [[0.4, T.ink1, "Steam 40%"], [0.6, "url(#spare)", "YouTube 60%"]],
    [["steam", "Steam", "20.0 Mbps · also using 12.0 Mbps of spare speed"], ["youtube", "YouTube", "idle · Steam uses its share meanwhile"]]);
  s += `</svg>\n`;
  fs.writeFileSync(path.join(out, "split-guarantee.svg"), s);
}
console.log("wrote", fs.readdirSync(out).join(", "));
