// Подставные команды Tauri, чтобы смотреть экран в обычном браузере (npm run dev).
// В APK не попадает: main.ts подключает этот модуль только в режиме разработки.
import { mockIPC } from "@tauri-apps/api/mocks";

const SUB = "https://sub.example.com/s/demo";
const settings = {
  servers: [
    { id: "a1", name: "🇳🇱 Нидерланды", link: "", subscription: SUB },
    { id: "a2", name: "🇩🇪 Германия", link: "", subscription: SUB },
    { id: "a3", name: "🇫🇮 Финляндия — очень длинное название сервера", link: "", subscription: SUB },
    { id: "k1", name: "Мой hysteria2", link: "", subscription: null as string | null },
  ],
  subscriptions: [{ url: SUB, name: "Friends VPN" }],
  selected: "a1" as string | null,
  rules: {
    final_route: "direct",
    proxy_apps: [{ name: "Telegram", package: "org.telegram.messenger" }],
    direct_apps: [{ name: "Т-Банк", package: "com.idamob.tinkoff.android" }],
    proxy_domains: ["example.com"],
    direct_domains: ["gosuslugi.ru"],
    presets: ["youtube", "discord"],
  } as unknown,
};
let state = "stopped";

const PRESETS = [
  { id: "youtube", title: "YouTube", domains: ["youtube.com"] },
  { id: "discord", title: "Discord", domains: ["discord.com"] },
  { id: "instagram", title: "Instagram", domains: ["instagram.com"] },
  { id: "x", title: "X / Twitter", domains: ["x.com"] },
  { id: "claude", title: "Claude", domains: ["claude.ai"] },
  { id: "chatgpt", title: "ChatGPT", domains: ["chatgpt.com"] },
  { id: "spotify", title: "Spotify", domains: ["spotify.com"] },
];

const APPS = [
  ["Chrome", "com.android.chrome"],
  ["Discord", "com.discord"],
  ["Instagram", "com.instagram.android"],
  ["Spotify", "com.spotify.music"],
  ["Telegram", "org.telegram.messenger"],
  ["Госуслуги", "ru.rostel"],
  ["Т-Банк", "com.idamob.tinkoff.android"],
  ["YouTube", "com.google.android.youtube"],
].map(([name, pkg]) => ({ name, package: pkg, icon: null }));

const later = (fn: () => void, ms: number) => setTimeout(fn, ms);
const copy = () => structuredClone(settings);

mockIPC((cmd, args) => {
  const a = (args ?? {}) as Record<string, any>;
  switch (cmd) {
    case "get_settings":
    case "refresh_subscriptions":
      return copy();
    case "add_from_text":
      settings.servers.push({ id: `k${Date.now()}`, name: a.text.slice(0, 30), link: a.text, subscription: null });
      return copy();
    case "remove_server":
      settings.servers = settings.servers.filter((s) => s.id !== a.id);
      return copy();
    case "remove_subscription":
      settings.servers = settings.servers.filter((s) => s.subscription !== a.url);
      settings.subscriptions = settings.subscriptions.filter((s) => s.url !== a.url);
      return copy();
    case "select_server":
      settings.selected = a.id;
      return copy();
    case "set_rules":
      settings.rules = a.rules;
      return copy();
    case "get_presets":
      return PRESETS;
    case "list_apps":
      return new Promise((done) => later(() => done(APPS), 400));
    case "vpn_prepare":
      return true;
    case "vpn_connect":
      state = "starting";
      later(() => (state = "started"), 1200);
      return null;
    case "vpn_disconnect":
      state = "stopping";
      later(() => (state = "stopped"), 600);
      return null;
    case "vpn_status":
      return { state, error: null };
    case "ping_servers":
      return settings.servers.map((s, i) => ({
        id: s.id,
        ms: s.subscription ? [48, 135, null][i % 3] : null,
        udp: !s.subscription && state !== "started",
        via_vpn: state === "started",
      }));
    case "clipboard_candidate": {
      const clip = "vless://uuid@example.org:443?security=reality#%F0%9F%87%BA%F0%9F%87%B8%20США";
      return settings.servers.some((s) => s.link === clip) ? null : clip;
    }
    case "vpn_log":
      return "+0300 2026-11-03 12:00:00 INFO sing-box started (демо-журнал)";
  }
});
