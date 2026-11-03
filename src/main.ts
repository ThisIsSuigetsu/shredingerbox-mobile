import { invoke } from "@tauri-apps/api/core";
import { mascot, Mood } from "./mascot";

declare const __APP_VERSION__: string;

type State = "stopped" | "starting" | "started" | "stopping";
interface Status {
  state: State;
  error: string | null;
}
interface Server {
  id: string;
  name: string;
  link: string;
  subscription: string | null;
}
interface Subscription {
  url: string;
  name: string;
}
interface Settings {
  servers: Server[];
  subscriptions: Subscription[];
  selected: string | null;
}
interface Ping {
  id: string;
  ms: number | null;
  udp: boolean;
  via_vpn: boolean;
}

const app = document.getElementById("app")!;
app.innerHTML = `<main class="screen">
  <section class="hero">
    <div class="mascot" id="mascot"></div>
    <p class="status" id="status">Выключено</p>
    <p class="current" id="current"></p>
    <button class="connect" id="connect">Подключить</button>
    <p class="error" id="error" hidden></p>
  </section>

  <section class="clip" id="clip" hidden>
    <p id="clip-text"></p>
    <div class="add-actions">
      <button class="link" id="clip-dismiss">Не сейчас</button>
      <button class="primary" id="clip-add">Добавить</button>
    </div>
  </section>

  <section class="servers">
    <div class="section-head">
      <h2>Серверы</h2>
      <button class="link" id="ping" hidden>Пинг</button>
      <button class="link" id="refresh" hidden>Обновить</button>
      <button class="link" id="add-toggle">Добавить</button>
    </div>
    <div class="add" id="add" hidden>
      <textarea class="key" id="add-text" rows="3" spellcheck="false" autocomplete="off"
        placeholder="Ключ vless://, hy2://, trojan://, ss:// или ссылка на подписку https://…"></textarea>
      <div class="add-actions">
        <button class="link" id="add-cancel">Отмена</button>
        <button class="primary" id="add-submit">Добавить</button>
      </div>
    </div>
    <div id="groups"></div>
  </section>

  <section class="log-section">
    <button class="link" id="log-toggle">Журнал</button>
    <div class="log" id="log" hidden>
      <div class="log-bar">
        <button class="link" id="log-refresh">Обновить</button>
        <button class="link" id="log-copy">Скопировать</button>
      </div>
      <pre id="log-text"></pre>
    </div>
  </section>
  <span class="version">ShredingerBox v${__APP_VERSION__}</span>
</main>`;

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const button = $<HTMLButtonElement>("connect");
const errorText = $("error");
const addBox = $("add");
const addText = $<HTMLTextAreaElement>("add-text");
const addSubmit = $<HTMLButtonElement>("add-submit");

let settings: Settings = { servers: [], subscriptions: [], selected: null };
let state: State = "stopped";
// Пока ждём ответа на нажатие, опрос статуса не должен перерисовывать кнопку.
let busy = false;
let pings: Record<string, Ping> = {};

const MOOD: Record<State, Mood> = { stopped: "sleep", starting: "wink", started: "peek", stopping: "wink" };
const LABEL: Record<State, string> = {
  stopped: "Выключено",
  starting: "Подключаемся…",
  started: "Подключено",
  stopping: "Отключаемся…",
};

function esc(s: string): string {
  return s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}

function showError(message: unknown) {
  const text = message ? String(message) : "";
  errorText.hidden = !text;
  errorText.textContent = text;
}

function selectedServer(): Server | undefined {
  return settings.servers.find((s) => s.id === settings.selected) ?? settings.servers[0];
}

// ---------- главный блок ----------

function renderHero() {
  $("mascot").innerHTML = mascot(120, MOOD[state]);
  const status = $("status");
  status.textContent = LABEL[state];
  status.classList.toggle("on", state === "started");
  $("current").textContent = selectedServer()?.name ?? "Нет серверов";
  button.textContent = state === "stopped" ? "Подключить" : "Отключить";
  button.classList.toggle("on", state !== "stopped");
  button.disabled = busy || state === "stopping" || (state === "stopped" && !settings.servers.length);
}

async function poll() {
  try {
    const s = await invoke<Status>("vpn_status");
    if (s.state !== state && !busy) {
      state = s.state;
      if (state === "stopped" && s.error) showError(s.error);
      renderHero();
      // После подключения пинг меряется уже через VPN — реальная задержка.
      if (state === "started" || state === "stopped") setTimeout(pingAll, 1500);
    }
  } catch (e) {
    showError(e);
  }
}

/** Общая обёртка для действий: блокирует кнопки, показывает ошибку. */
async function act(fn: () => Promise<void>) {
  busy = true;
  showError(null);
  renderHero();
  try {
    await fn();
  } catch (e) {
    showError(e);
  } finally {
    busy = false;
    renderHero();
  }
}

