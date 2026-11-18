// Tests for the landing page's script (site/main.js): the download buttons
// and the split demo, in jsdom. site/ stays buildless, so jsdom isn't a
// project dependency; install it somewhere temporary and point NODE_PATH at it:
//
//   npm install --prefix "$TEMP/site-test" jsdom
//   NODE_PATH="$TEMP/site-test/node_modules" node scripts/site-test.cjs
"use strict";

const fs = require("fs");
const path = require("path");
const assert = require("assert/strict");
const { JSDOM } = require("jsdom");

const SITE = path.join(__dirname, "..", "site");
const HTML = fs.readFileSync(path.join(SITE, "index.html"), "utf8");
const JS = fs.readFileSync(path.join(SITE, "main.js"), "utf8");
const REPO = "https://github.com/StergiosF/SnailNet";
const FIXED = `${REPO}/releases/latest/download/SnailNet-setup.exe`;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// A page with main.js run against `respond`, which stands in for GitHub's API.
async function page(respond) {
  const dom = new JSDOM(HTML, { url: "https://stergiosf.github.io/SnailNet/", pretendToBeVisual: true, runScripts: "outside-only" });
  const w = dom.window;
  w.fetch = async (url) => respond(url);
  w.eval(JS);
  await sleep(20);
  return w;
}
const json = (status, body) => ({ status, ok: status >= 200 && status < 300, json: async () => body });

const buttons = (w) => [...w.document.querySelectorAll("[data-download]")];
const releaseLines = (w) => [...w.document.querySelectorAll("[data-release]")];

const tests = [];
const test = (name, fn) => tests.push([name, fn]);

// ---------- Download buttons ----------

test("every download button starts on the releases page", async () => {
  const w = await page(() => new Promise(() => {})); // the API never answers
  const bs = buttons(w);
  assert.ok(bs.length >= 3, "nav, hero and close each have one");
  for (const b of bs) assert.equal(b.href, `${REPO}/releases`);
});

test("no release yet: the buttons say so and link the repository", async () => {
  const w = await page(() => json(404, {}));
  for (const b of buttons(w)) {
    assert.equal(b.href, REPO);
    assert.equal(b.querySelector("[data-download-label]").textContent, "Coming soon · watch on GitHub");
  }
  for (const l of releaseLines(w)) {
    assert.equal(l.hidden, false);
    assert.match(l.textContent, /No release yet/);
  }
});

test("a release with the fixed-name installer links straight to it", async () => {
  const w = await page(() =>
    json(200, { tag_name: "v1.2.0", assets: [{ name: "SnailNet-setup.exe", size: 5 * 1024 * 1024 }] })
  );
  for (const b of buttons(w)) assert.equal(b.href, FIXED);
  for (const l of releaseLines(w)) assert.equal(l.textContent, "Version 1.2.0 · 5.0 MB");
});

test("an older release falls back to the versioned installer", async () => {
  const url = `${REPO}/releases/download/v0.9.0/SnailNet_0.9.0_x64-setup.exe`;
  const w = await page(() =>
    json(200, {
      tag_name: "v0.9.0",
      assets: [{ name: "SnailNet_0.9.0_x64-setup.exe", size: 4 * 1024 * 1024, browser_download_url: url }],
    })
  );
  for (const b of buttons(w)) assert.equal(b.href, url);
});

test("rate-limited or offline: the buttons keep the releases page", async () => {
  for (const respond of [() => json(403, {}), () => Promise.reject(new Error("offline"))]) {
    const w = await page(respond);
    for (const b of buttons(w)) assert.equal(b.href, `${REPO}/releases`);
    for (const l of releaseLines(w)) assert.equal(l.hidden, true);
  }
});

test("the answer is cached for the session", async () => {
  let calls = 0;
  const dom = new JSDOM(HTML, { url: "https://stergiosf.github.io/SnailNet/", pretendToBeVisual: true, runScripts: "outside-only" });
  const w = dom.window;
  w.fetch = async () => (calls++, json(404, {}));
  w.eval(JS);
  await sleep(20);
  w.eval(JS); // a second run of the script reads the cache
  await sleep(20);
  assert.equal(calls, 1);
});

// ---------- The split demo ----------
// A 20 Mbps line: 5% is 1 Mbps. Steam wants all it can get; the video about 9 Mbps.

function demo(w) {
  const d = w.document.querySelector("[data-demo]");
  const q = (s) => d.querySelector(s);
  return {
    d,
    handle: q("[data-handle]"),
    bar: q("[data-bar]"),
    // Numbers keep their units on one line (no-break spaces); compare as plain text.
    steam: () => q('[data-status="a"]').textContent.replace(/\u00a0/g, " "),
    video: () => q('[data-status="b"]').textContent.replace(/\u00a0/g, " "),
    pct: (k) => [...d.querySelectorAll(`[data-pct="${k}"]`)].map((e) => e.textContent),
    step: (k, dir) => q(`[data-step="${k}"][data-dir="${dir}"]`),
    pause: q("[data-pause]"),
    live: q("[data-live]"),
    key: (key) => q("[data-handle]").dispatchEvent(new w.KeyboardEvent("keydown", { key, bubbles: true })),
  };
}

