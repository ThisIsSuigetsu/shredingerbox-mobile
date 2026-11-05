mod config;
mod i18n;
// Копия links.rs десктопа (сообщения переведены) - часть функций здесь не нужна.
#[allow(dead_code)]
mod links;
mod presets;
mod settings;
mod subscription;
mod traffic;
mod update;
mod vpn;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use settings::{Mode, Rules, Settings};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};

struct AppState {
    settings: Mutex<Settings>,
    path: PathBuf,
    /// Clash API запущенного ядра; новый порт и пароль на каждое подключение.
    api: Mutex<Option<config::Api>>,
    traffic: traffic::Traffic,
    app: AppHandle,
}

impl AppState {
    fn snapshot(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    fn store(&self, s: Settings) -> Result<(), String> {
        s.save(&self.path)?;
        *self.settings.lock().unwrap() = s;
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move { sync_config(&app).await });
        Ok(())
    }
}

/// Отдать Android-стороне конфиг по текущим настройкам: по нему подключаются плитка
/// в шторке, автозапуск и Always-on, когда приложение не открыто.
async fn sync_config(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(api) = new_api() else { return };
    // Серверов ещё нет - подключаться не к чему, старый конфиг не трогаем.
    let Ok(config) = build_config(&state.snapshot(), &api) else { return };
    let _ = vpn::call::<_, Value>(app, "saveConfig", json!({ "config": config.to_string() })).await;
}

fn server_tag(id: &str) -> String {
    format!("srv-{id}")
}

/// Свободный порт на 127.0.0.1 и случайный пароль: порты loopback на Android
/// общие для всех приложений, поэтому фиксированный порт мог бы быть занят.
fn new_api() -> Result<config::Api, String> {
    use rand::distributions::Alphanumeric;
    use rand::Rng;
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map_err(|e| tr!("Не удалось выбрать порт для ядра: {e}", "Could not pick a port for the core: {e}"))?
        .port();
    let secret = rand::thread_rng().sample_iter(&Alphanumeric).take(32).map(char::from).collect();
    Ok(config::Api { port, secret })
}

/// Конфиг со всеми серверами: выбранный - по умолчанию в селекторе.
fn build_config(s: &Settings, api: &config::Api) -> Result<Value, String> {
    let selected = s.selected_server().ok_or_else(|| tr!("Добавьте ключ или подписку", "Add a key or a subscription"))?;
    let servers: Vec<(String, Value)> = s
        .servers
        .iter()
        .filter_map(|srv| links::parse(&srv.link).ok().map(|p| (server_tag(&srv.id), p.outbound)))
        .collect();
    if !servers.iter().any(|(tag, _)| *tag == server_tag(&selected.id)) {
        let name = &selected.name;
        return Err(tr!("Не удалось разобрать ключ сервера «{name}»", "Could not read the key of server \"{name}\""));
    }
    Ok(config::build(s.mode, &s.rules, &servers, &server_tag(&selected.id), s.auto_select, api))
}

fn api_client() -> Option<reqwest::Client> {
    reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(8)).build().ok()
}

/// Реальная задержка через конкретный сервер: ядро делает запрос к generate_204.
async fn url_test(api: &config::Api, tag: &str) -> Option<u64> {
    let url = format!(
        "http://127.0.0.1:{}/proxies/{tag}/delay?url=https%3A%2F%2Fwww.gstatic.com%2Fgenerate_204&timeout=5000",
        api.port
    );
    let text = api_client()?.get(url).bearer_auth(&api.secret).send().await.ok()?.text().await.ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value["delay"].as_u64()
}

/// Переключить сервер в работающем ядре без переподключения.
async fn clash_select(api: &config::Api, tag: &str) -> bool {
    let Some(client) = api_client() else { return false };
    client
        .put(format!("http://127.0.0.1:{}/proxies/proxy", api.port))
        .bearer_auth(&api.secret)
        .header("Content-Type", "application/json")
        .body(json!({ "name": tag }).to_string())
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// API ядра, если VPN сейчас подключён.
async fn running_api(app: &AppHandle, state: &AppState) -> Result<Option<config::Api>, String> {
    if status(app).await?.state != "started" {
        return Ok(None);
    }
    Ok(state.api.lock().unwrap().clone())
}

// ---------- серверы и подписки ----------

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.snapshot()
}

