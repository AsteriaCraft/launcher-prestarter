//! TLS and the launcher host (ADR 0001, 0007): the jar copy and the signed policy over HTTPS from a loopback server
//! with a test CA, the trust settings, and the same-origin redirect rule.

mod support;

use std::sync::atomic::AtomicBool;

use prestarter_lib::jar::fetch::{FetchError, fetch_copy};
use prestarter_lib::net::client::{self, Roots};
use prestarter_lib::net::download::{DownloadError, sha256_bytes};
use prestarter_lib::policy::{self, MANIFEST_FILE, POLICY_FILE, SIGNATURE_FILE};
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use support::{Response, Server, TempDir, TestCa, test_jar};

#[test]
fn the_jar_copy_comes_over_https_from_a_trusted_root() {
    let ca = TestCa::new();
    let server = Server::https(ca.server.clone());
    server.route("/Asterium.jar", Response::bytes(test_jar()));
    let dir = TempDir::new("https-jar");
    let dest = dir.path().join("launcher").join("Asterium.jar");
    let client = client::launcher_host(ca.roots()).unwrap();
    let record =
        fetch_copy(&client, &server.url("/Asterium.jar"), &dest, &AtomicBool::new(false), &mut |_, _| {}).unwrap();
    assert_eq!(record.sha256, sha256_bytes(&test_jar()));
    assert_eq!(std::fs::read(&dest).unwrap(), test_jar());
}

#[test]
fn a_server_outside_the_trusted_roots_is_refused() {
    let ca = TestCa::new();
    let server = Server::https(ca.server.clone());
    server.route("/Asterium.jar", Response::bytes(test_jar()));
    let dir = TempDir::new("https-untrusted");
    let dest = dir.path().join("Asterium.jar");
    // Mozilla's roots (what the release uses for the launcher host) do not include the test CA.
    let client = client::launcher_host(Roots::Mozilla).unwrap();
    let err =
        fetch_copy(&client, &server.url("/Asterium.jar"), &dest, &AtomicBool::new(false), &mut |_, _| {}).unwrap_err();
    assert!(matches!(err, FetchError::Download(DownloadError::Network { .. })), "{err}");
    assert!(!dest.exists());
}

#[test]
fn a_redirect_to_another_origin_is_not_followed() {
    let ca = TestCa::new();
    let server = Server::https(ca.server.clone());
    let other = Server::https(ca.server.clone());
    other.route("/Asterium.jar", Response::bytes(test_jar()));
    server.route("/Asterium.jar", Response::Redirect(other.url("/Asterium.jar").to_string()));
    server.route("/moved.jar", Response::Redirect("/Asterium2.jar".into()));
    server.route("/Asterium2.jar", Response::bytes(test_jar()));
    let dir = TempDir::new("https-redirect");
    let client = client::launcher_host(ca.roots()).unwrap();
    let never = AtomicBool::new(false);

    let err = fetch_copy(&client, &server.url("/Asterium.jar"), &dir.path().join("a.jar"), &never, &mut |_, _| {})
        .unwrap_err();
    assert!(matches!(err, FetchError::Download(DownloadError::Status { status: 302, .. })), "{err}");
    assert_eq!(other.hit_count("/Asterium.jar"), 0, "the other origin was never contacted");

    fetch_copy(&client, &server.url("/moved.jar"), &dir.path().join("b.jar"), &never, &mut |_, _| {}).unwrap();
}

#[test]
fn a_file_that_is_not_a_jar_never_replaces_the_copy() {
    let ca = TestCa::new();
    let server = Server::https(ca.server.clone());
    server.route("/Asterium.jar", Response::bytes(b"<html>maintenance</html>".to_vec()));
    let dir = TempDir::new("https-notjar");
    let dest = dir.path().join("Asterium.jar");
    std::fs::write(&dest, test_jar()).unwrap();
    let client = client::launcher_host(ca.roots()).unwrap();
    let err =
        fetch_copy(&client, &server.url("/Asterium.jar"), &dest, &AtomicBool::new(false), &mut |_, _| {}).unwrap_err();
    assert!(matches!(err, FetchError::NotAJar(_)), "{err}");
    assert_eq!(std::fs::read(&dest).unwrap(), test_jar(), "the working copy is untouched");
    let leftovers: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().collect();
    assert_eq!(leftovers.len(), 1, "no partial file is left behind");
}

