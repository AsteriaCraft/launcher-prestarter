//! The background thread that runs the flow while the window shows it, and the shared state of the window.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use super::events::{self, FailurePayload, TauriReporter};
use crate::flow::{FlowError, Plan, Reporter, Session, Stage, Work};
use crate::i18n::Lang;

/// What the window starts with.
#[derive(Debug, Clone)]
pub enum Start {
    Work(Work),
    ShowError(FlowError),
    /// "Try again": plan from scratch.
    Retry,
}

pub struct AppState {
    pub session: Mutex<Option<Session>>,
    pub start: Mutex<Option<Start>>,
    pub play_now: Arc<AtomicBool>,
    pub running: AtomicBool,
    pub started: AtomicBool,
    /// The exit code if the player closes the window now (0, or the code of the error on screen).
    pub exit_code: AtomicI32,
    pub lang: Lang,
    pub test_mode: bool,
    pub logs_dir: std::path::PathBuf,
    /// When the window was asked for (the log says how long the page took to report ready).
    pub opened: std::time::Instant,
}

/// How long "Asterium is running" stays on screen before the prestarter exits.
const DONE_PAUSE: Duration = Duration::from_millis(700);

pub fn spawn(app: AppHandle) {
    let state = app.state::<AppState>();
    if state.running.swap(true, Ordering::SeqCst) {
        return;
    }
    state.play_now.store(false, Ordering::SeqCst);
    let thread_app = app.clone();
    std::thread::spawn(move || {
        let state = thread_app.state::<AppState>();
        let reporter = TauriReporter::new(thread_app.clone());
        let start = state.start.lock().ok().and_then(|mut s| s.take()).unwrap_or(Start::Retry);
        let session = state.session.lock().ok().and_then(|mut s| s.take());
        let Some(mut session) = session else {
            log::error!("no session for the worker");
            return;
        };
        let result = match start {
            Start::ShowError(err) => Err(err),
            Start::Work(work) => session.run(&work, &reporter),
            Start::Retry => match session.plan() {
                Plan::Launch => session.launch(&reporter),
                Plan::Window(work) => session.run(&work, &reporter),
                Plan::Fail(err) => Err(err),
            },
        };
        if let Ok(mut slot) = state.session.lock() {
            *slot = Some(session);
        }
        state.running.store(false, Ordering::SeqCst);
        match result {
            Ok(launched) => {
                log::info!("launcher started ({launched:?})");
                state.exit_code.store(0, Ordering::SeqCst);
                reporter.stage(Stage::Done);
                let _ = thread_app.emit(events::DONE, ());
                std::thread::sleep(DONE_PAUSE);
                thread_app.exit(0);
            }
            Err(err) => {
                log::error!("{err}");
                state.exit_code.store(err.exit_code(), Ordering::SeqCst);
                let _ = thread_app.emit(events::FAILURE, FailurePayload::from_error(&err, state.lang));
            }
        }
    });
}
