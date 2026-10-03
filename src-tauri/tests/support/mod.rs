//! Shared pieces of the integration tests: a loopback HTTP/HTTPS server (with a test CA), a fake JRE built around
//! `examples/fake_java.rs`, a test launcher jar, and a ready `Context`.

#![allow(dead_code)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use prestarter_lib::flow::{Context, Notice, Reporter, Stage, Timing};
use prestarter_lib::i18n::Lang;
use prestarter_lib::jre::catalog::{self, JreTarget, Package};
use prestarter_lib::launch::EnvSnapshot;
use prestarter_lib::net::client::Roots;
use prestarter_lib::net::overrides::Overrides;
use prestarter_lib::platform::Host;
use prestarter_lib::store::StorePaths;
use prestarter_lib::store::atomic::unique_suffix;
use reqwest::Url;

/// A folder name with letters beyond ASCII that Java on this machine can still read: Cyrillic; on a Windows whose
/// ANSI code page has no Cyrillic (GitHub's runners use 1252), Latin letters with diacritics; plain ASCII last.
pub fn readable_folder() -> &'static str {
    ["Ігри з пробілом", "Spiele für alle", "Games"]
        .into_iter()
        .find(|name| prestarter_lib::platform::java_cannot_write(Path::new(name)).is_none())
        .expect("ASCII is always readable")
}

// ---------------------------------------------------------------------------------------------------------------
// Temporary directories

pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Self {
        // A spaced name beyond ASCII on purpose: paths like this must work wherever Java can read them (Cyrillic,
        // or Latin letters with diacritics on a Windows whose ANSI code page has no Cyrillic).
        let word = ["тест", "tëst", "test"]
            .into_iter()
            .find(|word| prestarter_lib::platform::java_cannot_write(Path::new(word)).is_none())
            .expect("ASCII is always readable");
        let path = std::env::temp_dir().join(format!("asterium {word} {tag}-{}", unique_suffix()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if std::env::var_os("ASTERIUM_KEEP_TEST_DIRS").is_none() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Loopback server

#[derive(Clone)]
pub enum Response {
    Ok {
        body: Arc<Vec<u8>>,
        content_type: &'static str,
    },
    Status(u16),
    Redirect(String),
    /// Sends the body in chunks with a pause between them (to test progress and "Play now").
    Slow {
        body: Arc<Vec<u8>>,
        chunk: usize,
        pause: Duration,
    },
}

impl Response {
    pub fn bytes(body: Vec<u8>) -> Self {
        Response::Ok { body: Arc::new(body), content_type: "application/octet-stream" }
    }

    pub fn json(body: impl Into<String>) -> Self {
        Response::Ok { body: Arc::new(body.into().into_bytes()), content_type: "application/json" }
    }
}

pub struct Server {
    pub base: String,
    routes: Arc<Mutex<HashMap<String, Response>>>,
    hits: Arc<Mutex<Vec<String>>>,
}

impl Server {
    pub fn http() -> Self {
        Self::start(None)
    }

    pub fn https(tls: Arc<rustls::ServerConfig>) -> Self {
        Self::start(Some(tls))
    }

    fn start(tls: Option<Arc<rustls::ServerConfig>>) -> Self {
        prestarter_lib::net::client::install_crypto_provider();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let scheme = if tls.is_some() { "https" } else { "http" };
        let routes: Arc<Mutex<HashMap<String, Response>>> = Arc::default();
        let hits: Arc<Mutex<Vec<String>>> = Arc::default();
        let (thread_routes, thread_hits) = (Arc::clone(&routes), Arc::clone(&hits));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let routes = Arc::clone(&thread_routes);
                let hits = Arc::clone(&thread_hits);
                let tls = tls.clone();
                std::thread::spawn(move || {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                    match tls {
                        None => serve(stream, &routes, &hits),
                        Some(config) => {
                            let Ok(connection) = rustls::ServerConnection::new(config) else { return };
                            serve(rustls::StreamOwned::new(connection, stream), &routes, &hits)
                        }
                    }
                });
            }
        });
        Self { base: format!("{scheme}://127.0.0.1:{port}"), routes, hits }
    }

    pub fn route(&self, path: &str, response: Response) {
        self.routes.lock().unwrap().insert(path.to_owned(), response);
    }

    pub fn url(&self, path: &str) -> Url {
        Url::parse(&format!("{}{path}", self.base)).unwrap()
    }

    /// Requested paths (with query), in order.
    pub fn hits(&self) -> Vec<String> {
        self.hits.lock().unwrap().clone()
    }

    pub fn hit_count(&self, path: &str) -> usize {
        self.hits().iter().filter(|h| h.split('?').next() == Some(path)).count()
    }
}

