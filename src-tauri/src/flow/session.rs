//! One start of the prestarter (crossplatform.md 4.2): decide whether the launcher can start right away (no window,
//! no network) or what has to happen first, then do it and start the launcher. No Tauri here: the window, the
//! no-WebView mode and the integration tests all drive this through [`Reporter`].

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::Utc;
use log::{info, warn};
use reqwest::Url;
use reqwest::blocking::Client;
use semver::Version;

use super::error::{ErrorKind, FlowError};
use super::report::{Notice, Reporter, Stage};
use crate::i18n::{self, Lang};
use crate::jar::{self, JarSource, fetch};
use crate::jre::api::{self, JreRelease};
use crate::jre::catalog::{self, JreTarget};
use crate::jre::install::{self, InstallError, InstallObserver, InstallStage, InstalledJre};
use crate::jre::{self, fallback, layout};
use crate::launch::{self, EnvSnapshot, WrapperOutcome, environment};
use crate::net::client::{self, Roots};
use crate::net::download::DownloadError;
use crate::net::overrides::Overrides;
use crate::platform::{Host, Os};
use crate::policy::{self, Policy, Verdict};
use crate::store::state::{JreRecord, Loaded, PolicyRecord, State};
use crate::store::{InstallLock, StorePaths};

/// Everything a start needs to know, decided by `main` (or by a test).
pub struct Context {
    pub host: Host,
    pub own_version: Version,
    pub lang: Lang,
    pub paths: StorePaths,
    pub overrides: Overrides,
    pub env: EnvSnapshot,
    pub args: Vec<OsString>,
    pub self_exe: PathBuf,
    pub jre_api: Url,
    pub keys: Vec<[u8; 32]>,
    pub x64_emulation: bool,
    pub liberica_roots: Roots,
    pub launcher_roots: Roots,
    pub timing: Timing,
}

#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// The background JRE update check (never blocks play for longer).
    pub api_check: Duration,
    pub wrapper_watch: Duration,
    pub retry_watch: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            api_check: Duration::from_secs(3),
            wrapper_watch: launch::WRAPPER_WATCH,
            retry_watch: launch::RETRY_WATCH,
        }
    }
}

/// What has to happen before the launcher starts. Empty work = start right away, without a window.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Work {
    pub install_jre: bool,
    pub update_jre: Option<JreRelease>,
    pub fetch_jar: bool,
    pub notices: Vec<Notice>,
}

impl Work {
    pub fn is_empty(&self) -> bool {
        !self.install_jre && self.update_jre.is_none() && !self.fetch_jar && self.notices.is_empty()
    }

