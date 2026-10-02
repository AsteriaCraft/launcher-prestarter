//! One start of the prestarter, from "what is needed" to "the launcher runs" (crossplatform.md 4.2). Tauri-free:
//! the window, the no-WebView mode and the integration tests drive it through `report::Reporter`.

// A FlowError carries everything the error window shows (sentence parameters, a command, the log tail, a link).
// It is created at most a few times per start, so its size on the error path does not matter.
#![allow(clippy::result_large_err)]

pub mod error;
pub mod report;
pub mod session;

pub use error::{ErrorKind, FlowError};
pub use report::{LogReporter, Notice, Reporter, Stage};
pub use session::{Context, Launched, Plan, Session, Timing, Work};
