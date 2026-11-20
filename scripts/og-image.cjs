// Renders the website's link preview (site/og.png, 1200 × 630): the snail
// on the app's ground, lit from above as the app's window is (Obsidian
// Jade "Blend", app/src/app.css). The PNG is committed; run this again
// after changing the art:
//   npm i --prefix <some temp dir> @resvg/resvg-js@2.6.2
//   NODE_PATH=<that dir>/node_modules node scripts/og-image.cjs
const fs = require("fs");
const path = require("path");
const { Resvg } = require("@resvg/resvg-js");

const W = 1200, H = 630, size = 360;
const x = (W - size) / 2 + 12, y = (H - size) / 2 + 6;
const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">
<defs>
<radialGradient id="a" cx="50%" cy="-14%" r="80%"><stop offset="0" stop-color="#2db894" stop-opacity=".2"/><stop offset="1" stop-color="#2db894" stop-opacity="0"/></radialGradient>
<radialGradient id="b" cx="-6%" cy="110%" r="60%"><stop offset="0" stop-color="#1f6f64" stop-opacity=".14"/><stop offset="1" stop-color="#1f6f64" stop-opacity="0"/></radialGradient>
</defs>
<rect width="${W}" height="${H}" fill="#0f1b18"/>
<rect width="${W}" height="${H}" fill="url(#a)"/>
<rect width="${W}" height="${H}" fill="url(#b)"/>
<g transform="translate(${x} ${y}) scale(${size / 512})">
  <path d="M64 404c0-18 14-30 34-30h300c34 0 58-26 62-62l4-34c2-16 26-14 26 2v52c0 54-42 98-96 98H98c-20 0-34-10-34-26z" fill="#8fd3bf"/>
  <path d="M470 276 442 192M484 276l4-86" stroke="#8fd3bf" stroke-width="16" stroke-linecap="round" fill="none"/>
  <circle cx="440" cy="182" r="17" fill="#8fd3bf"/><circle cx="488" cy="182" r="17" fill="#8fd3bf"/>
  <circle cx="224" cy="250" r="150" fill="#2FA88E"/>
  <path d="M224 250m-22 0a22 22 0 1 1 44 0a50 50 0 1 1-100 0a78 78 0 1 1 156 0a106 106 0 1 1-212 0" stroke="#0E3B35" stroke-width="20" stroke-linecap="round" fill="none"/>
</g>
</svg>`;
const out = path.resolve(__dirname, "..", "site", "og.png");
fs.writeFileSync(out, new Resvg(svg).render().asPng());
console.log("wrote", out);
