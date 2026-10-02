//! Commands the front end calls.

use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_opener::OpenerExt;

use super::worker::{self, AppState, Start};
use crate::i18n::Lang;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Boot {
    pub lang: Lang,
    pub test_mode: bool,
    pub os: &'static str,
}

/// Language and mode for the first render (the version comes from `getVersion()`).
#[tauri::command]
pub fn boot(state: State<'_, AppState>) -> Boot {
    Boot { lang: state.lang, test_mode: state.test_mode, os: crate::platform::Host::current().os.as_str() }
}

/// The front end has painted its first frame: show the window (hidden until now, so there is no white flash) and
/// start the work.
#[tauri::command]
pub fn ready(app: AppHandle, window: WebviewWindow, state: State<'_, AppState>) {
    if !state.started.swap(true, Ordering::SeqCst) {
        let _ = window.show();
        let _ = window.set_focus();
        worker::spawn(app);
    }
}

#[tauri::command]
pub fn retry(app: AppHandle, state: State<'_, AppState>) {
    if let Ok(mut start) = state.start.lock() {
        *start = Some(Start::Retry);
    }
    worker::spawn(app);
}

/// "Play now" during a JRE update: stop the download and start with the installed JRE.
#[tauri::command]
pub fn play_now(state: State<'_, AppState>) {
    log::info!("Play now pressed");
    state.play_now.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn open_logs(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    app.opener().open_path(state.logs_dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

/// Opens one of our own pages (the download page from the signed policy or the error).
#[tauri::command]
pub fn open_page(app: AppHandle, url: String) -> Result<(), String> {
    let parsed = reqwest::Url::parse(&url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "https" {
        return Err("only https pages are opened".into());
    }
    app.opener().open_url(parsed.as_str(), None::<&str>).map_err(|e| e.to_string())
}

/// The window's close button: exit with the code of the error on screen, if any.
#[tauri::command]
pub fn quit(app: AppHandle) {
    let code = app.state::<AppState>().exit_code.load(Ordering::SeqCst);
    app.exit(code);
}

#[tauri::command]
pub fn minimize(window: WebviewWindow) {
    let _ = window.minimize();
}