button.addEventListener("click", () =>
  act(async () => {
    if (state === "stopped") {
      if (!(await invoke<boolean>("vpn_prepare"))) throw "Без разрешения на VPN подключиться нельзя";
      await invoke("vpn_connect");
      state = "starting";
    } else {
      await invoke("vpn_disconnect");
      state = "stopping";
    }
  }),
);

// ---------- серверы ----------

/** Кнопка удаления срабатывает со второго нажатия, чтобы не удалить случайно. */
const armed = new Set<string>();

function deleteButton(key: string, title: string): string {
  const on = armed.has(key);
  return `<button class="delete${on ? " armed" : ""}" data-delete="${esc(key)}" aria-label="${esc(title)}">${
    on ? "Удалить?" : "✕"
  }</button>`;
}

function pingView(id: string): string {
  const p = pings[id];
  if (!p) return "";
  if (p.udp) return `<span class="ping muted">UDP</span>`;
  if (p.ms == null) return `<span class="ping bad">нет ответа</span>`;
  // Через VPN задержка складывается из двух плеч — порог выше.
  const good = p.via_vpn ? 300 : 120;
  const cls = p.ms <= good ? "good" : p.ms <= good * 2.5 ? "slow" : "bad";
  return `<span class="ping ${cls}">${p.ms} мс</span>`;
}

let pinging = false;

async function pingAll() {
  if (pinging || !settings.servers.length) return;
  pinging = true;
  const btn = $<HTMLButtonElement>("ping");
  btn.disabled = true;
  btn.textContent = "Меряем…";
  try {
    const list = await invoke<Ping[]>("ping_servers");
    pings = Object.fromEntries(list.map((p) => [p.id, p]));
  } catch {
    // Пинг — подсказка, а не действие: ошибку не показываем.
  } finally {
    pinging = false;
    btn.disabled = false;
    btn.textContent = "Пинг";
    renderServers();
  }
}

function serverRow(s: Server, removable: boolean): string {
  const selected = s.id === selectedServer()?.id;
  return `<div class="server${selected ? " selected" : ""}">
    <button class="server-pick" data-select="${esc(s.id)}">
      <span class="dot"></span><span class="server-name">${esc(s.name)}</span>${pingView(s.id)}
    </button>
    ${removable ? deleteButton(`server:${s.id}`, `Удалить ${s.name}`) : ""}
  </div>`;
}

function renderServers() {
  const groups: string[] = [];
  for (const sub of settings.subscriptions) {
    const list = settings.servers.filter((s) => s.subscription === sub.url);
    groups.push(`<div class="group">
      <div class="group-head">
        <span class="group-name">${esc(sub.name)}</span>
        <span class="group-count">${list.length}</span>
        ${deleteButton(`sub:${sub.url}`, `Удалить подписку ${sub.name}`)}
      </div>
      ${list.map((s) => serverRow(s, false)).join("")}
    </div>`);
  }
  const keys = settings.servers.filter((s) => !s.subscription);
  if (keys.length) {
    groups.push(`<div class="group">
      <div class="group-head"><span class="group-name">Ключи</span><span class="group-count">${keys.length}</span></div>
      ${keys.map((s) => serverRow(s, true)).join("")}
    </div>`);
  }
  $("groups").innerHTML = groups.length
    ? groups.join("")
    : `<p class="empty">Добавьте ключ или ссылку на подписку — их присылает тот, кто дал вам VPN.</p>`;
  $("refresh").hidden = !settings.subscriptions.length;
  $("ping").hidden = !settings.servers.length;
  if (!settings.servers.length) addBox.hidden = false;
}

function render() {
  renderHero();
  renderServers();
}

$("groups").addEventListener("click", (e) => {
  const target = e.target as HTMLElement;
  const del = target.closest<HTMLElement>("[data-delete]");
  if (del) {
    const key = del.dataset.delete!;
    if (!armed.has(key)) {
      armed.add(key);
      setTimeout(() => {
        armed.delete(key);
        renderServers();
      }, 3000);
      renderServers();
      return;
    }
    armed.delete(key);
    const [kind, value] = [key.slice(0, key.indexOf(":")), key.slice(key.indexOf(":") + 1)];
    act(async () => {
      settings = kind === "sub"
        ? await invoke<Settings>("remove_subscription", { url: value })
        : await invoke<Settings>("remove_server", { id: value });
      render();
    });
    return;
  }
  const pick = target.closest<HTMLElement>("[data-select]");
  if (pick && pick.dataset.select !== selectedServer()?.id) {
    const id = pick.dataset.select!;
    // Сразу отмечаем выбор, не дожидаясь переподключения.
    settings.selected = id;
    render();
    act(async () => {
      settings = await invoke<Settings>("select_server", { id });
      render();
    });
  }
});

