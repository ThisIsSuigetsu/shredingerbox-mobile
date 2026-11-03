import { invoke } from "@tauri-apps/api/core";
import { mascot, Mood } from "./mascot";
import { changesOf, notesHtml } from "./changelog";

declare const __APP_VERSION__: string;

type State = "stopped" | "starting" | "started" | "stopping";
type View = "home" | "servers" | "rules" | "settings";
type Mode = "tun" | "proxy";
type FinalRoute = "proxy" | "direct";
type AppList = "proxy_apps" | "direct_apps";
type SiteList = "proxy_domains" | "direct_domains";

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
interface AppRule {
  name: string;
  package: string;
}
interface Rules {
  final_route: FinalRoute;
  proxy_apps: AppRule[];
  direct_apps: AppRule[];
  proxy_domains: string[];
  direct_domains: string[];
  presets: string[];
}
interface Settings {
  servers: Server[];
  subscriptions: Subscription[];
  selected: string | null;
  mode: Mode;
  autostart: boolean;
  rules: Rules;
}
interface UpdateInfo {
  version: string;
  current: string;
  notes: string | null;
  url: string;
  size: number;
}
interface Ping {
  id: string;
  ms: number | null;
  udp: boolean;
  via_vpn: boolean;
}
interface Preset {
  id: string;
  title: string;
  domains: string[];
}
interface Usage {
  up: number;
  down: number;
}
interface MonthTraffic {
  month: string;
  servers: Record<string, Usage>;
  speed: Usage;
}
interface AppInfo {
  name: string;
  package: string;
  icon: string | null;
}
type Modal =
  | { kind: "add-server"; text: string; busy: boolean }
  | { kind: "clipboard"; text: string; busy: boolean; from: "clipboard" | "link" }
  | { kind: "add-app"; list: AppList; filter: string; picked: Set<string> }
  | { kind: "add-sites"; list: SiteList; text: string }
  | { kind: "update"; info: UpdateInfo; busy: boolean }
  | { kind: "whats-new"; version: string; text: string };

/** Порт локального прокси - settings.rs::PROXY_PORT. */
const PROXY = "127.0.0.1:2080";

/** Карточки, которые сворачиваются нажатием на заголовок. */
type Fold = "mode" | "log";
const FOLD_KEYS: Record<Fold, string> = {
  mode: "shredingerbox.modeFolded",
  log: "shredingerbox.logFolded",
};

/** Удобства одного телефона в localStorage: может быть недоступен - тогда просто по умолчанию. */
function readFlag(key: string): boolean {
  try {
    return localStorage.getItem(key) === "1";
  } catch {
    return false;
  }
}

const st = {
  view: "home" as View,
  settings: {
    servers: [],
    subscriptions: [],
    selected: null,
    mode: "tun",
    autostart: false,
    rules: { final_route: "direct", proxy_apps: [], direct_apps: [], proxy_domains: [], direct_domains: [], presets: [] },
  } as Settings,
  state: "stopped" as State,
  // Пока ждём ответа на нажатие, опрос статуса не должен перерисовывать кнопку.
  busy: false,
  pings: {} as Record<string, Ping>,
  pinging: false,
  presets: [] as Preset[],
  /** Установленные приложения - загружаются, когда впервые нужны правилам. */
  apps: null as AppInfo[] | null,
  modal: null as Modal | null,
  siteTab: "proxy_domains" as SiteList,
  log: "",
  /** null - ещё не проверяли, false - обновлений нет. */
  update: null as UpdateInfo | false | null,
  checkingUpdate: false,
  /** Свёрнутые карточки - запоминаются на этом телефоне. */
  folded: { mode: readFlag(FOLD_KEYS.mode), log: readFlag(FOLD_KEYS.log) } as Record<Fold, boolean>,
  /** Расход по серверам за текущий месяц и скорость. */
  usage: { month: "", servers: {}, speed: { up: 0, down: 0 } } as MonthTraffic,
};

const LIST_TITLE: Record<AppList | SiteList, string> = {
  proxy_apps: "Приложения через VPN",
  direct_apps: "Приложения напрямую",
  proxy_domains: "Сайты через VPN",
  direct_domains: "Сайты напрямую",
};

const root = document.getElementById("app")!;

// ---------- помощники ----------

const esc = (s: unknown) =>
  String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);

const icon = {
  power: (s = 20) => `<svg width="${s}" height="${s}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M12 3v9"/><path d="M17.66 6.34a8 8 0 1 1-11.32 0"/></svg>`,
  rules: `<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 6h10M4 12h16M4 18h7"/><circle cx="17" cy="6" r="2"/><circle cx="14" cy="18" r="2"/></svg>`,
  globe: `<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.7 3.8 5.7 3.8 9s-1.3 6.3-3.8 9c-2.5-2.7-3.8-5.7-3.8-9S9.5 5.7 12 3z"/></svg>`,
  gear: `<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M2 12h3M19 12h3M4.9 19.1L7 17M17 7l2.1-2.1"/></svg>`,
  plus: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>`,
  qr: `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><path d="M14 14h3v3h-3zM20 14v.01M14 20h.01M17 20h4v-3"/></svg>`,
  chevron: `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 15l6-6 6 6"/></svg>`,
  x: (s = 14) => `<svg width="${s}" height="${s}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg>`,
  refresh: `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 11a8 8 0 0 0-14.6-4.5L4 8M4 4v4h4M4 13a8 8 0 0 0 14.6 4.5L20 16M20 20v-4h-4"/></svg>`,
  info: `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true" style="flex-shrink:0;margin-top:1px"><circle cx="12" cy="12" r="9"/><path d="M12 11v5M12 8h.01"/></svg>`,
};

/** «🇳🇱 Нидерланды» → { code: "NL", name: "Нидерланды" } */
function splitFlag(name: string): { code: string; name: string } {
  const chars = Array.from(name.trim());
  const isRI = (c?: string) => !!c && c.codePointAt(0)! >= 0x1f1e6 && c.codePointAt(0)! <= 0x1f1ff;
  if (isRI(chars[0]) && isRI(chars[1])) {
    const code = String.fromCharCode(chars[0].codePointAt(0)! - 0x1f1e6 + 65, chars[1].codePointAt(0)! - 0x1f1e6 + 65);
    return { code, name: chars.slice(2).join("").trim() || code };
  }
  const letters = name.replace(/[^\p{L}]/gu, "");
  return { code: (letters.slice(0, 2) || "??").toUpperCase(), name };
}