/// Ключи (по одному на строку) или ссылка на подписку.
#[tauri::command]
async fn add_from_text(state: State<'_, AppState>, text: String) -> Result<Settings, String> {
    let text = text.trim().to_string();
    let mut s = state.snapshot();
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        let fetched = subscription::fetch(&text).await?;
        s.upsert_subscription(&text, &fetched.name, fetched.usage);
        s.merge_subscription(&text, fetched.links);
    } else {
        s.add_keys(&text)?;
    }
    state.store(s.clone())?;
    Ok(s)
}

/// Обновляет все подписки. Если какая-то не загрузилась, остальные всё равно обновятся.
#[tauri::command]
async fn refresh_subscriptions(state: State<'_, AppState>) -> Result<Settings, String> {
    let mut s = state.snapshot();
    let mut errors = vec![];
    for sub in s.subscriptions.clone() {
        match subscription::fetch(&sub.url).await {
            Ok(fetched) => {
                s.upsert_subscription(&sub.url, &fetched.name, fetched.usage);
                s.merge_subscription(&sub.url, fetched.links);
            }
            Err(e) => errors.push(format!("{}: {e}", sub.name)),
        }
    }
    state.store(s.clone())?;
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok(s)
}

/// Подписки старше 12 часов обновляются при запуске сами, тихо: новые серверы и адреса
/// из панели доходят до всех без кнопки. Что не загрузилось - попробуем в следующий раз.
/// None - обновлять было нечего.
#[tauri::command]
async fn refresh_stale_subscriptions(state: State<'_, AppState>) -> Result<Option<Settings>, String> {
    const STALE: u64 = 12 * 60 * 60;
    let now = settings::now_secs();
    let mut s = state.snapshot();
    let stale: Vec<String> = s
        .subscriptions
        .iter()
        .filter(|sub| sub.updated.map_or(true, |t| now.saturating_sub(t) >= STALE))
        .map(|sub| sub.url.clone())
        .collect();
    let mut changed = false;
    for url in stale {
        if let Ok(fetched) = subscription::fetch(&url).await {
            s.upsert_subscription(&url, &fetched.name, fetched.usage);
            s.merge_subscription(&url, fetched.links);
            changed = true;
        }
    }
    if !changed {
        return Ok(None);
    }
    // Пока скачивали, пользователь мог что-то поменять - берём свежие настройки
    // и переносим в них только подписки.
    let mut current = state.snapshot();
    let urls: Vec<String> = s
        .subscriptions
        .iter()
        .filter(|sub| current.subscriptions.iter().any(|c| c.url == sub.url))
        .map(|sub| sub.url.clone())
        .collect();
    for url in &urls {
        let sub = s.subscriptions.iter().find(|sub| &sub.url == url).unwrap();
        current.upsert_subscription(url, &sub.name, sub.usage);
        let links = s.servers.iter().filter(|srv| srv.subscription.as_deref() == Some(url)).map(|srv| srv.link.clone()).collect();
        current.merge_subscription(url, links);
    }
    state.store(current.clone())?;
    Ok(Some(current))
}

#[tauri::command]
fn remove_server(state: State<AppState>, id: String) -> Result<Settings, String> {
    let mut s = state.snapshot();
    s.remove_server(&id);
    state.store(s.clone())?;
    Ok(s)
}

#[tauri::command]
fn remove_subscription(state: State<AppState>, url: String) -> Result<Settings, String> {
    let mut s = state.snapshot();
    s.remove_subscription(&url);
    state.store(s.clone())?;
    Ok(s)
}

