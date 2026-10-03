//! What happened to Gravit's wrapper, read from its exit code and `launcher-start.log` (ADR 0006, copy mode).
//!
//! The wrapper exits with 0 whenever it got as far as starting the launcher JVM; that JVM's early death only shows
//! as the wrapper's log line `Process exit with error code: N`. The wrapper fails by itself on a damaged jar
//! (`Invalid or corrupt jarfile`, `Unable to access jarfile`, a `ZipException`) or on settings it refuses.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapperOutcome {
    /// Still running after the watch period, or exited cleanly: the launcher is up.
    Started,
    /// The jar copy is damaged: fetch it again once and retry.
    CorruptJar,
    /// The launcher JVM died within the wrapper's 3 s: retry once with `waitProcess` to capture why.
    LauncherDied,
    /// The wrapper failed for another reason (for example an agent in `JAVA_OPTS`): show the log.
    WrapperFailed(i32),
}

const CORRUPT_JAR_MARKERS: &[&str] =
    &["Invalid or corrupt jarfile", "Unable to access jarfile", "java.util.zip.ZipException"];
const LAUNCHER_DIED_MARKER: &str = "Process exit with error code";

/// `exit_code`: `None` while the wrapper still runs at the end of the watch.
pub fn classify(exit_code: Option<i32>, log: &str) -> WrapperOutcome {
    match exit_code {
        None => WrapperOutcome::Started,
        Some(0) if log.contains(LAUNCHER_DIED_MARKER) => WrapperOutcome::LauncherDied,
        Some(0) => WrapperOutcome::Started,
        Some(_) if CORRUPT_JAR_MARKERS.iter().any(|m| log.contains(m)) => WrapperOutcome::CorruptJar,
        Some(code) => WrapperOutcome::WrapperFailed(code),
    }
}

/// The `waitProcess` retry: the wrapper now waits for the launcher JVM, so any exit within the watch means the
/// launcher stopped again (its own output, the stack trace, is in the log).
pub fn classify_retry(exit_code: Option<i32>, log: &str) -> WrapperOutcome {
    match exit_code {
        None => WrapperOutcome::Started,
        Some(_) if CORRUPT_JAR_MARKERS.iter().any(|m| log.contains(m)) => WrapperOutcome::CorruptJar,
        Some(_) => WrapperOutcome::LauncherDied,
    }
}

/// The last `lines` lines of a log, for the error window.
pub fn tail(log: &str, lines: usize) -> String {
    let all: Vec<&str> = log.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_of_the_adr_table() {
        assert_eq!(classify(None, ""), WrapperOutcome::Started);
        assert_eq!(
            classify(Some(0), "[main] INFO ClientLauncherWrapper - Process exit with code 0"),
            WrapperOutcome::Started
        );
        assert_eq!(
            classify(
                Some(0),
                "[main] ERROR pro.gravit.launcher.start.ClientLauncherWrapper - Process exit with error code: 1"
            ),
            WrapperOutcome::LauncherDied
        );
        assert_eq!(
            classify(Some(1), "Error: Invalid or corrupt jarfile /s/launcher/Asterium.jar"),
            WrapperOutcome::CorruptJar
        );
        assert_eq!(classify(Some(1), "Error: Unable to access jarfile /s/x.jar"), WrapperOutcome::CorruptJar);
        assert_eq!(
            classify(
                Some(1),
                "Exception in thread \"main\" java.lang.SecurityException: JavaAgent in global options not allow"
            ),
            WrapperOutcome::WrapperFailed(1)
        );
    }

    #[test]
    fn retry_rows() {
        assert_eq!(classify_retry(None, ""), WrapperOutcome::Started);
        assert_eq!(
            classify_retry(Some(0), "Exception in thread \"main\" java.lang.UnsatisfiedLinkError"),
            WrapperOutcome::LauncherDied
        );
        assert_eq!(classify_retry(Some(1), "Error: Invalid or corrupt jarfile x"), WrapperOutcome::CorruptJar);
    }

    #[test]
    fn tails() {
        assert_eq!(tail("a\nb\nc\nd", 2), "c\nd");
        assert_eq!(tail("a", 5), "a");
        assert_eq!(tail("", 5), "");
    }
}