fn serve<S: Read + Write>(mut stream: S, routes: &Mutex<HashMap<String, Response>>, hits: &Mutex<Vec<String>>) {
    let mut request = Vec::new();
    let mut buffer = [0u8; 4096];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(n) => request.extend_from_slice(&buffer[..n]),
        }
        if request.len() > 64 * 1024 {
            return;
        }
    }
    let text = String::from_utf8_lossy(&request);
    let target = text.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("/").to_owned();
    hits.lock().unwrap().push(target.clone());
    let path = target.split('?').next().unwrap_or("/").to_owned();
    let response = routes.lock().unwrap().get(&path).cloned().unwrap_or(Response::Status(404));
    let head = |status: u16, extra: &str, length: usize| {
        format!("HTTP/1.1 {status} X\r\nContent-Length: {length}\r\nConnection: close\r\n{extra}\r\n")
    };
    let _ = match response {
        Response::Ok { body, content_type } => stream
            .write_all(head(200, &format!("Content-Type: {content_type}\r\n"), body.len()).as_bytes())
            .and_then(|()| stream.write_all(&body)),
        Response::Status(status) => stream.write_all(head(status, "", 0).as_bytes()),
        Response::Redirect(location) => stream.write_all(head(302, &format!("Location: {location}\r\n"), 0).as_bytes()),
        Response::Slow { body, chunk, pause } => {
            let mut result = stream.write_all(head(200, "", body.len()).as_bytes());
            for part in body.chunks(chunk) {
                if result.is_err() {
                    break;
                }
                result = stream.write_all(part).and_then(|()| stream.flush());
                std::thread::sleep(pause);
            }
            result
        }
    };
    let _ = stream.flush();
}

// ---------------------------------------------------------------------------------------------------------------
// Test CA

pub struct TestCa {
    pub ca_der: Vec<u8>,
    pub server: Arc<rustls::ServerConfig>,
}

impl TestCa {
    pub fn new() -> Self {
        prestarter_lib::net::client::install_crypto_provider();
        let mut ca_params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca_params.distinguished_name.push(rcgen::DnType::CommonName, "Asterium test CA");
        ca_params.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign, rcgen::KeyUsagePurpose::DigitalSignature];
        let ca_key = rcgen::KeyPair::generate().unwrap();
        let ca_cert = ca_params.self_signed(&ca_key).unwrap();
        let issuer = rcgen::Issuer::new(ca_params, ca_key);

        let mut leaf_params =
            rcgen::CertificateParams::new(vec!["127.0.0.1".to_owned(), "localhost".to_owned()]).unwrap();
        leaf_params.distinguished_name.push(rcgen::DnType::CommonName, "127.0.0.1");
        leaf_params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
        leaf_params.use_authority_key_identifier_extension = true;
        let leaf_key = rcgen::KeyPair::generate().unwrap();
        let leaf_cert = leaf_params.signed_by(&leaf_key, &issuer).unwrap();