test("it starts at Steam 40%, YouTube 60%, YouTube lending what it doesn't use", async () => {
  const t = demo(await page(() => json(404, {})));
  assert.deepEqual(t.pct("a"), ["40%", "40%"]);
  assert.deepEqual(t.pct("b"), ["60%", "60%"]);
  assert.equal(t.steam(), "11.0 Mbps · 3.0 Mbps of it is spare room");
  assert.equal(t.video(), "9.0 Mbps · lending 3.0 Mbps it isn't using");
  assert.equal(t.handle.getAttribute("aria-valuenow"), "40");
  assert.equal(t.handle.getAttribute("aria-valuetext"), "Steam 40%, YouTube 60%");
});

test("− and + move 5% at a time, in the app's words", async () => {
  const t = demo(await page(() => json(404, {})));
  t.step("a", "1").click(); // Steam 45%: YouTube's 11 Mbps still covers the video
  assert.deepEqual(t.pct("a"), ["45%", "45%"]);
  assert.equal(t.video(), "9.0 Mbps · lending 2.0 Mbps it isn't using");
  t.step("a", "1").click(); // 50%: YouTube's slice is 10 Mbps
  t.step("b", "-1").click(); // 55%: 9 Mbps, exactly what the video wants
  assert.equal(t.video(), "9.0 Mbps · gets all it asks for");
  assert.equal(t.steam(), "11.0 Mbps · using its whole slice, wants more");
  t.step("a", "1").click(); // 60%: YouTube's 8 Mbps is less than the video wants
  assert.equal(t.video(), "8.0 Mbps · using its whole slice, wants more");
  assert.equal(t.steam(), "12.0 Mbps · using its whole slice, wants more");
});

test("the handle works from the keyboard and stops at 5% and 95%", async () => {
  const t = demo(await page(() => json(404, {})));
  t.key("ArrowRight");
  assert.equal(t.handle.getAttribute("aria-valuenow"), "45");
  t.key("PageUp");
  assert.equal(t.handle.getAttribute("aria-valuenow"), "55");
  t.key("End");
  assert.equal(t.handle.getAttribute("aria-valuenow"), "95");
  assert.equal(t.step("a", "1").disabled, true);
  assert.equal(t.step("b", "-1").disabled, true);
  t.key("ArrowUp");
  assert.equal(t.handle.getAttribute("aria-valuenow"), "95");
  t.key("Home");
  assert.equal(t.handle.getAttribute("aria-valuenow"), "5");
  assert.equal(t.step("a", "-1").disabled, true);
  assert.equal(t.step("b", "1").disabled, true);
});

test("dragging follows the pointer, then settles on a step of 5", async () => {
  const w = await page(() => json(404, {}));
  const t = demo(w);
  t.bar.getBoundingClientRect = () => ({ left: 100, width: 1000, top: 0, height: 84, right: 1100, bottom: 84 });
  t.bar.setPointerCapture = () => {};
  const ev = (type, x) => t.bar.dispatchEvent(new w.MouseEvent(type, { clientX: x, button: 0, bubbles: true }));
  ev("pointerdown", 100 + 720);
  assert.ok(t.bar.classList.contains("is-dragging"));
  assert.equal(t.handle.getAttribute("aria-valuenow"), "72");
  ev("pointermove", 100 + 640);
  assert.equal(t.handle.getAttribute("aria-valuenow"), "64");
  ev("pointerup", 100 + 640);
  assert.ok(!t.bar.classList.contains("is-dragging"));
  assert.equal(t.handle.getAttribute("aria-valuenow"), "65");
  assert.equal(t.bar.style.getPropertyValue("--p"), "65");
});

test("pausing the video lends its whole slice to Steam, and says so politely", async () => {
  const t = demo(await page(() => json(404, {})));
  t.pause.click();
  assert.equal(t.pause.getAttribute("aria-pressed"), "true");
  assert.equal(t.pause.querySelector("[data-pause-label]").textContent, "Play the video");
  assert.equal(t.video(), "quiet right now · lending 12.0 Mbps it isn't using");
  assert.equal(t.steam(), "20.0 Mbps · 12.0 Mbps of it is spare room");
  await sleep(600);
  assert.equal(t.live.textContent, "Steam 20.0 Mbps, YouTube 0.0 Mbps.");
  t.pause.click();
  assert.equal(t.pause.getAttribute("aria-pressed"), "false");
  assert.equal(t.video(), "9.0 Mbps · lending 3.0 Mbps it isn't using");
});

(async () => {
  let failed = 0;
  for (const [name, fn] of tests) {
    try {
      await fn();
      console.log(`ok   ${name}`);
    } catch (e) {
      failed++;
      console.log(`FAIL ${name}\n     ${e.message.split("\n").join("\n     ")}`);
    }
  }
  console.log(`\n${tests.length - failed} of ${tests.length} passed`);
  process.exit(failed ? 1 : 0);
})();
