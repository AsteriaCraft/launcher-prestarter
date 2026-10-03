//! `fake-java`: stands in for `bin/java(w)` in the integration tests. It records how it was started and then behaves
//! like one of the cases Gravit's wrapper produces (ADR 0006). Built by `cargo test` (examples are), never shipped.
//!
//! - `-version`: prints a Java banner, exit 0 (the install check).
//! - otherwise writes `<FAKE_JAVA_DIR>/run-<n>.json` (args, environment, working directory, pid, Unix session id)
//!   and acts out `FAKE_JAVA_PLAN` (comma separated, one action per run, the last one repeats):
//!   `ok` exit 0 · `stay` sleep 30 s · `corrupt` "Invalid or corrupt jarfile", exit 1 · `died` the wrapper's
//!   "Process exit with error code" line, exit 0 (with `-Dlauncher.waitProcess=true`: a stack trace, exit 0) ·
//!   `fail` a SecurityException, exit 1 · `rewrite` after `FAKE_JAVA_REWRITE_MS` (300) rewrites the `-jar` file
//!   in place like Gravit's self-update and records the result in `rewrite.json`.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-version") {
        eprintln!("openjdk version \"25.0.4.1\" 2026-08-18 LTS");
        return;
    }
    let dir = PathBuf::from(std::env::var("FAKE_JAVA_DIR").expect("FAKE_JAVA_DIR is set by the test"));
    fs::create_dir_all(&dir).unwrap();
    let run = (0..).find(|n| !dir.join(format!("run-{n}.json")).exists()).unwrap();
    let plan = std::env::var("FAKE_JAVA_PLAN").unwrap_or_else(|_| "ok".into());
    let actions: Vec<&str> = plan.split(',').collect();
    let action = actions.get(run).or(actions.last()).copied().unwrap_or("ok");

    #[cfg(unix)]
    let sid = unsafe { libc::getsid(0) };
    #[cfg(not(unix))]
    let sid = -1;
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let record = serde_json::json!({
        "args": args,
        "env": env,
        "cwd": std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default(),
        "pid": std::process::id(),
        "sid": sid,
        "action": action,
    });
    fs::write(dir.join(format!("run-{run}.json")), serde_json::to_vec_pretty(&record).unwrap()).unwrap();

    let jar = args.iter().position(|a| a == "-jar").and_then(|i| args.get(i + 1)).cloned().unwrap_or_default();
    let wait_process = args.iter().any(|a| a == "-Dlauncher.waitProcess=true");
    match action {
        "stay" => std::thread::sleep(Duration::from_secs(30)),
        "corrupt" => {
            eprintln!("Error: Invalid or corrupt jarfile {jar}");
            std::process::exit(1);
        }
        "died" if wait_process => {
            eprintln!("Exception in thread \"main\" java.lang.UnsatisfiedLinkError: no glassgtk3 in java.library.path");
            eprintln!("\tat java.base/java.lang.ClassLoader.loadLibrary(ClassLoader.java:2458)");
        }
        "died" => {
            eprintln!("[main] ERROR pro.gravit.launcher.start.ClientLauncherWrapper - Process exit with error code: 1");
        }
        "fail" => {
            eprintln!(
                "Exception in thread \"main\" java.lang.SecurityException: JavaAgent in global options not allow"
            );
            std::process::exit(1);
        }
        "rewrite" => {
            let delay = std::env::var("FAKE_JAVA_REWRITE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
            std::thread::sleep(Duration::from_millis(delay));
            let result = fs::read(&jar).and_then(|bytes| {
                // Exactly what LauncherBackendImpl's update hook does: CREATE, WRITE, TRUNCATE_EXISTING on the jar.
                let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(&jar)?;
                file.write_all(&bytes)
            });
            let outcome = match result {
                Ok(()) => serde_json::json!({ "ok": true, "afterMs": delay }),
                Err(err) => serde_json::json!({ "ok": false, "afterMs": delay, "error": err.to_string() }),
            };
            fs::write(dir.join("rewrite.json"), serde_json::to_vec(&outcome).unwrap()).unwrap();
        }
        _ => {}
    }
}
