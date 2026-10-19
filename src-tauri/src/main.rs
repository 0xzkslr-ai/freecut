#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    freecut_lib::desktop::run();
    #[cfg(not(windows))]
    eprintln!("The Tauri screenshot backend currently supports Windows only.");
}
