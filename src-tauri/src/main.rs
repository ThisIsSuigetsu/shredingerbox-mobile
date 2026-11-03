// Десктопная точка входа нужна только для `tauri dev` на компьютере;
// на телефоне приложение стартует из run() в lib.rs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    shredingerbox_lib::run()
}
