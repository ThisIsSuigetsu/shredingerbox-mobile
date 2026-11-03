//! Мост к Kotlin-плагину `VpnPlugin`: разрешение на VPN, запуск и остановка ядра.
//! На компьютере плагина нет — команды честно отвечают ошибкой.

use serde::de::DeserializeOwned;
use serde::Serialize;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{AppHandle, Runtime};

#[cfg(target_os = "android")]
struct Vpn<R: Runtime>(tauri::plugin::PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("vpn")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                let handle = _api.register_android_plugin("org.shredingerbox.mobile", "VpnPlugin")?;
                _app.manage(Vpn(handle));
            }
            Ok(())
        })
        .build()
}

/// Вызывает команду Kotlin-плагина и ждёт ответа, не блокируя поток Tauri:
/// `prepare` может долго ждать, пока пользователь ответит на системный запрос.
#[cfg(target_os = "android")]
pub async fn call<R: Runtime, T: DeserializeOwned>(
    app: &AppHandle<R>,
    command: &str,
    payload: impl Serialize,
) -> Result<T, String> {
    use tauri::Manager;
    app.state::<Vpn<R>>()
        .0
        .run_mobile_plugin_async(command, payload)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "android"))]
pub async fn call<R: Runtime, T: DeserializeOwned>(
    _app: &AppHandle<R>,
    _command: &str,
    _payload: impl Serialize,
) -> Result<T, String> {
    Err("VPN пока работает только на Android".into())
}
