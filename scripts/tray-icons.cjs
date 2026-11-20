// Renders the tray snail's moods (PLAN.md §14.4) from
// app/src-tauri/icons/tray/*.svg to PNGs next to them, at 32 and 64 px.
// The PNGs are committed, so the app's build needs no Node step. Run it
// again after changing the art:
//   npm i --prefix <some temp dir> @resvg/resvg-js@2.6.2
//   NODE_PATH=<that dir>/node_modules node scripts/tray-icons.cjs
const fs = require("fs");
const path = require("path");
const { Resvg } = require("@resvg/resvg-js");

const dir = path.resolve(__dirname, "..", "app", "src-tauri", "icons", "tray");
for (const name of ["snail-working", "snail-struggling"]) {
  const svg = fs.readFileSync(path.join(dir, name + ".svg"));
  for (const size of [32, 64]) {
    const png = new Resvg(svg, { fitTo: { mode: "width", value: size } }).render().asPng();
    fs.writeFileSync(path.join(dir, `${name}-${size}.png`), png);
    console.log(`${name}-${size}.png`);
  }
}