    /// Whether the player will wait for a download (the no-WebView mode says so first).
    pub fn downloads(&self) -> bool {
        self.install_jre || self.update_jre.is_some() || self.fetch_jar
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    Launch,
    Window(Work),
    Fail(FlowError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launched {
    /// Embedded jar: the process was created and the prestarter leaves.
    Detached,
    /// Copy mode: the wrapper was watched and the launcher is up.
    Confirmed,
}

pub struct Session {
    ctx: Context,
    state: State,
    jar: Result<JarSource, FlowError>,
    target: JreTarget,
    policy: Policy,
    play_now: Arc<AtomicBool>,
    liberica: Option<Client>,
    launcher: Option<Client>,
}

fn internal(detail: impl std::fmt::Display) -> FlowError {
    FlowError::new(ErrorKind::Internal, detail.to_string())
}

impl Session {
    pub fn open(ctx: Context) -> Result<Session, FlowError> {
        for dir in [ctx.paths.root().to_path_buf(), ctx.paths.jre_dir(), ctx.paths.logs_dir(), ctx.paths.tmp_dir()] {
            std::fs::create_dir_all(&dir).map_err(|e| {
                FlowError::new(ErrorKind::Store, e.to_string()).param("path", dir.display().to_string())
            })?;
        }
        let (state, loaded) =
            State::load(&ctx.paths.state_file()).map_err(|e| FlowError::new(ErrorKind::Store, e.to_string()))?;
        if let Loaded::Reset(reason) = &loaded {
            warn!("state.json was reset: {reason}");
        }
        let jar = jar::source::detect(&ctx.self_exe, ctx.paths.launcher_jar()).map_err(|err| {
            FlowError::new(ErrorKind::EmbeddedCorrupt, format!("{}: {err}", ctx.self_exe.display()))
                .param("path", ctx.self_exe.display().to_string())
        });
        match &jar {
            Ok(JarSource::Embedded { info, .. }) => {
                info!("embedded jar after {} bytes, Main-Class {}", info.prefix_len, info.main_class)
            }
            Ok(JarSource::Copy { path }) => info!("copy mode, jar {}", path.display()),
            Err(err) => warn!("{err}"),
        }
        let target = catalog::for_host(ctx.host, ctx.x64_emulation);
        if target.degraded {
            warn!(
                "this Windows cannot run x64 code: installing the native ARM64 JRE (no embedded web pages, ADR 0004)"
            );
        }
        let policy = policy::effective(state.policy.as_ref());
        Ok(Session {
            ctx,
            state,
            jar,
            target,
            policy,
            play_now: Arc::new(AtomicBool::new(false)),
            liberica: None,
            launcher: None,
        })
    }

    pub fn context(&self) -> &Context {
        &self.ctx
    }

    /// Set by "Play now": stops a JRE update and launches with the current JRE.
    pub fn play_now_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.play_now)
    }

    fn is_copy_mode(&self) -> bool {
        matches!(self.jar, Ok(JarSource::Copy { .. }))
    }

    fn feature(&self) -> u32 {
        if self.is_copy_mode() { self.policy.java_feature } else { jre::DEFAULT_FEATURE }
    }

    fn jar_url(&self) -> Url {
        self.ctx.overrides.launcher_url.clone().unwrap_or_else(|| self.policy.jar_url())
    }

    fn liberica(&mut self) -> Result<Client, FlowError> {
        if self.liberica.is_none() {
            self.liberica = Some(client::liberica(self.ctx.liberica_roots.clone()).map_err(internal)?);
        }
        Ok(self.liberica.clone().expect("set above"))
    }

    fn launcher_client(&mut self) -> Result<Client, FlowError> {
        if self.launcher.is_none() {
            self.launcher = Some(client::launcher_host(self.ctx.launcher_roots.clone()).map_err(internal)?);
        }
        Ok(self.launcher.clone().expect("set above"))
    }

    fn save_state(&self) {
        if let Err(err) = self.state.save(&self.ctx.paths.state_file()) {
            warn!("cannot save state.json: {err}");
        }
    }

    /// The installed JRE if it is usable for this target and Java version.
    fn current_jre(&self) -> Option<(JreRecord, PathBuf)> {
        let record = self.state.jre.clone()?;
        if record.target != self.target.key || record.feature != self.feature() {
            return None;
        }
        let home = self.ctx.paths.jre_home(&record.dir);
        match layout::check_files(&home, self.ctx.host.os, record.feature) {
            Ok(()) => Some((record, home)),
            Err(err) => {
                warn!("installed JRE {} is not usable: {err}", record.dir);
                None
            }
        }
    }

    fn refresh_policy(&mut self) {
        if self.ctx.overrides.launcher_url.is_some() {
            info!("policy: not fetched while the launcher URL is overridden (test mode)");
            return;
        }
        let now = Utc::now();
        if !policy::refresh_due(self.state.policy_checked_at, now) {
            return;
        }
        self.state.policy_checked_at = Some(now);
        let client = match self.launcher_client() {
            Ok(client) => client,
            Err(err) => return warn!("policy: {err}"),
        };
        let base = policy::base_for(&self.policy.jar_url());
        match policy::fetch(&client, &base, &self.ctx.keys) {
            Ok(fresh) if policy::accept(self.state.policy.as_ref(), &fresh) => {
                info!("policy from release {} accepted", fresh.release_version);
                self.state.policy = Some(PolicyRecord {
                    release_version: fresh.release_version.to_string(),
                    policy_json: fresh.policy_json,
                    fetched_at: now,
                });
                self.policy = fresh.policy;
            }
            Ok(fresh) => warn!("policy from older release {} ignored (anti-rollback)", fresh.release_version),
            Err(err) => warn!("policy not refreshed: {err}"),
        }
    }

    /// Windows: Java reads its command line in the ANSI code page, so a jar or JRE under a name that code page cannot
    /// write would fail inside Java ("Unable to access jarfile" with `?` in the path), and so would Gravit's own
    /// relaunch. Refused up front with a sentence that says what to do, before 130 MB of Java are downloaded.
    fn path_encoding_error(&self) -> Option<FlowError> {
        let jar = self.jar.as_ref().ok()?.path().to_path_buf();
        let jre_dir = self.ctx.paths.jre_dir();
        [jar.as_path(), jre_dir.as_path()].into_iter().find_map(|path| {
            crate::platform::java_cannot_write(path).map(|code_page| {
                FlowError::new(
                    ErrorKind::PathEncoding,
                    format!("the ANSI code page {code_page} cannot write {}", path.display()),
                )
                .param("path", path.display().to_string())
            })
        })
    }

    /// Decides what this start needs. Network only for the weekly JRE check and the daily policy (3 s each).
    pub fn plan(&mut self) -> Plan {
        if let Err(err) = &self.jar {
            return Plan::Fail(err.clone());
        }
        if let Some(err) = self.path_encoding_error() {
            return Plan::Fail(err);
        }
        let mut work = Work::default();
        if self.is_copy_mode() {
            self.refresh_policy();
            match policy::verdict(&self.ctx.own_version, &self.policy) {
                Verdict::TooOld { min, page } => {
                    self.save_state();
                    let mut err = FlowError::new(
                        ErrorKind::WrapperTooOld,
                        format!("wrapper {} < minimum {min}", self.ctx.own_version),
                    );
                    err.link = Some(page);
                    return Plan::Fail(err);
                }
                Verdict::NewerAvailable { latest, page } => {
                    let now = Utc::now();
                    let shown_recently = self.state.wrapper_notice_at.is_some_and(|at| {
                        at <= now && now.signed_duration_since(at) < chrono::Duration::hours(policy::REFRESH_HOURS)
                    });
                    if !shown_recently {
                        self.state.wrapper_notice_at = Some(now);
                        work.notices.push(Notice::NewerWrapper { version: latest.to_string(), page });
                    }
                }
                Verdict::Current => {}
            }
            if !fetch::copy_is_valid(&self.ctx.paths.launcher_jar()) {
                work.fetch_jar = true;
            }
        }

        match self.current_jre() {
            None => work.install_jre = true,
            Some((record, _)) => {
                if jre::update_check_due(self.state.jre_checked_at, Utc::now()) {
                    match self.liberica().and_then(|client| {
                        api::fetch(
                            &client,
                            &self.ctx.jre_api,
                            &self.target,
                            self.feature(),
                            Some(self.ctx.timing.api_check),
                        )
                        .map_err(internal)
                    }) {
                        Ok(release) if jre::is_upgrade(&record, &release.version) => {
                            info!("JRE update available: {} -> {}", record.version, release.version_text);
                            work.update_jre = Some(release);
                        }
                        Ok(_) => {
                            info!("JRE {} is current", record.version);
                            self.state.jre_checked_at = Some(Utc::now());
                        }
                        Err(err) => {
                            info!("JRE update check skipped: {}", err.detail);
                            self.state.jre_checked_at = Some(Utc::now());
                        }
                    }
                }
            }
        }

        if !work.is_empty() {
            if self.ctx.overrides.is_test_mode() {
                work.notices.push(Notice::TestMode);
            }
            if self.ctx.host.os == Os::MacOs && crate::platform::macos::is_translocated_or_on_dmg(&self.ctx.self_exe) {
                work.notices.push(Notice::Translocated);
            }
        }
        self.save_state();
        if work.is_empty() { Plan::Launch } else { Plan::Window(work) }
    }

    /// Does `work`, then starts the launcher.
    pub fn run(&mut self, work: &Work, reporter: &dyn Reporter) -> Result<Launched, FlowError> {
        for notice in &work.notices {
            reporter.notice(notice.clone());
        }
        if work.downloads() {
            reporter.stage(Stage::Preparing);
            let lock =
                InstallLock::acquire(&self.ctx.paths.lock_file(), || reporter.stage(Stage::WaitLock)).map_err(|e| {
                    FlowError::new(ErrorKind::Store, e.to_string())
                        .param("path", self.ctx.paths.root().display().to_string())
                })?;
            // Another start may have finished the same work while we waited.
            if let Ok((fresh, _)) = State::load(&self.ctx.paths.state_file()) {
                self.state = fresh;
            }
            install::clean_tmp(&self.ctx.paths.tmp_dir());
            if work.install_jre && self.current_jre().is_none() {
                self.install_jre(None, reporter)?;
            } else if let Some(release) = &work.update_jre {
                reporter.notice(Notice::JreUpdate);
                match self.install_jre(Some(release.clone()), reporter) {
                    Ok(()) => {}
                    Err(err) if self.play_now.load(Ordering::Relaxed) => info!("Play now: {}", err.detail),
                    Err(err) if self.current_jre().is_some() => {
                        warn!("JRE update failed, starting with the current JRE: {err}");
                    }
                    Err(err) => return Err(err),
                }
            }
            if work.fetch_jar && !fetch::copy_is_valid(&self.ctx.paths.launcher_jar()) {
                self.fetch_jar(reporter)?;
            }
            drop(lock);
        }
        self.launch(reporter)
    }

    fn install_jre(&mut self, release: Option<JreRelease>, reporter: &dyn Reporter) -> Result<(), FlowError> {
        reporter.stage(Stage::JreCheck);
        let client = self.liberica()?;
        let feature = self.feature();
        let release = match release {
            Some(release) => release,
            None => match api::fetch(&client, &self.ctx.jre_api, &self.target, feature, None) {
                Ok(release) => release,
                Err(err) => {
                    warn!("Liberica API failed ({err}); using the built-in fallback");
                    fallback::release_for(&self.target, feature).ok_or_else(|| {
                        FlowError::new(
                            ErrorKind::JreDownload,
                            format!("API failed ({err}) and no fallback for {} / Java {feature}", self.target.key),
                        )
                    })?
                }
            },
        };
        info!("installing JRE {} from {} ({:?})", release.version_text, release.url, release.source);

        struct Observer<'a> {
            reporter: &'a dyn Reporter,
        }
        impl InstallObserver for Observer<'_> {
            fn stage(&mut self, stage: InstallStage) {
                self.reporter.stage(match stage {
                    InstallStage::Download => Stage::JreDownload,
                    InstallStage::Unpack => Stage::JreInstall,
                    InstallStage::Verify => Stage::JreVerify,
                });
            }
            fn progress(&mut self, done: u64, total: u64) {
                self.reporter.progress(done, total);
            }
        }