const num = (x: number) => x.toLocaleString("ru-RU", { maximumFractionDigits: 1 });

function bytes(n: number): string {
  if (n >= 1024 ** 3) return `${num(n / 1024 ** 3)} ГБ`;
  if (n >= 1024 ** 2) return `${num(n / 1024 ** 2)} МБ`;
  return `${num(n / 1024)} КБ`;
}

function speed(n: number): string {
  return n >= 1024 * 1024 ? `${num(n / 1024 / 1024)} МБ/с` : `${num(n / 1024)} КБ/с`;
}

const MONTHS = ["январь", "февраль", "март", "апрель", "май", "июнь", "июль", "август", "сентябрь", "октябрь", "ноябрь", "декабрь"];
/** «2026-11» → «ноябрь» */
const monthName = (month: string) => MONTHS[Number(month.slice(5, 7)) - 1] ?? "этот месяц";

/** Расход сервера за месяц, «1,2 ГБ». Пусто, если трафика не было. */
function usageShort(id: string): string {
  const u = st.usage.servers[id];
  return u && u.up + u.down > 0 ? bytes(u.up + u.down) : "";
}

function usageView(id: string): string {
  return `<span class="usage" data-usage-id="${esc(id)}">${esc(usageShort(id))}</span>`;
}

function speedsView(): string {
  const s = st.usage.speed;
  return `<div class="speeds" id="speeds">
    <span class="speed down">↓ <span id="spd-down">${speed(s.down)}</span></span>
    <span class="speed up">↑ <span id="spd-up">${speed(s.up)}</span></span>
  </div>`;
}

/** Обновляем цифры на месте: перерисовка всего экрана раз в секунду ни к чему. */
async function refreshUsage() {
  try {
    st.usage = await invoke<MonthTraffic>("get_traffic");
  } catch {
    return;
  }
  document.querySelectorAll<HTMLElement>("[data-usage-id]").forEach((el) => (el.textContent = usageShort(el.dataset.usageId!)));
  const down = document.getElementById("spd-down");
  const up = document.getElementById("spd-up");
  if (down && up) {
    down.textContent = speed(st.usage.speed.down);
    up.textContent = speed(st.usage.speed.up);
  }
  const month = document.getElementById("usage-month");
  if (month) month.textContent = monthText();
}

/** «1,2 ГБ за ноябрь» - расход текущего сервера. */
function monthText(): string {
  const srv = selectedServer();
  const short = srv ? usageShort(srv.id) : "";
  return short ? `${short} за ${monthName(st.usage.month)}` : "";
}

function pingView(id: string): string {
  const p = st.pings[id];
  if (!p) return "";
  if (p.udp) return `<span class="ping muted">UDP</span>`;
  if (p.ms == null) return `<span class="ping bad">нет ответа</span>`;
  // Через VPN задержка складывается из двух плеч - порог выше.
  const good = p.via_vpn ? 300 : 120;
  const cls = p.ms <= good ? "good" : p.ms <= good * 2.5 ? "slow" : "bad";
  return `<span class="ping ${cls}">${p.ms} мс</span>`;
}

let toastTimer = 0;
function toast(text: string, error = false) {
  document.querySelector(".toast")?.remove();
  const el = document.createElement("div");
  el.className = `toast${error ? " error" : ""}`;
  el.setAttribute("role", "status");
  el.textContent = text;
  document.body.append(el);
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => el.remove(), error ? 6000 : 2500);
}

const errText = (e: unknown) => (typeof e === "string" ? e : (e as Error)?.message ?? String(e));

function selectedServer(): Server | undefined {
  const s = st.settings;
  return s.servers.find((x) => x.id === s.selected) ?? s.servers[0];
}

function mood(): Mood {
  if (st.state === "started") return "peek";
  return st.state === "stopped" && !st.busy ? "sleep" : "wink";
}

/** «https://www.Discord.com/app», «*.discord.com» → «discord.com». Кириллица остаётся как есть,
 *  в punycode её переводит Rust при сборке конфига. */
function normalizeDomain(raw: string): string | null {
  const s = raw.trim().toLowerCase().replace(/^[a-z]+:\/\//, "").replace(/[/?#].*$/, "").replace(/:\d+$/, "")
    .replace(/^\*?\./, "").replace(/^www\./, "");
  if (!s || !s.includes(".")) return null;
  try {
    const host = new URL(`http://${s}`).hostname;
    return /^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(host) ? s : null;
  } catch {
    return null;
  }
}

/** Что добавится из поля сайтов: без дубликатов и без уже добавленных в этот список. */
function siteStats(list: SiteList, text: string): { add: string[]; dup: number } {
  const existing = new Set(st.settings.rules[list]);
  const seen = new Set<string>();
  const add: string[] = [];
  let dup = 0;
  for (const part of text.split(/[\s,;]+/)) {
    const d = normalizeDomain(part);
    if (!d) continue;
    if (seen.has(d) || existing.has(d)) {
      dup++;
      continue;
    }
    seen.add(d);
    add.push(d);
  }
  return { add, dup };
}

function statsText(x: { add: string[]; dup: number }): string {
  if (!x.add.length && !x.dup) return "Вставьте адреса сайтов - через пробел, запятую или с новой строки";
  return `Будет добавлено: ${x.add.length}` + (x.dup ? ` · дубликатов пропущено: ${x.dup}` : "");
}

/** «ключ «Германия»», «3 ключа», «подписка с panel.example.org» - без самих секретов. */
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
    // Кривое имя - покажем без него.
  }
  return name ? `ключ «${name}»` : `ключ ${lines[0].split("://")[0]}://…`;
}

/** Кнопка удаления срабатывает со второго нажатия, чтобы не удалить случайно. */
const armed = new Set<string>();

function deleteButton(key: string, title: string): string {
  const on = armed.has(key);
  return `<button class="x-btn${on ? " armed" : ""}" data-action="delete" data-key="${esc(key)}" aria-label="${esc(title)}">${
    on ? "Удалить?" : icon.x()
  }</button>`;
}

