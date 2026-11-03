/// Где запущено приложение — проверка связи интерфейса с Rust.
#[tauri::command]
fn platform() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![platform])
        .run(tauri::generate_context!())
        .expect("не удалось запустить ShredingerBox");
}
