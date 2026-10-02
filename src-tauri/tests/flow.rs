//! The whole start without a window: JRE installation, the jar copy, the launch and what happens after it, against a
//! loopback server and fake-java (ADR 0005, 0006, 0008).

mod support;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use chrono::Utc;
use prestarter_lib::flow::{ErrorKind, Launched, Notice, Plan, Session, Stage};
use prestarter_lib::store::InstallLock;
use prestarter_lib::store::state::{PolicyRecord, State};
use support::{Fixture, Recorder, Response, args_of, env_of};

fn window_work(session: &mut Session) -> prestarter_lib::flow::Work {
    match session.plan() {
        Plan::Window(work) => work,
        other => panic!("expected a window, got {other:?}"),
    }
}

#[test]
fn copy_mode_installs_java_fetches_the_jar_and_starts_the_launcher() {
    let fx = Fixture::new("copy");
    fx.serve_jre("25.0.4.1+1");
    fx.serve_jar();
    let mut session = Session::open(fx.context(fx.bare_exe(), "ok", &[], true)).unwrap();
    let work = window_work(&mut session);
    assert!(work.install_jre && work.fetch_jar && work.update_jre.is_none());
    assert!(work.notices.contains(&Notice::TestMode));

    let recorder = Recorder::default();
    assert_eq!(session.run(&work, &recorder).unwrap(), Launched::Confirmed);

    let stages = recorder.stages.lock().unwrap().clone();
    for stage in [
        Stage::JreCheck,
        Stage::JreDownload,
        Stage::JreInstall,
        Stage::JreVerify,
        Stage::JarDownload,
        Stage::Launching,
        Stage::Done,
    ] {
        assert!(stages.contains(&stage), "{stage:?} missing from {stages:?}");
    }
    let progress = recorder.progress.lock().unwrap().clone();
    assert!(!progress.is_empty());

    let state = State::load(&fx.store.join("state.json")).unwrap().0;
    let jre = state.jre.expect("JRE recorded");
    assert_eq!(jre.version, "25.0.4.1+1");
    assert!(jre.dir.starts_with("liberica-25-"));
    assert!(state.jar.is_some());

    let run = fx.run_record(0);
    let jar = fx.store.join("launcher").join("Asterium.jar");
    assert_eq!(args_of(&run), ["-Dlauncher.noJavaCheck=true", "-jar", &jar.to_string_lossy(), "--debug"]);
    let env = env_of(&run);
    assert!(env.contains_key("FAKE_JAVA_DIR"));
    for leaked in ["__GL_THREADED_OPTIMIZATIONS", "__NV_DISABLE_EXPLICIT_SYNC", "WEBKIT_DISABLE_DMABUF_RENDERER"] {
        assert!(!env.contains_key(leaked), "{leaked} must not reach the launcher");
    }
    #[cfg(unix)]
    assert_eq!(run["sid"], run["pid"], "the launcher runs in its own session (setsid)");
    assert!(
        fx.server
            .hits()
            .iter()
            .any(|h| h.starts_with("/api?") && h.contains(&format!("arch={}", support::host_target().api_arch)))
    );

    // The next start needs nothing: no window, no network.
    let hits = fx.server.hits().len();
    let mut again = Session::open(fx.context(fx.bare_exe(), "ok", &[], true)).unwrap();
    assert_eq!(again.plan(), Plan::Launch);
    assert_eq!(fx.server.hits().len(), hits);
}

