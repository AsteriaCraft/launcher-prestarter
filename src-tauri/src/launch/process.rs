//! Starting Gravit's wrapper as a detached process (ADR 0006): its own session on Unix (`setsid`), no console and
//! its own process group on Windows, stdin from null, stdout and stderr into `launcher-start.log`.

use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub struct Spawn<'a> {
    pub java: &'a Path,
    pub args: &'a [OsString],
    pub env: &'a [(OsString, OsString)],
    pub cwd: Option<&'a Path>,
    pub log: &'a Path,
}

pub fn spawn_detached(spec: &Spawn<'_>) -> io::Result<Child> {
    if let Some(parent) = spec.log.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let stdout = File::create(spec.log)?;
    let stderr = stdout.try_clone()?;
    let mut command = Command::new(spec.java);
    command
        .args(spec.args)
        .env_clear()
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    if let Some(cwd) = spec.cwd {
        command.current_dir(cwd);
        command.env("PWD", cwd);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(crate::platform::windows::DETACHED_CREATION_FLAGS);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: the closure only calls setsid, which is async-signal-safe (required between fork and exec).
        unsafe {
            command.pre_exec(crate::platform::unix::new_session);
        }
    }
    command.spawn()
}

/// Waits up to `timeout` for `child`; `None` means it is still running.
pub fn wait_for(child: &mut Child, timeout: Duration) -> io::Result<Option<i32>> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status.code().unwrap_or(-1)));
        }
        if started.elapsed() >= timeout {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
