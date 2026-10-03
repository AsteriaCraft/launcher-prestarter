//! What the flow tells whoever shows it: the window (Tauri events), the no-WebView mode (native dialogs), or a
//! test. Stages and notices are serialised as they are sent to the front end.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Preparing,
    WaitLock,
    JreCheck,
    JreDownload,
    JreInstall,
    JreVerify,
    JarDownload,
    Launching,
    Done,
}

impl Stage {
    /// The i18n key of the stage's text.
    pub fn message_key(self) -> &'static str {
        match self {
            Stage::Preparing => "stage.preparing",
            Stage::WaitLock => "stage.waitLock",
            Stage::JreCheck => "stage.jreCheck",
            Stage::JreDownload => "stage.jreDownload",
            Stage::JreInstall => "stage.jreInstall",
            Stage::JreVerify => "stage.jreVerify",
            Stage::JarDownload => "stage.jarDownload",
            Stage::Launching => "stage.launching",
            Stage::Done => "stage.done",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Notice {
    /// A JRE update is being installed; "Play now" starts the launcher with the current JRE instead.
    JreUpdate,
    /// The signed policy names a newer wrapper (AppImage, macOS).
    NewerWrapper { version: String, page: String },
    /// macOS: running from the DMG or a translocated copy.
    Translocated,
    /// An ADR 0008 override is active.
    TestMode,
}

pub trait Reporter: Send + Sync {
    fn stage(&self, stage: Stage);
    fn progress(&self, done: u64, total: u64);
    fn notice(&self, notice: Notice);
}

/// Writes everything to the log only (the fast path, and tests).
pub struct LogReporter;

impl Reporter for LogReporter {
    fn stage(&self, stage: Stage) {
        log::info!("stage {stage:?}");
    }

    fn progress(&self, _done: u64, _total: u64) {}

    fn notice(&self, notice: Notice) {
        log::info!("notice {notice:?}");
    }
}