#[test]
fn embedded_mode_starts_the_launcher_and_returns_at_once() {
    let fx = Fixture::new("embedded");
    fx.serve_jre("25.0.4.1+1");
    let exe = fx.embedded_exe();
    let mut session = Session::open(fx.context(exe.clone(), "stay", &[], false)).unwrap();
    let work = window_work(&mut session);
    assert!(work.install_jre && !work.fetch_jar, "an embedded jar is never fetched");
    let recorder = Recorder::default();
    let started = Instant::now();
    assert_eq!(session.run(&work, &recorder).unwrap(), Launched::Detached);
    let run = fx.run_record(0);
    assert_eq!(args_of(&run)[2], exe.to_string_lossy(), "Gravit sees the prestarter's own file");
    // fake-java stays alive for 30 s; the prestarter did not wait for it.
    assert!(started.elapsed() < Duration::from_secs(25));
    assert_eq!(fx.server.hit_count("/Asterium.jar"), 0);
}

#[test]
fn a_corrupt_jar_copy_is_fetched_again_once() {
    let fx = Fixture::new("corrupt");
    fx.serve_jre("25.0.4.1+1");
    fx.serve_jar();
    let mut session = Session::open(fx.context(fx.bare_exe(), "corrupt,ok", &[], true)).unwrap();
    let work = window_work(&mut session);
    assert_eq!(session.run(&work, &Recorder::default()).unwrap(), Launched::Confirmed);
    assert_eq!(fx.server.hit_count("/Asterium.jar"), 2);
    assert_eq!(fx.runs(), 2);
}

#[test]
fn an_early_launcher_crash_is_retried_with_wait_process_and_shown() {
    let fx = Fixture::new("died");
    fx.serve_jre("25.0.4.1+1");
    fx.serve_jar();
    let mut session = Session::open(fx.context(fx.bare_exe(), "died", &[], true)).unwrap();
    let work = window_work(&mut session);
    let err = session.run(&work, &Recorder::default()).unwrap_err();
    assert_eq!(err.kind, ErrorKind::LaunchFailed);
    assert_eq!(err.exit_code(), 6);
    assert!(err.log_tail.as_deref().unwrap_or("").contains("UnsatisfiedLinkError"), "{:?}", err.log_tail);
    assert!(args_of(&fx.run_record(1)).contains(&"-Dlauncher.waitProcess=true".to_owned()));
    assert_eq!(fx.server.hit_count("/Asterium.jar"), 1, "a crash is not a corrupt jar");
}

#[test]
fn a_wrapper_failure_is_reported_without_fetching_the_jar_again() {
    let fx = Fixture::new("fail");
    fx.serve_jre("25.0.4.1+1");
    fx.serve_jar();
    let mut session = Session::open(fx.context(fx.bare_exe(), "fail", &[], true)).unwrap();
    let work = window_work(&mut session);
    let err = session.run(&work, &Recorder::default()).unwrap_err();
    assert_eq!(err.kind, ErrorKind::LaunchFailed);
    assert!(err.log_tail.unwrap().contains("SecurityException"));
    assert_eq!(fx.server.hit_count("/Asterium.jar"), 1);
}

#[test]
fn a_tampered_jre_is_retried_once_and_never_installed() {
    let fx = Fixture::new("tampered");
    let archive = support::jre_archive("25.0.4.1+1");
    let path = support::archive_name("25.0.4.1+1");
    let wrong = "0".repeat(40);
    fx.server.route(
        "/api",
        Response::json(support::api_answer(&fx.server.url(&path), &archive, "25.0.4.1+1", Some(&wrong))),
    );
    fx.server.route(&path, Response::bytes(archive));
    let mut session = Session::open(fx.context(fx.embedded_exe(), "ok", &[], false)).unwrap();
    let work = window_work(&mut session);
    let err = session.run(&work, &Recorder::default()).unwrap_err();
    assert_eq!(err.kind, ErrorKind::JreDownload);
    assert_eq!(err.exit_code(), 3);
    assert_eq!(fx.server.hit_count(&path), 2, "one automatic retry");
    let jre_dir = fx.store.join("jre");
    let installed: Vec<_> = std::fs::read_dir(&jre_dir).unwrap().flatten().collect();
    assert!(installed.is_empty(), "nothing is left in {}", jre_dir.display());
    assert_eq!(fx.runs(), 0);
}