$("add-toggle").addEventListener("click", () => {
  addBox.hidden = !addBox.hidden;
  if (!addBox.hidden) addText.focus();
});
$("add-cancel").addEventListener("click", () => {
  addText.value = "";
  if (settings.servers.length) addBox.hidden = true;
});
addSubmit.addEventListener("click", () =>
  act(async () => {
    const text = addText.value.trim();
    if (!text) throw "Вставьте ключ или ссылку на подписку";
    addSubmit.disabled = true;
    addSubmit.textContent = "Добавляем…";
    try {
      settings = await invoke<Settings>("add_from_text", { text });
    } finally {
      addSubmit.disabled = false;
      addSubmit.textContent = "Добавить";
    }
    addText.value = "";
    addBox.hidden = true;
    render();
    pingAll();
  }),
);
$("ping").addEventListener("click", pingAll);
$("refresh").addEventListener("click", () =>
  act(async () => {
    const refresh = $<HTMLButtonElement>("refresh");
    refresh.textContent = "Обновляем…";
    try {
      settings = await invoke<Settings>("refresh_subscriptions");
    } catch (e) {
      // Часть подписок могла обновиться — перечитываем то, что сохранилось.
      settings = await invoke<Settings>("get_settings");
      throw e;
    } finally {
      refresh.textContent = "Обновить";
      render();
      pingAll();
    }
  }),
);

// ---------- буфер обмена ----------

const clipBox = $("clip");
let clipCandidate = "";
/** Текст из буфера, который уже отклонили, — второй раз не предлагаем. */
let clipDismissed = "";

/** «ключ «Германия»», «3 ключа», «подписка с panel.example.org» — без самих секретов. */
function clipSummary(text: string): string {
  if (/^https?:\/\//i.test(text)) {
    try {
      return `ссылка на подписку с ${new URL(text).host}`;
    } catch {
      return "ссылка на подписку";
    }
  }
  const lines = text.split("\n").filter(Boolean);
  if (lines.length > 1) return `ключи: ${lines.length}`;
  const hash = lines[0].indexOf("#");
  let name = "";
  try {
    name = hash >= 0 ? decodeURIComponent(lines[0].slice(hash + 1)) : "";
  } catch {
    // Кривое имя — покажем без него.
  }
  return name ? `ключ «${name}»` : `ключ ${lines[0].split("://")[0]}://…`;
}

async function checkClipboard() {
  if (document.visibilityState !== "visible") return;
  try {
    const text = await invoke<string | null>("clipboard_candidate");
    if (!text || text === clipDismissed || text === clipCandidate) return;
    clipCandidate = text;
    $("clip-text").textContent = `В буфере обмена ${clipSummary(text)}. Добавить?`;
    clipBox.hidden = false;
  } catch {
    // Буфер недоступен — молча пропускаем.
  }
}

function hideClip() {
  clipCandidate = "";
  clipBox.hidden = true;
}

$("clip-dismiss").addEventListener("click", () => {
  clipDismissed = clipCandidate;
  hideClip();
});
$("clip-add").addEventListener("click", () =>
  act(async () => {
    const text = clipCandidate;
    hideClip();
    try {
      settings = await invoke<Settings>("add_from_text", { text });
    } catch (e) {
      clipDismissed = text;
      throw e;
    }
    addBox.hidden = true;
    render();
    pingAll();
  }),
);
// Буфер читается только когда окно в фокусе — проверяем при каждом возвращении в приложение.
window.addEventListener("focus", checkClipboard);
document.addEventListener("visibilitychange", checkClipboard);

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
$("log-refresh").addEventListener("click", loadLog);
$("log-copy").addEventListener("click", async () => {
  const copy = $("log-copy");
  try {
    await navigator.clipboard.writeText(logText.textContent ?? "");
    copy.textContent = "Скопировано";
  } catch {
    copy.textContent = "Не удалось";
  }
  setTimeout(() => (copy.textContent = "Скопировать"), 1500);
});

// ---------- запуск ----------

/** Первая версия хранила один ключ в localStorage — переносим его в настройки. */
async function migrateOldKey() {
  let key = "";
  try {
    key = localStorage.getItem("shredingerbox.key") ?? "";
  } catch {
    return;
  }
  if (!key) return;
  if (!settings.servers.length) {
    try {
      settings = await invoke<Settings>("add_from_text", { text: key });
    } catch {
      // Битый ключ просто не переносим.
    }
  }
  try {
    localStorage.removeItem("shredingerbox.key");
  } catch {
    // ignore
  }
}

(async () => {
  // В обычном браузере (npm run dev) Tauri нет — подставляем демо-данные.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) await import("./mock");
  try {
    settings = await invoke<Settings>("get_settings");
    await migrateOldKey();
  } catch (e) {
    showError(e);
  }
  render();
  poll();
  setInterval(poll, 1000);
  pingAll();
  // Сразу после запуска окно может ещё не получить фокус — буфер вернёт пусто.
  setTimeout(checkClipboard, 700);
})();