        let chain = vec![leaf_cert.der().clone(), ca_cert.der().clone()];
        let key = rustls::pki_types::PrivateKeyDer::Pkcs8(leaf_key.serialize_der().into());
        let server = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .expect("test certificate");
        Self { ca_der: ca_cert.der().to_vec(), server: Arc::new(server) }
    }

    pub fn roots(&self) -> Roots {
        Roots::Only(vec![reqwest::Certificate::from_der(&self.ca_der).unwrap()])
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Fake JRE and jars

/// `target/<profile>/examples/fake_java(.exe)`, built by `cargo test`.
pub fn fake_java_binary() -> PathBuf {
    let deps = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    let name = if cfg!(windows) { "fake_java.exe" } else { "fake_java" };
    let path = deps.parent().unwrap().join("examples").join(name);
    assert!(path.is_file(), "{} is missing: run the tests with `cargo test` (examples are built)", path.display());
    path
}

pub fn host_target() -> JreTarget {
    catalog::for_host(Host::current(), true)
}

pub const TOP_DIR: &str = "jre-25.0.4.1-full";

/// A JRE archive for this OS whose java binaries are fake-java.
pub fn jre_archive(java_version: &str) -> Vec<u8> {
    let fake = fs::read(fake_java_binary()).unwrap();
    let release = format!("IMPLEMENTOR=\"Test\"\nJAVA_VERSION=\"{java_version}\"\n");
    match host_target().package {
        Package::Zip => {
            let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            writer.add_directory(format!("{TOP_DIR}/"), options).unwrap();
            writer.add_directory(format!("{TOP_DIR}/bin/"), options).unwrap();
            for name in ["java.exe", "javaw.exe"] {
                writer.start_file(format!("{TOP_DIR}/bin/{name}"), options).unwrap();
                writer.write_all(&fake).unwrap();
            }
            writer.start_file(format!("{TOP_DIR}/release"), options).unwrap();
            writer.write_all(release.as_bytes()).unwrap();
            writer.finish().unwrap().into_inner()
        }
        Package::TarGz => {
            let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
            let mut add = |path: &str, data: &[u8], mode: u32| {
                let mut header = tar::Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(mode);
                header.set_cksum();
                builder.append_data(&mut header, path, data).unwrap();
            };
            add(&format!("{TOP_DIR}/bin/java"), &fake, 0o755);
            add(&format!("{TOP_DIR}/release"), release.as_bytes(), 0o644);
            builder.into_inner().unwrap().finish().unwrap()
        }
    }
}

pub fn sha1_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A Liberica API answer with one record for this host's target.
pub fn api_answer(download_url: &Url, archive: &[u8], version: &str, sha1: Option<&str>) -> String {
    let target = host_target();
    let filename = download_url.path().rsplit('/').next().unwrap();
    serde_json::json!([{
        "downloadUrl": download_url.as_str(),
        "filename": filename,
        "size": archive.len(),
        "sha1": sha1.map(str::to_owned).unwrap_or_else(|| sha1_hex(archive)),
        "version": version,
        "featureVersion": 25,
        "GA": true,
        "FX": true,
        "bundleType": "jre-full",
        "packageType": target.package.api_name(),
        "os": target.api_os,
        "architecture": target.api_arch,
        "bitness": 64,
    }])
    .to_string()
}

pub fn archive_name(version: &str) -> String {
    let ext = match host_target().package {
        Package::Zip => "zip",
        Package::TarGz => "tar.gz",
    };
    format!("/bellsoft-jre{version}-test-full.{ext}")
}

pub const MANIFEST: &str =
    "Manifest-Version: 1.0\r\nMain-Class: pro.gravit.launcher.start.ClientLauncherWrapper\r\n\r\n";

pub fn test_jar() -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    writer.start_file("META-INF/MANIFEST.MF", options).unwrap();
    writer.write_all(MANIFEST.as_bytes()).unwrap();
    writer.start_file("pro/gravit/launcher/start/ClientLauncherWrapper.class", options).unwrap();
    writer.write_all(&[0xCA, 0xFE, 0xBA, 0xBE]).unwrap();
    writer.finish().unwrap().into_inner()
}

// ---------------------------------------------------------------------------------------------------------------
// Context and reporter

pub struct Fixture {
    pub dir: TempDir,
    pub store: PathBuf,
    pub java_dir: PathBuf,
    pub server: Server,
}

impl Fixture {
    pub fn new(tag: &str) -> Self {
        let dir = TempDir::new(tag);
        let store = dir.path().join("store");
        let java_dir = dir.path().join("fake-java");
        Self { dir, store, java_dir, server: Server::http() }
    }

    /// The store under `folder`, the way a Windows user name puts it into the profile; the rest as [`Fixture::new`].
    pub fn with_store_in(tag: &str, folder: &str) -> Self {
        let mut fixture = Self::new(tag);
        fixture.store = fixture.dir.path().join(folder).join("store");
        fixture
    }

    /// Serves an API answer and the archive for `version`.
    pub fn serve_jre(&self, version: &str) {
        let archive = jre_archive(version);
        let path = archive_name(version);
        self.server.route("/api", Response::json(api_answer(&self.server.url(&path), &archive, version, None)));
        self.server.route(&path, Response::bytes(archive));
    }

    pub fn serve_jar(&self) {
        self.server.route("/Asterium.jar", Response::bytes(test_jar()));
    }

    /// A file without a jar (copy mode).
    pub fn bare_exe(&self) -> PathBuf {
        let path = self.dir.path().join("Prestarter-bare");
        fs::write(&path, vec![0x7fu8; 4096]).unwrap();
        path
    }