#[test]
fn play_now_starts_with_the_old_jre_and_asks_again_next_time() {
    let fx = Fixture::new("playnow");
    fx.serve_jre("25.0.4+9");
    let exe = fx.embedded_exe();
    let mut first = Session::open(fx.context(exe.clone(), "ok", &[], false)).unwrap();
    let work = window_work(&mut first);
    first.run(&work, &Recorder::default()).unwrap();

    // A week later the API has a newer JRE, served slowly.
    let state_path = fx.store.join("state.json");
    let mut state = State::load(&state_path).unwrap().0;
    let old_check = Utc::now() - chrono::Duration::days(8);
    state.jre_checked_at = Some(old_check);
    state.save(&state_path).unwrap();
    let archive = support::jre_archive("25.0.4.1+1");
    let path = support::archive_name("25.0.4.1+1");
    fx.server.route("/api", Response::json(support::api_answer(&fx.server.url(&path), &archive, "25.0.4.1+1", None)));
    fx.server
        .route(&path, Response::Slow { body: Arc::new(archive), chunk: 64 * 1024, pause: Duration::from_millis(50) });

    let mut session = Session::open(fx.context(exe, "ok", &[], false)).unwrap();
    let work = window_work(&mut session);
    assert_eq!(work.update_jre.as_ref().map(|r| r.version_text.as_str()), Some("25.0.4.1+1"));
    let flag = session.play_now_flag();
    let recorder =
        Recorder { on_progress: Some(Box::new(move || flag.store(true, Ordering::SeqCst))), ..Recorder::default() };
    assert_eq!(session.run(&work, &recorder).unwrap(), Launched::Detached);
    assert!(recorder.notices.lock().unwrap().contains(&Notice::JreUpdate));
    let state = State::load(&state_path).unwrap().0;
    assert_eq!(state.jre.unwrap().version, "25.0.4+9", "the old JRE is kept");
    assert_eq!(state.jre_checked_at.map(|t| t.timestamp()), Some(old_check.timestamp()), "the update is offered again");
}

#[test]
fn an_update_without_play_now_replaces_the_jre_and_keeps_the_previous_one() {
    let fx = Fixture::new("update");
    fx.serve_jre("25.0.4+9");
    let exe = fx.embedded_exe();
    let mut first = Session::open(fx.context(exe.clone(), "ok", &[], false)).unwrap();
    let work = window_work(&mut first);
    first.run(&work, &Recorder::default()).unwrap();
    let state_path = fx.store.join("state.json");
    let mut state = State::load(&state_path).unwrap().0;
    state.jre_checked_at = Some(Utc::now() - chrono::Duration::days(8));
    state.save(&state_path).unwrap();
    fx.serve_jre("25.0.4.1+1");

    let mut session = Session::open(fx.context(exe, "ok", &[], false)).unwrap();
    let work = window_work(&mut session);
    session.run(&work, &Recorder::default()).unwrap();
    let state = State::load(&state_path).unwrap().0;
    assert_eq!(state.jre.as_ref().unwrap().version, "25.0.4.1+1");
    let previous = state.previous_jre.clone().unwrap();
    assert!(previous.ends_with("25.0.4+9"));
    assert!(fx.store.join("jre").join(&previous).is_dir(), "the previous JRE stays until the next update");
}