function appIcon(pkg: string, name: string): string {
  const src = st.apps?.find((a) => a.package === pkg)?.icon;
  return src
    ? `<img class="app-icon" src="${src}" alt="">`
    : `<div class="app-icon">${esc(name.charAt(0).toUpperCase())}</div>`;
}

// ---------- страницы ----------

/** Заголовок сворачиваемой карточки; в свёрнутом виде рядом короткое состояние. */
function foldHead(card: Fold, title: string, summary = ""): string {
  const folded = st.folded[card];
  return `<button class="fold-head" data-action="fold" data-fold="${card}" aria-expanded="${!folded}">
    <h2>${title}${folded && summary ? ` <span class="count">· ${esc(summary)}</span>` : ""}</h2>
    <span class="chevron">${icon.chevron}</span>
  </button>`;
}

function serverRow(x: Server, removable: boolean): string {
  const f = splitFlag(x.name);
  const on = x.id === selectedServer()?.id;
  return `<div class="server${on ? " on" : ""}">
    <button class="server-pick" data-action="select" data-id="${esc(x.id)}" aria-pressed="${on}">
      <span class="flag">${esc(f.code)}</span><span class="name">${esc(f.name)}</span>${usageView(x.id)}${pingView(x.id)}
    </button>
    ${removable ? deleteButton(`server:${x.id}`, `Удалить ${x.name}`) : ""}
  </div>`;
}

function homeView(): string {
  const s = st.settings;
  const srv = selectedServer();
  const on = st.state === "started";
  const working = st.busy || st.state === "starting" || st.state === "stopping";
  const title = st.state === "starting" ? "Подключаемся…" : st.state === "stopping" ? "Отключаемся…" : on ? "Подключено" : "Не подключено";
  const flag = srv ? splitFlag(srv.name) : null;
  const sub = srv
    ? `<span class="flag">${esc(flag!.code)}</span><span class="cur-name">${esc(flag!.name)}</span>${st.pings[srv.id] ? pingView(srv.id) : ""}`
    : "Добавьте сервер, чтобы подключиться";
  const servers = s.servers.length
    ? s.servers.map((x) => serverRow(x, false)).join("")
    : `<div class="empty">Серверов пока нет. Вставьте ключ <span class="mono">vless://</span>, <span class="mono">hy2://</span> или ссылку на подписку - их присылает тот, кто дал вам VPN.</div>`;
  const modeOpt = (m: Mode, t: string) =>
    `<button class="${s.mode === m ? "on" : ""}" data-action="set-mode" data-mode="${m}" aria-pressed="${s.mode === m}">${t}</button>`;
  return `<header class="brand">${mascot(40, mood())}<span>ShredingerBox</span></header>
    <section class="home-center">
      <button class="power ${working ? "busy" : on ? "on" : ""}" data-action="toggle" aria-label="${st.state === "stopped" ? "Подключиться" : "Отключиться"}"
        ${st.state === "stopping" || (st.state === "stopped" && !s.servers.length) ? "disabled" : ""}>
        <span class="inner">${icon.power(60)}</span>
      </button>
      <div class="status-title">${title}</div>
      <div class="status-sub">${sub}</div>
      <div class="status-month" id="usage-month">${esc(monthText())}</div>
      ${on ? speedsView() : ""}
    </section>
    <section class="card fold${st.folded.mode ? " folded" : ""}">
      ${foldHead("mode", "Режим", s.mode === "tun" ? "TUN" : "Прокси")}
      ${st.folded.mode ? "" : `
      <div class="seg" role="group" aria-label="Режим">${modeOpt("tun", "TUN")}${modeOpt("proxy", "Прокси")}</div>
      ${s.mode === "tun"
        ? `<p class="card-hint">VPN для всего телефона - по правилам.</p>`
        : `<div class="hint">${icon.info}<span>Без VPN: через прокси <b class="mono">${PROXY}</b> ходят только приложения, где он указан, - например, Telegram → Данные и память → Прокси (SOCKS5 или HTTP), или настройки Wi-Fi. Остальные идут напрямую.</span></div>`}`}
    </section>
    <section class="card">
      <div class="card-head"><h2>Серверы</h2>${s.servers.length ? `<button class="link-btn" data-action="ping" ${st.pinging ? "disabled" : ""}>${st.pinging ? "Меряем…" : "Проверить пинг"}</button>` : ""}</div>
      <div class="server-list">${servers}</div>
      <button class="dashed" data-action="open-add-server">${icon.plus}Добавить подписку или ключ</button>
    </section>`;
}

function serversView(): string {
  const s = st.settings;
  const host = (url: string) => {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  };
  const groups = s.subscriptions.map((sub) => {
    const list = s.servers.filter((x) => x.subscription === sub.url);
    return `<section class="card">
      <div class="card-head">
        <div class="sub-title">
          <h2>${esc(sub.name)} <span class="count">· ${list.length}</span></h2>
          <span class="muted mono">${esc(host(sub.url))}</span>
        </div>
        <div class="row">
          <button class="x-btn" data-action="refresh-subs" aria-label="Обновить подписки">${icon.refresh}</button>
          ${deleteButton(`sub:${sub.url}`, `Удалить подписку ${sub.name}`)}
        </div>
      </div>
      <div class="server-list">${list.map((x) => serverRow(x, false)).join("") || `<div class="empty">В подписке нет серверов</div>`}</div>
    </section>`;
  }).join("");
  const manual = s.servers.filter((x) => !x.subscription);
  const manualCard = manual.length
    ? `<section class="card"><div class="card-head"><h2>Добавленные вручную <span class="count">· ${manual.length}</span></h2></div>
        <div class="server-list">${manual.map((x) => serverRow(x, true)).join("")}</div></section>`
    : "";
  const empty = !s.servers.length ? `<section class="card"><div class="empty">Серверов пока нет. Вставьте ключ или ссылку на подписку.</div></section>` : "";
  return `<div class="page-head">
      <h1>Серверы</h1>
      <p>Пока VPN включён, пинг меряется через туннель, а переключение мгновенное.
        Рядом с сервером - расход трафика за ${esc(monthName(st.usage.month))}.</p>
      <div class="row">
        ${s.servers.length ? `<button class="btn-ghost" data-action="ping" ${st.pinging ? "disabled" : ""}>${st.pinging ? "Меряем…" : "Проверить пинг"}</button>` : ""}
        <button class="btn-light" data-action="open-add-server">Добавить</button>
      </div>
    </div>
    ${empty}${groups}${manualCard}`;
}

