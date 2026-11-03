mod config;
// Копия links.rs десктопа без изменений; пинг подключится следующим шагом.
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
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

struct AppState {
    settings: Mutex<Settings>,
    path: PathBuf,
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

/// Конфиг со всеми серверами: выбранный — по умолчанию в селекторе.
fn build_config(s: &Settings) -> Result<Value, String> {
    let selected = s.selected_server().ok_or("Добавьте ключ или подписку")?;
    let servers: Vec<(String, Value)> = s
        .servers
        .iter()
        .filter_map(|srv| links::parse(&srv.link).ok().map(|p| (server_tag(&srv.id), p.outbound)))
        .collect();
    if !servers.iter().any(|(tag, _)| *tag == server_tag(&selected.id)) {
        return Err(format!("Не удалось разобрать ключ сервера «{}»", selected.name));
    }
    Ok(config::build(&servers, &server_tag(&selected.id)))
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

/// Выбрать сервер. Если VPN включён — переподключаемся к новому.
#[tauri::command]
async fn select_server(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<Settings, String> {
    let mut s = state.snapshot();
    if !s.servers.iter().any(|srv| srv.id == id) {
        return Err("Сервер не найден".into());
    }
    s.selected = Some(id);
    state.store(s.clone())?;
    if status(&app).await?.state != "stopped" {
        restart(&app, &s).await?;
    }
    Ok(s)
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

async fn start(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let config = build_config(s)?;
    let _: Value = vpn::call(app, "start", json!({ "config": config.to_string() })).await?;
    Ok(())
}

/// Полный перезапуск ядра с новым конфигом. Переключение без разрыва
/// (через селектор ядра) появится вместе с пингом.
async fn restart(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let _: Value = vpn::call(app, "stop", ()).await?;
    for _ in 0..50 {
        if status(app).await?.state == "stopped" {
            return start(app, s).await;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("VPN не остановился за 5 секунд".into())
}

#[tauri::command]
async fn vpn_connect(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    start(&app, &state.snapshot()).await
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
            app.manage(AppState { settings: Mutex::new(Settings::load(&path)), path });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            add_from_text,
            refresh_subscriptions,
            remove_server,
            remove_subscription,
            select_server,
            vpn_prepare,
            vpn_connect,
            vpn_disconnect,
            vpn_status,
            vpn_log
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить ShredingerBox");
}