/// Выбрать сервер. Если VPN включён - переключаем прямо в ядре, без переподключения.
#[tauri::command]
async fn select_server(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<Settings, String> {
    let mut s = state.snapshot();
    if !s.servers.iter().any(|srv| srv.id == id) {
        return Err(tr!("Сервер не найден", "Server not found"));
    }
    s.selected = Some(id.clone());
    state.store(s.clone())?;
    if status(&app).await?.state == "stopped" {
        return Ok(s);
    }
    let switched = match running_api(&app, &state).await? {
        Some(api) => clash_select(&api, &server_tag(&id)).await,
        None => false,
    };
    // Ядро ещё подключается или сервера нет в запущенном конфиге
    // (добавлен после подключения) - перезапускаем.
    if !switched {
        restart(&app, &state, &s).await?;
    }
    Ok(s)
}

#[derive(Serialize)]
struct Ping {
    id: String,
    ms: Option<u64>,
    /// UDP-протокол (Hysteria2) - TCP-пинг не меряется.
    udp: bool,
    /// Задержка измерена через само ядро (реальный запрос через VPN).
    via_vpn: bool,
}

/// Пинг серверов. Пока VPN включён, каждый сервер проверяется реальным запросом
/// через ядро (Hysteria2 тоже); без VPN - TCP-подключением (у Hysteria2 это UDP - не меряем).
#[tauri::command]
async fn ping_servers(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<Ping>, String> {
    let api = running_api(&app, &state).await?;
    let mut tasks = vec![];
    for srv in state.snapshot().servers {
        let api = api.clone();
        tasks.push(tauri::async_runtime::spawn(async move {
            if let Some(api) = api {
                let ms = url_test(&api, &server_tag(&srv.id)).await;
                return Ping { id: srv.id, ms, udp: false, via_vpn: true };
            }
            let mut udp = false;
            let ms = match links::parse(&srv.link) {
                Ok(p) if !p.udp_only => {
                    let started = Instant::now();
                    match tokio::time::timeout(
                        Duration::from_secs(3),
                        tokio::net::TcpStream::connect((p.host.as_str(), p.port)),
                    )
                    .await
                    {
                        Ok(Ok(_)) => Some(started.elapsed().as_millis() as u64),
                        _ => None,
                    }
                }
                Ok(_) => {
                    udp = true;
                    None
                }
                Err(_) => None,
            };
            Ping { id: srv.id, ms, udp, via_vpn: false }
        }));
    }
    let mut result = vec![];
    for t in tasks {
        if let Ok(p) = t.await {
            result.push(p);
        }
    }
    Ok(result)
}

#[derive(Deserialize)]
struct ClipboardText {
    text: Option<String>,
}

/// Ключи или ссылка подписки из буфера обмена, которых ещё нет в списке.
#[tauri::command]
async fn clipboard_candidate(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>, String> {
    let clip: ClipboardText = vpn::call(&app, "clipboard", ()).await?;
    let Some(candidate) = clip.text.as_deref().and_then(links::clipboard_candidate) else {
        return Ok(None);
    };
    let s = state.snapshot();
    let known = candidate.lines().all(|l| {
        s.servers.iter().any(|srv| srv.link == l) || s.subscriptions.iter().any(|sub| sub.url == l)
    });
    Ok((!known).then_some(candidate))
}

// ---------- правила ----------

#[tauri::command]
fn get_presets() -> Vec<presets::Preset> {
    presets::PRESETS.to_vec()
}

/// Сохранить правила. Если VPN включён - переподключаемся: правила живут в конфиге ядра.
#[tauri::command]
async fn set_rules(app: AppHandle, state: State<'_, AppState>, rules: Rules) -> Result<Settings, String> {
    let mut s = state.snapshot();
    s.rules = rules;
    state.store(s.clone())?;
    if status(&app).await?.state != "stopped" {
        restart(&app, &state, &s).await?;
    }
    Ok(s)
}

#[derive(Deserialize)]
struct Apps {
    apps: Value,
}

/// Установленные приложения с иконками: [{ name, package, icon }].
#[tauri::command]
async fn list_apps(app: AppHandle) -> Result<Value, String> {
    let r: Apps = vpn::call(&app, "apps", ()).await?;
    Ok(r.apps)
}

/// TUN или Прокси. Если ядро запущено - переподключаемся в новом режиме.
#[tauri::command]
async fn set_mode(app: AppHandle, state: State<'_, AppState>, mode: Mode) -> Result<Settings, String> {
    let mut s = state.snapshot();
    s.mode = mode;
    state.store(s.clone())?;
    if status(&app).await?.state != "stopped" {
        restart(&app, &state, &s).await?;
    }
    Ok(s)
}

/// «Лучший сервер автоматически». Если ядро запущено - переподключаемся с новым конфигом.
#[tauri::command]
async fn set_auto_select(app: AppHandle, state: State<'_, AppState>, on: bool) -> Result<Settings, String> {
    let mut s = state.snapshot();
    s.auto_select = on;
    state.store(s.clone())?;
    if status(&app).await?.state != "stopped" {
        restart(&app, &state, &s).await?;
    }
    Ok(s)
}

/// Какой сервер сейчас выбрал «Авто» (id), если VPN включён.
#[tauri::command]
async fn auto_server(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>, String> {
    let Some(api) = running_api(&app, &state).await? else { return Ok(None) };
    let Some(client) = api_client() else { return Ok(None) };
    let url = format!("http://127.0.0.1:{}/proxies/{}", api.port, config::AUTO_TAG);
    let Ok(resp) = client.get(url).bearer_auth(&api.secret).send().await else { return Ok(None) };
    let Ok(text) = resp.text().await else { return Ok(None) };
    let Ok(value) = serde_json::from_str::<Value>(&text) else { return Ok(None) };
    Ok(value["now"].as_str().and_then(|t| t.strip_prefix("srv-")).map(str::to_string))
}

/// Язык интерфейса: запоминаем, переводим сообщения Rust и отдаём Android-части
/// (уведомление, плитка), которая работает и без открытого приложения.
#[tauri::command]
async fn set_language(app: AppHandle, state: State<'_, AppState>, lang: String) -> Result<Settings, String> {
    let lang = if lang == "en" { "en" } else { "ru" }.to_string();
    i18n::set(&lang);
    let _: Value = vpn::call(&app, "setLanguage", json!({ "lang": lang })).await?;
    let mut s = state.snapshot();
    if s.language.as_deref() != Some(lang.as_str()) {
        s.language = Some(lang);
        state.store(s.clone())?;
    }
    Ok(s)
}

/// Тёмные значки в строке состояния и навигации - для светлой темы интерфейса.
#[tauri::command]
async fn set_system_bars(app: AppHandle, light: bool) -> Result<(), String> {
    let _: Value = vpn::call(&app, "setSystemBars", json!({ "on": light })).await?;
    Ok(())
}

// ---------- запуск без приложения, ссылки, QR ----------

#[tauri::command]
async fn set_autostart(app: AppHandle, state: State<'_, AppState>, on: bool) -> Result<Settings, String> {
    let _: Value = vpn::call(&app, "setAutostart", json!({ "on": on })).await?;
    let mut s = state.snapshot();
    s.autostart = on;
    state.store(s.clone())?;
    Ok(s)
}

#[tauri::command]
async fn open_vpn_settings(app: AppHandle) -> Result<(), String> {
    let _: Value = vpn::call(&app, "openVpnSettings", ()).await?;
    Ok(())
}

#[derive(Deserialize)]
struct Supported {
    supported: bool,
}

/// Предложить добавить плитку в шторку. false - Android старше 13, только вручную.
#[tauri::command]
async fn add_tile(app: AppHandle) -> Result<bool, String> {
    let r: Supported = vpn::call(&app, "addTile", ()).await?;
    Ok(r.supported)
}

#[derive(Deserialize)]
struct Text {
    text: Option<String>,
}

/// Подписка или ключ из ссылки shredingerbox://add?url=…, которой открыли приложение.
#[tauri::command]
async fn take_link(app: AppHandle) -> Result<Option<String>, String> {
    let r: Text = vpn::call(&app, "takeLink", ()).await?;
    Ok(r.text)
}

/// Отсканировать QR-код. None - сканер закрыли.
#[tauri::command]
async fn scan_qr(app: AppHandle) -> Result<Option<String>, String> {
    let r: Text = vpn::call(&app, "scanQr", ()).await?;
    Ok(r.text)
}

// ---------- обновления ----------

#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Option<update::UpdateInfo>, String> {
    update::check(&app.package_info().version.to_string(), i18n::en()).await
}

/// Скачать APK и открыть установщик. Приложение перезапустится уже новой версией.
#[tauri::command]
async fn install_update(app: AppHandle, url: String) -> Result<(), String> {
    let path = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("update.apk");
    update::download(&url, &path).await?;
    let _: Value = vpn::call(&app, "install", json!({ "path": path.to_string_lossy() })).await?;
    Ok(())
}

// ---------- VPN ----------

#[derive(Deserialize)]
struct Granted {
    granted: bool,
}

/// Спрашивает у Android разрешение на VPN. Система показывает окно только в первый раз.
#[tauri::command]
async fn vpn_prepare(app: AppHandle) -> Result<bool, String> {
    let r: Granted = vpn::call(&app, "prepare", ()).await?;
    Ok(r.granted)
}

async fn start(app: &AppHandle, state: &AppState, s: &Settings) -> Result<(), String> {
    let api = new_api()?;
    let config = build_config(s, &api)?;
    let _: Value = vpn::call(app, "start", json!({ "config": config.to_string() })).await?;
    *state.api.lock().unwrap() = Some(api.clone());
    traffic::spawn_counter(app.clone(), api);
    Ok(())
}

/// Полный перезапуск ядра с новым конфигом.
async fn restart(app: &AppHandle, state: &AppState, s: &Settings) -> Result<(), String> {
    let _: Value = vpn::call(app, "stop", ()).await?;
    for _ in 0..50 {
        if status(app).await?.state == "stopped" {
            return start(app, state, s).await;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(tr!("VPN не остановился за 5 секунд", "VPN did not stop within 5 seconds"))
}

#[tauri::command]
async fn vpn_connect(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    start(&app, &state, &state.snapshot()).await
}

#[tauri::command]
async fn vpn_disconnect(app: AppHandle) -> Result<(), String> {
    let _: Value = vpn::call(&app, "stop", ()).await?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Status {
    /// stopped | starting | started | stopping
    state: String,
    /// Почему ядро остановилось, если не по кнопке.
    error: Option<String>,
    /// Clash API запущенного ядра - интерфейсу не нужен.
    #[serde(default, skip_serializing)]
    api: Option<RunningApi>,
}

#[derive(Deserialize)]
struct RunningApi {
    port: u16,
    secret: String,
}

async fn status(app: &AppHandle) -> Result<Status, String> {
    vpn::call(app, "status", ()).await
}

/// Если ядро запустили без приложения (плиткой, при включении телефона, Always-on),
/// подхватываем его Clash API: тогда работают пинг через VPN, мгновенное
/// переключение и счётчик трафика.
#[tauri::command]
async fn vpn_status(app: AppHandle, state: State<'_, AppState>) -> Result<Status, String> {
    let s = status(&app).await?;
    if let (true, Some(running)) = (s.state == "started", &s.api) {
        let mut api = state.api.lock().unwrap();
        if api.as_ref().map(|a| a.secret.as_str()) != Some(running.secret.as_str()) {
            let adopted = config::Api { port: running.port, secret: running.secret.clone() };
            *api = Some(adopted.clone());
            traffic::spawn_counter(app.clone(), adopted);
        }
    }
    Ok(s)
}

#[derive(Serialize)]
struct MonthTraffic {
    month: String,
    servers: std::collections::HashMap<String, traffic::Usage>,
    /// Скорость за последнюю секунду: байты вверх и вниз.
    speed: traffic::Usage,
}

/// Расход трафика по серверам за текущий месяц и текущая скорость.
#[tauri::command]
fn get_traffic(state: State<AppState>) -> MonthTraffic {
    let month = traffic::current_month();
    MonthTraffic { servers: state.traffic.month(&month), month, speed: state.traffic.speed() }
}

#[derive(Deserialize)]
struct Log {
    text: String,
}

/// Последние строки журнала ядра.
#[tauri::command]
async fn vpn_log(app: AppHandle) -> Result<String, String> {
    let log: Log = vpn::call(&app, "log", ()).await?;
    Ok(log.text)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(vpn::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let path = dir.join("settings.json");
            let settings = Settings::load(&path);
            if let Some(lang) = &settings.language {
                i18n::set(lang);
            }
            app.manage(AppState {
                settings: Mutex::new(settings),
                path,
                api: Mutex::new(None),
                traffic: traffic::Traffic::load(dir.join("traffic.json")),
                app: app.handle().clone(),
            });
            // Конфиг для плитки и автозапуска - по настройкам, сохранённым до обновления.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move { sync_config(&handle).await });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            add_from_text,
            refresh_subscriptions,
            refresh_stale_subscriptions,
            set_auto_select,
            auto_server,
            remove_server,
            remove_subscription,
            select_server,
            ping_servers,
            clipboard_candidate,
            get_presets,
            set_rules,
            list_apps,
            set_mode,
            set_language,
            set_system_bars,
            check_update,
            install_update,
            get_traffic,
            set_autostart,
            open_vpn_settings,
            add_tile,
            take_link,
            scan_qr,
            vpn_prepare,
            vpn_connect,
            vpn_disconnect,
            vpn_status,
            vpn_log
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить ShredingerBox");
}