function ruleRows(list: AppList | SiteList): string {
  const r = st.settings.rules;
  const rows = list === "proxy_apps" || list === "direct_apps"
    ? r[list].map((a) => `<div class="app-row">${appIcon(a.package, a.name)}
        <span class="n">${esc(a.name)}<small>${esc(a.package)}</small></span>
        <button class="x-btn" data-action="remove-app" data-list="${list}" data-package="${esc(a.package)}" aria-label="Убрать ${esc(a.name)}">${icon.x()}</button></div>`)
    : r[list].map((d) => `<div class="app-row"><div class="app-icon">${esc(d.charAt(0).toUpperCase())}</div>
        <span class="n">${esc(d)}</span>
        <button class="x-btn" data-action="remove-domain" data-list="${list}" data-domain="${esc(d)}" aria-label="Убрать ${esc(d)}">${icon.x()}</button></div>`);
  return rows.join("") || `<div class="empty small">Пока пусто</div>`;
}

function appCard(list: AppList, hollow: boolean, hint: string): string {
  return `<section class="card tight">
    <div class="card-head"><div class="row"><span class="dot ${hollow ? "hollow" : ""}"></span><h2>${LIST_TITLE[list]}</h2></div><span class="count">${st.settings.rules[list].length}</span></div>
    <p class="card-hint">${hint}</p>
    ${ruleRows(list)}
    <button class="dashed" data-action="open-add-app" data-list="${list}">${icon.plus}Добавить приложение</button>
  </section>`;
}

function rulesView(): string {
  const r = st.settings.rules;
  const tab = st.siteTab;
  const siteTab = (list: SiteList, label: string) =>
    `<button role="tab" aria-selected="${tab === list}" class="${tab === list ? "on" : ""}" data-action="site-tab" data-list="${list}">${label} · ${r[list].length}</button>`;
  const presets = st.presets.map((p) => {
    const on = r.presets.includes(p.id);
    return `<button class="preset ${on ? "on" : ""}" data-action="toggle-preset" data-id="${p.id}" aria-pressed="${on}">${on ? "✓ " : ""}${esc(p.title)}</button>`;
  }).join("");
  return `<div class="page-head">
      <h1>Правила</h1>
      <p>Что идёт через VPN, а что напрямую</p>
    </div>
    ${st.settings.mode === "proxy"
      ? `<div class="hint warn">${icon.info}<span>Сейчас режим «Прокси»: правила для приложений не действуют - без VPN Android не говорит, какое приложение открыло соединение. Правила для сайтов работают.</span></div>`
      : ""}
    <section class="card">
      <h2>Остальной трафик</h2>
      <div class="seg" role="group" aria-label="Остальной трафик">
        <button class="${r.final_route === "proxy" ? "on" : ""}" data-action="set-final" data-value="proxy">Через VPN</button>
        <button class="${r.final_route === "direct" ? "on" : ""}" data-action="set-final" data-value="direct">Напрямую</button>
      </div>
      <p class="card-hint">${r.final_route === "proxy"
        ? "Через VPN идёт всё, кроме того, что ниже отмечено «напрямую»."
        : "Через VPN идёт только то, что указано ниже, - остальное напрямую, как без VPN."}</p>
    </section>
    ${appCard("proxy_apps", false, "Весь трафик этих приложений идёт через VPN.")}
    ${appCard("direct_apps", true, "Эти приложения ходят мимо VPN и не видят его - удобно для банков.")}
    <section class="card tight">
      <div class="card-head"><div class="row"><span class="dot ${tab === "direct_domains" ? "hollow" : ""}"></span><h2>Сайты</h2></div></div>
      <div class="seg inset" role="tablist" aria-label="Какие сайты">${siteTab("proxy_domains", "Через VPN")}${siteTab("direct_domains", "Напрямую")}</div>
      ${ruleRows(tab)}
      <button class="dashed" data-action="open-add-sites">${icon.plus}Добавить сайты</button>
      ${tab === "proxy_domains"
        ? `<div class="sep"></div>
          <span class="card-hint">Готовые наборы - нажмите, чтобы включить</span>
          <div class="chips">${presets}</div>`
        : `<div class="hint">${icon.info}<span>Эти сайты всегда идут напрямую - даже из приложений «через VPN» и готовых наборов. Удобно для банков и госуслуг.</span></div>`}
    </section>`;
}

function mb(bytes: number): string {
  return `${(bytes / 1024 / 1024).toFixed(1)} МБ`;
}

function settingsView(): string {
  const u = st.update;
  const updateLine = st.checkingUpdate
    ? "Проверяем…"
    : u
      ? `Доступна версия ${esc(u.version)}`
      : u === false
        ? "Установлена последняя версия"
        : "Новые версии выходят на GitHub";
  return `<div class="page-head"><h1>Настройки</h1></div>
    <section class="card">
      <div class="card-head">
        <div class="sub-title"><h2>ShredingerBox v${esc(__APP_VERSION__)}</h2><span class="muted">${updateLine}</span></div>
        ${u
          ? `<button class="btn-accent small" data-action="open-update">Обновить</button>`
          : `<button class="btn-ghost" data-action="check-update" ${st.checkingUpdate ? "disabled" : ""}>Проверить</button>`}
      </div>
    </section>
    <section class="card">
      <h2>Запуск</h2>
      <div class="set-row">
        <div class="t"><b>При включении телефона</b><span>Подключаться сразу после перезагрузки</span></div>
        <button class="switch ${st.settings.autostart ? "on" : ""}" data-action="toggle-autostart" role="switch" aria-checked="${st.settings.autostart}" aria-label="Подключаться при включении телефона"><span></span></button>
      </div>
      <div class="set-row">
        <div class="t"><b>Плитка в шторке</b><span>Включать VPN, не открывая приложение</span></div>
        <button class="btn-ghost" data-action="add-tile">Добавить</button>
      </div>
      <div class="set-row">
        <div class="t"><b>Постоянная VPN</b><span>Android сам держит VPN включённым и не пускает трафик мимо него</span></div>
        <button class="btn-ghost" data-action="vpn-settings">Открыть</button>
      </div>
    </section>
    <section class="card fold${st.folded.log ? " folded" : ""}">
      ${foldHead("log", "Журнал ядра")}
      ${st.folded.log ? "" : `
      <div class="row log-bar">
        <p class="card-hint">Записи sing-box - пригодятся, если что-то не работает.</p>
        <button class="x-btn" data-action="log-refresh" aria-label="Обновить журнал">${icon.refresh}</button>
        <button class="link-btn" data-action="log-copy">Скопировать</button>
      </div>
      <pre class="code" id="log-text">${esc(st.log || "Журнал пуст - подключитесь, и здесь появятся записи ядра.")}</pre>`}
    </section>`;
}

