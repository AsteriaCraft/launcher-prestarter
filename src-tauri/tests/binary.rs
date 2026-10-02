//! The real `Prestarter` binary (the one cargo builds for this test run), started the way a player starts it.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use chrono::Utc;
use prestarter_lib::jre::catalog;
use prestarter_lib::store::state::{JreRecord, JreSource, State};
use support::{Fixture, test_jar};

fn prestarter() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_Prestarter"))
}

/// A store with an installed fake JRE whose update check is fresh: the next start takes the fast path.
fn installed_store(store: &Path) {
    let target = support::host_target();
    let dir = catalog::dir_name(&target, 25, "25.0.4.1+1");
    let home = store.join("jre").join(&dir);
    fs::create_dir_all(home.join("bin")).unwrap();
    let fake = support::fake_java_binary();
    let names: &[&str] = if cfg!(windows) { &["java.exe", "javaw.exe"] } else { &["java"] };
    for name in names {
        fs::copy(&fake, home.join("bin").join(name)).unwrap();
    }
    fs::write(home.join("release"), "JAVA_VERSION=\"25.0.4.1\"\n").unwrap();
    let state = State {
        jre: Some(JreRecord {
            dir,
            version: "25.0.4.1+1".into(),
            feature: 25,
            target: target.key.into(),
            source: JreSource::Api,
            archive_sha256: "0".repeat(64),
            installed_at: Utc::now(),
        }),
        jre_checked_at: Some(Utc::now()),
        ..State::default()
    };
    state.save(&store.join("state.json")).unwrap();
}

fn embedded_copy(dir: &Path) -> PathBuf {
    let name = if cfg!(windows) { "Asterium.exe" } else { "Asterium_linux" };
    let path = dir.join(support::readable_folder()).join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut bytes = fs::read(prestarter()).unwrap();
    bytes.extend(test_jar());
    fs::write(&path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

fn command(exe: &Path, fx: &Fixture, plan: &str) -> Command {
    let mut command = Command::new(exe);
    command
        .env("ASTERIUM_PRESTARTER_STORE", &fx.store)
        .env("ASTERIUM_PRESTARTER_NONINTERACTIVE", "1")
        .env("FAKE_JAVA_DIR", &fx.java_dir)
        .env("FAKE_JAVA_PLAN", plan)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn wait_exit(child: &mut std::process::Child, limit: Duration) -> i32 {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status.code().unwrap_or(-1);
        }
        assert!(started.elapsed() < limit, "the prestarter did not exit within {limit:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// ADR 0006 (review finding 2): Gravit rewrites the embedded file in place when it updates itself; the
/// prestarter must already be gone by then. fake-java plays Gravit and rewrites the file 500 ms after its start.
#[test]
fn embedded_prestarter_exits_before_gravit_rewrites_its_file() {
    let fx = Fixture::new("binary-rewrite");
    installed_store(&fx.store);
    let exe = embedded_copy(fx.dir.path());
    let started = Instant::now();
    let mut child = command(&exe, &fx, "rewrite").env("FAKE_JAVA_REWRITE_MS", "500").spawn().unwrap();
    let code = wait_exit(&mut child, Duration::from_secs(20));
    let exited_after = started.elapsed();
    assert_eq!(code, 0);
    let run = fx.run_record(0);
    let args = support::args_of(&run);
    assert_eq!(args[..2], ["-Dlauncher.noJavaCheck=true".to_owned(), "-jar".to_owned()]);
    assert_eq!(fs::canonicalize(&args[2]).unwrap(), fs::canonicalize(&exe).unwrap());
    assert!(!args[2].starts_with(r"\\?\"), "no verbatim prefix for Gravit: {}", args[2]);

    let result_path = fx.java_dir.join("rewrite.json");
    let waited = Instant::now();
    while !result_path.exists() {
        assert!(waited.elapsed() < Duration::from_secs(10), "fake-java never tried to rewrite the file");
        std::thread::sleep(Duration::from_millis(20));
    }
    let result: serde_json::Value = serde_json::from_slice(&fs::read(&result_path).unwrap()).unwrap();
    assert_eq!(result["ok"], true, "rewrite failed (prestarter exited after {exited_after:?}): {result}");
}

#[test]
fn a_launcher_url_outside_loopback_is_refused_with_exit_code_2() {
    let fx = Fixture::new("binary-override");
    let mut child = command(&prestarter(), &fx, "ok")
        .env("ASTERIUM_PRESTARTER_LAUNCHER_URL", "https://launcher.asterium.pro/Asterium.jar")
        .spawn()
        .unwrap();
    assert_eq!(wait_exit(&mut child, Duration::from_secs(20)), 2);
    assert_eq!(fx.runs(), 0);
}

#[test]
fn the_log_names_the_home_directory_as_tilde() {
    let fx = Fixture::new("binary-log");
    installed_store(&fx.store);
    let exe = embedded_copy(fx.dir.path());
    let mut child = command(&exe, &fx, "ok").spawn().unwrap();
    assert_eq!(wait_exit(&mut child, Duration::from_secs(20)), 0);
    let log = fs::read_to_string(fx.store.join("logs").join("prestarter-1.log")).unwrap();
    assert!(log.contains("Asterium prestarter"), "{log}");
    assert!(log.contains("override ASTERIUM_PRESTARTER_STORE="), "{log}");
    if let Some(home) = dirs::home_dir() {
        assert!(!log.contains(&*home.to_string_lossy()), "the home directory must be written as ~:\n{log}");
    }
}
