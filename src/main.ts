import { mount } from "svelte";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App.svelte";
import "./app.css";

mount(App, { target: document.getElementById("app")! });

function syncWindowFocus(): void {
  document.documentElement.classList.toggle("window-inactive", !document.hasFocus());
}

window.addEventListener("focus", syncWindowFocus);
window.addEventListener("blur", syncWindowFocus);
document.addEventListener("visibilitychange", syncWindowFocus);
syncWindowFocus();

if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
  const appWindow = getCurrentWindow();
  const syncTauriFocus = () => {
    void appWindow
      .isFocused()
      .then((focused) => {
        document.documentElement.classList.toggle("window-inactive", !focused);
      })
      .catch(() => {
        // Keep the DOM focus fallback when the window API is unavailable.
      });
  };
  setInterval(syncTauriFocus, 400);
  syncTauriFocus();
}