// ---------- окна ----------

function pickerRows(m: Extract<Modal, { kind: "add-app" }>): string {
  if (!st.apps) return `<div class="empty">Загружаем список приложений…</div>`;
  const taken = new Set(st.settings.rules[m.list].map((a) => a.package));
  const other: AppList = m.list === "proxy_apps" ? "direct_apps" : "proxy_apps";
  const inOther = new Set(st.settings.rules[other].map((a) => a.package));
  const q = m.filter.trim().toLowerCase();
  const rows = st.apps
    .filter((a) => !taken.has(a.package))
    .filter((a) => !q || a.name.toLowerCase().includes(q) || a.package.includes(q))
    .map((a) => `<label class="proc">
      <input type="checkbox" data-pick="${esc(a.package)}" ${m.picked.has(a.package) ? "checked" : ""}>
      ${appIcon(a.package, a.name)}
      <span class="n">${esc(a.name)}<small>${inOther.has(a.package) ? `сейчас ${other === "direct_apps" ? "напрямую" : "через VPN"} · ` : ""}${esc(a.package)}</small></span>
    </label>`);
  return rows.join("") || `<div class="empty">Ничего не нашлось</div>`;
}

function modalView(): string {
  const m = st.modal;
  if (!m) return "";
  const head = (title: string) =>
    `<div class="modal-head"><h1>${title}</h1><button class="x-btn" data-action="close-modal" aria-label="Закрыть">${icon.x(16)}</button></div>`;
  let body = "";
  if (m.kind === "add-server") {
    body = `${head("Добавить серверы")}
      <div class="modal-body">
        <textarea class="field" id="modal-text" rows="5" spellcheck="false" autocomplete="off"
          placeholder="Ключи vless://, hy2://, trojan://, ss:// - по одному на строку, или ссылка на подписку https://…">${esc(m.text)}</textarea>
        <button class="dashed" data-action="scan-qr">${icon.qr}Сканировать QR-код</button>
      </div>
      <div class="modal-foot"><button class="btn-ghost" data-action="close-modal">Отмена</button><span class="grow"></span>
        <button class="btn-accent" data-action="submit-server" ${m.busy ? "disabled" : ""}>${m.busy ? "Добавляем…" : "Добавить"}</button></div>`;
  } else if (m.kind === "clipboard") {
    body = `${head(m.from === "link" ? "Добавить по ссылке?" : "Добавить из буфера обмена?")}
      <div class="modal-body"><p class="lead">${m.from === "link" ? "Это" : "В буфере обмена"} ${esc(clipSummary(m.text))}.</p></div>
      <div class="modal-foot"><button class="btn-ghost" data-action="dismiss-clipboard">Не сейчас</button><span class="grow"></span>
        <button class="btn-accent" data-action="accept-clipboard" ${m.busy ? "disabled" : ""}>${m.busy ? "Добавляем…" : "Добавить"}</button></div>`;
  } else if (m.kind === "update") {
    const i = m.info;
    body = `${head(`Доступна версия ${esc(i.version)}`)}
      <div class="modal-body">
        <div class="row">${mascot(56, "wink")}<p class="lead">Сейчас установлена ${esc(i.current)}.${i.size ? ` Скачается ${mb(i.size)}.` : ""}
          После загрузки Android попросит подтвердить установку.</p></div>
        ${i.notes ? notesHtml(i.notes) : ""}
      </div>
      <div class="modal-foot"><button class="btn-ghost" data-action="snooze-update" ${m.busy ? "disabled" : ""}>Позже</button><span class="grow"></span>
        <button class="btn-accent" data-action="install-update" ${m.busy ? "disabled" : ""}>${m.busy ? "Скачиваем…" : "Обновить"}</button></div>`;
  } else if (m.kind === "whats-new") {
    body = `${head(`Что нового в ${esc(m.version)}`)}
      <div class="modal-body">
        <div class="row">${mascot(56, "peek")}<p class="lead">Приложение обновилось. Вот что изменилось:</p></div>
        ${notesHtml(m.text)}
      </div>
      <div class="modal-foot"><span class="grow"></span><button class="btn-accent" data-action="close-modal">Понятно</button></div>`;
  } else if (m.kind === "add-app") {
    body = `${head(LIST_TITLE[m.list])}
      <div class="modal-body">
        <input class="field" id="app-filter" type="search" placeholder="Поиск по названию" value="${esc(m.filter)}" autocomplete="off">
        <div class="pick-list" id="pick-list">${pickerRows(m)}</div>
      </div>
      <div class="modal-foot"><button class="btn-ghost" data-action="close-modal">Отмена</button><span class="grow"></span>
        <button class="btn-accent" id="pick-submit" data-action="submit-apps" ${m.picked.size ? "" : "disabled"}>Добавить${m.picked.size ? ` (${m.picked.size})` : ""}</button></div>`;
  } else {
    body = `${head(LIST_TITLE[m.list])}
      <div class="modal-body">
        <textarea class="field" id="modal-text" rows="5" spellcheck="false" autocomplete="off" autocapitalize="off"
          placeholder="example.com, discord.gg - можно сразу списком">${esc(m.text)}</textarea>
        <span class="card-hint" id="site-stats">${esc(statsText(siteStats(m.list, m.text)))}</span>
      </div>
      <div class="modal-foot"><button class="btn-ghost" data-action="close-modal">Отмена</button><span class="grow"></span>
        <button class="btn-accent" data-action="submit-sites">Добавить</button></div>`;
  }
  return `<div class="overlay" data-action="overlay"><div class="modal" role="dialog">${body}</div></div>`;
}

