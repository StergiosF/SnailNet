import { getCurrentWindow } from "@tauri-apps/api/window";
import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import Flyout from "./lib/components/Flyout.svelte";
import { installFocusRing } from "./lib/focusRing";

installFocusRing();

// No browser menu (Back, Refresh, Print…) on right-click: this is an app,
// not a web page. Text fields keep theirs for cut, copy and paste. Dev
// builds keep it everywhere for Inspect.
if (!import.meta.env.DEV) {
  document.addEventListener("contextmenu", (e) => {
    const el = e.target;
    const typing =
      el instanceof HTMLTextAreaElement ||
      (el instanceof HTMLInputElement && ["text", "number", "search", "email", "url"].includes(el.type)) ||
      (el instanceof HTMLElement && el.isContentEditable);
    if (!typing) e.preventDefault();
  });
}

const target = document.getElementById("app");
if (!target) throw new Error("missing #app");

// Both windows load this page; the tray flyout (tray.rs) gets its own view.
const view = getCurrentWindow().label === "flyout" ? Flyout : App;

export default mount(view, { target });
