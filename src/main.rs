#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    if let Err(error) = english_companion_lite::app::run() {
        english_companion_lite::app::fatal(&error);
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    eprintln!("English Companion Lite requires Windows 10/11 x64.");
}
