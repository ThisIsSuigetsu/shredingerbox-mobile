mod config;
// Копия links.rs десктопа без изменений; подписки и пинг подключатся следующими шагами.
#[allow(dead_code)]
mod links;
mod vpn;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;

/// Где запущено приложение — проверка связи интерфейса с Rust.
#[tauri::command]
fn platform() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

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

/// Подключается по одному ключу. Возвращает имя сервера из ключа.
#[tauri::command]
async fn vpn_connect(app: AppHandle, link: String) -> Result<String, String> {
    let parsed = links::parse(&link)?;
    let servers = vec![("server".to_string(), parsed.outbound)];
    let config = config::build(&servers, "server");
    let _: Value = vpn::call(&app, "start", json!({ "config": config.to_string() })).await?;
    Ok(parsed.name)
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

#[tauri::command]
async fn vpn_status(app: AppHandle) -> Result<Status, String> {
    vpn::call(&app, "status", ()).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(vpn::init())
        .invoke_handler(tauri::generate_handler![
            platform,
            vpn_prepare,
            vpn_connect,
            vpn_disconnect,
            vpn_status
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить ShredingerBox");
}