        let cancel = Arc::clone(&self.play_now);
        let result = install::install(
            &self.ctx.paths,
            &client,
            &release,
            &self.target,
            feature,
            self.ctx.host.os,
            &cancel,
            &mut Observer { reporter },
        );
        let installed: InstalledJre = result.map_err(|err| self.install_error(err))?;
        let previous = self.state.jre.as_ref().map(|r| r.dir.clone()).filter(|d| *d != installed.record.dir);
        self.state.previous_jre = previous;
        self.state.jre = Some(installed.record);
        self.state.jre_checked_at = Some(Utc::now());
        self.save_state();
        Ok(())
    }

    fn install_error(&self, err: InstallError) -> FlowError {
        let lang = self.ctx.lang;
        match err {
            InstallError::NoSpace { needed, available, path } => {
                FlowError::new(ErrorKind::NoSpace, format!("{needed} needed, {available} available"))
                    .param("needed", i18n::size(lang, needed.saturating_sub(available)))
                    .param("path", path.display().to_string())
            }
            InstallError::Download(e) => FlowError::new(ErrorKind::JreDownload, e.to_string()),
            InstallError::Integrity(e) => FlowError::new(ErrorKind::JreDownload, e),
            InstallError::Cancelled => FlowError::new(ErrorKind::JreInstall, "cancelled"),
            other => FlowError::new(ErrorKind::JreInstall, other.to_string()),
        }
    }

    fn fetch_jar(&mut self, reporter: &dyn Reporter) -> Result<(), FlowError> {
        reporter.stage(Stage::JarDownload);
        let client = self.launcher_client()?;
        let url = self.jar_url();
        let never = AtomicBool::new(false);
        let mut attempt = 0;
        let record = loop {
            attempt += 1;
            let result =
                fetch::fetch_copy(&client, &url, &self.ctx.paths.launcher_jar(), &never, &mut |done, total| {
                    reporter.progress(done, total.unwrap_or(0))
                });
            match result {
                Ok(record) => break record,
                Err(fetch::FetchError::Download(e)) if attempt == 1 && e.is_transient() => {
                    warn!("jar download failed, retrying once: {e}")
                }
                Err(fetch::FetchError::NotAJar(e)) => {
                    return Err(FlowError::new(ErrorKind::JarCorrupt, format!("{url}: {e}")));
                }
                Err(fetch::FetchError::Download(DownloadError::Status { status: 404, .. })) => {
                    return Err(FlowError::new(ErrorKind::JarDownload, format!("{url}: HTTP 404")));
                }
                Err(e) => return Err(FlowError::new(ErrorKind::JarDownload, format!("{url}: {e}"))),
            }
        };
        self.state.jar = Some(record);
        self.save_state();
        Ok(())
    }

    fn launch_environment(&self) -> Vec<(OsString, OsString)> {
        let macos_icon = (self.ctx.host.os == Os::MacOs).then(|| {
            crate::platform::macos::bundle_of(&self.ctx.self_exe).and_then(|b| crate::platform::macos::bundle_icon(&b))
        });
        environment::child_environment(&self.ctx.env, macos_icon.as_ref().map(|icon| icon.as_deref()))
    }

    /// Checks that do not depend on timing (ADR 0006): jar, JRE, and on Linux the JavaFX libraries.
    fn preflight(
        &mut self,
        home: &std::path::Path,
        env: &[(OsString, OsString)],
        reporter: &dyn Reporter,
    ) -> Result<(), FlowError> {
        match &self.jar {
            Ok(JarSource::Copy { path }) if !fetch::copy_is_valid(path) => {
                warn!("the launcher jar copy is not usable, fetching it again");
                self.fetch_jar(reporter)?;
            }
            Ok(_) => {}
            Err(err) => return Err(err.clone()),
        }
        layout::check_files(home, self.ctx.host.os, self.feature())
            .map_err(|e| FlowError::new(ErrorKind::JreInstall, e.to_string()))?;
        if self.ctx.host.os == Os::Linux {
            match launch::libs::check(home, env) {
                None => warn!("ldd is not available: JavaFX libraries not checked"),
                Some(report) => {
                    if !report.optional_missing.is_empty() {
                        warn!(
                            "optional JavaFX libraries missing (software rendering or no sound): {:?}",
                            report.optional_missing
                        );
                    }
                    if !report.is_ok() {
                        let distro = std::fs::read_to_string("/etc/os-release")
                            .map(|t| launch::libs::distro_from_os_release(&t))
                            .unwrap_or(launch::libs::Distro::Unknown);
                        let (packages, unknown) = launch::libs::packages(&report.critical_missing, distro);
                        let mut err = FlowError::new(
                            ErrorKind::MissingLibs,
                            format!("{:?} on {distro:?}", report.critical_missing),
                        )
                        .param("libraries", report.critical_missing.join(", "));
                        err.command = launch::libs::install_command(distro, &packages);
                        if !unknown.is_empty() {
                            warn!("no package known for {unknown:?}");
                        }
                        return Err(err);
                    }
                }
            }
        }
        Ok(())
    }

    /// Starts the launcher with the installed JRE.
    pub fn launch(&mut self, reporter: &dyn Reporter) -> Result<Launched, FlowError> {
        reporter.stage(Stage::Launching);
        let Some((record, home)) = self.current_jre() else {
            return Err(FlowError::new(ErrorKind::JreInstall, "no usable JRE after installation"));
        };
        let env = self.launch_environment();
        self.preflight(&home, &env, reporter)?;
        let jar = self.jar.clone()?;
        // Under an AppImage the prestarter's working directory is inside the mount, which disappears after it exits:
        // the launcher starts where the player started the AppImage (OWD), or in the home directory.
        let cwd = self
            .ctx
            .env
            .appimage_dir()
            .and_then(|_| self.ctx.env.original_dir().filter(|d| d.is_dir()).or_else(dirs::home_dir));
        let log_path = self.ctx.paths.launch_log();
        let java = layout::launcher_java(&home, self.ctx.host.os);
        info!("starting {} -jar {} (JRE {})", java.display(), jar.path().display(), record.version);
        let args = self.ctx.args.clone();
        let spec = launch::Launch {
            java,
            jar: jar.path(),
            prestarter_args: &args,
            env: &env,
            cwd: cwd.as_deref(),
            log: &log_path,
        };

        let launched = if jar.is_embedded() {
            launch::start_and_leave(&spec).map_err(|e| FlowError::new(ErrorKind::LaunchFailed, e.to_string()))?;
            Launched::Detached
        } else {
            self.watch(&spec, reporter)?
        };
        self.collect_old_jres(&record);
        reporter.stage(Stage::Done);
        Ok(launched)
    }

    fn watch(&mut self, spec: &launch::Launch<'_>, reporter: &dyn Reporter) -> Result<Launched, FlowError> {
        let failed = |log: &str, detail: String| {
            let mut err = FlowError::new(ErrorKind::LaunchFailed, detail);
            err.log_tail = Some(launch::outcome::tail(log, 30));
            err
        };
        let io = |e: std::io::Error| FlowError::new(ErrorKind::LaunchFailed, e.to_string());
        let (mut outcome, mut log) = launch::start_and_watch(spec, false, self.ctx.timing.wrapper_watch).map_err(io)?;
        info!("wrapper outcome: {outcome:?}");
        if outcome == WrapperOutcome::CorruptJar {
            warn!("the wrapper refused the jar copy; fetching it again");
            self.fetch_jar(reporter)?;
            reporter.stage(Stage::Launching);
            (outcome, log) = launch::start_and_watch(spec, false, self.ctx.timing.wrapper_watch).map_err(io)?;
            info!("wrapper outcome after a new jar: {outcome:?}");
        }
        if outcome == WrapperOutcome::LauncherDied {
            warn!("the launcher JVM stopped within 3 s; retrying once with -Dlauncher.waitProcess=true");
            (outcome, log) = launch::start_and_watch(spec, true, self.ctx.timing.retry_watch).map_err(io)?;
            info!("wrapper outcome with waitProcess: {outcome:?}");
        }
        match outcome {
            WrapperOutcome::Started => Ok(Launched::Confirmed),
            WrapperOutcome::CorruptJar => {
                Err(FlowError { kind: ErrorKind::JarCorrupt, ..failed(&log, "the jar is refused again".into()) })
            }
            WrapperOutcome::LauncherDied => Err(failed(&log, "the launcher JVM stopped right after start".into())),
            WrapperOutcome::WrapperFailed(code) => Err(failed(&log, format!("the wrapper exited with {code}"))),
        }
    }

    fn collect_old_jres(&self, current: &JreRecord) {
        let mut keep = vec![current.dir.as_str()];
        if let Some(previous) = self.state.previous_jre.as_deref() {
            keep.push(previous);
        }
        if let Ok(Some(_lock)) = InstallLock::try_acquire(&self.ctx.paths.lock_file()) {
            install::collect(&self.ctx.paths.jre_dir(), &keep);
        }
    }

    /// The `headless.preparing` text for the no-WebView mode.
    pub fn preparing_text(&self) -> String {
        let size = fallback::release_for(&self.target, self.feature()).map_or(130_000_000, |r| r.size);
        i18n::t(self.ctx.lang, "headless.preparing", &[("size", &i18n::size(self.ctx.lang, size))])
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.ctx.paths.logs_dir()
    }
}