// ---------- отрисовка ----------

function render() {
  const page = { home: homeView, servers: serversView, rules: rulesView, settings: settingsView }[st.view]();
  const tab = (v: View, label: string, ic: string) =>
    `<button class="tab ${st.view === v ? "on" : ""}" data-action="nav" data-view="${v}" aria-current="${st.view === v ? "page" : "false"}">${ic}<span>${label}</span></button>`;
  // Опрос статуса и пинг перерисовывают всё - поле ввода в окне не должно терять фокус.
  const active = document.activeElement as HTMLInputElement | null;
  const focusId = active?.id && root.contains(active) ? active.id : "";
  const caret = focusId ? active!.selectionStart : null;
  root.innerHTML = `<main class="view">${page}</main>
    <nav class="tabs" aria-label="Разделы">
      ${tab("home", "Главная", icon.power(22))}
      ${tab("servers", "Серверы", icon.globe)}
      ${tab("rules", "Правила", icon.rules)}
      ${tab("settings", "Настройки", icon.gear)}
    </nav>
    ${modalView()}`;
  document.body.classList.toggle("locked", !!st.modal);
  const field = focusId ? (document.getElementById(focusId) as HTMLInputElement | null) : null;
  if (field) {
    field.focus();
    if (caret != null) {
      try {
        field.setSelectionRange(caret, caret);
      } catch {
        // У кнопок и чекбоксов курсора нет.
      }
    }
  }
}

/** Общая обёртка для действий: блокирует кнопку подключения, показывает ошибку. */
async function act(fn: () => Promise<void>) {
  st.busy = true;
  render();
  try {
    await fn();
  } catch (e) {
    toast(errText(e), true);
  } finally {
    st.busy = false;
    render();
  }
}

async function pingAll() {
  if (st.pinging || !st.settings.servers.length) return;
  st.pinging = true;
  render();
  try {
    const list = await invoke<Ping[]>("ping_servers");
    st.pings = Object.fromEntries(list.map((p) => [p.id, p]));
  } catch {
    // Пинг - подсказка, а не действие: ошибку не показываем.
  } finally {
    st.pinging = false;
    render();
  }
}

async function poll() {
  try {
    const s = await invoke<Status>("vpn_status");
    if (s.state !== st.state && !st.busy) {
      st.state = s.state;
      if (st.state === "stopped" && s.error) toast(s.error, true);
      render();
      // После подключения пинг меряется уже через VPN - реальная задержка.
      if (st.state === "started" || st.state === "stopped") setTimeout(pingAll, 1500);
    }
    // Пока подключены, счётчик в Rust растёт каждую секунду.
    if (st.state === "started") refreshUsage();
  } catch (e) {
    toast(errText(e), true);
  }
}

async function loadApps() {
  if (st.apps) return;
  try {
    st.apps = await invoke<AppInfo[]>("list_apps");
  } catch (e) {
    st.apps = [];
    toast(errText(e), true);
  }
  render();
}

/** Сохранить правила. Если VPN включён, Rust переподключит его с новым конфигом. */
function saveRules(change: (r: Rules) => void) {
  const rules: Rules = structuredClone(st.settings.rules);
  change(rules);
  st.settings.rules = rules;
  const restart = st.state !== "stopped";
  act(async () => {
    try {
      st.settings = await invoke<Settings>("set_rules", { rules });
      if (restart) toast("Правила применены");
    } catch (e) {
      st.settings = await invoke<Settings>("get_settings");
      throw e;
    }
  });
}

async function loadLog() {
  try {
    st.log = await invoke<string>("vpn_log");
  } catch (e) {
    st.log = errText(e);
  }
  render();
  const pre = document.getElementById("log-text");
  if (pre) pre.scrollTop = pre.scrollHeight;
}

async function addFromText(text: string) {
  st.settings = await invoke<Settings>("add_from_text", { text });
  pingAll();
}

// ---------- действия ----------

