// SnailNet's landing page: the download buttons, the split demo, the header.
// No framework; everything works without this file except the demo.
"use strict";

(() => {
  const REPO = "StergiosF/SnailNet";
  const FIXED_URL = `https://github.com/${REPO}/releases/latest/download/SnailNet-setup.exe`;

  // ---------- Download buttons ----------
  // The buttons start on the releases page, which never 404s. GitHub's API
  // says what the latest release is: with the fixed-name installer on it,
  // they point straight at the file; with no release yet, they say so.

  const CACHE_KEY = "snailnet-release-v1";
  const CACHE_MS = 10 * 60 * 1000;

  function readCache() {
    try {
      const raw = sessionStorage.getItem(CACHE_KEY);
      if (!raw) return null;
      const c = JSON.parse(raw);
      return Date.now() - c.t < CACHE_MS ? c : null;
    } catch {
      return null;
    }
  }
  function writeCache(c) {
    try {
      sessionStorage.setItem(CACHE_KEY, JSON.stringify({ ...c, t: Date.now() }));
    } catch {
      /* private mode or blocked storage: no cache, still works */
    }
  }

  const megabytes = (bytes) => `${(bytes / (1024 * 1024)).toFixed(1)} MB`;

  function showRelease(c) {
    const buttons = document.querySelectorAll("[data-download]");
    const lines = document.querySelectorAll("[data-release]");
    if (c.kind === "none") {
      buttons.forEach((b) => {
        b.href = `https://github.com/${REPO}`;
        b.querySelector("[data-download-label]").textContent = "Coming soon · watch on GitHub";
      });
      lines.forEach((l) => {
        l.textContent = "No release yet. Watch the repository on GitHub to hear when the first one is out.";
        l.hidden = false;
      });
      return;
    }
    if (c.kind === "release") {
      if (c.url) buttons.forEach((b) => (b.href = c.url));
      const parts = [`Version ${c.version}`];
      if (c.size) parts.push(megabytes(c.size));
      lines.forEach((l) => {
        l.textContent = parts.join(" · ");
        l.hidden = false;
      });
    }
  }

  async function checkRelease() {
    const cached = readCache();
    if (cached) return showRelease(cached);
    const ctl = new AbortController();
    const timer = setTimeout(() => ctl.abort(), 6000);
    try {
      const r = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
        headers: { Accept: "application/vnd.github+json" },
        signal: ctl.signal,
      });
      if (r.status === 404) {
        const c = { kind: "none" };
        writeCache(c);
        return showRelease(c);
      }
      if (!r.ok) return; // rate limit or an outage: the buttons keep the releases page
      const rel = await r.json();
      const assets = Array.isArray(rel.assets) ? rel.assets : [];
      const fixed = assets.find((a) => a.name === "SnailNet-setup.exe");
      const versioned = assets.find((a) => /_x64-setup\.exe$/i.test(a.name));
      const asset = fixed || versioned;
      const c = {
        kind: "release",
        version: String(rel.tag_name || "").replace(/^v/, ""),
        size: asset ? asset.size : 0,
        url: fixed ? FIXED_URL : versioned ? versioned.browser_download_url : null,
      };
      writeCache(c);
      showRelease(c);
    } catch {
      /* offline or blocked: the buttons keep the releases page */
    } finally {
      clearTimeout(timer);
    }
  }
  checkRelease();

  // ---------- The header's edge once the page moves ----------

  const nav = document.querySelector(".nav");
  let ticking = false;
  const onScroll = () => {
    if (ticking) return;
    ticking = true;
    requestAnimationFrame(() => {
      nav.classList.toggle("is-scrolled", window.scrollY > 8);
      ticking = false;
    });
  };
  window.addEventListener("scroll", onScroll, { passive: true });
  onScroll();

  // ---------- The header marks the section you're in ----------

  const links = [...document.querySelectorAll('.nav__links a[href^="#"]')];
  const targets = links.map((a) => document.querySelector(a.getAttribute("href"))).filter(Boolean);
  if ("IntersectionObserver" in window && targets.length) {
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (!e.isIntersecting) continue;
          links.forEach((a) => {
            if (a.getAttribute("href") === `#${e.target.id}`) a.setAttribute("aria-current", "true");
            else a.removeAttribute("aria-current");
          });
        }
      },
      // A section counts as "in" while it crosses the middle of the screen.
      { rootMargin: "-50% 0px -50% 0px" }
    );
    targets.forEach((t) => io.observe(t));
    // Above the first named section (the hero, the why): nothing is marked.
    const first = targets[0];
    const clear = () => {
      if (first.getBoundingClientRect().top > innerHeight / 2) links.forEach((a) => a.removeAttribute("aria-current"));
    };
    window.addEventListener("scroll", clear, { passive: true });
  }

  // ---------- Charts draw themselves once, as they come into view ----------
  // Without the observer, or with reduced motion, they stay drawn.

  const reveals = document.querySelectorAll("[data-reveal]");
  if ("IntersectionObserver" in window && !matchMedia("(prefers-reduced-motion: reduce)").matches) {
    const seen = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (!e.isIntersecting) continue;
          e.target.dataset.reveal = "in";
          seen.unobserve(e.target);
        }
      },
      { threshold: 0.3 }
    );
    reveals.forEach((el) => {
      el.dataset.reveal = "wait";
      seen.observe(el);
    });
  }

  // ---------- Features: one at a time ----------
  // A tab list (arrow keys, Home and End move between them). Without the
  // script every picture shows, one under another.

  const fx = document.querySelector("[data-fx]");
  if (fx) {
    const tabs = [...fx.querySelectorAll('[role="tab"]')];
    const show = (tab, focus) => {
      tabs.forEach((t) => {
        const on = t === tab;
        t.setAttribute("aria-selected", String(on));
        t.tabIndex = on ? 0 : -1;
        document.getElementById(t.getAttribute("aria-controls")).hidden = !on;
      });
      if (focus) tab.focus();
    };
    const keys = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 };
    tabs.forEach((t, i) => {
      t.addEventListener("click", () => show(t, false));
      t.addEventListener("keydown", (e) => {
        let to;
        if (e.key in keys) to = (i + keys[e.key] + tabs.length) % tabs.length;
        else if (e.key === "Home") to = 0;
        else if (e.key === "End") to = tabs.length - 1;
        else return;
        e.preventDefault();
        show(tabs[to], true);
      });
    });
    fx.dataset.fx = "on";
    show(tabs.find((t) => t.getAttribute("aria-selected") === "true") || tabs[0], false);
  }

  // ---------- The split demo ----------
  // 20 Mbps of internet (5% is 1 Mbps) between a Steam download (wants all it can get) and
  // YouTube (a stream wanting about 9 Mbps). Each share is a guarantee when
  // both are busy; what YouTube leaves unused, Steam gets. Words are the
  // app's own (app/src/lib/i18n/en.json, status.*).

  const demo = document.querySelector("[data-demo]");
  if (!demo) return;

  const LINE = 20;
  const VIDEO_WANTS = 9;
  const MIN = 5;
  const MAX = 95;
  const STEP = 5;

  const bar = demo.querySelector("[data-bar]");
  const handle = demo.querySelector("[data-handle]");
  const segA = demo.querySelector('[data-seg="a"]');
  const segB = demo.querySelector('[data-seg="b"]');
  const pauseBtn = demo.querySelector("[data-pause]");
  const pauseLabel = demo.querySelector("[data-pause-label]");
  const pauseIcon = demo.querySelector("[data-pause-icon]");
  const live = demo.querySelector("[data-live]");

  let p = 40; // Steam's share, percent
  let paused = false;
  let liveTimer = 0;

  const f = (v) => v.toFixed(1);
  const clamp = (v) => Math.min(MAX, Math.max(MIN, v));

  function render(announce) {
    const shown = Math.round(p);
    const keepA = (LINE * p) / 100;
    const keepB = LINE - keepA;
    const videoGets = Math.min(paused ? 0 : VIDEO_WANTS, keepB);
    const steamGets = LINE - videoGets;
    const lent = keepB - videoGets;

    bar.style.setProperty("--p", String(p));
    segB.style.setProperty("--lent", String(keepB > 0 ? (lent / keepB) * 100 : 0));
    demo.querySelectorAll('[data-pct="a"]').forEach((el) => (el.textContent = `${shown}%`));
    demo.querySelectorAll('[data-pct="b"]').forEach((el) => (el.textContent = `${100 - shown}%`));

    const steam =
      steamGets > keepA + 0.05
        ? `${f(steamGets)}\u00a0Mbps · also using ${f(steamGets - keepA)}\u00a0Mbps of spare speed`
        : `${f(steamGets)}\u00a0Mbps · using all of its share`;
    let video;
    if (paused) video = "idle · Steam uses its share meanwhile";
    else if (videoGets < VIDEO_WANTS - 0.05) video = `${f(videoGets)}\u00a0Mbps · using all of its share`;
    else if (lent > 0.05) video = `${f(videoGets)}\u00a0Mbps · not using all of its share`;
    else video = `${f(videoGets)}\u00a0Mbps · getting all it needs`;
    demo.querySelector('[data-status="a"]').textContent = steam;
    demo.querySelector('[data-status="b"]').textContent = video;

    handle.setAttribute("aria-valuenow", String(shown));
    handle.setAttribute("aria-valuetext", `Steam ${shown}%, YouTube ${100 - shown}%`);
    demo.querySelectorAll("[data-step]").forEach((b) => {
      const grow = (b.dataset.step === "a") === (b.dataset.dir === "1");
      b.disabled = grow ? shown >= MAX : shown <= MIN;
    });

    // A segment too narrow for its name shows only its share.
    const inner = bar.clientWidth - 4;
    [
      [segA, (inner * p) / 100],
      [segB, (inner * (100 - p)) / 100],
    ].forEach(([seg, width]) => {
      seg.classList.remove("is-narrow");
      const need = seg.querySelector(".dbar__label").scrollWidth + 48;
      seg.classList.toggle("is-narrow", width < need);
    });

    if (announce) {
      clearTimeout(liveTimer);
      liveTimer = setTimeout(() => {
        live.textContent = `Steam ${f(steamGets)} Mbps, YouTube ${f(videoGets)} Mbps.`;
      }, 500);
    }
  }

  function setShare(next, announce = true) {
    p = clamp(next);
    render(announce);
  }
  const snap = (v) => Math.round(v / STEP) * STEP;

  // The steppers move to the next multiple of 5, as the app's − and + do.
  demo.querySelectorAll("[data-step]").forEach((b) => {
    b.addEventListener("click", () => {
      const growA = (b.dataset.step === "a") === (b.dataset.dir === "1");
      setShare(growA ? Math.floor(p / STEP) * STEP + STEP : Math.ceil(p / STEP) * STEP - STEP);
    });
  });

  // Dragging: the bar follows the pointer, then settles on a step of 5.
  const shareAt = (clientX) => {
    const r = bar.getBoundingClientRect();
    return ((clientX - r.left) / r.width) * 100;
  };
  bar.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    bar.setPointerCapture(e.pointerId);
    bar.classList.add("is-dragging");
    setShare(shareAt(e.clientX), false);
    handle.focus({ preventScroll: true });
  });
  bar.addEventListener("pointermove", (e) => {
    if (bar.classList.contains("is-dragging")) setShare(shareAt(e.clientX), false);
  });
  const endDrag = () => {
    if (!bar.classList.contains("is-dragging")) return;
    bar.classList.remove("is-dragging");
    setShare(snap(p));
  };
  bar.addEventListener("pointerup", endDrag);
  bar.addEventListener("pointercancel", endDrag);
  bar.addEventListener("lostpointercapture", endDrag);

  handle.addEventListener("keydown", (e) => {
    const moves = {
      ArrowLeft: -STEP,
      ArrowDown: -STEP,
      ArrowRight: STEP,
      ArrowUp: STEP,
      PageDown: -2 * STEP,
      PageUp: 2 * STEP,
    };
    if (e.key in moves) setShare(snap(p) + moves[e.key]);
    else if (e.key === "Home") setShare(MIN);
    else if (e.key === "End") setShare(MAX);
    else return;
    e.preventDefault();
  });

  pauseBtn.addEventListener("click", () => {
    paused = !paused;
    pauseBtn.setAttribute("aria-pressed", String(paused));
    pauseLabel.textContent = paused ? "Play the video" : "Pause the video";
    pauseIcon.setAttribute("href", paused ? "#g-play" : "#g-pause");
    render(true);
  });

  let resizeTimer = 0;
  window.addEventListener("resize", () => {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => render(false), 120);
  });

  render(false);
})();
