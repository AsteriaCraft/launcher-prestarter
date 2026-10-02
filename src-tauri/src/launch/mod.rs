//! Starting the launcher (ADR 0006): command, environment, detached process, and what happened afterwards.

pub mod command;
pub mod environment;
pub mod libs;
pub mod outcome;
pub mod process;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub use environment::EnvSnapshot;
pub use outcome::WrapperOutcome;

/// How long the copy mode watches the wrapper (it exits ~3 s after starting the launcher JVM).
pub const WRAPPER_WATCH: Duration = Duration::from_secs(10);
/// How long the `waitProcess` retry watches: if the launcher still runs then, it started.
pub const RETRY_WATCH: Duration = Duration::from_secs(20);

pub struct Launch<'a> {
    pub java: PathBuf,
    pub jar: &'a Path,
    pub prestarter_args: &'a [OsString],
    pub env: &'a [(OsString, OsString)],
    pub cwd: Option<&'a Path>,
    pub log: &'a Path,
}

/// Embedded mode: start and return at once. The prestarter must exit right after this, because its own file is the
/// jar that Gravit rewrites in place when it updates itself.
pub fn start_and_leave(launch: &Launch<'_>) -> std::io::Result<()> {
    let args = command::launcher_args(launch.jar, false, launch.prestarter_args);
    process::spawn_detached(&process::Spawn {
        java: &launch.java,
        args: &args,
        env: launch.env,
        cwd: launch.cwd,
        log: launch.log,
    })
    .map(drop)
}

/// Copy mode: start, watch the wrapper for up to `watch`, and classify the result from its exit code and log.
/// Returns the outcome and the log text.
pub fn start_and_watch(
    launch: &Launch<'_>,
    wait_process: bool,
    watch: Duration,
) -> std::io::Result<(WrapperOutcome, String)> {
    let args = command::launcher_args(launch.jar, wait_process, launch.prestarter_args);
    let mut child = process::spawn_detached(&process::Spawn {
        java: &launch.java,
        args: &args,
        env: launch.env,
        cwd: launch.cwd,
        log: launch.log,
    })?;
    let code = process::wait_for(&mut child, watch)?;
    let log = std::fs::read(launch.log).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let result = if wait_process { outcome::classify_retry(code, &log) } else { outcome::classify(code, &log) };
    Ok((result, log))
}
