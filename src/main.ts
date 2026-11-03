import { invoke } from "@tauri-apps/api/core";
import { mascot } from "./mascot";

declare const __APP_VERSION__: string;

// Пока каркас: экран-заглушка проверяет, что интерфейс и Rust-часть
// собрались и разговаривают друг с другом на телефоне.
async function render() {
  const app = document.getElementById("app")!;
  let platform = "…";
  try {
    platform = await invoke<string>("platform");
  } catch (e) {
    platform = `ошибка: ${e}`;
  }
  app.innerHTML = `<main class="screen">
    ${mascot(160, "sleep")}
    <h1 class="title">ShredingerBox</h1>
    <p class="muted">Скоро здесь будет VPN</p>
    <span class="version">v${__APP_VERSION__} · ${platform}</span>
  </main>`;
}

render();