#[test]
fn the_signed_policy_is_fetched_and_verified_over_https() {
    let ca = TestCa::new();
    let server = Server::https(ca.server.clone());
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let key = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    let public: [u8; 32] = key.public_key().as_ref().try_into().unwrap();

    let policy_bytes = br#"{"schema":1,"jarUrl":"https://launcher.asterium.pro/Asterium.jar","javaFeature":25,"minWrapperVersion":"0.3.0","latestWrapperVersion":"0.3.2","downloadPage":"https://asterium.pro/launcher"}"#.to_vec();
    let manifest = format!(
        r#"{{"schema":1,"repo":"AsteriaCraft/launcher-prestarter","component":"prestarter","tag":"v0.3.2","version":"0.3.2","channel":"stable","commit":"{}","assets":[{{"name":"{POLICY_FILE}","size":{},"sha256":"{}"}}]}}"#,
        "c".repeat(40),
        policy_bytes.len(),
        sha256_bytes(&policy_bytes)
    )
    .into_bytes();
    server.route("/downloads/prestarter-release.json", Response::bytes(manifest.clone()));
    server.route("/downloads/prestarter-release.json.sig", Response::bytes(key.sign(&manifest).as_ref().to_vec()));
    server.route("/downloads/prestarter-policy.json", Response::bytes(policy_bytes));

    let client = client::launcher_host(ca.roots()).unwrap();
    let base = policy::base_for(&server.url("/Asterium.jar"));
    let verified = policy::fetch(&client, &base, &[public]).unwrap();
    assert_eq!(verified.release_version.to_string(), "0.3.2");
    assert_eq!(verified.policy.latest_wrapper_version.as_deref(), Some("0.3.2"));

    // The real release key does not accept a manifest signed by another key.
    let err = policy::fetch(&client, &base, &prestarter_lib::policy::verify::builtin_keys()).unwrap_err();
    assert!(err.to_string().contains("signature"), "{err}");
}

#[test]
fn the_files_a_launchserver_mirrors_into_downloads_verify() {
    // tests/fixtures/release/launchserver-mirror: what nginx served from downloads/ in the gravit-docker e2e (section X2)
    // for v0.3.0 as this repository's release scripts write it (make-policy.sh, release-manifest.sh with publish.yml's
    // asset list, sign-release.sh), installed and mirrored by AsteriumReleases 2.3.0, signed with that run's throwaway
    // key (signing.pub.pem; its private half was never kept). See the README there.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/release/launchserver-mirror");
    let read = |name: &str| std::fs::read(dir.join(name)).unwrap();
    let key = policy::verify::parse_public_key_pem(&String::from_utf8(read("signing.pub.pem")).unwrap()).unwrap();
    let ca = TestCa::new();
    let server = Server::https(ca.server.clone());
    for name in [MANIFEST_FILE, SIGNATURE_FILE, POLICY_FILE] {
        server.route(&format!("/downloads/{name}"), Response::bytes(read(name)));
    }
    let client = client::launcher_host(ca.roots()).unwrap();
    let base = policy::base_for(&server.url("/Asterium.jar"));

    let verified = policy::fetch(&client, &base, &[key]).unwrap();
    assert_eq!(verified.release_version.to_string(), "0.3.0");
    assert_eq!(verified.policy.min_wrapper_version, "0.3.0");
    assert_eq!(verified.policy.latest_wrapper_version.as_deref(), Some("0.3.0"));
    assert_eq!(verified.policy.jar_url, "https://launcher.asterium.pro/Asterium.jar");
    assert_eq!(verified.policy_json.as_bytes(), read(POLICY_FILE).as_slice());
    // The manifest describes every asset with os/arch/format/role (ADR 0009); the policy check reads only its own.
    let manifest: serde_json::Value = serde_json::from_slice(&read(MANIFEST_FILE)).unwrap();
    assert_eq!(manifest["assets"].as_array().unwrap().len(), 9);

    // Signed by the e2e key, so the compiled-in release key refuses it.
    let err = policy::fetch(&client, &base, &policy::verify::builtin_keys()).unwrap_err();
    assert!(err.to_string().contains("signature"), "{err}");
}
