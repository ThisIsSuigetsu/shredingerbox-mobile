mod config;
// Копия links.rs десктопа без изменений — часть функций здесь не нужна.
#[allow(dead_code)]
mod links;
mod settings;
mod subscription;
mod vpn;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use settings::Settings;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};

struct AppState {
    settings: Mutex<Settings>,
    path: PathBuf,
    /// Clash API запущенного ядра; новый порт и пароль на каждое подключение.
    api: Mutex<Option<config::Api>>,
}

impl AppState {
    fn snapshot(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    fn store(&self, s: Settings) -> Result<(), String> {
        s.save(&self.path)?;
        *self.settings.lock().unwrap() = s;
        Ok(())
    }
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
        .map_err(|e| format!("Не удалось выбрать порт для ядра: {e}"))?
        .port();
    let secret = rand::thread_rng().sample_iter(&Alphanumeric).take(32).map(char::from).collect();
    Ok(config::Api { port, secret })
}

/// Конфиг со всеми серверами: выбранный — по умолчанию в селекторе.
fn build_config(s: &Settings, api: &config::Api) -> Result<Value, String> {
    let selected = s.selected_server().ok_or("Добавьте ключ или подписку")?;
    let servers: Vec<(String, Value)> = s
        .servers
        .iter()
        .filter_map(|srv| links::parse(&srv.link).ok().map(|p| (server_tag(&srv.id), p.outbound)))
        .collect();
    if !servers.iter().any(|(tag, _)| *tag == server_tag(&selected.id)) {
        return Err(format!("Не удалось разобрать ключ сервера «{}»", selected.name));
    }
    Ok(config::build(&servers, &server_tag(&selected.id), api))
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
        s.upsert_subscription(&text, &fetched.name);
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
                s.upsert_subscription(&sub.url, &fetched.name);
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

/// Выбрать сервер. Если VPN включён — переключаем прямо в ядре, без переподключения.
#[tauri::command]
async fn select_server(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<Settings, String> {
    let mut s = state.snapshot();
    if !s.servers.iter().any(|srv| srv.id == id) {
        return Err("Сервер не найден".into());
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
    // (добавлен после подключения) — перезапускаем.
    if !switched {
        restart(&app, &state, &s).await?;
    }
    Ok(s)
}

#[derive(Serialize)]
struct Ping {
    id: String,
    ms: Option<u64>,
    /// UDP-протокол (Hysteria2) — TCP-пинг не меряется.
    udp: bool,
    /// Задержка измерена через само ядро (реальный запрос через VPN).
    via_vpn: bool,
}

/// Пинг серверов. Пока VPN включён, каждый сервер проверяется реальным запросом
/// через ядро (Hysteria2 тоже); без VPN — TCP-подключением (у Hysteria2 это UDP — не меряем).
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
struct Clipboard {
    text: Option<String>,
}

/// Ключи или ссылка подписки из буфера обмена, которых ещё нет в списке.
#[tauri::command]
async fn clipboard_candidate(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>, String> {
    let clip: Clipboard = vpn::call(&app, "clipboard", ()).await?;
    let Some(candidate) = clip.text.as_deref().and_then(links::clipboard_candidate) else {
        return Ok(None);
    };
    let s = state.snapshot();
    let known = candidate.lines().all(|l| {
        s.servers.iter().any(|srv| srv.link == l) || s.subscriptions.iter().any(|sub| sub.url == l)
    });
    Ok((!known).then_some(candidate))
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
    *state.api.lock().unwrap() = Some(api);
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
    Err("VPN не остановился за 5 секунд".into())
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
}

async fn status(app: &AppHandle) -> Result<Status, String> {
    vpn::call(app, "status", ()).await
}

#[tauri::command]
async fn vpn_status(app: AppHandle) -> Result<Status, String> {
    status(&app).await
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
            let path = app.path().app_data_dir()?.join("settings.json");
            app.manage(AppState { settings: Mutex::new(Settings::load(&path)), path, api: Mutex::new(None) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            add_from_text,
            refresh_subscriptions,
            remove_server,
            remove_subscription,
            select_server,
            ping_servers,
            clipboard_candidate,
            vpn_prepare,
            vpn_connect,
            vpn_disconnect,
            vpn_status,
            vpn_log
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить ShredingerBox");
}