    /// A file with the test jar appended (embedded mode), named like the server's build, in a folder with letters
    /// beyond ASCII ([`readable_folder`]).
    pub fn embedded_exe(&self) -> PathBuf {
        self.embedded_exe_in(readable_folder())
    }

    pub fn embedded_exe_in(&self, folder: &str) -> PathBuf {
        let mut bytes = vec![0x4du8, 0x5a];
        bytes.extend(vec![0u8; 8192]);
        bytes.extend(test_jar());
        let path = self.dir.path().join(folder).join("Asterium.exe");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }

    pub fn env(&self, plan: &str, extra: &[(&str, &str)]) -> EnvSnapshot {
        let replaced = |name: &str| extra.iter().any(|(k, _)| k.eq_ignore_ascii_case(name));
        let mut pairs: Vec<(OsString, OsString)> = std::env::vars_os()
            .filter(|(k, _)| {
                let k = k.to_string_lossy();
                !k.starts_with("FAKE_JAVA_") && !k.starts_with("ASTERIUM_") && !replaced(&k)
            })
            .collect();
        pairs.push(("FAKE_JAVA_DIR".into(), self.java_dir.clone().into_os_string()));
        pairs.push(("FAKE_JAVA_PLAN".into(), plan.into()));
        for (k, v) in extra {
            pairs.push(((*k).into(), (*v).into()));
        }
        EnvSnapshot::from_pairs(pairs)
    }

    pub fn context(&self, self_exe: PathBuf, plan: &str, extra_env: &[(&str, &str)], copy_mode: bool) -> Context {
        Context {
            host: Host::current(),
            own_version: semver::Version::parse(prestarter_lib::VERSION).unwrap(),
            lang: Lang::En,
            paths: StorePaths::new(self.store.clone()),
            overrides: Overrides {
                jre_api: Some(self.server.url("/api")),
                launcher_url: copy_mode.then(|| self.server.url("/Asterium.jar")),
                store: Some(self.store.clone()),
                noninteractive: true,
            },
            env: self.env(plan, extra_env),
            args: vec!["--debug".into()],
            self_exe,
            jre_api: self.server.url("/api"),
            keys: prestarter_lib::policy::verify::builtin_keys(),
            x64_emulation: true,
            liberica_roots: Roots::Platform,
            launcher_roots: Roots::Platform,
            timing: Timing {
                api_check: Duration::from_secs(3),
                wrapper_watch: Duration::from_secs(4),
                retry_watch: Duration::from_secs(4),
            },
        }
    }

    /// The `n`-th fake-java start, waiting up to 10 s for it to appear.
    pub fn run_record(&self, n: usize) -> serde_json::Value {
        let path = self.java_dir.join(format!("run-{n}.json"));
        let started = Instant::now();
        loop {
            if let Ok(bytes) = fs::read(&path)
                && let Ok(value) = serde_json::from_slice(&bytes)
            {
                return value;
            }
            assert!(started.elapsed() < Duration::from_secs(10), "fake-java run {n} never happened");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn runs(&self) -> usize {
        (0..).find(|n| !self.java_dir.join(format!("run-{n}.json")).exists()).unwrap()
    }
}

#[derive(Default)]
pub struct Recorder {
    pub stages: Mutex<Vec<Stage>>,
    pub notices: Mutex<Vec<Notice>>,
    pub progress: Mutex<Vec<(u64, u64)>>,
    pub on_progress: Option<Box<dyn Fn() + Send + Sync>>,
}

impl Reporter for Recorder {
    fn stage(&self, stage: Stage) {
        self.stages.lock().unwrap().push(stage);
    }

    fn progress(&self, done: u64, total: u64) {
        self.progress.lock().unwrap().push((done, total));
        if let Some(callback) = &self.on_progress {
            callback();
        }
    }

    fn notice(&self, notice: Notice) {
        self.notices.lock().unwrap().push(notice);
    }
}

pub fn args_of(record: &serde_json::Value) -> Vec<String> {
    record["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_owned()).collect()
}

pub fn env_of(record: &serde_json::Value) -> HashMap<String, String> {
    record["env"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned())).collect()
}

pub fn connect_refused_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let _ = TcpStream::connect_timeout(&format!("127.0.0.1:{port}").parse().unwrap(), Duration::from_millis(10));
    port
}
