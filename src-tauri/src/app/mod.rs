//! The Tauri side: the window, its commands and events, and the fallback to the mode without a window. Everything
//! else lives in Tauri-free modules (`flow` and below).

pub mod commands;
pub mod events;
pub mod headless;
pub mod worker;

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{Manager, WebviewWindowBuilder};

use crate::flow::Session;
use worker::AppState;
pub use worker::Start;

/// If the page never calls `ready` (a broken front end), the window is shown and the work starts anyway.
const READY_TIMEOUT: Duration = Duration::from_secs(15);

/// Whether a webview can exist here: WebView2 on Windows, a display for WebKitGTK on Linux.
fn webview_available(session: &Session) -> bool {
    let env = &session.context().env;
    if cfg!(target_os = "linux") && env.get("DISPLAY").is_none() && env.get("WAYLAND_DISPLAY").is_none() {
        log::warn!("no DISPLAY or WAYLAND_DISPLAY: running without a window");
        return false;
    }
    match tauri::webview_version() {
        Ok(version) => {
            log::info!("webview {version}");
            true
        }
        Err(err) => {
            log::warn!("no webview ({err}): running without a window");
            false
        }
    }
}

/// Shows the window and runs `start` in it; falls back to the mode without a window. Returns the exit code.
pub fn run(session: Session, start: Start) -> i32 {
    if !webview_available(&session) {
        return headless::run(session, start);
    }
    let lang = session.context().lang;
    let test_mode = session.context().overrides.is_test_mode();
    let logs_dir = session.logs_dir();
    let play_now = session.play_now_flag();
    let shared = Arc::new(Mutex::new(Some(session)));
    let pending = Arc::new(Mutex::new(Some(start)));
    let state = AppState {
        session: Mutex::new(None),
        start: Mutex::new(None),
        play_now,
        running: AtomicBool::new(false),
        started: AtomicBool::new(false),
        exit_code: AtomicI32::new(0),
        lang,
        test_mode,
        logs_dir,
    };

    let setup_shared = Arc::clone(&shared);
    let setup_pending = Arc::clone(&pending);
    let built = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::boot,
            commands::ready,
            commands::retry,
            commands::play_now,
            commands::open_logs,
            commands::open_page,
            commands::quit,
            commands::minimize,
        ])
        .setup(move |app| {
            let state = app.state::<AppState>();
            *state.session.lock().expect("fresh mutex") = setup_shared.lock().expect("fresh mutex").take();
            *state.start.lock().expect("fresh mutex") = setup_pending.lock().expect("fresh mutex").take();
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .cloned()
                .ok_or("tauri.conf.json has no `main` window")?;
            match WebviewWindowBuilder::from_config(app.handle(), &config).and_then(|builder| builder.build()) {
                Ok(_) => {
                    let handle = app.handle().clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(READY_TIMEOUT);
                        let state = handle.state::<AppState>();
                        if !state.started.swap(true, Ordering::SeqCst) {
                            log::warn!(
                                "the page did not report ready in {} s; starting anyway",
                                READY_TIMEOUT.as_secs()
                            );
                            if let Some(window) = handle.get_webview_window("main") {
                                let _ = window.show();
                            }
                            worker::spawn(handle.clone());
                        }
                    });
                }
                Err(err) => {
                    log::warn!("cannot create the window ({err}): running without a window");
                    let handle = app.handle().clone();
                    let session = state.session.lock().expect("set above").take();
                    let start = state.start.lock().expect("set above").take().unwrap_or(Start::Retry);
                    std::thread::spawn(move || {
                        let code = session.map_or(10, |session| headless::run(session, start));
                        handle.exit(code);
                    });
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!());

    match built {
        Ok(app) => app.run_return(|_, _| {}),
        Err(err) => {
            log::error!("Tauri could not start ({err}): running without a window");
            let session = shared.lock().ok().and_then(|mut s| s.take());
            let start = pending.lock().ok().and_then(|mut s| s.take()).unwrap_or(Start::Retry);
            session.map_or(10, |session| headless::run(session, start))
        }
    }
}
