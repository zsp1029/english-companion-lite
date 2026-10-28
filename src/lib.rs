#[cfg(windows)]
pub mod app;
pub mod cache;
pub mod config;
#[cfg(windows)]
pub mod credentials;
#[cfg(windows)]
pub mod overlay;
pub mod sentence;
pub mod translator;
#[cfg(windows)]
pub mod tray;
#[cfg(windows)]
pub mod uia;

#[cfg(windows)]
pub mod settings_ui;

#[cfg(windows)]
mod ui_graphics;
