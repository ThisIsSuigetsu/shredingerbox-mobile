import { invoke } from "@tauri-apps/api/core";
import { mascot, Mood } from "./mascot";

declare const __APP_VERSION__: string;

// Первый рабочий экран: один ключ и кнопка. Подписки, список серверов
// и пинг придут следующими шагами MVP.

type State = "stopped" | "starting" | "started" | "stopping";
interface Status {
  state: State;
  error: string | null;
}

const KEY_STORAGE = "shredingerbox.key";

function loadKey(): string {
  try {
    return localStorage.getItem(KEY_STORAGE) ?? "";
  } catch {
    return "";
  }
}

function saveKey(key: string) {
  try {
    localStorage.setItem(KEY_STORAGE, key);
  } catch {
    // Без хранилища ключ просто не запомнится до следующего запуска.
  }
}

const app = document.getElementById("app")!;
app.innerHTML = `<main class="screen">
  <div class="mascot" id="mascot"></div>
  <h1 class="title">ShredingerBox</h1>
  <p class="status" id="status">Выключено</p>
  <textarea class="key" id="key" rows="3" spellcheck="false" autocomplete="off"
    placeholder="Вставьте ключ vless://, hy2://, trojan:// или ss://"></textarea>
  <button class="connect" id="connect">Подключить</button>
  <p class="error" id="error" hidden></p>
  <button class="link" id="log-toggle">Журнал</button>
  <section class="log" id="log" hidden>
    <div class="log-bar">
      <button class="link" id="log-refresh">Обновить</button>
      <button class="link" id="log-copy">Скопировать</button>
    </div>
    <pre id="log-text"></pre>
  </section>
  <span class="version">v${__APP_VERSION__}</span>
</main>`;

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const keyInput = $<HTMLTextAreaElement>("key");
const button = $<HTMLButtonElement>("connect");
const statusText = $("status");
const errorText = $("error");

keyInput.value = loadKey();

let state: State = "stopped";
let serverName = "";
// Пока ждём ответа на нажатие, опрос статуса не должен перерисовывать кнопку.
let busy = false;

const MOOD: Record<State, Mood> = { stopped: "sleep", starting: "wink", started: "peek", stopping: "wink" };
const LABEL: Record<State, string> = {
  stopped: "Выключено",
  starting: "Подключаемся…",
  started: "Подключено",
  stopping: "Отключаемся…",
};

function showError(message: string | null) {
  errorText.hidden = !message;
  errorText.textContent = message ?? "";
}

function render() {
  $("mascot").innerHTML = mascot(140, MOOD[state]);
  statusText.textContent = state === "started" && serverName ? `Подключено · ${serverName}` : LABEL[state];
  statusText.classList.toggle("on", state === "started");
  button.textContent = state === "stopped" ? "Подключить" : "Отключить";
  button.classList.toggle("on", state !== "stopped");
  button.disabled = busy || state === "stopping";
  keyInput.disabled = state !== "stopped";
}

async function poll() {
  try {
    const s = await invoke<Status>("vpn_status");
    if (s.state !== state && !busy) {
      state = s.state;
      if (state === "stopped" && s.error) showError(s.error);
      render();
    }
  } catch (e) {
    showError(String(e));
  }
}

button.addEventListener("click", async () => {
  busy = true;
  showError(null);
  render();
  try {
    if (state === "stopped") {
      const key = keyInput.value.trim();
      if (!key) throw "Сначала вставьте ключ";
      if (!(await invoke<boolean>("vpn_prepare"))) throw "Без разрешения на VPN подключиться нельзя";
      saveKey(key);
      serverName = await invoke<string>("vpn_connect", { link: key });
      state = "starting";
    } else {
      await invoke("vpn_disconnect");
      state = "stopping";
    }
  } catch (e) {
    showError(String(e));
  } finally {
    busy = false;
    render();
  }
});

// ---------- журнал ядра ----------

const logBox = $("log");
const logText = $("log-text");

async function loadLog() {
  try {
    logText.textContent = (await invoke<string>("vpn_log")) || "Журнал пуст — подключитесь, и здесь появятся записи ядра.";
    logText.scrollTop = logText.scrollHeight;
  } catch (e) {
    logText.textContent = String(e);
  }
}

$("log-toggle").addEventListener("click", () => {
  logBox.hidden = !logBox.hidden;
  if (!logBox.hidden) loadLog();
});
$("log-refresh").addEventListener("click", loadLog);$("log-copy").addEventListener("click", async () => {
  const button = $("log-copy");
  try {
    await navigator.clipboard.writeText(logText.textContent ?? "");
    button.textContent = "Скопировано";
  } catch {
    button.textContent = "Не удалось";
  }
  setTimeout(() => (button.textContent = "Скопировать"), 1500);
});

render();
poll();
setInterval(poll, 1000);
