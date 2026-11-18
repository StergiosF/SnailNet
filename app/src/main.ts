import { getCurrentWindow } from "@tauri-apps/api/window";
import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import Flyout from "./lib/components/Flyout.svelte";
import { installFocusRing } from "./lib/focusRing";

installFocusRing();

const target = document.getElementById("app");
if (!target) throw new Error("missing #app");

// Both windows load this page; the tray flyout (tray.rs) gets its own view.
const view = getCurrentWindow().label === "flyout" ? Flyout : App;

export default mount(view, { target });