async function onAction(action: string, el: HTMLElement, ev: Event) {
  const s = st.settings;
  switch (action) {
    case "nav":
      st.view = el.dataset.view as View;
      window.scrollTo(0, 0);
      render();
      if (st.view === "rules") loadApps();
      if (st.view === "settings" && !st.folded.log) loadLog();
      return;
    case "toggle":
      return act(async () => {
        if (st.state === "stopped") {
          // В режиме «Прокси» VPN не поднимается - разрешение не нужно.
          if (st.settings.mode === "tun" && !(await invoke<boolean>("vpn_prepare"))) throw "Без разрешения на VPN подключиться нельзя";
          await invoke("vpn_connect");
          st.state = "starting";
        } else {
          await invoke("vpn_disconnect");
          st.state = "stopping";
        }
      });
    case "select": {
      const id = el.dataset.id!;
      if (id === selectedServer()?.id) return;
      // Сразу отмечаем выбор, не дожидаясь ответа ядра.
      s.selected = id;
      return act(async () => {
        st.settings = await invoke<Settings>("select_server", { id });
      });
    }
    case "ping":
      return pingAll();
    case "delete": {
      const key = el.dataset.key!;
      if (!armed.has(key)) {
        armed.add(key);
        setTimeout(() => {
          armed.delete(key);
          render();
        }, 3000);
        return render();
      }
      armed.delete(key);
      const i = key.indexOf(":");
      const [kind, value] = [key.slice(0, i), key.slice(i + 1)];
      return act(async () => {
        st.settings = kind === "sub"
          ? await invoke<Settings>("remove_subscription", { url: value })
          : await invoke<Settings>("remove_server", { id: value });
      });
    }
    case "refresh-subs":
      return act(async () => {
        try {
          st.settings = await invoke<Settings>("refresh_subscriptions");
          toast("Подписки обновлены");
        } catch (e) {
          // Часть подписок могла обновиться - перечитываем то, что сохранилось.
          st.settings = await invoke<Settings>("get_settings");
          throw e;
        } finally {
          pingAll();
        }
      });
    case "open-add-server":
      st.modal = { kind: "add-server", text: "", busy: false };
      render();
      document.getElementById("modal-text")?.focus();
      return;
    case "submit-server": {
      const m = st.modal as Extract<Modal, { kind: "add-server" }>;
      const text = m.text.trim();
      if (!text) return toast("Вставьте ключ или ссылку на подписку", true);
      m.busy = true;
      render();
      try {
        await addFromText(text);
        st.modal = null;
        toast("Добавлено");
      } catch (e) {
        m.busy = false;
        toast(errText(e), true);
      }
      return render();
    }
    case "accept-clipboard": {
      const m = st.modal as Extract<Modal, { kind: "clipboard" }>;
      m.busy = true;
      render();
      try {
        await addFromText(m.text);
        st.modal = null;
        toast("Добавлено");
      } catch (e) {
        clipDismissed = m.text;
        st.modal = null;
        toast(errText(e), true);
      }
      return render();
    }
    case "dismiss-clipboard":
      if (st.modal?.kind === "clipboard") clipDismissed = st.modal.text;
      st.modal = null;
      return render();
    case "overlay":
      if (ev.target !== el) return;
    // fallthrough: нажатие мимо окна закрывает его
    case "close-modal": {
      const was = st.modal?.kind;
      if (st.modal?.kind === "clipboard") clipDismissed = st.modal.text;
      st.modal = null;
      render();
      // Пока висело «Что нового», могли прийти ссылка или ключ в буфере.
      if (was === "whats-new") onReturn();
      return;
    }
    case "set-final":
      if (s.rules.final_route === el.dataset.value) return;
      return saveRules((r) => (r.final_route = el.dataset.value as FinalRoute));
    case "site-tab":
      st.siteTab = el.dataset.list as SiteList;
      return render();
    case "toggle-preset": {
      const id = el.dataset.id!;
      return saveRules((r) => {
        r.presets = r.presets.includes(id) ? r.presets.filter((x) => x !== id) : [...r.presets, id];
      });
    }
    case "remove-app": {
      const list = el.dataset.list as AppList;
      return saveRules((r) => (r[list] = r[list].filter((a) => a.package !== el.dataset.package)));
    }
    case "remove-domain": {
      const list = el.dataset.list as SiteList;
      return saveRules((r) => (r[list] = r[list].filter((d) => d !== el.dataset.domain)));
    }
    case "open-add-app":
      st.modal = { kind: "add-app", list: el.dataset.list as AppList, filter: "", picked: new Set() };
      render();
      return loadApps();
    case "submit-apps": {
      const m = st.modal as Extract<Modal, { kind: "add-app" }>;
      const other: AppList = m.list === "proxy_apps" ? "direct_apps" : "proxy_apps";
      const add = (st.apps ?? []).filter((a) => m.picked.has(a.package)).map((a) => ({ name: a.name, package: a.package }));
      st.modal = null;
      // Приложение может быть только в одном списке - из другого его убираем.
      return saveRules((r) => {
        r[m.list] = [...r[m.list], ...add];
        r[other] = r[other].filter((a) => !m.picked.has(a.package));
      });
    }
    case "open-add-sites":
      st.modal = { kind: "add-sites", list: st.siteTab, text: "" };
      render();
      document.getElementById("modal-text")?.focus();
      return;
    case "submit-sites": {
      const m = st.modal as Extract<Modal, { kind: "add-sites" }>;
      const { add } = siteStats(m.list, m.text);
      if (!add.length) return toast("Не нашлось ни одного нового адреса сайта", true);
      st.modal = null;
      return saveRules((r) => (r[m.list] = [...r[m.list], ...add]));
    }
    case "scan-qr": {
      const m = st.modal;
      if (m?.kind !== "add-server") return;
      try {
        const text = await invoke<string | null>("scan_qr");
        if (!text) return;
        // Код с ключом или подпиской - сразу в поле; добавить можно одной кнопкой.
        m.text = m.text.trim() ? `${m.text.trim()}\n${text.trim()}` : text.trim();
        render();
      } catch (e) {
        toast(errText(e), true);
      }
      return;
    }
    case "toggle-autostart": {
      const on = !s.autostart;
      s.autostart = on;
      render();
      try {
        st.settings = await invoke<Settings>("set_autostart", { on });
      } catch (e) {
        s.autostart = !on;
        toast(errText(e), true);
      }
      return render();
    }
    case "add-tile":
      try {
        if (!(await invoke<boolean>("add_tile"))) {
          toast("Откройте шторку, нажмите карандаш и перетащите плитку ShredingerBox наверх");
        }
      } catch (e) {
        toast(errText(e), true);
      }
      return;
    case "vpn-settings":
      try {
        await invoke("open_vpn_settings");
        toast("Нажмите шестерёнку у ShredingerBox и включите «Постоянная VPN»");
      } catch (e) {
        toast(errText(e), true);
      }
      return;
    case "fold": {
      const card = el.dataset.fold as Fold;
      st.folded[card] = !st.folded[card];
      try {
        localStorage.setItem(FOLD_KEYS[card], st.folded[card] ? "1" : "");
      } catch {
        // Не запомнится - не страшно.
      }
      render();
      if (card === "log" && !st.folded.log) loadLog();
      return;
    }
    case "set-mode": {
      const mode = el.dataset.mode as Mode;
      if (mode === s.mode) return;
      if (mode === "tun" && st.state !== "stopped" && !(await invoke<boolean>("vpn_prepare"))) {
        return toast("Без разрешения на VPN режим TUN не включить", true);
      }
      s.mode = mode;
      return act(async () => {
        try {
          st.settings = await invoke<Settings>("set_mode", { mode });
        } catch (e) {
          st.settings = await invoke<Settings>("get_settings");
          throw e;
        }
      });
    }
    case "check-update":
      return checkUpdate(true);
    case "open-update":
      if (st.update) st.modal = { kind: "update", info: st.update, busy: false };
      return render();
    case "snooze-update":
      if (st.modal?.kind === "update") updateSnoozed = st.modal.info.version;
      st.modal = null;
      return render();
    case "install-update": {
      const m = st.modal as Extract<Modal, { kind: "update" }>;
      m.busy = true;
      render();
      try {
        await invoke("install_update", { url: m.info.url });
        st.modal = null;
      } catch (e) {
        m.busy = false;
        toast(errText(e), true);
      }
      return render();
    }
    case "log-refresh":
      return loadLog();
    case "log-copy":
      try {
        await navigator.clipboard.writeText(st.log);
        toast("Журнал скопирован");
      } catch {
        toast("Не удалось скопировать", true);
      }
      return;
  }
}