#[test]
fn a_wrapper_below_the_signed_minimum_is_refused() {
    let fx = Fixture::new("policy");
    let state_path = fx.store.join("state.json");
    let policy = r#"{"schema":1,"jarUrl":"https://launcher.asterium.pro/Asterium.jar","javaFeature":25,"minWrapperVersion":"9.0.0","latestWrapperVersion":"9.1.0","downloadPage":"https://asterium.pro/launcher"}"#;
    let state = State {
        policy: Some(PolicyRecord {
            release_version: "9.1.0".into(),
            policy_json: policy.into(),
            fetched_at: Utc::now(),
        }),
        policy_checked_at: Some(Utc::now()),
        ..State::default()
    };
    std::fs::create_dir_all(&fx.store).unwrap();
    state.save(&state_path).unwrap();
    let mut session = Session::open(fx.context(fx.bare_exe(), "ok", &[], true)).unwrap();
    match session.plan() {
        Plan::Fail(err) => {
            assert_eq!(err.kind, ErrorKind::WrapperTooOld);
            assert_eq!(err.exit_code(), 7);
            assert_eq!(err.link.as_deref(), Some("https://asterium.pro/launcher"));
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_newer_wrapper_is_announced_once_a_day() {
    let fx = Fixture::new("newer");
    fx.serve_jre("25.0.4.1+1");
    fx.serve_jar();
    let policy = r#"{"schema":1,"jarUrl":"https://launcher.asterium.pro/Asterium.jar","javaFeature":25,"minWrapperVersion":"0.3.0","latestWrapperVersion":"0.9.0","downloadPage":"https://asterium.pro/launcher"}"#;
    let state = State {
        policy: Some(PolicyRecord {
            release_version: "0.9.0".into(),
            policy_json: policy.into(),
            fetched_at: Utc::now(),
        }),
        policy_checked_at: Some(Utc::now()),
        ..State::default()
    };
    std::fs::create_dir_all(&fx.store).unwrap();
    state.save(&fx.store.join("state.json")).unwrap();
    let mut session = Session::open(fx.context(fx.bare_exe(), "ok", &[], true)).unwrap();
    let work = window_work(&mut session);
    assert!(work.notices.iter().any(|n| matches!(n, Notice::NewerWrapper { version, .. } if version == "0.9.0")));
    session.run(&work, &Recorder::default()).unwrap();
    let mut again = Session::open(fx.context(fx.bare_exe(), "ok", &[], true)).unwrap();
    assert_eq!(again.plan(), Plan::Launch, "shown at most once a day");
}

#[test]
fn a_damaged_embedded_jar_is_refused_before_anything_else() {
    let fx = Fixture::new("damaged");
    let exe = fx.embedded_exe();
    let mut bytes = std::fs::read(&exe).unwrap();
    bytes.extend_from_slice(&[0x30; 3000]); // a signature table the EOCD comment does not cover
    std::fs::write(&exe, bytes).unwrap();
    let mut session = Session::open(fx.context(exe, "ok", &[], false)).unwrap();
    match session.plan() {
        Plan::Fail(err) => {
            assert_eq!(err.kind, ErrorKind::EmbeddedCorrupt);
            assert_eq!(err.exit_code(), 4);
            assert!(!err.kind.retryable());
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(fx.server.hits().is_empty());
}

/// Java on Windows reads its command line in the ANSI code page: a jar under a name that code page cannot write is
/// refused with a sentence that says what to do, before anything is downloaded. Elsewhere the same path just works.
#[test]
fn a_jar_under_a_name_java_cannot_read_is_refused_before_any_download() {
    let fx = Fixture::new("ansi");
    // U+1F600 is in no ANSI code page; only the UTF-8 one ("Beta: Use Unicode UTF-8") writes it.
    let exe = fx.embedded_exe_in("Ігри \u{1F600}");
    let mut session = Session::open(fx.context(exe.clone(), "stay", &[], false)).unwrap();
    let plan = session.plan();
    let refused = prestarter_lib::platform::ansi_code_page().is_some_and(|code_page| code_page != 65001);
    if refused {
        match plan {
            Plan::Fail(err) => {
                assert_eq!(err.kind, ErrorKind::PathEncoding);
                assert_eq!(err.exit_code(), 6);
                assert!(!err.kind.retryable());
                let text = err.message(prestarter_lib::i18n::Lang::Uk);
                assert!(text.contains(&exe.display().to_string()), "{text}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    } else {
        assert!(matches!(plan, Plan::Window(_)), "{plan:?}");
    }
    assert!(fx.server.hits().is_empty());
    assert_eq!(fx.runs(), 0);
}

#[test]
fn the_appimage_environment_never_reaches_the_launcher() {
    let fx = Fixture::new("appimage");
    fx.serve_jre("25.0.4.1+1");
    fx.serve_jar();
    let mount = "/tmp/.mount_AsteriTest";
    let owd = fx.dir.path().join("Downloads");
    std::fs::create_dir_all(&owd).unwrap();
    let owd_text = owd.to_string_lossy().into_owned();
    let path_value = format!("{mount}/usr/bin:{}", std::env::var("PATH").unwrap_or_default());
    let extra = [
        ("APPIMAGE", "/home/p/Asterium.AppImage"),
        ("APPDIR", mount),
        ("ARGV0", "./Asterium.AppImage"),
        ("OWD", owd_text.as_str()),
        ("GDK_BACKEND", "x11"),
        ("GTK_THEME", "Adwaita:light"),
        ("LD_LIBRARY_PATH", "/tmp/.mount_AsteriTest/usr/lib:"),
        ("GIO_EXTRA_MODULES", "/tmp/.mount_AsteriTest/usr/lib/gio/modules"),
        ("PYTHONDONTWRITEBYTECODE", "1"),
    ];
    let mut context = fx.context(fx.bare_exe(), "ok", &extra, true);
    if !cfg!(windows) {
        let mut pairs: Vec<_> = context.env.vars().to_vec();
        pairs.retain(|(k, _)| k != "PATH");
        pairs.push(("PATH".into(), path_value.into()));
        context.env = prestarter_lib::launch::EnvSnapshot::from_pairs(pairs);
    }
    let mut session = Session::open(context).unwrap();
    let work = window_work(&mut session);
    session.run(&work, &Recorder::default()).unwrap();
    let run = fx.run_record(0);
    let env = env_of(&run);
    for name in [
        "APPIMAGE",
        "APPDIR",
        "ARGV0",
        "OWD",
        "GDK_BACKEND",
        "GTK_THEME",
        "LD_LIBRARY_PATH",
        "GIO_EXTRA_MODULES",
        "PYTHONDONTWRITEBYTECODE",
    ] {
        assert!(!env.contains_key(name), "{name} leaked into the launcher");
    }
    for (name, value) in &env {
        assert!(!value.contains(mount), "{name}={value} points into the AppImage mount");
    }
    let cwd = std::path::PathBuf::from(run["cwd"].as_str().unwrap());
    assert_eq!(
        dunce::canonicalize(cwd).unwrap(),
        dunce::canonicalize(&owd).unwrap(),
        "the launcher starts where the player started the AppImage"
    );
}

#[test]
fn a_second_start_waits_for_the_install_lock() {
    let fx = Fixture::new("lock");
    fx.serve_jre("25.0.4.1+1");
    std::fs::create_dir_all(&fx.store).unwrap();
    let held = InstallLock::acquire(&fx.store.join(".install.lock"), || {}).unwrap();
    let mut session = Session::open(fx.context(fx.embedded_exe(), "ok", &[], false)).unwrap();
    let work = window_work(&mut session);
    let recorder = Arc::new(Recorder::default());
    let thread_recorder = Arc::clone(&recorder);
    let worker =
        std::thread::spawn(move || session.run(&work, thread_recorder.as_ref()).map(|_| ()).map_err(|e| e.to_string()));
    let started = Instant::now();
    while !recorder.stages.lock().unwrap().contains(&Stage::WaitLock) {
        assert!(started.elapsed() < Duration::from_secs(10), "the second start never reported waiting");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!recorder.stages.lock().unwrap().contains(&Stage::JreDownload));
    drop(held);
    worker.join().unwrap().unwrap();
    assert!(recorder.stages.lock().unwrap().contains(&Stage::JreDownload));
}
