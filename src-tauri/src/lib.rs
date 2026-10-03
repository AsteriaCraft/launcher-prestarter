//! Asterium prestarter: installs Java for the Asterium launcher and starts it, on Windows, Linux and macOS.
//!
//! Layout by role (crossplatform.md 4.1): `app` is the only module that knows Tauri; `flow` decides and runs one
//! start; `jre`, `jar`, `launch`, `policy`, `store`, `net`, `platform` and `i18n` are the pieces it uses.

pub mod app;
pub mod flow;
pub mod i18n;
pub mod jar;
pub mod jre;
pub mod launch;
pub mod net;
pub mod platform;
pub mod policy;
pub mod store;

use std::ffi::OsStr;
use std::panic::AssertUnwindSafe;

use log::{error, info, warn};
use reqwest::Url;
use semver::Version;

use flow::{Context, ErrorKind, FlowError, LogReporter, Plan, Session, Timing};
use i18n::Lang;
use launch::EnvSnapshot;
use net::client::Roots;
use net::overrides::Overrides;
use platform::Host;
use store::StorePaths;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Variables the prestarter sets for its own WebKitGTK window (NVIDIA and DMA-BUF quirks). They are set after the
/// environment snapshot, so they never reach the launcher (ADR 0006); a value the player set is left alone.
#[cfg(target_os = "linux")]
fn set_webkit_variables(snapshot: &EnvSnapshot) {
    for (name, value) in [
        ("__GL_THREADED_OPTIMIZATIONS", "0"),
        ("__NV_DISABLE_EXPLICIT_SYNC", "1"),
        ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
    ] {
        if snapshot.get(name).is_none() {
            // SAFETY: called at the very start of `run`, before any other thread exists.
            unsafe { std::env::set_var(name, value) };
        }
    }
}

fn fail_early(err: &FlowError, lang: Lang, noninteractive: bool) -> i32 {
    error!("{err}");
    let text = err.message(lang);
    eprintln!("{text}");
    #[cfg(windows)]
    if !noninteractive {
        platform::windows::message_box("Asterium", &text, platform::windows::BoxKind::Error);
    }
    let _ = noninteractive;
    err.exit_code()
}

fn start(env: EnvSnapshot) -> i32 {
    let lang = Lang::detect();
    let host = Host::current();
    let home = dirs::home_dir();
    let args: Vec<_> = std::env::args_os().skip(1).collect();

    let overrides = match Overrides::from_lookup(|name| env.get(name).map(OsStr::to_os_string)) {
        Ok(overrides) => overrides,
        Err(err) => {
            store::logs::init(None, home.as_deref());
            error!("{err}");
            eprintln!("{err}");
            return 2;
        }
    };
    let Some(root) = overrides.store.clone().or_else(store::paths::default_root_here) else {
        store::logs::init(None, home.as_deref());
        return fail_early(
            &FlowError::new(ErrorKind::Store, "no data directory").param("path", "?"),
            lang,
            overrides.noninteractive,
        );
    };
    let paths = StorePaths::new(root);
    let _ = std::fs::create_dir_all(paths.logs_dir());
    let log_file = store::logs::init(Some(&paths.logs_dir()), home.as_deref());
    info!("Asterium prestarter {VERSION} ({host}), language {}, log {:?}", lang.code(), log_file);
    for line in overrides.describe() {
        warn!("override {line}");
    }
    if let Some(appdir) = env.appimage_dir() {
        info!("running inside an AppImage mounted at {}", appdir.display());
    }

    let self_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            return fail_early(
                &FlowError::new(ErrorKind::Internal, format!("current_exe: {err}")),
                lang,
                overrides.noninteractive,
            );
        }
    };
    let jre_api = overrides.jre_api.clone().unwrap_or_else(|| Url::parse(jre::api::DEFAULT_API).expect("constant URL"));
    let noninteractive = overrides.noninteractive;
    let context = Context {
        host,
        own_version: Version::parse(VERSION).expect("Cargo version is semver"),
        lang,
        paths,
        overrides,
        env,
        args,
        self_exe,
        jre_api,
        keys: policy::verify::builtin_keys(),
        x64_emulation: platform::x64_emulation_available(),
        liberica_roots: Roots::Platform,
        launcher_roots: Roots::Mozilla,
        timing: Timing::default(),
    };
    let mut session = match Session::open(context) {
        Ok(session) => session,
        Err(err) => return fail_early(&err, lang, noninteractive),
    };
    match session.plan() {
        Plan::Launch => match session.launch(&LogReporter) {
            Ok(launched) => {
                info!("launcher started ({launched:?}) without a window");
                0
            }
            Err(err) => {
                warn!("fast start failed, showing the error: {err}");
                app::run(session, app::Start::ShowError(err))
            }
        },
        Plan::Window(work) => app::run(session, app::Start::Work(work)),
        Plan::Fail(err) => app::run(session, app::Start::ShowError(err)),
    }
}

/// The whole program; returns the process exit code (ADR 0006).
pub fn run() -> i32 {
    // First, before anything can change it: the environment the launcher will inherit.
    let env = EnvSnapshot::capture();
    #[cfg(target_os = "linux")]
    set_webkit_variables(&env);
    net::client::install_crypto_provider();
    std::panic::set_hook(Box::new(|info| {
        error!("panic: {info}");
        eprintln!("panic: {info}");
    }));
    std::panic::catch_unwind(AssertUnwindSafe(|| start(env))).unwrap_or(10)
}