root.addEventListener("click", (ev) => {
  const el = (ev.target as HTMLElement).closest<HTMLElement>("[data-action]");
  if (el && !(el as HTMLButtonElement).disabled) onAction(el.dataset.action!, el, ev);
});

// Поля в окнах: значение храним в состоянии и перерисовываем только то, что от него зависит,
// иначе полная перерисовка сбросит фокус и клавиатуру.
root.addEventListener("input", (ev) => {
  const t = ev.target as HTMLInputElement;
  const m = st.modal;
  if (!m) return;
  if (t.id === "modal-text" && (m.kind === "add-server" || m.kind === "add-sites")) {
    m.text = t.value;
    if (m.kind === "add-sites") document.getElementById("site-stats")!.textContent = statsText(siteStats(m.list, m.text));
  } else if (t.id === "app-filter" && m.kind === "add-app") {
    m.filter = t.value;
    document.getElementById("pick-list")!.innerHTML = pickerRows(m);
  }
});

root.addEventListener("change", (ev) => {
  const t = ev.target as HTMLInputElement;
  const m = st.modal;
  if (!t.dataset.pick || m?.kind !== "add-app") return;
  if (t.checked) m.picked.add(t.dataset.pick);
  else m.picked.delete(t.dataset.pick);
  const btn = document.getElementById("pick-submit") as HTMLButtonElement;
  btn.disabled = !m.picked.size;
  btn.textContent = m.picked.size ? `Добавить (${m.picked.size})` : "Добавить";
});

// ---------- обновления ----------

/** Версия, от которой отказались «Позже», - до перезапуска больше не предлагаем. */
let updateSnoozed = "";

const SEEN_KEY = "shredingerbox.seenVersion";

/**
 * После обновления показать, что изменилось в этой версии. Версию, которую уже видели,
 * помним на телефоне. При первой установке окна нет: показывать ещё нечего.
 */
function checkWhatsNew() {
  let seen: string | null = null;
  try {
    seen = localStorage.getItem(SEEN_KEY);
    localStorage.setItem(SEEN_KEY, __APP_VERSION__);
  } catch {
    return;
  }
  // До 0.4.2 версию не запоминали - обновившихся узнаём по уже добавленным серверам.
  const updated = seen ? seen !== __APP_VERSION__ : st.settings.servers.length > 0;
  const text = changesOf(__APP_VERSION__);
  if (!updated || !text || st.modal) return;
  st.modal = { kind: "whats-new", version: __APP_VERSION__, text };
  render();
}

async function checkUpdate(manual = false) {
  if (st.checkingUpdate) return;
  st.checkingUpdate = true;
  render();
  try {
    const info = await invoke<UpdateInfo | null>("check_update");
    st.update = info ?? false;
    if (info && (manual || info.version !== updateSnoozed) && !st.modal) {
      st.modal = { kind: "update", info, busy: false };
    }
  } catch (e) {
    // Молча при запуске: нет сети - не повод для ошибки.
    if (manual) toast(errText(e), true);
  } finally {
    st.checkingUpdate = false;
    render();
  }
}

// ---------- буфер обмена ----------

/** Текст из буфера, который уже отклонили, - второй раз не предлагаем. */
let clipDismissed = "";

async function checkClipboard() {
  if (document.visibilityState !== "visible" || st.modal) return;
  try {
    const text = await invoke<string | null>("clipboard_candidate");
    if (!text || text === clipDismissed || st.modal) return;
    st.modal = { kind: "clipboard", text, busy: false, from: "clipboard" };
    render();
  } catch {
    // Буфер недоступен - молча пропускаем.
  }
}

/** Приложение открыли ссылкой shredingerbox://add?url=… - предлагаем добавить. */
async function checkLink() {
  try {
    const text = await invoke<string | null>("take_link");
    if (!text) return;
    st.modal = { kind: "clipboard", text, busy: false, from: "link" };
    render();
  } catch {
    // Нет ссылки - нечего предлагать.
  }
}

/** Ссылка важнее буфера обмена: её открыли нарочно. */
async function onReturn() {
  // Открытое окно не перебиваем: ссылка ждёт в Android, пока его не закроют.
  if (document.visibilityState !== "visible" || st.modal) return;
  await checkLink();
  await checkClipboard();
}

// Буфер читается только когда окно в фокусе - проверяем при каждом возвращении в приложение.
window.addEventListener("focus", onReturn);
document.addEventListener("visibilitychange", onReturn);

// ---------- запуск ----------

/** Первая версия хранила один ключ в localStorage - переносим его в настройки. */
async function migrateOldKey() {
  let key = "";
  try {
    key = localStorage.getItem("shredingerbox.key") ?? "";
  } catch {
    return;
  }
  if (!key) return;
  if (!st.settings.servers.length) {
    try {
      st.settings = await invoke<Settings>("add_from_text", { text: key });
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
  // В обычном браузере (npm run dev) Tauri нет - подставляем демо-данные.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) await import("./mock");
  try {
    st.settings = await invoke<Settings>("get_settings");
    st.presets = await invoke<Preset[]>("get_presets");
    await migrateOldKey();
  } catch (e) {
    toast(errText(e), true);
  }
  render();
  poll();
  setInterval(poll, 1000);
  pingAll();
  refreshUsage();
  // Сразу после запуска окно может ещё не получить фокус - буфер вернёт пусто.
  // Сначала «Что нового»: буфер обмена и ссылка подождут, пока окно закроют.
  checkWhatsNew();
  setTimeout(onReturn, 700);
  // Буфер обмена важнее: обновление предложим, только если окно ещё свободно.
  setTimeout(checkUpdate, 2500);
})();
