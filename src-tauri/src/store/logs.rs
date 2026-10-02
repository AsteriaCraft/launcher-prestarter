//! The prestarter's own log: `logs/prestarter-1.log` is this run, `-2` … `-5` the runs before (ADR 0005). Each file
//! stops at 1 MiB. The home directory is written as `~`, so a log can be shared with support as is. Warnings and
//! errors are mirrored to stderr (a terminal start on Linux or macOS shows them).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use log::{Level, LevelFilter, Log, Metadata, Record};

pub const KEEP_RUNS: usize = 5;
pub const MAX_BYTES: u64 = 1024 * 1024;
const CRATE_TARGET: &str = "prestarter_lib";

pub fn log_file(dir: &Path, run: usize) -> PathBuf {
    dir.join(format!("prestarter-{run}.log"))
}

/// Shifts `prestarter-1.log` … `-4` up by one; the oldest falls off.
pub fn rotate(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let _ = fs::remove_file(log_file(dir, KEEP_RUNS));
    for run in (1..KEEP_RUNS).rev() {
        let from = log_file(dir, run);
        if from.exists() {
            fs::rename(&from, log_file(dir, run + 1))?;
        }
    }
    Ok(())
}

/// Replaces every occurrence of the home directory (with either separator) by `~`.
pub fn redact_home(line: &str, home: Option<&str>) -> String {
    let Some(home) = home.filter(|h| h.len() > 1) else {
        return line.to_owned();
    };
    let home = home.trim_end_matches(['/', '\\']);
    let alternate = if home.contains('\\') { home.replace('\\', "/") } else { home.replace('/', "\\") };
    line.replace(home, "~").replace(&alternate, "~")
}

struct Sink {
    file: Option<File>,
    written: u64,
    full: bool,
}

pub struct FileLogger {
    sink: Mutex<Sink>,
    home: Option<String>,
    mirror_stderr: bool,
}

impl FileLogger {
    pub fn open(path: &Path, home: Option<&Path>) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).write(true).truncate(true).open(path)?;
        Ok(Self {
            sink: Mutex::new(Sink { file: Some(file), written: 0, full: false }),
            home: home.map(|h| h.to_string_lossy().into_owned()),
            mirror_stderr: true,
        })
    }

    /// A logger that only writes to stderr (used before the store exists, or when it cannot be created).
    pub fn stderr_only(home: Option<&Path>) -> Self {
        Self {
            sink: Mutex::new(Sink { file: None, written: 0, full: false }),
            home: home.map(|h| h.to_string_lossy().into_owned()),
            mirror_stderr: true,
        }
    }

    fn write_line(&self, level: Level, target: &str, message: &str) {
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");
        let short_target = target.strip_prefix(CRATE_TARGET).map(|t| t.trim_start_matches("::")).unwrap_or(target);
        let line = redact_home(&format!("{now} {level:<5} [{short_target}] {message}\n"), self.home.as_deref());
        if self.mirror_stderr && level <= Level::Warn {
            let _ = io::stderr().write_all(line.as_bytes());
        }
        let Ok(mut sink) = self.sink.lock() else { return };
        if sink.full {
            return;
        }
        let Sink { file, written, full } = &mut *sink;
        let Some(file) = file.as_mut() else { return };
        if *written + line.len() as u64 > MAX_BYTES {
            let _ = file.write_all(b"... log truncated at 1 MiB\n");
            *full = true;
            return;
        }
        if file.write_all(line.as_bytes()).is_ok() {
            *written += line.len() as u64;
        }
    }
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        if metadata.target().starts_with(CRATE_TARGET) {
            metadata.level() <= Level::Info
        } else {
            metadata.level() <= Level::Warn
        }
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            self.write_line(record.level(), record.target(), &record.args().to_string());
        }
    }

    fn flush(&self) {
        if let Ok(mut sink) = self.sink.lock()
            && let Some(file) = sink.file.as_mut()
        {
            let _ = file.flush();
        }
    }
}

/// Rotates the logs in `dir`, opens this run's file and installs the global logger. Falls back to stderr when the
/// directory cannot be written. Calling it twice keeps the first logger (the `log` crate allows one).
pub fn init(dir: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    let (logger, path) = match dir.map(|d| (rotate(d), log_file(d, 1))) {
        Some((Ok(()), path)) => match FileLogger::open(&path, home) {
            Ok(logger) => (logger, Some(path)),
            Err(_) => (FileLogger::stderr_only(home), None),
        },
        _ => (FileLogger::stderr_only(home), None),
    };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::atomic::unique_suffix;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("asterium-logs-{}", unique_suffix()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn keeps_the_last_five_runs() {
        let dir = temp_dir();
        for run in 1..=7 {
            rotate(&dir).unwrap();
            fs::write(log_file(&dir, 1), format!("run {run}")).unwrap();
        }
        let names: Vec<String> = (1..=KEEP_RUNS).map(|r| fs::read_to_string(log_file(&dir, r)).unwrap()).collect();
        assert_eq!(names, ["run 7", "run 6", "run 5", "run 4", "run 3"]);
        assert!(!log_file(&dir, 6).exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn home_is_redacted_with_either_separator() {
        assert_eq!(redact_home(r"C:\Users\Макс\AppData\x", Some(r"C:\Users\Макс")), r"~\AppData\x");
        assert_eq!(redact_home("C:/Users/Макс/AppData", Some(r"C:\Users\Макс")), "~/AppData");
        assert_eq!(redact_home("/home/p/.local/share", Some("/home/p/")), "~/.local/share");
        assert_eq!(redact_home("/x", Some("/")), "/x", "a root home is never replaced");
        assert_eq!(redact_home("/x", None), "/x");
    }

    #[test]
    fn a_file_stops_at_one_mebibyte() {
        let dir = temp_dir();
        let path = log_file(&dir, 1);
        let logger = FileLogger { mirror_stderr: false, ..FileLogger::open(&path, None).unwrap() };
        let chunk = "x".repeat(1000);
        for _ in 0..1200 {
            logger.write_line(Level::Info, "prestarter_lib::test", &chunk);
        }
        logger.flush();
        let size = fs::metadata(&path).unwrap().len();
        assert!(size <= MAX_BYTES + 64, "size {size}");
        assert!(fs::read_to_string(&path).unwrap().ends_with("... log truncated at 1 MiB\n"));
        drop(logger);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn our_info_is_kept_and_foreign_info_is_dropped() {
        let logger = FileLogger::stderr_only(None);
        let ours = Metadata::builder().level(Level::Info).target("prestarter_lib::jre").build();
        let foreign = Metadata::builder().level(Level::Info).target("reqwest::connect").build();
        let foreign_warn = Metadata::builder().level(Level::Warn).target("reqwest::connect").build();
        assert!(logger.enabled(&ours));
        assert!(!logger.enabled(&foreign));
        assert!(logger.enabled(&foreign_warn));
    }
}
