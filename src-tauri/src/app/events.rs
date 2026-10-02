//! The flow's reports as Tauri events for the Svelte front end. Progress is throttled to one event per 100 ms (the
//! last chunk always goes through), so a fast download does not flood the webview.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::flow::{FlowError, Notice, Reporter, Stage};
use crate::i18n::Lang;

pub const STAGE: &str = "stage";
pub const PROGRESS: &str = "progress";
pub const NOTICE: &str = "notice";
pub const FAILURE: &str = "failure";
pub const DONE: &str = "done";

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Serialize)]
pub struct StagePayload {
    pub stage: Stage,
}

#[derive(Clone, Serialize)]
pub struct ProgressPayload {
    pub done: u64,
    pub total: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailurePayload {
    pub message: String,
    pub code: i32,
    pub retryable: bool,
    pub command: Option<String>,
    pub log_tail: Option<String>,
    pub link: Option<String>,
}

impl FailurePayload {
    pub fn from_error(err: &FlowError, lang: Lang) -> Self {
        Self {
            message: err.message(lang),
            code: err.exit_code(),
            retryable: err.kind.retryable(),
            command: err.command.clone(),
            log_tail: err.log_tail.clone(),
            link: err.link.clone(),
        }
    }
}

/// Decides whether a progress update is sent now (pure, so the throttle is unit-tested).
pub struct Throttle {
    last: Option<Instant>,
}

impl Throttle {
    pub fn new() -> Self {
        Self { last: None }
    }

    pub fn allow(&mut self, now: Instant, done: u64, total: u64) -> bool {
        let finished = total > 0 && done >= total;
        let due = self.last.is_none_or(|last| now.duration_since(last) >= PROGRESS_INTERVAL);
        if finished || due {
            self.last = Some(now);
            true
        } else {
            false
        }
    }
}

impl Default for Throttle {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TauriReporter {
    app: AppHandle,
    throttle: Mutex<Throttle>,
}

impl TauriReporter {
    pub fn new(app: AppHandle) -> Self {
        Self { app, throttle: Mutex::new(Throttle::new()) }
    }
}

impl Reporter for TauriReporter {
    fn stage(&self, stage: Stage) {
        log::info!("stage {stage:?}");
        if let Ok(mut throttle) = self.throttle.lock() {
            *throttle = Throttle::new();
        }
        let _ = self.app.emit(STAGE, StagePayload { stage });
    }

    fn progress(&self, done: u64, total: u64) {
        let allowed = self.throttle.lock().map(|mut t| t.allow(Instant::now(), done, total)).unwrap_or(true);
        if allowed {
            let _ = self.app.emit(PROGRESS, ProgressPayload { done, total });
        }
    }

    fn notice(&self, notice: Notice) {
        log::info!("notice {notice:?}");
        let _ = self.app.emit(NOTICE, notice);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttles_to_one_update_per_100_ms_but_never_drops_the_last() {
        let mut throttle = Throttle::new();
        let start = Instant::now();
        assert!(throttle.allow(start, 1, 100));
        assert!(!throttle.allow(start + Duration::from_millis(30), 2, 100));
        assert!(!throttle.allow(start + Duration::from_millis(99), 3, 100));
        assert!(throttle.allow(start + Duration::from_millis(100), 4, 100));
        assert!(throttle.allow(start + Duration::from_millis(101), 100, 100), "the final chunk is always sent");
    }

    #[test]
    fn failure_payload() {
        let err = FlowError::new(crate::flow::ErrorKind::JarDownload, "x");
        let payload = FailurePayload::from_error(&err, Lang::En);
        assert_eq!(payload.code, 4);
        assert!(payload.retryable);
        assert!(payload.message.starts_with("The Asterium server is not reachable"));
    }
}
