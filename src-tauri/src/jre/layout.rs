//! What an installed JRE must look like before the prestarter trusts it (ADR 0005): the java binary, a `release`
//! file of the expected feature version, and `java -version` exiting with 0. The last check catches a JRE of the
//! wrong CPU or a macOS too old for it before the player sees "the launcher does not start".

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::platform::Os;

/// Generous: the first JVM start from freshly unpacked DLLs can be slow while an antivirus scans them.
pub const JAVA_VERSION_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LayoutError {
    #[error("{0} is missing")]
    Missing(String),
    #[error("release file: {0}")]
    Release(String),
    #[error("`java -version` failed: {0}")]
    JavaVersion(String),
}

/// The binary that starts the launcher: `javaw.exe` on Windows (no console window), `java` elsewhere.
pub fn launcher_java(home: &Path, os: Os) -> PathBuf {
    match os {
        Os::Windows => home.join("bin").join("javaw.exe"),
        _ => home.join("bin").join("java"),
    }
}

/// The console binary, for `java -version`.
pub fn console_java(home: &Path, os: Os) -> PathBuf {
    match os {
        Os::Windows => home.join("bin").join("java.exe"),
        _ => home.join("bin").join("java"),
    }
}

/// `JAVA_VERSION` from the `release` file.
pub fn release_version(home: &Path) -> Result<String, LayoutError> {
    let text = fs::read_to_string(home.join("release")).map_err(|e| LayoutError::Missing(format!("release ({e})")))?;
    parse_release(&text).ok_or_else(|| LayoutError::Release("no JAVA_VERSION".into()))
}

pub fn parse_release(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("JAVA_VERSION="))
        .map(|value| value.trim().trim_matches('"').to_owned())
        .filter(|v| !v.is_empty())
}

/// Files and version only (fast: used on every start).
pub fn check_files(home: &Path, os: Os, feature: u32) -> Result<(), LayoutError> {
    for binary in [launcher_java(home, os), console_java(home, os)] {
        if !binary.is_file() {
            return Err(LayoutError::Missing(binary.display().to_string()));
        }
    }
    let version = release_version(home)?;
    let major = version.split(['.', '+', '-']).next().unwrap_or("");
    if major != feature.to_string() {
        return Err(LayoutError::Release(format!("JAVA_VERSION {version}, expected {feature}")));
    }
    Ok(())
}

/// Runs `java -version` with a timeout and returns its first output line.
pub fn run_java_version(home: &Path, os: Os, timeout: Duration) -> Result<String, LayoutError> {
    let mut command = Command::new(console_java(home, os));
    command.arg("-version").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(crate::platform::windows::NO_WINDOW_CREATION_FLAGS);
    }
    let mut child =
        crate::platform::spawn_when_not_busy(&mut command).map_err(|e| LayoutError::JavaVersion(e.to_string()))?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| LayoutError::JavaVersion(e.to_string()))? {
            break status;
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(LayoutError::JavaVersion(format!("no answer in {} s", timeout.as_secs())));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    let first = stderr.lines().next().unwrap_or("").trim().to_owned();
    if status.success() { Ok(first) } else { Err(LayoutError::JavaVersion(format!("exit {status}: {first}"))) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::atomic::unique_suffix;

    #[test]
    fn parses_the_release_file() {
        let text = "IMPLEMENTOR=\"BellSoft\"\nJAVA_RUNTIME_VERSION=\"25.0.4.1+1-LTS\"\nJAVA_VERSION=\"25.0.4.1\"\n";
        assert_eq!(parse_release(text).as_deref(), Some("25.0.4.1"));
        assert_eq!(parse_release("JAVA_VERSION=\"\"\n"), None);
        assert_eq!(parse_release("X=1\n"), None);
    }

    #[test]
    fn checks_files_and_feature() {
        let home = std::env::temp_dir().join(format!("asterium-layout-{}", unique_suffix()));
        fs::create_dir_all(home.join("bin")).unwrap();
        let os = crate::platform::Host::current().os;
        assert!(matches!(check_files(&home, os, 25), Err(LayoutError::Missing(_))));
        fs::write(launcher_java(&home, os), b"").unwrap();
        fs::write(console_java(&home, os), b"").unwrap();
        assert!(matches!(check_files(&home, os, 25), Err(LayoutError::Missing(_))));
        fs::write(home.join("release"), "JAVA_VERSION=\"21.0.2\"\n").unwrap();
        assert!(matches!(check_files(&home, os, 25), Err(LayoutError::Release(_))));
        fs::write(home.join("release"), "JAVA_VERSION=\"25.0.4.1\"\n").unwrap();
        assert_eq!(check_files(&home, os, 25), Ok(()));
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn binaries_per_os() {
        let home = Path::new("/j");
        assert_eq!(launcher_java(home, Os::Windows), Path::new("/j/bin/javaw.exe"));
        assert_eq!(console_java(home, Os::Windows), Path::new("/j/bin/java.exe"));
        assert_eq!(launcher_java(home, Os::Linux), Path::new("/j/bin/java"));
        assert_eq!(console_java(home, Os::MacOs), Path::new("/j/bin/java"));
    }

    #[test]
    fn a_missing_java_fails_the_version_check() {
        let home = std::env::temp_dir().join(format!("asterium-nojava-{}", unique_suffix()));
        let err = run_java_version(&home, crate::platform::Host::current().os, Duration::from_secs(5)).unwrap_err();
        assert!(matches!(err, LayoutError::JavaVersion(_)));
    }
}
